//! Selected texturized-object member (`sub_100043990`) and strict adapter.

use crate::*;

pub(super) fn install(
    lua: &Lua,
    globals: &mlua::Table,
    render: Arc<Mutex<RenderBridge>>,
    resource_runtime: Arc<Mutex<ResourceRuntime>>,
    data_root: Arc<PathBuf>,
) -> LuaResult<()> {
    globals.set(
        "drawSelectedTexturizedObject",
        lua.create_function(move |_, args: MultiValue| {
            // sub_100085634 requires two exact STRING slots followed by
            // four exact NUMBER slots and ignores trailing values.
            let sprite = native_required_string(&args, 0, "drawSelectedTexturizedObject")?;
            let texture = native_required_string(&args, 1, "drawSelectedTexturizedObject")?;
            let x = native_required_number(&args, 2, "drawSelectedTexturizedObject")?;
            let y = native_required_number(&args, 3, "drawSelectedTexturizedObject")?;
            let scale_x = native_required_number(&args, 4, "drawSelectedTexturizedObject")?;
            let scale_y = native_required_number(&args, 5, "drawSelectedTexturizedObject")?;
            // sub_100085634 crosses the generated float32 ABI before the
            // member performs any multiply or divide.
            let [x, y, scale_x, scale_y] = [x, y, scale_x, scale_y].map(|value| value as f32);
            let (bound_region, masked_texture_binding) = {
                let resources = resource_runtime
                    .lock()
                    .expect("resource runtime lock poisoned");
                let binding = resources
                    .active_masked_texture_source(&texture, &data_root)
                    .map_or(MaskedTextureBinding::Missing, MaskedTextureBinding::Source);
                (
                    resources.active_atlas_catalog_region(&sprite, &data_root),
                    binding,
                )
            };
            let mut bridge = render.lock().expect("render bridge lock poisoned");
            let world_scale = bridge.world_scale as f32;
            let top_left_x = bridge.top_left_x as f32;
            let top_left_y = bridge.top_left_y as f32;
            // sub_100043990 writes these scalar fields into the live
            // GL_Context while preserving angle, pivot and alpha.
            bridge.state.translate_x = f64::from(-top_left_x / scale_x);
            bridge.state.translate_y = f64::from(-top_left_y / scale_y);
            bridge.state.scale_x = f64::from(world_scale * scale_x);
            bridge.state.scale_y = f64::from(world_scale * scale_y);
            bridge.state.matrix = None;
            let atlas_pivot = bound_region.as_ref().map_or([0.0_f32; 2], |region| {
                [
                    f32::from(region.sprite.pivot_x),
                    f32::from(region.sprite.pivot_y),
                ]
            });
            // sub_100043990 preserves the current rotation/pivot while
            // replacing translation and scale. sub_10008D428 subsequently
            // computes fill coordinates from the unprojected position and
            // Scale*Rotation basis; the atlas pivot is already represented by
            // the region-local vertices used by the deferred renderer.
            bridge.state.masked_texture_matrix = Some(RenderState::native_masked_texture_matrix(
                x * 20.0_f32,
                y * 20.0_f32,
                scale_x,
                scale_y,
                bridge.state.angle as f32,
                bridge.state.pivot_x as f32 - atlas_pivot[0],
                bridge.state.pivot_y as f32 - atlas_pivot[1],
            ));
            let state = bridge.state;
            bridge.push_render_command(RenderCommand {
                order: 0,
                sprite,
                texture: Some(texture),
                texture_scale: 1.0,
                masked_texture_binding: Some(masked_texture_binding),
                bound_region,
                bound_composite: None,
                shader: None,
                clip_holes: Vec::new(),
                dirt: None,
                x: f64::from((x * 20.0_f32) / scale_x),
                y: f64::from((y * 20.0_f32) / scale_y),
                state,
                world_space: false,
            });
            Ok(())
        })?,
    )
}
