//! Resource-backed `drawString` member at `sub_100448728`.

use crate::*;

pub(super) fn install(
    lua: &Lua,
    resource_api: &mlua::Table,
    render: Arc<Mutex<RenderBridge>>,
    resource_runtime: Arc<Mutex<ResourceRuntime>>,
    locale_runtime: Arc<Mutex<LocaleRuntime>>,
    data_root: Arc<PathBuf>,
) -> LuaResult<()> {
    resource_api.set(
        "drawString",
        lua.create_function(move |_, args: MultiValue| {
            let group = native_required_string(&args, 0, "drawString")?;
            let key = native_required_string(&args, 1, "drawString")?;
            let x = native_required_number(&args, 2, "drawString")? as f32;
            let y = native_required_number(&args, 3, "drawString")? as f32;
            let mut horizontal_anchor = "LEFT".to_owned();
            let mut vertical_anchor = "TOP".to_owned();
            for index in 4..args.len().min(6) {
                let anchor = native_required_string(&args, index, "drawString")?;
                match anchor.as_str() {
                    "" => {}
                    "LEFT" | "HCENTER" | "RIGHT" | "HPIVOT" => horizontal_anchor = anchor,
                    "TOP" | "VCENTER" | "BOTTOM" | "BASELINE" | "VPIVOT" => {
                        vertical_anchor = anchor;
                    }
                    _ => return Err(runtime_error(format!("Invalid anchor: {anchor}"))),
                }
            }
            let (font, font_binding) = resource_runtime
                .lock()
                .expect("resource runtime lock poisoned")
                .current_text_font_binding(&data_root)
                .ok_or_else(|| runtime_error("No font is set while trying to draw string"))?;
            let content =
                resolve_localized_string(&resource_runtime, &locale_runtime, &group, &key)?;
            let mut bridge = render.lock().expect("render bridge lock poisoned");
            let state = bridge.state;
            let origin = native_text_state_origin(state, x, y);
            let matrix = Some(native_text_state_matrix(state));
            let position_matrix = matches!(&font_binding, TextFontBinding::System(_))
                .then(|| native_system_text_position_matrix(state));
            bridge.push_text_command(TextRenderCommand {
                order: 0,
                text: content,
                font,
                font_binding: Some(font_binding),
                x: origin[0],
                y: origin[1],
                native_system_origin: Some([x, y]),
                scale_x: state.scale_x,
                scale_y: state.scale_y,
                angle: state.angle,
                matrix,
                position_matrix,
                alpha: state.alpha,
                horizontal_anchor,
                vertical_anchor,
                projection_3d: None,
                clip_rect: state.clip_rect,
            });
            Ok(())
        })?,
    )
}
