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

/// Screen-space base point used by both native IFont implementations.
///
/// `GL_Image::draw` (`sub_10059E254`) does not rotate its x/y arguments.
/// It adds them to the live translation, applies only the context-pivot
/// correction, and finally applies the independent axis scales. BitmapFont
/// arranges its temporary pivot so glyph cursor offsets are rotated later;
/// SystemFont passes its already-anchored x/y straight into this same image
/// path. This is therefore deliberately different from
/// [`native_state_screen_point`], which transforms a geometry point.
pub(crate) fn native_text_state_origin(state: RenderState, point_x: f32, point_y: f32) -> [f64; 2] {
    let translate_x = state.translate_x as f32;
    let translate_y = state.translate_y as f32;
    let scale_x = state.scale_x as f32;
    let scale_y = state.scale_y as f32;
    let pivot_x = state.pivot_x as f32;
    let pivot_y = state.pivot_y as f32;
    let angle = state.angle as f32;

    if state.matrix.is_none() && angle == 0.0 {
        return [
            f64::from((translate_x + point_x) * scale_x),
            f64::from((translate_y + point_y) * scale_y),
        ];
    }

    if let Some([m00, m01, m10, m11]) = state.matrix {
        // Host-authored affine snapshots already include axis scale. Express
        // `(T + point + pivot) * Scale - M * pivot` without attempting to
        // decompose a possibly sheared matrix back into an angle.
        let [m00, m01, m10, m11] = [m00, m01, m10, m11].map(|value| value as f32);
        let translated_x = (pivot_x + point_x) + translate_x;
        let translated_y = (pivot_y + point_y) + translate_y;
        return [
            f64::from((-m00).mul_add(pivot_x, translated_x * scale_x) + (-m01) * pivot_y),
            f64::from((-m10).mul_add(pivot_x, translated_y * scale_y) + (-m11) * pivot_y),
        ];
    }

    let (sine, cosine) = angle.sin_cos();
    native_text_state_origin_with_rotation(state, point_x, point_y, sine, cosine)
}

/// Variant used by `drawUITextNative`, which has already called `cosf` and
/// `sinf` and stores those exact results into the GL context.
pub(crate) fn native_text_state_origin_with_rotation(
    state: RenderState,
    point_x: f32,
    point_y: f32,
    sine: f32,
    cosine: f32,
) -> [f64; 2] {
    let translate_x = state.translate_x as f32;
    let translate_y = state.translate_y as f32;
    let scale_x = state.scale_x as f32;
    let scale_y = state.scale_y as f32;
    let pivot_x = state.pivot_x as f32;
    let pivot_y = state.pivot_y as f32;
    if (state.angle as f32) == 0.0 {
        return [
            f64::from((translate_x + point_x) * scale_x),
            f64::from((translate_y + point_y) * scale_y),
        ];
    }
    // 0x10059E300..0x10059E3A8: each axis first adds pivot and input,
    // then translation; the first pivot product is fused with that sum, the
    // cross term is multiplied separately, and axis scale is applied last.
    let translated_x = translate_x + (pivot_x + point_x);
    let translated_y = translate_y + (pivot_y + point_y);
    let unscaled_x = (-cosine).mul_add(pivot_x, translated_x) + (-sine) * -pivot_y;
    let unscaled_y = (-sine).mul_add(pivot_x, translated_y) + cosine * -pivot_y;
    [
        f64::from(unscaled_x * scale_x),
        f64::from(unscaled_y * scale_y),
    ]
}

/// Float32 Scale * Rotation basis installed in the GL context for text quads.
pub(crate) fn native_text_state_matrix(state: RenderState) -> [f64; 4] {
    if let Some(matrix) = state.matrix {
        return matrix.map(|value| f64::from(value as f32));
    }
    let (sine, cosine) = (state.angle as f32).sin_cos();
    native_text_state_matrix_with_rotation(state, sine, cosine)
}

/// Matrix companion to [`native_text_state_origin_with_rotation`].
pub(crate) fn native_text_state_matrix_with_rotation(
    state: RenderState,
    sine: f32,
    cosine: f32,
) -> [f64; 4] {
    let scale_x = state.scale_x as f32;
    let scale_y = state.scale_y as f32;
    [
        f64::from(scale_x * cosine),
        f64::from(-scale_x * sine),
        f64::from(scale_y * sine),
        f64::from(scale_y * cosine),
    ]
}

/// SystemFont anchors x/y before drawing its cached label. Those offsets are
/// axis-scaled but do not inherit the label quad's rotation.
pub(crate) fn native_system_text_position_matrix(state: RenderState) -> [f64; 4] {
    [
        f64::from(state.scale_x as f32),
        0.0,
        0.0,
        f64::from(state.scale_y as f32),
    ]
}
