//! ARM64 scalar conversions shared by generated and hand-written Lua members.

/// Match AArch64 `FCVTZS Wd, Sn`: truncate, saturate signed overflow, and
/// convert NaN to zero. Unlike x86 CVTTSS2SI, AArch64 does not return an
/// integer-indefinite i32::MIN for every invalid conversion. Rust's saturating
/// float-to-integer cast has the same value semantics on every host target.
pub(crate) fn native_fcvtzs_f32(value: f32) -> i32 {
    value as i32
}

/// Match AArch64 `FCVTZU Wd, Sn`: truncate finite in-range values, clamp
/// negative/NaN inputs to zero, and saturate positive overflow to `u32::MAX`.
pub(crate) fn native_fcvtzu_f32(value: f32) -> u32 {
    if value.is_nan() || value <= 0.0 {
        0
    } else if value >= 4_294_967_296.0_f32 {
        u32::MAX
    } else {
        value.trunc() as u32
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn signed_conversion_uses_arm64_saturation_not_x86_integer_indefinite() {
        for (input, expected) in [
            (f32::NAN, 0),
            (f32::INFINITY, i32::MAX),
            (f32::NEG_INFINITY, i32::MIN),
            (2_147_483_648.0, i32::MAX),
            (-2_147_483_648.0, i32::MIN),
            (-2_147_483_904.0, i32::MIN),
            (2_147_483_520.0, 2_147_483_520),
            (-1.9, -1),
            (1.9, 1),
        ] {
            assert_eq!(native_fcvtzs_f32(input), expected, "{input:?}");
        }
    }

    #[cfg(target_arch = "aarch64")]
    #[test]
    fn signed_conversion_matches_the_actual_recovered_arm64_instruction() {
        // Independent instruction oracle, not another restatement of the
        // Rust conversion. Includes all float classes and both signed limits.
        let mut bits = 0xA341_316Cu32;
        for fixed in [
            0,
            0x7F80_0000,
            0xFF80_0000,
            0x7FC0_0000,
            0xCF00_0000,
            0x4F00_0000,
        ] {
            for index in 0..1024 {
                bits = bits.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                let input = f32::from_bits(if index == 0 { fixed } else { bits });
                let native: i32;
                // SAFETY: register-only scalar conversion with no memory or
                // stack access. Supported by every AArch64 target of this test.
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
