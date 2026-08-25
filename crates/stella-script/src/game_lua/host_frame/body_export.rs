//! Per-display-frame RenderObjectData-to-Lua export in `sub_10005E898`.

use crate::*;

#[derive(Debug)]
pub(super) struct NativeBodyLuaState {
    name: String,
    motion: Option<NativeBodyLuaMotion>,
    out_of_boundaries: bool,
    angle: f32,
    sleeping: bool,
}

#[derive(Debug)]
struct NativeBodyLuaMotion {
    x: f32,
    y: f32,
    velocity: f32,
    recorded_velocity: Option<(f32, f32, f32)>,
}

impl NativeBodyLuaState {
    pub(super) fn collect(bridge: &RenderBridge) -> Vec<Self> {
        let [x_min, x_max, y_min, y_max] = bridge.level_limits;
        bridge
            .scene
            .iter()
            .filter_map(|(name, object)| {
                if !object.has_physics_body() {
                    return None;
                }
                let awake = !object.sleeping;
                let reports_motion = object.kinematic_body || object.inverse_mass > 0.0;
                let motion = (reports_motion && (object.native_was_awake || awake)).then(|| {
                    let velocity_x = object.velocity_x as f32;
                    let velocity_y = object.velocity_y as f32;
                    NativeBodyLuaMotion {
                        // RenderObjectData+0xA4/+0xA8 contain the interpolated
                        // display pose written at 0x10005F108..0x10005F168.
                        x: object.render_x as f32,
                        y: object.render_y as f32,
                        // 0x10005F608..0x10005F610 squares y first, then
                        // folds x*x into it with one float32 FMADD.
                        velocity: velocity_x
                            .mul_add(velocity_x, velocity_y * velocity_y)
                            .sqrt(),
                        recorded_velocity: (object.controllable || object.record_velocity)
                            .then_some((velocity_x, velocity_y, object.render_angle as f32)),
                    }
                });
                let (sine, cosine) = (object.angle as f32).sin_cos();
                Some(Self {
                    name: name.clone(),
                    out_of_boundaries: motion.is_some()
                        && object.inverse_mass > 0.0
                        && (object.render_x < x_min
                            || object.render_x > x_max
                            || object.render_y < y_min
                            || object.render_y > y_max),
                    motion,
                    // The live Box2D sweep angle is unbounded. Purple exports
                    // b2Transform::q through atan2f(s, c), not sweep.a.
                    angle: sine.atan2(cosine),
                    sleeping: object.sleeping,
                })
            })
            .collect()
    }
}

impl StellaLua {
    pub(super) fn export_native_body_lua_state(
        &self,
        world: &mlua::Table,
        out_of_boundaries: &mlua::Table,
        states: Vec<NativeBodyLuaState>,
    ) -> Result<(), ScriptError> {
        for state in states {
            let Value::Table(entry) = world.raw_get::<Value>(state.name.as_str())? else {
                continue;
            };
            if let Some(motion) = state.motion {
                entry.set("x", motion.x)?;
                entry.set("y", motion.y)?;
                // sub_10002BCE8 uses lua_settable rather than lua_rawset, so
                // an authored __newindex observes x/y before velocity.
                if state.out_of_boundaries {
                    out_of_boundaries.set(state.name.as_str(), true)?;
                }
                entry.set("velocity", motion.velocity)?;
                if let Some((velocity_x, velocity_y, render_angle)) = motion.recorded_velocity {
                    entry.set("xVel", velocity_x)?;
                    entry.set("yVel", velocity_y)?;
                    // Purple writes the interpolated angle in the optional
                    // velocity-recording path, then the body angle below.
                    entry.set("angle", render_angle)?;
                }
            }
            entry.set("angle", state.angle)?;
            entry.set("sleeping", state.sleeping)?;
        }
        Ok(())
    }
}
