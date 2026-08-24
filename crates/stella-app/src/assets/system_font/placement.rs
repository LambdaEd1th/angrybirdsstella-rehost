//! Native label anchor conversion and local-coordinate truncation.

use super::{SystemFontRenderBinding, TextRenderCommand};

pub(crate) fn native_system_label_offset(
    command: &TextRenderCommand,
    stroke_width: i32,
    horizontal_anchor: i32,
    vertical_anchor: i32,
) -> [f64; 2] {
    let Some([origin_x, origin_y]) = command.native_system_origin else {
        return [
            f64::from(-stroke_width - horizontal_anchor),
            f64::from(-stroke_width - vertical_anchor),
        ];
    };
    // Both LabelPool hit and miss branches at 0x100475E04/0x1004762A0 use
    // FCVTZS on the anchored float coordinates before Texture::draw receives
    // them. Subtract the original local origin because command.x/y already
    // contain that origin transformed through the captured GL_Context.
    // Preserve the two native FSUB roundings instead of collapsing the two
    // integer offsets before the floating-point operation.
    let anchored_x =
        native_fcvtzs_f32((origin_x - stroke_width as f32) - horizontal_anchor as f32) as f32;
    let anchored_y =
        native_fcvtzs_f32((origin_y - stroke_width as f32) - vertical_anchor as f32) as f32;
    [
        f64::from(anchored_x - origin_x),
        f64::from(anchored_y - origin_y),
    ]
}

pub(crate) fn native_system_label_vertical_anchor(
    binding: &SystemFontRenderBinding,
    vertical_anchor: &str,
) -> i32 {
    let font_height = binding.ascending.wrapping_add(binding.descending);
    match vertical_anchor {
        "VCENTER" => font_height / 2,
        "BOTTOM" => font_height,
        "BASELINE" => binding.ascending,
        _ => 0,
    }
}

fn native_fcvtzs_f32(value: f32) -> i32 {
    if !value.is_finite() || !(-2_147_483_648.0_f32..2_147_483_648.0_f32).contains(&value) {
        i32::MIN
    } else {
        value.trunc() as i32
    }
}

pub(crate) fn native_system_label_horizontal_anchor(
    binding: &SystemFontRenderBinding,
    text: &str,
    horizontal_anchor: &str,
) -> anyhow::Result<i32> {
    if !matches!(horizontal_anchor, "RIGHT" | "HCENTER") {
        return Ok(0);
    }
    let width = binding.native_string_width(text);
    Ok(if horizontal_anchor == "RIGHT" {
        width
    } else {
        width >> 1
    })
}
