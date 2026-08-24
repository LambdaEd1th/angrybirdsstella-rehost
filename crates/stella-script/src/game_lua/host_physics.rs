//! Fixed Box2D stepping inside native update member `sub_10005E898`.

mod contact_manager;
mod islands;
mod toi;

use crate::*;
use std::time::Instant;

const PHYSICS_STEP_F32: f32 = f32::from_bits(0x3D08_8889);
const PHYSICS_STEP: f64 = PHYSICS_STEP_F32 as f64;
const VELOCITY_ITERATIONS: usize = 10;
const POSITION_ITERATIONS: usize = 10;
const MAX_TRANSLATION: f64 = 0.16;
const MAX_ROTATION: f64 = 15_708.0 / 10_000.0;

impl StellaLua {
    /// Advance the native physics world using the fixed step recovered from
    /// sub_10005E898. The original accumulates scaled frame time and invokes
    /// Lua `updatePhysics(1/30)` immediately before every Box2D step.
    pub(crate) fn step_physics(&self, scaled_delta: f64) -> Result<f32, ScriptError> {
        // Purple calls b2World::Step(dt, 10, 10) at sub_10005E898. Its
        // embedded b2Island::Solve at sub_10086CE84 also exposes these
        // otherwise compile-time Box2D tuning constants directly.
        let step_count = {
            let mut bridge = self.render.lock().expect("render bridge lock poisoned");
            if !bridge.physics_enabled {
                // 0x10005ECDC branches around the accumulator itself while
                // GameLua+0x6A8 is nonzero. Locked wall time is not banked,
                // but the pre-lock float32 remainder is retained verbatim.
                return Ok(0.0);
            }
            // 0x10005ED6C performs one FADD into the float accumulator and
            // 0x10005EDA0/0x10005EDE4 compare and subtract 0x3D088889 once
            // per step. A double division/floor followed by one multiplied
            // subtraction changes both threshold behavior and the remainder.
            bridge.physics_accumulator += scaled_delta as f32;
            let mut steps = 0;
            while bridge.physics_accumulator >= PHYSICS_STEP_F32 {
                bridge.physics_accumulator += -PHYSICS_STEP_F32;
                steps += 1;
            }
            steps
        };
        let environment = game_environment(&self.lua)?;
        let mut physics_update_micros = 0_u64;
        for _ in 0..step_count {
            if let Value::Function(update_physics) = environment.get::<Value>("updatePhysics")? {
                update_physics.call::<()>(PHYSICS_STEP)?;
            }
            // Purple samples its microsecond monotonic clock immediately
            // after updatePhysics and again when b2World::Step returns. The
            // published diagnostic excludes removeBlocks and interpolation.
            let physics_update_started = Instant::now();
            // The native contact filter reads objects.world material tables
            // during b2World::Step rather than caching them in fixtures.
            self.sync_native_collision_filter_state()?;

            self.render
                .lock()
                .expect("render bridge lock poisoned")
                .physics_world_locked = true;
            let native_step = (|| {
                let contact_events = self.refresh_contact_manager()?;
                let (mut contact_events, mut toi_sweep_starts) =
                    self.solve_discrete_islands(contact_events);
                self.solve_continuous_islands(&mut contact_events, &mut toi_sweep_starts)?;
                Ok::<_, ScriptError>(contact_events)
            })();
            self.render
                .lock()
                .expect("render bridge lock poisoned")
                .physics_world_locked = false;
            let contact_events = native_step?;
            {
                let mut bridge = self.render.lock().expect("render bridge lock poisoned");
                // b2World::Step's auto-clear pass follows SolveTOI and clears
                // every body, including forces applied by BeginContact.
                for object in bridge.scene.values_mut() {
                    object.force_x = 0.0;
                    object.force_y = 0.0;
                    object.torque = 0.0;
                }
            }
            let elapsed_micros = physics_update_started.elapsed().as_micros();
            physics_update_micros = physics_update_micros
                .saturating_add(u64::try_from(elapsed_micros).unwrap_or(u64::MAX));
            {
                let bridge = self.render.lock().expect("render bridge lock poisoned");
                trace_physics_body(&bridge, "after-position");
            }

            for event in &contact_events {
                if std::env::var_os("STELLA_TRACE_CONTACTS").is_some() {
                    eprintln!(
                        "contact first={:?} second={:?} sensor={} began={} ended={} impulse={} normal=({}, {}) point=({}, {}) first_mass={} first_velocity=({}, {}) second_mass={} second_velocity=({}, {})",
                        event.first,
                        event.second,
                        event.sensor,
                        event.began,
                        event.ended,
                        event.impulse,
                        event.normal_x,
                        event.normal_y,
                        event.point_x,
                        event.point_y,
                        event.first_mass,
                        event.first_velocity_x,
                        event.first_velocity_y,
                        event.second_mass,
                        event.second_velocity_x,
                        event.second_velocity_y
                    );
                }
            }
            // sub_10005E898 invokes removeBlocks immediately after World::Step
            // (and therefore after BeginContact's Lua callbacks), before the
            // collision-velocity map is consumed.
            if let Value::Function(remove_blocks) = environment.get::<Value>("removeBlocks")? {
                remove_blocks.call::<()>(())?;
                self.sync_scene_lifetime()?;
            }

            let states = {
                let mut bridge = self.render.lock().expect("render bridge lock poisoned");
                bridge.apply_pending_collision_velocities();
                bridge
                    .scene
                    .iter()
                    .filter(|(_, object)| object.moves_during_step())
                    .map(|(name, object)| {
                        (
                            name.clone(),
                            object.x,
                            object.y,
                            object.velocity_x,
                            object.velocity_y,
                            object.angle,
                            object.angular_velocity,
                            object.sleeping,
                        )
                    })
                    .collect::<Vec<_>>()
            };
            let world = object_world(&self.lua)?;
            for (name, x, y, velocity_x, velocity_y, angle, angular_velocity, sleeping) in states {
                if let Value::Table(entry) = world.raw_get::<Value>(name.as_str())? {
                    entry.set("x", x)?;
                    entry.set("y", y)?;
                    entry.set("velocityX", velocity_x)?;
                    entry.set("velocityY", velocity_y)?;
                    entry.set("xVel", velocity_x)?;
                    entry.set("yVel", velocity_y)?;
                    entry.set("angle", angle)?;
                    entry.set("angularVelocity", angular_velocity)?;
                    entry.set("sleeping", sleeping)?;
                }
            }
        }
        // sub_10005E898 clears the per-render-frame Lua force closures after
        // the fixed-step loop, including frames whose accumulator did not yet
        // reach 1/30 second. A force queued by the later Lua update therefore
        // survives exactly until the next native physics pass.
        if let Value::Function(clear_forces) = environment.get::<Value>("clearLuaForceFunctions")? {
            clear_forces.call::<()>(())?;
        }
        Ok((physics_update_micros as f32) * 0.001_f32)
    }
}
