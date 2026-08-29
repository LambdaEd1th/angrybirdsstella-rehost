//! The one-body `BirdSimulation` predictor (`sub_100032970`).

use crate::*;

pub(in crate::game_lua::trajectory_registration) fn install_update(
    lua: &Lua,
    globals: &mlua::Table,
    render: Arc<Mutex<RenderBridge>>,
) -> LuaResult<()> {
    globals.set(
        "updateBirdTrajectoryTable",
        lua.create_function(move |lua, _: MultiValue| {
            let mut bridge = render.lock().expect("render bridge lock poisoned");
            // b2World::IsLocked is the first native guard. In particular it
            // leaves both GameLua+0x88 (the selected simulation bird) and the
            // previous trajectory untouched so a callback during Step cannot
            // tear down the aiming state halfway through a frame.
            if bridge.physics_world_locked {
                return Ok(());
            }
            let current_time_step = native_trajectory_current_time_step(lua)?;
            let iterations = bridge.simulation_iterations;
            let time_step_multiplier = bridge.simulation_time_step_multiplier;
            let point_sampler = bridge.simulation_store_points_sampler;
            // The native member always clears this pointer before returning.
            bridge.selected_simulation_bird = None;
            let Some(mut simulation) = bridge.game_lua_object("BirdSimulation").cloned() else {
                bridge.trajectory_points.clear();
                return Ok(());
            };
            // GameLua+0x3A0 is the insertion-only list populated by object
            // parameter 32. Every retained head fixture is tested against
            // BirdSimulation before each specialized one-body step.
            let aiming_force_sources = bridge
                .scene
                .values()
                .filter(|object| object.aiming_aid_collideable)
                .cloned()
                .collect::<Vec<_>>();
            let sensor_force_settings = (
                bridge.gravity_force_multiplier as f32,
                bridge.water_force_multiplier as f32,
                bridge.bird_water_drag as f32,
                bridge.object_water_drag as f32,
                bridge.game_world_scale as f32,
            );

            let step = current_time_step * time_step_multiplier;
            bridge.trajectory_points.clear();
            for iteration in 0..iterations.max(0) {
                for sensor in &aiming_force_sources {
                    if sensor.native_head_fixture_overlaps(&simulation) {
                        apply_native_sensor_forces_to_object(
                            sensor,
                            &mut simulation,
                            sensor_force_settings.0,
                            sensor_force_settings.1,
                            sensor_force_settings.2,
                            sensor_force_settings.3,
                            sensor_force_settings.4,
                        );
                    }
                }
                // At 0x100032B8C Purple applies this positive-only force as
                // mass*additionalGravity at the render transform origin.
                if bridge.additional_bird_gravity > 0.0 {
                    let force_y =
                        (bridge.additional_bird_gravity as f32) * simulation.native_body_mass();
                    simulation.apply_native_force_at(
                        (0.0_f32, force_y),
                        (simulation.x as f32, simulation.y as f32),
                    );
                }
                // sub_10086F6AC ignores solid contacts, joints and
                // gravityScale while retaining Box2D motion clamps.
                step_native_trajectory_body(
                    &mut simulation,
                    (bridge.world_gravity_x as f32, bridge.world_gravity_y as f32),
                    step,
                );
                // sub_10086F624 is b2World::ClearForces.
                simulation.force_x = 0.0;
                simulation.force_y = 0.0;
                simulation.torque = 0.0;
                // AArch64 SDIV yields zero on division by zero, so sampler=0
                // retains only iteration zero.
                if (point_sampler == 0 && iteration == 0)
                    || (point_sampler != 0 && iteration % point_sampler == 0)
                {
                    bridge.trajectory_points.push((simulation.x, simulation.y));
                }
            }
            // AimStream::setPoints ignores fewer than four samples and
            // otherwise duplicates both endpoints around the sample vector.
            if bridge.trajectory_points.len() >= 4 {
                let first = bridge.trajectory_points[0];
                let last = *bridge
                    .trajectory_points
                    .last()
                    .expect("non-empty trajectory");
                let mut control_points = Vec::with_capacity(bridge.trajectory_points.len() + 2);
                control_points.push(first);
                control_points.extend(bridge.trajectory_points.iter().copied());
                control_points.push(last);
                bridge.aim_stream_control_points = control_points;
                bridge.update_native_aim_stream(0.0_f32);
            }
            Ok(())
        })?,
    )
}
