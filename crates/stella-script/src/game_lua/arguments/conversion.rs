//! ARM64 scalar conversions shared by generated and hand-written Lua members.

/// Match AArch64 `FCVTZS Wd, Sn`, including its signed integer-indefinite
/// result for NaN and values outside the i32 destination range.
pub(crate) fn native_fcvtzs_f32(value: f32) -> i32 {
    if !value.is_finite() || !(-2_147_483_648.0_f32..2_147_483_648.0_f32).contains(&value) {
        i32::MIN
    } else {
        value.trunc() as i32
    }
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
