use crate::*;

pub(crate) fn native_color_alpha(alpha: f64) -> f64 {
    if alpha > 1.0 { alpha / 255.0 } else { alpha }.clamp(0.0, 1.0)
}

pub(crate) fn native_packed_color_channel(value: f64) -> f64 {
    let converted = native_fcvtzs_f32(value as f32);
    f64::from((converted as u32 & 0xff) as u8) / 255.0
}
