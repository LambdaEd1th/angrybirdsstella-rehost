//! The global AimStream draw member (`sub_10004C4CC`).

use crate::*;

pub(in crate::game_lua::trajectory_registration) fn install_draw(
    lua: &Lua,
    globals: &mlua::Table,
    render: Arc<Mutex<RenderBridge>>,
    resources: Arc<Mutex<ResourceRuntime>>,
    data_root: Arc<PathBuf>,
) -> LuaResult<()> {
    globals.set(
        "native_drawSimulationTrajectory",
        lua.create_function(move |_, _: MultiValue| {
            let resources = resources.lock().expect("resource runtime lock poisoned");
            let mut bridge = render.lock().expect("render bridge lock poisoned");
            // This member draws only the global AimStream, never either
            // double-buffered flight-trail record.
            if bridge.aim_stream_control_points.len() <= 3 {
                return Ok(());
            }
            let control_points = bridge.aim_stream_control_points.clone();
            let particles = bridge.aim_stream_particles.clone();
            let sprite = bridge.aiming_aid_sprite.clone();
            let top_left_x = bridge.top_left_x as f32;
            let top_left_y = bridge.top_left_y as f32;
            let world_scale = bridge.world_scale as f32;
            let physics_scale = bridge.physics_simulation_scale as f32;
            let commands = if sprite.is_empty() {
                Vec::new()
            } else {
                let bound_region = resources
                    .active_atlas_catalog_region(&sprite, &data_root)
                    .map(Arc::new);
                let mut bound_composite = resources.active_bound_composite(&sprite).map(Arc::new);
                if bound_region.is_none() && bound_composite.is_none() {
                    bound_composite = Some(Arc::new(Vec::new()));
                }
                let sprite: SharedSpriteName = sprite.into();
                particles
                    .into_iter()
                    .filter_map(|particle| {
                        let (curve_x, curve_y) =
                            native_aim_stream_point(&control_points, particle.path_parameter)?;
                        let draw_x = (curve_x * physics_scale) / particle.scale;
                        let draw_y = (curve_y * physics_scale) / particle.scale;
                        let context_scale = world_scale * particle.scale;
                        Some(RenderCommand {
                            order: 0,
                            sprite: sprite.clone(),
                            texture: None,
                            bound_region: bound_region.clone(),
                            bound_composite: bound_composite.clone(),
                            geometry: None,
                            shader: None,
                            dirt: None,
                            x: f64::from(draw_x),
                            y: f64::from(draw_y),
                            state: RenderState {
                                translate_x: f64::from(-top_left_x / particle.scale),
                                translate_y: f64::from(-top_left_y / particle.scale),
                                scale_x: f64::from(context_scale),
                                scale_y: f64::from(context_scale),
                                angle: f64::from(particle.angle),
                                pivot_x: 10.0,
                                pivot_y: 10.0,
                                ..RenderState::default()
                            },
                            world_space: false,
                        })
                    })
                    .collect::<Vec<_>>()
            };
            bridge.extend_render_commands(commands);
            // The enabled flag is copied to the AimStream only after this
            // frame's draw; a false-to-true edge repopulates the next frame.
            let enabled = bridge.aiming_aid_enabled;
            bridge.set_native_aim_stream_active(enabled);
            Ok(())
        })?,
    )
}
