//! Native full 2D affine composition used by entity queries and rendering.

use super::{
    super::model::*, hierarchy::animation_definition_contains_entity,
    hierarchy::animation_local_transform,
};

impl AnimationAffine {
    pub(crate) fn from_transform(transform: AnimationTransform) -> Self {
        // Purple stores entity matrices as six float32 values. The ordinary
        // rotation target at sub_10041F4B0 calls __sincosf_stret and performs
        // the following scale products in float32 before the matrix is ever
        // composed with its parent.
        let angle = transform.angle as f32;
        let (sine, cosine) = angle.sin_cos();
        let scale_x = transform.scale_x as f32;
        let scale_y = transform.scale_y as f32;
        Self {
            m00: f64::from(cosine * scale_x),
            m01: f64::from(-sine * scale_y),
            m10: f64::from(sine * scale_x),
            m11: f64::from(cosine * scale_y),
            x: f64::from(transform.x as f32),
            y: f64::from(transform.y as f32),
        }
    }

    pub(crate) fn set_translation(&mut self, x: f64, y: f64) {
        self.x = f64::from(x as f32);
        self.y = f64::from(y as f32);
    }

    pub(crate) fn set_rotation(&mut self, angle: f64) {
        // sub_1000147B8 overwrites only the two linear basis columns with the
        // raw __sincosf_stret result, thereby discarding the previous scale.
        let (sine, cosine) = (angle as f32).sin_cos();
        self.m00 = f64::from(cosine);
        self.m01 = f64::from(-sine);
        self.m10 = f64::from(sine);
        self.m11 = f64::from(cosine);
    }

    pub(crate) fn set_scale(&mut self, scale_x: f64, scale_y: f64) {
        // sub_100014978 normalizes each existing basis through
        // sub_10057B644, then applies the requested scale with FMUL.
        let normalize = |x: f64, y: f64| {
            let x = x as f32;
            let y = y as f32;
            let length = x.mul_add(x, y * y).sqrt();
            let inverse = if length >= f32::MIN_POSITIVE {
                1.0_f32 / length
            } else {
                0.0
            };
            (inverse * x, inverse * y)
        };
        let (column_x_x, column_x_y) = normalize(self.m00, self.m10);
        let (column_y_x, column_y_y) = normalize(self.m01, self.m11);
        let scale_x = scale_x as f32;
        let scale_y = scale_y as f32;
        self.m00 = f64::from(column_x_x * scale_x);
        self.m10 = f64::from(column_x_y * scale_x);
        self.m01 = f64::from(column_y_x * scale_y);
        self.m11 = f64::from(column_y_y * scale_y);
    }

    /// Compose the transform installed by `SpriteComponentCustom` when it
    /// resolves an AnimationSkins attachment.
    ///
    /// This is intentionally not `from_transform`: Purple's ordinary
    /// animation rotation target (`sub_10041F4B0`) builds `R(angle)`, while
    /// the skin callback (`sub_100011BC4`) builds `R(-attachment.rotation)`.
    /// The latter also stores and normalizes both float32 basis vectors before
    /// applying the attachment's non-uniform scale.
    pub(crate) fn then_skin_attachment(self, attachment: &AnimationSkinTransform) -> Self {
        let angle = attachment.angle as f32;
        let sine = (-angle).sin();
        let cosine = angle.cos();
        let normalize = |x: f32, y: f32| {
            let inverse_length = 1.0_f32 / x.mul_add(x, y * y).sqrt();
            (x * inverse_length, y * inverse_length)
        };
        let (column_x_x, column_x_y) = normalize(cosine, sine);
        let (column_y_x, column_y_y) = normalize(-sine, cosine);
        let scale_x = attachment.scale_x as f32;
        let scale_y = attachment.scale_y as f32;
        self.compose(Self {
            m00: f64::from(scale_x * column_x_x),
            m01: f64::from(scale_y * column_y_x),
            m10: f64::from(scale_x * column_x_y),
            m11: f64::from(scale_y * column_y_y),
            x: f64::from(attachment.x as f32),
            y: f64::from(attachment.y as f32),
        })
    }

    pub(crate) fn compose(self, local: Self) -> Self {
        // sub_10001E440 first rounds the second product with FMUL, then feeds
        // it to FMADD as the addend of the first product. Translation is a
        // separate final FADD. Preserve that mixed ARM64 ordering exactly;
        // neither an all-fused f64 expression nor two rounded products match
        // the native matrix used by the large LEAVES sprites.
        let dot = |left_a: f64, right_a: f64, left_b: f64, right_b: f64| {
            let rounded_addend = (left_b as f32) * (right_b as f32);
            f64::from((left_a as f32).mul_add(right_a as f32, rounded_addend))
        };
        let translate = |parent: f64, linear: f64| f64::from((parent as f32) + (linear as f32));
        Self {
            m00: dot(self.m00, local.m00, self.m01, local.m10),
            m01: dot(self.m00, local.m01, self.m01, local.m11),
            m10: dot(self.m10, local.m00, self.m11, local.m10),
            m11: dot(self.m10, local.m01, self.m11, local.m11),
            x: translate(self.x, dot(self.m00, local.x, self.m01, local.y)),
            y: translate(self.y, dot(self.m10, local.x, self.m11, local.y)),
        }
    }

    fn determinant(self) -> f32 {
        // Transform::GetWorldMatrix calls sub_10057B734. Hopper shows the
        // off-diagonal product rounded by FMUL before the diagonal product is
        // fused with its negation.
        let m00 = self.m00 as f32;
        let m01 = self.m01 as f32;
        let m10 = self.m10 as f32;
        let m11 = self.m11 as f32;
        m00.mul_add(m11, -(m01 * m10))
    }

    /// Apply Transform::GetWorldMatrix's mirrored-parent correction from
    /// `sub_10043C758` to one local matrix.
    ///
    /// Purple removes the two positive basis magnitudes, post-multiplies the
    /// normalized local basis by a +/- twice-angle rotation, and restores the
    /// magnitudes through the generic matrix inverse. Algebraically this
    /// reverses the local rotation while preserving translation and signed
    /// scale, but retaining the original sequence is required for float32
    /// rounding and degenerate matrices.
    fn compensate_parent_reflection(self) -> Self {
        let m00 = self.m00 as f32;
        let m01 = self.m01 as f32;
        let m10 = self.m10 as f32;
        let m11 = self.m11 as f32;
        let determinant = self.determinant();
        let angle = m10.atan2(m00);
        let correction_angle = if determinant < 0.0_f32 {
            angle + angle
        } else {
            angle * -2.0_f32
        };
        let (sine, cosine) = correction_angle.sin_cos();
        let correction = Self {
            m00: f64::from(cosine),
            m01: f64::from(-sine),
            m10: f64::from(sine),
            m11: f64::from(cosine),
            x: 0.0,
            y: 0.0,
        };

        let length_x = m00.mul_add(m00, m10 * m10).sqrt();
        let length_y = m01.mul_add(m01, m11 * m11).sqrt();
        let inverse_scale = Self {
            m00: f64::from(1.0_f32 / length_x),
            m01: 0.0,
            m10: 0.0,
            m11: f64::from(1.0_f32 / length_y),
            x: 0.0,
            y: 0.0,
        };
        self.compose(inverse_scale)
            .compose(correction)
            .compose(inverse_scale.inverse())
    }

    /// Invert the ordinary 2D matrix with the instruction order in
    /// `sub_10000F46C`/`sub_1000152BC`: a rounded cross product feeds the
    /// determinant FMADD, and each translated component uses one rounded
    /// product followed by a fused subtract. Singular matrices deliberately
    /// retain IEEE infinities/NaNs like the native FDIV path.
    pub(crate) fn inverse(self) -> Self {
        let m00 = self.m00 as f32;
        let m01 = self.m01 as f32;
        let m10 = self.m10 as f32;
        let m11 = self.m11 as f32;
        let x = self.x as f32;
        let y = self.y as f32;
        let determinant = m00.mul_add(m11, -(m01 * m10));
        let inverse_determinant = 1.0_f32 / determinant;
        let inverse_m00 = m11 * inverse_determinant;
        let positive_m01 = m01 * inverse_determinant;
        let positive_m10 = m10 * inverse_determinant;
        let inverse_m11 = m00 * inverse_determinant;
        let rounded_x = inverse_m00 * x;
        let rounded_y = inverse_m11 * y;
        Self {
            m00: f64::from(inverse_m00),
            m01: f64::from(-positive_m01),
            m10: f64::from(-positive_m10),
            m11: f64::from(inverse_m11),
            x: f64::from(y.mul_add(positive_m01, -rounded_x)),
            y: f64::from(x.mul_add(positive_m10, -rounded_y)),
        }
    }

    pub(crate) fn scale_x(self) -> f64 {
        // sub_100015000/sub_10000F46C round the second square with FMUL,
        // fuse the first square with FMADD, then execute FSQRT in float32.
        let m00 = self.m00 as f32;
        let m10 = self.m10 as f32;
        f64::from(m00.mul_add(m00, m10 * m10).sqrt())
    }

    pub(crate) fn scale_y(self) -> f64 {
        let m01 = self.m01 as f32;
        let m11 = self.m11 as f32;
        f64::from(m01.mul_add(m01, m11 * m11).sqrt())
    }

    pub(crate) fn angle(self) -> f64 {
        // getEntityWorldTransform calls atan2f directly on the float matrix.
        f64::from((self.m10 as f32).atan2(self.m00 as f32))
    }
}

pub(crate) fn animation_node_world_affine(
    definition: &AnimationDefinition,
    playback: &AnimationPlayback,
    entity: &str,
    scene: AnimationAffine,
    descendant_reflection: bool,
) -> Option<AnimationAffine> {
    if !animation_definition_contains_entity(definition, entity) {
        return None;
    }
    let mut path = vec![entity];
    let mut current = entity;
    while let Some(parent) = definition.parents.get(current) {
        path.push(parent);
        current = parent;
    }
    path.reverse();
    let mut world = scene;
    for name in path {
        let mut local =
            AnimationAffine::from_transform(animation_local_transform(definition, playback, name)?);
        // Animation scene construction marks names beginning with `SLOT_`
        // through sub_10043CAF0. Those nodes skip the correction entirely.
        // Every other descendant compares its retained byte against the sign
        // of its parent's already-computed world determinant.
        if !name.starts_with("SLOT_") && ((world.determinant() < 0.0_f32) != descendant_reflection)
        {
            local = local.compensate_parent_reflection();
        }
        world = world.compose(local);
    }
    Some(world)
}

#[cfg(test)]
mod tests {
    use std::collections::{BTreeMap, BTreeSet};

    use super::*;

    #[test]
    fn affine_composition_matches_native_fmul_fmadd_then_fadd_order() {
        let parent = AnimationAffine {
            m00: 12_345.678_9,
            m01: -0.000_321_987,
            m10: std::f64::consts::PI,
            m11: 98_765.43,
            x: 511.123_456,
            y: -383.987_654,
        };
        let local = AnimationAffine {
            m00: 0.123_456_789,
            m01: -7.654_321,
            m10: 0.000_987_654,
            m11: 2.345_678_9,
            x: 582.18,
            y: 254.05,
        };
        let composed = parent.compose(local);
        let dot =
            |a: f64, b: f64, c: f64, d: f64| (a as f32).mul_add(b as f32, (c as f32) * (d as f32));
        assert_eq!(
            composed.m00.to_bits(),
            f64::from(dot(parent.m00, local.m00, parent.m01, local.m10)).to_bits()
        );
        assert_eq!(
            composed.m01.to_bits(),
            f64::from(dot(parent.m00, local.m01, parent.m01, local.m11)).to_bits()
        );
        assert_eq!(
            composed.m10.to_bits(),
            f64::from(dot(parent.m10, local.m00, parent.m11, local.m10)).to_bits()
        );
        assert_eq!(
            composed.m11.to_bits(),
            f64::from(dot(parent.m10, local.m01, parent.m11, local.m11)).to_bits()
        );
        assert_eq!(
            composed.x.to_bits(),
            f64::from((parent.x as f32) + dot(parent.m00, local.x, parent.m01, local.y)).to_bits()
        );
        assert_eq!(
            composed.y.to_bits(),
            f64::from((parent.y as f32) + dot(parent.m10, local.x, parent.m11, local.y)).to_bits()
        );

        let one_plus_ulp = f32::from_bits(0x3f80_0001);
        let fma_sensitive = AnimationAffine {
            m00: f64::from(one_plus_ulp),
            m01: -f64::from(f32::from_bits(0x3f80_0002)),
            ..AnimationAffine::default()
        }
        .compose(AnimationAffine {
            m00: f64::from(one_plus_ulp),
            m10: 1.0,
            ..AnimationAffine::default()
        });
        assert_eq!((fma_sensitive.m00 as f32).to_bits(), 0x2880_0000);
        assert_eq!(
            (one_plus_ulp * one_plus_ulp - f32::from_bits(0x3f80_0002)).to_bits(),
            0
        );
    }

    #[test]
    fn affine_decomposition_uses_native_float32_fmadd_sqrt_and_atan2f() {
        let m00 = f32::from_bits(0x354c_d837);
        let m10 = f32::from_bits(0x3646_a981);
        let transform = AnimationAffine {
            m00: f64::from(m00),
            m01: -0.75,
            m10: f64::from(m10),
            m11: 1.25,
            x: 0.0,
            y: 0.0,
        };

        let native_scale_x = m00.mul_add(m00, m10 * m10).sqrt();
        assert_eq!((transform.scale_x() as f32).to_bits(), 0x364d_2816);
        assert_eq!(
            (transform.scale_x() as f32).to_bits(),
            native_scale_x.to_bits()
        );
        assert_ne!(
            (transform.scale_x() as f32).to_bits(),
            f32::hypot(m00, m10).to_bits()
        );
        assert_eq!(
            (transform.angle() as f32).to_bits(),
            m10.atan2(m00).to_bits()
        );
    }

    #[test]
    fn mirrored_parent_correction_reverses_non_slot_rotation_but_skips_slots() {
        let action = AnimationAction {
            targets: BTreeMap::from([
                (
                    "MIRROR".to_owned(),
                    AnimationTarget {
                        scale: vec![(0.0, [-1.0, 1.0])],
                        ..AnimationTarget::default()
                    },
                ),
                (
                    "JOINT".to_owned(),
                    AnimationTarget {
                        scale: vec![(0.0, [2.0, 3.0])],
                        rotation: vec![(0.0, 0.25)],
                        ..AnimationTarget::default()
                    },
                ),
                (
                    "SLOT_TEST".to_owned(),
                    AnimationTarget {
                        scale: vec![(0.0, [2.0, 3.0])],
                        rotation: vec![(0.0, 0.25)],
                        ..AnimationTarget::default()
                    },
                ),
            ]),
            ..AnimationAction::default()
        };
        let definition = AnimationDefinition {
            actions: BTreeMap::from([("idle".to_owned(), action)]),
            entities: BTreeSet::from([
                "MIRROR".to_owned(),
                "JOINT".to_owned(),
                "SLOT_TEST".to_owned(),
            ]),
            parents: BTreeMap::from([
                ("JOINT".to_owned(), "MIRROR".to_owned()),
                ("SLOT_TEST".to_owned(), "MIRROR".to_owned()),
            ]),
            slots: vec!["SLOT_TEST".to_owned()],
            ..AnimationDefinition::default()
        };
        let playback =
            AnimationPlayback::active("idle".to_owned(), "repeat".to_owned(), 0.0, 1.0, 1.0);
        let scene = AnimationAffine::default();
        let mirror = AnimationAffine::from_transform(AnimationTransform {
            scale_x: -1.0,
            ..AnimationTransform::default()
        });
        let local = AnimationAffine::from_transform(AnimationTransform {
            scale_x: 2.0,
            scale_y: 3.0,
            angle: 0.25,
            ..AnimationTransform::default()
        });
        let corrected_world =
            animation_node_world_affine(&definition, &playback, "JOINT", scene, false).unwrap();
        let slot_world =
            animation_node_world_affine(&definition, &playback, "SLOT_TEST", scene, false).unwrap();
        let expected_corrected = mirror.compose(local.compensate_parent_reflection());
        let expected_slot = mirror.compose(local);

        assert_eq!(
            corrected_world.m00.to_bits(),
            expected_corrected.m00.to_bits()
        );
        assert_eq!(
            corrected_world.m01.to_bits(),
            expected_corrected.m01.to_bits()
        );
        assert_eq!(
            corrected_world.m10.to_bits(),
            expected_corrected.m10.to_bits()
        );
        assert_eq!(
            corrected_world.m11.to_bits(),
            expected_corrected.m11.to_bits()
        );
        assert_eq!(slot_world.m00.to_bits(), expected_slot.m00.to_bits());
        assert_eq!(slot_world.m01.to_bits(), expected_slot.m01.to_bits());
        assert_eq!(slot_world.m10.to_bits(), expected_slot.m10.to_bits());
        assert_eq!(slot_world.m11.to_bits(), expected_slot.m11.to_bits());
        assert_ne!(corrected_world.m01.to_bits(), slot_world.m01.to_bits());
    }
}
