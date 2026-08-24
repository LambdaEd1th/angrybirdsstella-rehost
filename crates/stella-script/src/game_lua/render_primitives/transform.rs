use crate::*;

pub(crate) fn native_state_screen_point(
    state: RenderState,
    point_x: f64,
    point_y: f64,
) -> [f64; 2] {
    let translate_x = state.translate_x as f32;
    let translate_y = state.translate_y as f32;
    let scale_x = state.scale_x as f32;
    let scale_y = state.scale_y as f32;
    let point_x = point_x as f32;
    let point_y = point_y as f32;
    let base_x = translate_x * scale_x;
    let base_y = translate_y * scale_y;
    if let Some([m00, m01, m10, m11]) = state.matrix {
        return [
            f64::from(base_x + m00 as f32 * point_x + m01 as f32 * point_y),
            f64::from(base_y + m10 as f32 * point_x + m11 as f32 * point_y),
        ];
    }
    let angle = state.angle as f32;
    let (sine, cosine) = angle.sin_cos();
    let pivot_x = state.pivot_x as f32;
    let pivot_y = state.pivot_y as f32;
    let pivot_correction_x = pivot_x - cosine * pivot_x + sine * pivot_y;
    let pivot_correction_y = pivot_y - sine * pivot_x - cosine * pivot_y;
    [
        f64::from(
            base_x + scale_x * pivot_correction_x + scale_x * cosine * point_x
                - scale_x * sine * point_y,
        ),
        f64::from(
            base_y
                + scale_y * pivot_correction_y
                + scale_y * sine * point_x
                + scale_y * cosine * point_y,
        ),
    ]
}
