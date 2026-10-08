//! `sub_100031434`'s ordinary localized bitmap-font submission branch.

use super::{UiTextTransform, raw_number, raw_string, raw_truthy};
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
    // 031Axx reads width, then queries leading before anchors/localization.
    // Both values are discarded, but a missing/retired IFont still fails here.
    let _width = raw_number(text, "width")?;
    {
        let resources = resources.lock().expect("resource runtime lock poisoned");
        let font = resources
            .current_native_font()?
            .ok_or_else(|| runtime_error("No font is set while trying to get font leading"))?;
        let _leading = font.metric(FontMetric::Leading)?;
    }
    let _horizontal_anchor = raw_string(text, "hanchor")?;
    let _vertical_anchor = raw_string(text, "vanchor")?;
    let numeric_pivots = native_lua51_number(&text.raw_get::<Value>("rotationPivotX")?).is_some()
        && native_lua51_number(&text.raw_get::<Value>("rotationPivotY")?).is_some();
    let (pivot_x, pivot_y) = if numeric_pivots {
        (
            raw_number(text, "rotationPivotX")?,
            raw_number(text, "rotationPivotY")?,
        )
    } else {
        (0.0_f32, 0.0_f32)
    };
    let floor_coordinates = raw_truthy(text, "floorCoordinates")?;
    let mut draw_x = transform.x / transform.scale_x;
    let mut draw_y = transform.y / transform.scale_y;
    if floor_coordinates {
        draw_x = draw_x.floor();
        draw_y = draw_y.floor();
    }
    let temporary_alpha = supplied_alpha.is_some_and(|alpha| alpha < 1.0);
    // sub_100031434 leaves these GL_Context fields installed after the call.
    // It replaces any affine basis with a plain angle rotation, while keeping
    // translation and clipping intact. These writes precede anchor mapping
    // and ResourceManager's second IFont/locale checks; errors do not undo them.
    {
        let mut bridge = render.lock().expect("render bridge lock poisoned");
        if temporary_alpha {
            bridge.state.alpha = f64::from(supplied_alpha.unwrap_or(1.0));
        }
        bridge.state.scale_x = f64::from(transform.scale_x);
        bridge.state.scale_y = f64::from(transform.scale_y);
        bridge.state.angle = f64::from(transform.angle);
        bridge.state.matrix = None;
        bridge.state.pivot_x = f64::from(pivot_x);
        bridge.state.pivot_y = f64::from(pivot_y);
    }
    let mut horizontal_anchor = "LEFT".to_owned();
    let mut vertical_anchor = "TOP".to_owned();
    for key in ["hanchor", "vanchor"] {
        let anchor = raw_string(text, key)?;
        match anchor.as_str() {
            "" => {}
            "LEFT" | "HCENTER" | "RIGHT" | "HPIVOT" => horizontal_anchor = anchor,
            "TOP" | "VCENTER" | "BOTTOM" | "BASELINE" | "VPIVOT" => vertical_anchor = anchor,
            _ => return Err(runtime_error(format!("Invalid anchor: {anchor}"))),
        }
    }
    let group = raw_string(text, "group")?;
    let key = raw_string(text, "text")?;
    let (selected_font, font_binding) = resources
        .lock()
        .expect("resource runtime lock poisoned")
        .current_text_font_binding(data_root)?
        .ok_or_else(|| runtime_error("No font is set while trying to draw string"))?;
    let content = resolve_localized_string(resources, locales, &group, &key)?;
    let mut bridge = render.lock().expect("render bridge lock poisoned");
    let text_state = bridge.state;
    let effective_alpha = text_state.alpha;
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
        })?;
    }
    if temporary_alpha {
        bridge.state.alpha = 1.0;
    }
    Ok(())
}
