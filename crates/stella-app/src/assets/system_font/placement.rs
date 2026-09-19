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
    // The hit (100475DF0/DF4) and miss (100476294/298) paths use FCVTZS
    // W,S directly: signed saturation and NaN -> 0, not x86 integer-indefinite.
    value as i32
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn system_label_placement_conversion_uses_arm64_signed_saturation() {
        for (input, expected) in [
            (f32::NAN, 0),
            (f32::INFINITY, i32::MAX),
            (f32::NEG_INFINITY, i32::MIN),
            (2_147_483_648.0, i32::MAX),
            (-2_147_483_904.0, i32::MIN),
            (2_147_483_520.0, 2_147_483_520),
            (1.9, 1),
            (-1.9, -1),
        ] {
            assert_eq!(native_fcvtzs_f32(input), expected, "{input:?}");
        }
    }

    #[cfg(target_arch = "aarch64")]
    #[test]
    fn system_label_placement_conversion_matches_actual_arm64_instruction() {
        let mut bits = 0xA893_32C1u32;
        for fixed in [
            0,
            0x8000_0000,
            0x7F80_0000,
            0xFF80_0000,
            0x7FC0_0001,
            0xFFC0_0001,
            0x7F80_0001,
            0x4F00_0000,
            0xCF00_0000,
        ] {
            for index in 0..512 {
                bits = bits.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                let input = f32::from_bits(if index == 0 { fixed } else { bits });
                let native: i32;
                // SAFETY: a register-only scalar instruction with no memory,
                // stack or privileged access, supported on all AArch64 CPUs.
                unsafe {
                    std::arch::asm!(
                        "fcvtzs {result:w}, {value:s}",
                        result = out(reg) native,
                        value = in(vreg) input,
                        options(nomem, nostack),
                    );
                }
                assert_eq!(
                    native_fcvtzs_f32(input),
                    native,
                    "bits={:08x}",
                    input.to_bits()
                );
            }
        }
    }
}
