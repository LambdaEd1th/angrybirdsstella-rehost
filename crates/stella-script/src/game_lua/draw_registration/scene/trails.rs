//! Two retained trajectory buffers inserted at the first native anchor.

use crate::*;

pub(super) fn push_native_trajectory_streams(
    bridge: &mut RenderBridge,
    resources: &ResourceRuntime,
    data_root: &Path,
) {
    let top_left_x = bridge.top_left_x as f32;
    let top_left_y = bridge.top_left_y as f32;
    let world_scale = bridge.world_scale as f32;
    let game_world_scale = bridge.game_world_scale as f32;
    let trail_state = RenderState {
        translate_x: f64::from(-top_left_x / game_world_scale),
        translate_y: f64::from(-top_left_y / game_world_scale),
        scale_x: f64::from(world_scale * game_world_scale),
        scale_y: f64::from(world_scale * game_world_scale),
        ..RenderState::default()
    };

    // 0x10004BF64..0x10004BF94 writes this context before calling
    // sub_10006D9C0. It remains live for the anchor's pre-draw callback;
    // Purple does not restore the context after drawing either stream.
    bridge.state = trail_state;
    let mut commands = Vec::new();
    // sub_10006D9C0 reads the two fixed 0x38-byte records at GameLua+0x558
    // in slot order and re-reads each point-vector end while iterating. The
    // bridge lock excludes mutators here, so no trajectory clone is needed.
    for stream in &bridge.trajectory_streams {
        if !stream.normal_sprite.is_empty() {
            let bound_region =
                resources.active_atlas_catalog_region(&stream.normal_sprite, data_root);
            let mut bound_composite = resources
                .active_bound_composite(&stream.normal_sprite)
                .map(|owner| owner.snapshot());
            if bound_region.is_none() && bound_composite.is_none() {
                bound_composite = Some(Arc::new(Vec::new()));
            }
            let sprite: SharedSpriteName = stream.normal_sprite.as_str().into();
            commands.extend(stream.points.iter().map(|&(x, y)| RenderCommand {
                projection_3d: None,
                order: 0,
                sprite: sprite.clone(),
                texture: None,
                bound_region: bound_region.clone(),
                bound_composite: bound_composite.clone(),
                geometry: None,
                shader: None,
                dirt: None,
                x: x as f32 / game_world_scale,
                y: y as f32 / game_world_scale,
                state: trail_state.into(),
                world_space: false,
            }));
        }
        if let Some((x, y)) = stream.puff
            && !stream.special_sprite.is_empty()
        {
            let bound_region =
                resources.active_atlas_catalog_region(&stream.special_sprite, data_root);
            let mut bound_composite = resources
                .active_bound_composite(&stream.special_sprite)
                .map(|owner| owner.snapshot());
            if bound_region.is_none() && bound_composite.is_none() {
                bound_composite = Some(Arc::new(Vec::new()));
            }
            commands.push(RenderCommand {
                projection_3d: None,
                order: 0,
                sprite: stream.special_sprite.as_str().into(),
                texture: None,
                bound_region,
                bound_composite,
                geometry: None,
                shader: None,
                dirt: None,
                x: x as f32 / game_world_scale,
                y: y as f32 / game_world_scale,
                state: trail_state.into(),
                world_space: false,
            });
        }
    }
    bridge.extend_render_commands(commands);
}
