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
    supplied_alpha: Option<f32>,
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
        (Value::Integer(x), Value::Integer(y)) => (x as f32, y as f32),
        (Value::Integer(x), Value::Number(y)) => (x as f32, y as f32),
        (Value::Number(x), Value::Integer(y)) => (x as f32, y as f32),
        (Value::Number(x), Value::Number(y)) => (x as f32, y as f32),
        _ => (0.0_f32, 0.0_f32),
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

    let mut bridge = render.lock().expect("render bridge lock poisoned");
    let prior_alpha = bridge.state.alpha;
    let temporary_alpha = supplied_alpha.is_some_and(|alpha| alpha < 1.0);
    if temporary_alpha {
        bridge.state.alpha = f64::from(supplied_alpha.unwrap_or(1.0));
    }
    // sub_100031434 leaves these GL_Context fields installed after the call.
    // It replaces any affine basis with a plain angle rotation, while keeping
    // translation and clipping intact.
    bridge.state.scale_x = f64::from(transform.scale_x);
    bridge.state.scale_y = f64::from(transform.scale_y);
    bridge.state.angle = f64::from(transform.angle);
    bridge.state.matrix = None;
    bridge.state.pivot_x = f64::from(pivot_x);
    bridge.state.pivot_y = f64::from(pivot_y);
    let text_state = bridge.state;
    let effective_alpha = if temporary_alpha {
        f64::from(supplied_alpha.unwrap_or(1.0))
    } else {
        prior_alpha
    };
    let [origin_x, origin_y] = native_text_state_origin_with_rotation(
        text_state,
        draw_x,
        draw_y,
        transform.sine,
        transform.cosine,
    );
    let matrix =
        native_text_state_matrix_with_rotation(text_state, transform.sine, transform.cosine);
    let position_matrix = matches!(&font_binding, TextFontBinding::System(_))
        .then(|| native_system_text_position_matrix(text_state));
    let clip_rect = bridge.state.clip_rect;
    if !content.is_empty() {
        bridge.push_text_command(TextRenderCommand {
            order: 0,
            text: content,
            font: selected_font,
            font_binding: Some(font_binding),
            x: origin_x,
            y: origin_y,
            native_system_origin: Some([draw_x, draw_y]),
            scale_x: f64::from(transform.scale_x),
            scale_y: f64::from(transform.scale_y),
            angle: f64::from(transform.angle),
            matrix: Some(matrix),
            position_matrix,
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
