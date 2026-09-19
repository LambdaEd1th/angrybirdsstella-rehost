//! Explicit masked-image quad (`sub_1000343CC`/`sub_100096344`).

use crate::*;

fn masked_uv_x(coordinate: i32, factor: f32) -> f64 {
    // sub_100096344 first constructs float32 NDC, converts that value to
    // double for the half-range map and factor multiply, then stores float32.
    let ndc = ((coordinate as f32) / 1024.0_f32).mul_add(2.0_f32, -1.0_f32);
    let normalized = f64::from(ndc).mul_add(0.5_f64, 0.5_f64);
    f64::from((f64::from(factor) * normalized) as f32)
}

fn masked_uv_y(coordinate: i32, factor: f32) -> f64 {
    let ndc = ((coordinate as f32) / 768.0_f32).mul_add(2.0_f32, -1.0_f32);
    let normalized = (-f64::from(ndc)).mul_add(0.5_f64, 0.5_f64);
    f64::from((f64::from(factor) * normalized) as f32)
}

pub(super) fn install(
    lua: &Lua,
    globals: &mlua::Table,
    render: Arc<Mutex<RenderBridge>>,
    resource_runtime: Arc<Mutex<ResourceRuntime>>,
    data_root: Arc<PathBuf>,
) -> LuaResult<()> {
    globals.set(
        "renderMaskedImageNative",
        lua.create_function(move |_, args: MultiValue| {
            // sub_100087FC0 requires an exact STRING followed by nine
            // exact NUMBER slots and ignores trailing values.
            let sprite = native_required_string(&args, 0, "renderMaskedImageNative")?;
            let x1 = native_required_number(&args, 1, "renderMaskedImageNative")?;
            let y1 = native_required_number(&args, 2, "renderMaskedImageNative")?;
            let x2 = native_required_number(&args, 3, "renderMaskedImageNative")?;
            let y2 = native_required_number(&args, 4, "renderMaskedImageNative")?;
            let x3 = native_required_number(&args, 5, "renderMaskedImageNative")?;
            let y3 = native_required_number(&args, 6, "renderMaskedImageNative")?;
            let x4 = native_required_number(&args, 7, "renderMaskedImageNative")?;
            let y4 = native_required_number(&args, 8, "renderMaskedImageNative")?;
            let factor = native_required_number(&args, 9, "renderMaskedImageNative")?;
            // sub_100087FC0 reads every number as float32, then converts
            // only the first eight with FCVTZS. The ninth remains a
            // float32 UV factor rather than alpha.
            let [x1, y1, x2, y2, x3, y3, x4, y4] =
                [x1, y1, x2, y2, x3, y3, x4, y4].map(|value| native_fcvtzs_f32(value as f32));
            let factor = factor as f32;
            let bound_region = resource_runtime
                .lock()
                .expect("resource runtime lock poisoned")
                .active_atlas_catalog_region(&sprite, &data_root);
            let uv1 = [masked_uv_x(x1, factor), masked_uv_y(y4, factor)];
            let uv2 = [masked_uv_x(x2, factor), masked_uv_y(y3, factor)];
            let uv3 = [masked_uv_x(x3, factor), masked_uv_y(y2, factor)];
            let uv4 = [masked_uv_x(x4, factor), masked_uv_y(y1, factor)];
            let mut bridge = render.lock().expect("render bridge lock poisoned");
            // sub_1000343CC resets translation, scale, rotation basis and
            // angle while preserving pivot, alpha and clipping.
            bridge.state.translate_x = 0.0;
            bridge.state.translate_y = 0.0;
            bridge.state.scale_x = 0.0;
            bridge.state.scale_y = 0.0;
            bridge.state.angle = 0.0;
            bridge.state.matrix = None;
            bridge.state.draw_size = None;
            let explicit_quad = RenderQuad {
                positions: [
                    [f64::from(x4), f64::from(y4)],
                    [f64::from(x3), f64::from(y3)],
                    [f64::from(x2), f64::from(y2)],
                    [f64::from(x1), f64::from(y1)],
                ],
                uv: [uv4, uv3, uv2, uv1],
            };
            let state = bridge.state;
            bridge.push_render_command(RenderCommand {
                projection_3d: None,
                order: 0,
                sprite: sprite.into(),
                texture: None,
                bound_region,
                bound_composite: None,
                geometry: Some(SpriteGeometrySubmission::ExplicitQuad(Arc::new(
                    explicit_quad,
                ))),
                shader: None,
                dirt: None,
                x: 0.0,
                y: 0.0,
                state: state.into(),
                world_space: true,
            });
            Ok(())
        })?,
    )
}
