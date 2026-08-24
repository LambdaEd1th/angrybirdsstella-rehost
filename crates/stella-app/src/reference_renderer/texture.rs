use super::*;

pub(super) fn sample_bilinear_clamped(image: &RgbaImage, x: f64, y: f64) -> [u8; 4] {
    let right = image.width().saturating_sub(1);
    let bottom = image.height().saturating_sub(1);
    let x = x.clamp(0.0, f64::from(right));
    let y = y.clamp(0.0, f64::from(bottom));
    let x0 = x.floor() as u32;
    let y0 = y.floor() as u32;
    let x1 = x0.saturating_add(1).min(right);
    let y1 = y0.saturating_add(1).min(bottom);
    interpolate_rgba(
        image.get_pixel(x0, y0).0,
        image.get_pixel(x1, y0).0,
        image.get_pixel(x0, y1).0,
        image.get_pixel(x1, y1).0,
        x - f64::from(x0),
        y - f64::from(y0),
    )
}

pub(super) fn sample_bilinear_repeat(image: &RgbaImage, x: f64, y: f64) -> [u8; 4] {
    let width = image.width();
    let height = image.height();
    let x = x.rem_euclid(f64::from(width));
    let y = y.rem_euclid(f64::from(height));
    let x0 = x.floor() as u32;
    let y0 = y.floor() as u32;
    let x1 = (x0 + 1) % width;
    let y1 = (y0 + 1) % height;
    interpolate_rgba(
        image.get_pixel(x0, y0).0,
        image.get_pixel(x1, y0).0,
        image.get_pixel(x0, y1).0,
        image.get_pixel(x1, y1).0,
        x - f64::from(x0),
        y - f64::from(y0),
    )
}

fn interpolate_rgba(
    top_left: [u8; 4],
    top_right: [u8; 4],
    bottom_left: [u8; 4],
    bottom_right: [u8; 4],
    horizontal: f64,
    vertical: f64,
) -> [u8; 4] {
    std::array::from_fn(|channel| {
        let top = f64::from(top_left[channel]) * (1.0 - horizontal)
            + f64::from(top_right[channel]) * horizontal;
        let bottom = f64::from(bottom_left[channel]) * (1.0 - horizontal)
            + f64::from(bottom_right[channel]) * horizontal;
        (top * (1.0 - vertical) + bottom * vertical).round() as u8
    })
}
