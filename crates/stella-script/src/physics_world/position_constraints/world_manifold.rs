//! `b2PositionSolverManifold::Initialize` (`sub_100864CFC`).

use super::model::{
    PositionBodyState, PositionContactConstraint, PositionContactManifold, PositionWorldPoint,
};
#[cfg(test)]
use crate::SceneObject;

impl PositionContactConstraint {
    #[cfg(test)]
    pub(crate) fn world_point(
        &self,
        first: &SceneObject,
        second: &SceneObject,
        index: usize,
    ) -> Option<PositionWorldPoint> {
        self.world_point_from_states(
            PositionBodyState::capture(first),
            PositionBodyState::capture(second),
            index,
        )
    }

    pub(crate) fn world_point_from_states(
        &self,
        first: PositionBodyState,
        second: PositionBodyState,
        index: usize,
    ) -> Option<PositionWorldPoint> {
        match &self.manifold {
            PositionContactManifold::Circles {
                local_first,
                local_second,
                first_radius,
                second_radius,
            } => {
                let point_a = first.transform_point(
                    self.first_local_center,
                    (local_first.0 as f32, local_first.1 as f32),
                );
                let point_b = second.transform_point(
                    self.second_local_center,
                    (local_second.0 as f32, local_second.1 as f32),
                );
                let delta = (point_b.0 - point_a.0, point_b.1 - point_a.1);
                // 0x100864EB0..EB8 is FMUL(dy,dy), FMADD(dx,dx,dy²),
                // FSQRT. `hypot` has a different scaling/rounding contract.
                let distance = delta.0.mul_add(delta.0, delta.1 * delta.1).sqrt();
                let normal =
                    if distance.partial_cmp(&f32::EPSILON) != Some(std::cmp::Ordering::Less) {
                        (delta.0 / distance, delta.1 / distance)
                    } else {
                        (delta.0, delta.1)
                    };
                (index == 0).then_some(PositionWorldPoint {
                    normal,
                    point: (
                        (point_a.0 + point_b.0) * 0.5_f32,
                        (point_a.1 + point_b.1) * 0.5_f32,
                    ),
                    separation: delta.0.mul_add(normal.0, delta.1 * normal.1)
                        - *first_radius as f32
                        - *second_radius as f32,
                })
            }
            PositionContactManifold::FaceFirst {
                local_normal,
                local_plane_point,
                local_clip_points,
                first_radius,
                second_radius,
            } => {
                let (sine, cosine) = first.angle.sin_cos();
                let local_normal = (local_normal.0 as f32, local_normal.1 as f32);
                let normal = (
                    local_normal.0.mul_add(cosine, -(local_normal.1 * sine)),
                    local_normal.0.mul_add(sine, local_normal.1 * cosine),
                );
                let plane_point = first.transform_point(
                    self.first_local_center,
                    (local_plane_point.0 as f32, local_plane_point.1 as f32),
                );
                let local_clip_point = local_clip_points.get(index)?;
                let point = second.transform_point(
                    self.second_local_center,
                    (local_clip_point.0 as f32, local_clip_point.1 as f32),
                );
                Some(PositionWorldPoint {
                    normal,
                    point,
                    separation: (point.0 - plane_point.0)
                        .mul_add(normal.0, (point.1 - plane_point.1) * normal.1)
                        - *first_radius as f32
                        - *second_radius as f32,
                })
            }
            PositionContactManifold::FaceSecond {
                local_normal,
                local_plane_point,
                local_clip_points,
                first_radius,
                second_radius,
            } => {
                let (sine, cosine) = second.angle.sin_cos();
                let local_normal = (local_normal.0 as f32, local_normal.1 as f32);
                let reference_normal = (
                    local_normal.0.mul_add(cosine, -(local_normal.1 * sine)),
                    local_normal.0.mul_add(sine, local_normal.1 * cosine),
                );
                let normal = (-reference_normal.0, -reference_normal.1);
                let plane_point = second.transform_point(
                    self.second_local_center,
                    (local_plane_point.0 as f32, local_plane_point.1 as f32),
                );
                let local_clip_point = local_clip_points.get(index)?;
                let point = first.transform_point(
                    self.first_local_center,
                    (local_clip_point.0 as f32, local_clip_point.1 as f32),
                );
                Some(PositionWorldPoint {
                    normal,
                    point,
                    separation: (point.0 - plane_point.0).mul_add(
                        reference_normal.0,
                        (point.1 - plane_point.1) * reference_normal.1,
                    ) - *first_radius as f32
                        - *second_radius as f32,
                })
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        PositionBodyState, PositionContactConstraint, PositionContactManifold, PositionWorldPoint,
    };

    fn assert_point(point: PositionWorldPoint, expected: [u32; 5]) {
        assert_eq!(point.normal.0.to_bits(), expected[0]);
        assert_eq!(point.normal.1.to_bits(), expected[1]);
        assert_eq!(point.point.0.to_bits(), expected[2]);
        assert_eq!(point.point.1.to_bits(), expected[3]);
        assert_eq!(point.separation.to_bits(), expected[4]);
    }

    #[test]
    fn circle_length_uses_native_fmul_fmadd_fsqrt_order() {
        let x = f32::from_bits(0x3F23_C24E);
        let y = f32::from_bits(0xBF48_E555);
        let native = x.mul_add(x, y * y).sqrt();
        assert_eq!(native.to_bits(), 0x3F81_9780);
        assert_eq!(x.hypot(y).to_bits(), 0x3F81_9781);
    }

    #[test]
    fn sub_epsilon_circle_axis_stays_unnormalized() {
        let constraint = PositionContactConstraint {
            first_local_center: (0.0, 0.0),
            second_local_center: (0.0, 0.0),
            first_inverse_mass: 1.0,
            second_inverse_mass: 1.0,
            first_inverse_inertia: 0.0,
            second_inverse_inertia: 0.0,
            manifold: PositionContactManifold::Circles {
                local_first: (0.0, 0.0),
                local_second: (0.0, 0.0),
                first_radius: 0.0,
                second_radius: 0.0,
            },
        };
        let first = PositionBodyState {
            center: (0.0, 0.0),
            angle: 0.0,
        };
        let second = PositionBodyState {
            center: (f32::EPSILON * 0.5_f32, 0.0),
            angle: 0.0,
        };
        let point = constraint
            .world_point_from_states(first, second, 0)
            .unwrap();
        assert_eq!(point.normal, (f32::EPSILON * 0.5_f32, 0.0));
    }

    #[test]
    fn all_three_position_manifolds_keep_native_float32_order() {
        let first = PositionBodyState {
            center: (f32::from_bits(0x3F1A_B105), f32::from_bits(0xBF24_D17F)),
            angle: f32::from_bits(0x3E91_7A5C),
        };
        let second = PositionBodyState {
            center: (f32::from_bits(0x3F4E_EEA0), f32::from_bits(0x3EAD_30D2)),
            angle: f32::from_bits(0xBEA0_DBD4),
        };
        let base = |manifold| PositionContactConstraint {
            first_local_center: (f32::from_bits(0x3D13_7A5C), f32::from_bits(0xBD2B_918E)),
            second_local_center: (f32::from_bits(0xBD08_43A1), f32::from_bits(0x3D39_206B)),
            first_inverse_mass: 1.0,
            second_inverse_mass: 1.0,
            first_inverse_inertia: 1.0,
            second_inverse_inertia: 1.0,
            manifold,
        };
        let widened = |bits| f64::from(f32::from_bits(bits));
        let circles = base(PositionContactManifold::Circles {
            local_first: (widened(0x3E91_332A), widened(0xBE82_1A7C)),
            local_second: (widened(0xBE28_71C3), widened(0x3E6A_5109)),
            first_radius: widened(0x3DCC_CCCD),
            second_radius: widened(0x3E19_999A),
        });
        assert_point(
            circles.world_point_from_states(first, second, 0).unwrap(),
            [
                0xBDF6_0CB1,
                0x3F7E_254F,
                0x3F52_22CA,
                0xBDE6_1030,
                0x3F8B_AFB6,
            ],
        );
        let face_first = base(PositionContactManifold::FaceFirst {
            local_normal: (widened(0x3F19_999A), widened(0x3E80_0000)),
            local_plane_point: (widened(0x3E4C_CCCD), widened(0xBE99_999A)),
            local_clip_points: vec![(widened(0xBE19_999A), widened(0x3EA6_6666))],
            first_radius: widened(0x3B03_126F),
            second_radius: widened(0x3B03_126F),
        });
        assert_point(
            face_first
                .world_point_from_states(first, second, 0)
                .unwrap(),
            [
                0x3F01_800E,
                0x3ED0_FC18,
                0x3F48_A5BB,
                0x3F23_F429,
                0x3F13_BFBC,
            ],
        );
        let face_second = base(PositionContactManifold::FaceSecond {
            local_normal: (widened(0x3F00_0000), widened(0xBECC_CCCD)),
            local_plane_point: (widened(0xBDF5_C28F), widened(0x3E38_51EC)),
            local_clip_points: vec![(widened(0x3E0F_5C29), widened(0xBE57_0A3D))],
            first_radius: widened(0x3B03_126F),
            second_radius: widened(0x3B03_126F),
        });
        assert_point(
            face_second
                .world_point_from_states(first, second, 0)
                .unwrap(),
            [
                0xBEB4_2DCC,
                0x3F08_F1A9,
                0x3F40_4FA7,
                0xBF46_AAA7,
                0x3F2B_5566,
            ],
        );
    }
}
