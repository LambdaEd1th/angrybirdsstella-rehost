//! Projected bitmap text (`sub_10003457C`/`sub_100087BB4`).

use crate::*;

pub(super) fn install(
    lua: &Lua,
    globals: &mlua::Table,
    render: Arc<Mutex<RenderBridge>>,
    resources: Arc<Mutex<ResourceRuntime>>,
    locales: Arc<Mutex<LocaleRuntime>>,
    data_root: Arc<PathBuf>,
) -> LuaResult<()> {
    globals.set(
        "drawString3D",
        lua.create_function(move |_, args: MultiValue| {
            // sub_100087BB4 reads exact STRING slots 1/2 and exact
            // NUMBER slots 3..9; extras are not inspected.
            let group = native_required_string(&args, 0, "drawString3D")?;
            let key = native_required_string(&args, 1, "drawString3D")?;
            let x = native_required_number(&args, 2, "drawString3D")?;
            let y = native_required_number(&args, 3, "drawString3D")?;
            let z = native_required_number(&args, 4, "drawString3D")?;
            let rotation_x = native_required_number(&args, 5, "drawString3D")?;
            let scale_x = native_required_number(&args, 6, "drawString3D")?;
            let scale_y = native_required_number(&args, 7, "drawString3D")?;
            let alpha = native_required_number(&args, 8, "drawString3D")?;
            // All seven numeric slots cross sub_100087BB4 as float32.
            let [x, y, z, rotation_x, scale_x, scale_y, alpha] =
                [x, y, z, rotation_x, scale_x, scale_y, alpha].map(|value| f64::from(value as f32));
            // sub_10003457C forwards the first two strings unchanged to
            // ResourceManager::drawString. They are a TextGroupSet name
            // and localization key, not text plus font name.
            let text = resolve_localized_string(&resources, &locales, &group, &key)?;
            let (font, font_binding) = resources
                .lock()
                .expect("resource runtime lock poisoned")
                .current_text_font_binding(&data_root)
                .ok_or_else(|| runtime_error("No font is set while trying to draw string"))?;
            let mut bridge = render.lock().expect("render bridge lock poisoned");
            let clip_rect = bridge.state.clip_rect;
            bridge.push_text_command(TextRenderCommand {
                order: 0,
                text,
                font,
                font_binding: Some(font_binding),
                x,
                y,
                native_system_origin: Some([x as f32, y as f32]),
                scale_x,
                scale_y,
                // sub_10057BE78 rotates around (1,0,0), not the 2D Z axis.
                angle: 0.0,
                matrix: None,
                alpha,
                horizontal_anchor: "HCENTER".to_owned(),
                vertical_anchor: "VCENTER".to_owned(),
                projection_3d: Some(TextProjection3D { z, rotation_x }),
                clip_rect,
            });
            Ok(())
        })?,
    )
}
