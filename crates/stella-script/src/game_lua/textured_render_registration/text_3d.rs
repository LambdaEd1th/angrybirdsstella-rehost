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
            let text_x = native_required_number(&args, 6, "drawString3D")?;
            let text_y = native_required_number(&args, 7, "drawString3D")?;
            let alpha = native_required_number(&args, 8, "drawString3D")?;
            // All seven numeric slots cross sub_100087BB4 as float32.
            let [x, y, z, rotation_x, text_x, text_y, alpha] =
                [x, y, z, rotation_x, text_x, text_y, alpha].map(|value| value as f32);
            // These writes precede ResourceManager's font/locale checks. A
            // caught exception leaves perspective, model and alpha installed.
            {
                let mut bridge = render.lock().expect("render bridge lock poisoned");
                bridge.perspective_projection = true;
                bridge.state.custom_model = Some(TextProjection3D {
                    x,
                    y,
                    z,
                    rotation_x,
                    custom_model: true,
                });
                bridge.state.alpha = f64::from(alpha);
            }
            // sub_10003457C forwards the first two strings unchanged to
            // ResourceManager::drawString. They are a TextGroupSet name
            // and localization key, not text plus font name.
            // sub_10045C1FC tests ResourceManager+0x48 before resolving that
            // pair, so the no-current-font diagnostic wins over locale errors.
            let (font, font_binding) = resources
                .lock()
                .expect("resource runtime lock poisoned")
                .current_text_font_binding(&data_root)
                .ok_or_else(|| runtime_error("No font is set while trying to draw string"))?;
            let text = resolve_localized_string(&resources, &locales, &group, &key)?;
            let mut bridge = render.lock().expect("render bridge lock poisoned");
            let state = bridge.state;
            // s0/s1 passed at 0x1000346A8 are IFont draw coordinates, not
            // scale factors. Glyphs still use the preceding 2D context scale.
            let system_font = matches!(&font_binding, TextFontBinding::System(_));
            let origin = if system_font {
                native_text_state_origin(state, text_x, text_y)
            } else {
                // AtlasSprite's UV-array GL_Image overload +48 bypasses the
                // 2D state under custom model (0x10059E6EC..0x10059E718).
                [f64::from(text_x), f64::from(text_y)]
            };
            let position_matrix = system_font.then(|| native_system_text_position_matrix(state));
            bridge.push_text_command(TextRenderCommand {
                order: 0,
                text,
                font,
                font_binding: Some(font_binding),
                x: origin[0],
                y: origin[1],
                native_system_origin: Some([text_x, text_y]),
                scale_x: if system_font { state.scale_x } else { 1.0 },
                scale_y: if system_font { state.scale_y } else { 1.0 },
                angle: if system_font { state.angle } else { 0.0 },
                matrix: system_font.then(|| native_text_state_matrix(state)),
                position_matrix,
                alpha: f64::from(alpha),
                // `0x10003469C` passes a zeroed two-enum Anchor. The mapper
                // at sub_10040A248 identifies those zero values as LEFT/TOP.
                horizontal_anchor: "LEFT".to_owned(),
                vertical_anchor: "TOP".to_owned(),
                projection_3d: None,
                clip_rect: state.clip_rect,
            });
            // On the successful path `0x1000346B4..0x100034714` flushes the
            // glyphs, disables the custom model matrix, installs identity and
            // restores the ordinary projection. The separate 2D transform,
            // pivot and scissor are never overwritten by this member.
            bridge.state.custom_model = None;
            bridge.perspective_projection = false;
            Ok(())
        })?,
    )
}
