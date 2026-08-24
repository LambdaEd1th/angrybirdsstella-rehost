//! `sub_100031434`'s ordinary localized bitmap-font submission branch.

use super::UiTextTransform;
use crate::*;

pub(super) fn draw(
    text: &mlua::Table,
    render: &Arc<Mutex<RenderBridge>>,
    resources: &Arc<Mutex<ResourceRuntime>>,
    locales: &Arc<Mutex<LocaleRuntime>>,
    data_root: &Path,
    transform: UiTextTransform,
    supplied_alpha: Option<f64>,
) -> LuaResult<()> {
    // These fields are strict in the native binder. Width and the selected
    // font's leading are fetched even though their values do not participate
    // in the final draw call.
    let _width = table_required_number(text, "width", "drawUITextNative")?;
    let horizontal_anchor = table_required_string(text, "hanchor", "drawUITextNative")?;
    let vertical_anchor = table_required_string(text, "vanchor", "drawUITextNative")?;
    let (pivot_x, pivot_y) = match (
        text.get::<Value>("rotationPivotX")?,
        text.get::<Value>("rotationPivotY")?,
    ) {
        (Value::Integer(x), Value::Integer(y)) => (x as f64, y as f64),
        (Value::Integer(x), Value::Number(y)) => (x as f64, y),
        (Value::Number(x), Value::Integer(y)) => (x, y as f64),
        (Value::Number(x), Value::Number(y)) => (x, y),
        _ => (0.0, 0.0),
    };
    let floor_coordinates = matches!(text.get::<Value>("floorCoordinates")?, Value::Boolean(true));
    let mut draw_x = transform.x / transform.scale_x;
    let mut draw_y = transform.y / transform.scale_y;
    if floor_coordinates {
        draw_x = draw_x.floor();
        draw_y = draw_y.floor();
    }
    let group = table_required_string(text, "group", "drawUITextNative")?;
    let key = table_required_string(text, "text", "drawUITextNative")?;
    let content = resolve_localized_string(resources, locales, &group, &key)?;
    let (selected_font, font_binding) = resources
        .lock()
        .expect("resource runtime lock poisoned")
        .current_text_font_binding(data_root)
        .ok_or_else(|| runtime_error("No font is set while trying to get font leading"))?;

    let cosine = transform.angle.cos();
    let sine = transform.angle.sin();
    let mut bridge = render.lock().expect("render bridge lock poisoned");
    let prior_translation_x = bridge.state.translate_x;
    let prior_translation_y = bridge.state.translate_y;
    let prior_alpha = bridge.state.alpha;
    let temporary_alpha = supplied_alpha.is_some_and(|alpha| alpha < 1.0);
    if temporary_alpha {
        bridge.state.alpha = supplied_alpha.unwrap_or(1.0);
    }
    // sub_100031434 leaves these GL_Context fields installed after the call.
    // It replaces any affine basis with a plain angle rotation, while keeping
    // translation and clipping intact.
    bridge.state.scale_x = transform.scale_x;
    bridge.state.scale_y = transform.scale_y;
    bridge.state.angle = transform.angle;
    bridge.state.matrix = None;
    bridge.state.pivot_x = pivot_x;
    bridge.state.pivot_y = pivot_y;
    let effective_alpha = if temporary_alpha {
        supplied_alpha.unwrap_or(1.0)
    } else {
        prior_alpha
    };
    let pivot_correction_x = pivot_x - cosine * pivot_x + sine * pivot_y;
    let pivot_correction_y = pivot_y - sine * pivot_x - cosine * pivot_y;
    let snapped_x = draw_x * transform.scale_x;
    let snapped_y = draw_y * transform.scale_y;
    let clip_rect = bridge.state.clip_rect;
    if !content.is_empty() {
        bridge.push_text_command(TextRenderCommand {
            order: 0,
            text: content,
            font: selected_font,
            font_binding: Some(font_binding),
            x: prior_translation_x * transform.scale_x
                + snapped_x
                + transform.scale_x * pivot_correction_x,
            y: prior_translation_y * transform.scale_y
                + snapped_y
                + transform.scale_y * pivot_correction_y,
            native_system_origin: Some([draw_x as f32, draw_y as f32]),
            scale_x: transform.scale_x,
            scale_y: transform.scale_y,
            angle: transform.angle,
            matrix: Some([
                transform.scale_x * cosine,
                -transform.scale_x * sine,
                transform.scale_y * sine,
                transform.scale_y * cosine,
            ]),
            alpha: effective_alpha,
            horizontal_anchor,
            vertical_anchor,
            projection_3d: None,
            clip_rect,
        });
    }
    if temporary_alpha {
        bridge.state.alpha = 1.0;
    }
    Ok(())
}
