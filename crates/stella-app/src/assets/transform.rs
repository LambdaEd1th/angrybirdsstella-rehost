use super::*;

pub(crate) fn project_text_3d(
    resolution: GameResolution,
    translate_x: f64,
    translate_y: f64,
    projection: TextProjection3D,
    local_x: f64,
    local_y: f64,
) -> Option<[f64; 2]> {
    // sub_10057BAE0 creates Purple's row-major perspective matrix from
    // (-1.5, 0.001, 2000, -1.33). sub_10057BE78 then installs the model's
    // X-axis rotation before the bitmap-font vertices are submitted.
    let rotation_x = projection.rotation_x as f32;
    let (sine, cosine) = rotation_x.sin_cos();
    let local_x = local_x as f32;
    let local_y = local_y as f32;
    let world_x = translate_x as f32 + local_x;
    let world_y = cosine.mul_add(local_y, translate_y as f32);
    let world_z = sine.mul_add(local_y, projection.z as f32);
    if !world_z.is_finite() || world_z <= 0.001_f32 {
        return None;
    }
    let focal = (1.0_f32 / (-1.5_f32 * 0.5_f32).tan()).abs();
    // sub_10057BAE0 preserves this nominally cancelling near-plane scale as
    // FADD, FMUL, FMUL, FDIV. Keep the rounded matrix element, rather than
    // algebraically reducing it to `focal * -1.33`.
    let doubled_near = 0.001_f32 + 0.001_f32;
    let vertical_scale = (doubled_near * (focal * -1.33_f32)) / doubled_near;
    let ndc_x = focal * world_x / world_z;
    let ndc_y = vertical_scale * world_y / world_z;
    let screen = [
        f64::from((ndc_x + 1.0_f32) * (resolution.width as f32 * 0.5_f32)),
        f64::from((1.0_f32 - ndc_y) * (resolution.height as f32 * 0.5_f32)),
    ];
    screen.into_iter().all(f64::is_finite).then_some(screen)
}

impl SpriteTransform {
    pub(crate) fn from_scale_rotation(
        x: f32,
        y: f32,
        scale_x: f32,
        scale_y: f32,
        angle: f32,
        alpha: f32,
    ) -> Self {
        let (sine, cosine) = angle.sin_cos();
        Self {
            x,
            y,
            m00: cosine * scale_x,
            m01: -sine * scale_y,
            m10: sine * scale_x,
            m11: cosine * scale_y,
            alpha,
        }
    }

    pub(crate) fn transform_point(self, x: f32, y: f32) -> [f32; 2] {
        // Sprite's four-vertex path at sub_100467BE8 rounds the second term
        // with FMUL, uses it as the addend of an FMADD for the first term,
        // then adds translation with a separate FADD. This is the same mixed
        // ordering used by sub_10001E440's matrix composition.
        let component = |translation: f32,
                         fused_left: f32,
                         fused_right: f32,
                         addend_left: f32,
                         addend_right: f32| {
            translation + fused_left.mul_add(fused_right, addend_left * addend_right)
        };
        [
            component(self.x, self.m00, x, self.m01, y),
            component(self.y, self.m10, x, self.m11, y),
        ]
    }
}

pub(crate) fn text_glyph_transform(
    command: &TextRenderCommand,
    cursor: f64,
    anchor_y: f64,
) -> SpriteTransform {
    if let Some([m00, m01, m10, m11]) = command.matrix {
        let [m00, m01, m10, m11] = [m00, m01, m10, m11].map(|value| value as f32);
        let cursor = cursor as f32;
        let anchor_y = anchor_y as f32;
        return SpriteTransform {
            x: m00.mul_add(cursor, m01 * anchor_y) + command.x as f32,
            y: m10.mul_add(cursor, m11 * anchor_y) + command.y as f32,
            m00,
            m01,
            m10,
            m11,
            alpha: command.alpha as f32,
        };
    }
    let scale_x = command.scale_x as f32;
    let scale_y = command.scale_y as f32;
    let angle = command.angle as f32;
    let (sine, cosine) = angle.sin_cos();
    let local_x = cursor as f32 * scale_x;
    let local_y = anchor_y as f32 * scale_y;
    SpriteTransform::from_scale_rotation(
        cosine.mul_add(local_x, -sine * local_y) + command.x as f32,
        sine.mul_add(local_x, cosine * local_y) + command.y as f32,
        scale_x,
        scale_y,
        angle,
        command.alpha as f32,
    )
}

pub(crate) fn render_command_transform(command: &RenderCommand) -> SpriteTransform {
    let state = command.state;
    let alpha = state.alpha.clamp(0.0, 1.0);
    let base_x = if command.world_space {
        state.translate_x + command.x
    } else {
        (state.translate_x + command.x) * state.scale_x
    };
    let base_y = if command.world_space {
        state.translate_y + command.y
    } else {
        (state.translate_y + command.y) * state.scale_y
    };
    if let Some([m00, m01, m10, m11]) = state.matrix {
        return SpriteTransform {
            x: base_x,
            y: base_y,
            m00,
            m01,
            m10,
            m11,
            alpha,
        };
    }

    if command.world_space {
        // Host-generated scene, particle and utility commands already carry a
        // screen-space origin. Their local matrix is deliberately R * Scale;
        // exact native paths that need another order provide `state.matrix`.
        return SpriteTransform::from_scale_rotation(
            base_x,
            base_y,
            state.scale_x,
            state.scale_y,
            state.angle,
            alpha,
        );
    }

    // gr::gles2::GL_Context (sub_100598CC4) rotates around the state pivot in
    // its unscaled coordinate space, then applies the independent X/Y state
    // scales while projecting. This is Scale * Rotation, not Rotation *
    // Scale. AtlasSprite has already converted its own pivot into the draw
    // offset, so this correction applies only the renderer-state pivot.
    let scale_x = state.scale_x;
    let scale_y = state.scale_y;
    let pivot_x = state.pivot_x;
    let pivot_y = state.pivot_y;
    let (sine, cosine) = state.angle.sin_cos();
    let pivot_correction_x = (-cosine).mul_add(pivot_x, sine.mul_add(pivot_y, pivot_x));
    let pivot_correction_y = (-sine).mul_add(pivot_x, (-cosine).mul_add(pivot_y, pivot_y));
    SpriteTransform {
        x: scale_x.mul_add(pivot_correction_x, base_x),
        y: scale_y.mul_add(pivot_correction_y, base_y),
        m00: scale_x * cosine,
        m01: -scale_x * sine,
        m10: scale_y * sine,
        m11: scale_y * cosine,
        alpha,
    }
}

pub(crate) fn composite_child_transform(
    parent: SpriteTransform,
    part: &CompositePart,
) -> SpriteTransform {
    // Entry stores scale, flip multipliers and angle (already radians) as
    // separate float32 fields. sub_1004376D4 then multiplies the complete
    // parent and child matrices, retaining shear from non-uniform nesting.
    let scale_x = part.scale_x * part.flip_x;
    let scale_y = part.scale_y * part.flip_y;
    let (sine, cosine) = part.angle.sin_cos();
    let child_m00 = cosine * scale_x;
    let child_m01 = -sine * scale_y;
    let child_m10 = sine * scale_x;
    let child_m11 = cosine * scale_y;
    let offset_x = part.x;
    let offset_y = part.y;
    let [x, y] = parent.transform_point(offset_x, offset_y);
    SpriteTransform {
        x,
        y,
        m00: parent.m00.mul_add(child_m00, parent.m01 * child_m10),
        m01: parent.m00.mul_add(child_m01, parent.m01 * child_m11),
        m10: parent.m10.mul_add(child_m00, parent.m11 * child_m10),
        m11: parent.m10.mul_add(child_m01, parent.m11 * child_m11),
        alpha: parent.alpha,
    }
}

#[cfg(test)]
mod native_affine_tests {
    use super::*;

    #[test]
    fn sprite_vertices_keep_native_fmul_fmadd_then_fadd_order() {
        let transform = SpriteTransform {
            x: 511.123_44,
            y: -383.987_64,
            m00: 12_345.679,
            m01: -0.000_321_987,
            m10: std::f32::consts::PI,
            m11: 98_765.43,
            alpha: 1.0,
        };
        let x = 582.18_f32;
        let y = 254.05_f32;
        let expected_x = transform.x + transform.m00.mul_add(x, transform.m01 * y);
        let expected_y = transform.y + transform.m10.mul_add(x, transform.m11 * y);
        assert_eq!(transform.transform_point(x, y), [expected_x, expected_y]);
    }

    #[test]
    fn sprite_vertices_do_not_round_both_products_before_adding() {
        let one_plus_ulp = f32::from_bits(0x3f80_0001);
        let transform = SpriteTransform {
            x: 0.0,
            y: 0.0,
            m00: one_plus_ulp,
            m01: -f32::from_bits(0x3f80_0002),
            m10: 0.0,
            m11: 1.0,
            alpha: 1.0,
        };

        let point = transform.transform_point(one_plus_ulp, 1.0);
        assert_eq!(point[0].to_bits(), 0x2880_0000); // 2^-46
        assert_eq!(one_plus_ulp * one_plus_ulp + transform.m01, 0.0);
    }
}
