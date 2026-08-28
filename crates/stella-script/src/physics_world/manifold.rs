//! Contact manifold records and velocity-point conditioning.

use crate::NativeToiTransform;
#[cfg(test)]
use crate::{ContactBodyState, SceneObject};

#[derive(Debug, Clone, Copy)]
pub(crate) enum ContactManifoldType {
    Circles,
    FaceFirst,
    FaceSecond,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct ContactManifold {
    pub(crate) normal_x: f64,
    pub(crate) normal_y: f64,
    pub(crate) penetration: f64,
    pub(crate) point_x: f64,
    pub(crate) point_y: f64,
    pub(crate) feature_id: u32,
    pub(crate) secondary: Option<ContactPoint>,
    pub(crate) position: ContactLocalManifold,
}

impl ContactManifold {
    #[cfg(test)]
    pub(crate) fn manifold_type(self) -> ContactManifoldType {
        self.position.manifold_type
    }

    pub(crate) fn points(self) -> Vec<ContactPoint> {
        let mut points = vec![ContactPoint {
            penetration: self.penetration,
            point_x: self.point_x,
            point_y: self.point_y,
            feature_id: self.feature_id,
        }];
        if let Some(point) = self.secondary {
            points.push(point);
        }
        points
    }

    pub(crate) fn native_local_position(self) -> ContactLocalManifold {
        self.position
    }

    /// Rebuild `b2WorldManifold` from the persistent local manifold after a
    /// TOI position solve has changed one or both body transforms. Box2D does
    /// this inside `InitializeVelocityConstraints`; carrying the pre-solve
    /// world points forward changes the angular lever arms and injects energy
    /// into multi-contact TOI islands.
    pub(crate) fn at_native_transforms(
        self,
        first: NativeToiTransform,
        second: NativeToiTransform,
    ) -> Self {
        let local = self.position;
        let point_count = usize::from(local.point_count).min(2);
        let mut refreshed = [(0.0_f32, 0.0_f32, 0.0_f32); 2];
        let normal = match local.manifold_type {
            ContactManifoldType::Circles => {
                let point_a = first.point(local.local_point);
                let point_b = second.point(local.local_points[0]);
                let delta = (point_b.0 - point_a.0, point_b.1 - point_a.1);
                let distance_squared = delta.0.mul_add(delta.0, delta.1 * delta.1);
                let normal = if distance_squared > f32::EPSILON * f32::EPSILON {
                    let inverse_distance = distance_squared.sqrt().recip();
                    (delta.0 * inverse_distance, delta.1 * inverse_distance)
                } else {
                    (1.0_f32, 0.0_f32)
                };
                let surface_a = (
                    local.first_radius.mul_add(normal.0, point_a.0),
                    local.first_radius.mul_add(normal.1, point_a.1),
                );
                let surface_b = (
                    (-local.second_radius).mul_add(normal.0, point_b.0),
                    (-local.second_radius).mul_add(normal.1, point_b.1),
                );
                let separation = (surface_b.0 - surface_a.0)
                    .mul_add(normal.0, (surface_b.1 - surface_a.1) * normal.1);
                refreshed[0] = (
                    -separation,
                    (surface_a.0 + surface_b.0) * 0.5_f32,
                    (surface_a.1 + surface_b.1) * 0.5_f32,
                );
                normal
            }
            ContactManifoldType::FaceFirst => {
                let normal = first.rotate(local.local_normal);
                let plane_point = first.point(local.local_point);
                for (output, &local_point) in refreshed
                    .iter_mut()
                    .zip(local.local_points.iter())
                    .take(point_count)
                {
                    let clip_point = second.point(local_point);
                    let plane_distance = (clip_point.0 - plane_point.0)
                        .mul_add(normal.0, (clip_point.1 - plane_point.1) * normal.1);
                    let surface_a = (
                        (local.first_radius - plane_distance).mul_add(normal.0, clip_point.0),
                        (local.first_radius - plane_distance).mul_add(normal.1, clip_point.1),
                    );
                    let surface_b = (
                        (-local.second_radius).mul_add(normal.0, clip_point.0),
                        (-local.second_radius).mul_add(normal.1, clip_point.1),
                    );
                    let separation = (surface_b.0 - surface_a.0)
                        .mul_add(normal.0, (surface_b.1 - surface_a.1) * normal.1);
                    *output = (
                        -separation,
                        (surface_a.0 + surface_b.0) * 0.5_f32,
                        (surface_a.1 + surface_b.1) * 0.5_f32,
                    );
                }
                normal
            }
            ContactManifoldType::FaceSecond => {
                let reference_normal = second.rotate(local.local_normal);
                let plane_point = second.point(local.local_point);
                for (output, &local_point) in refreshed
                    .iter_mut()
                    .zip(local.local_points.iter())
                    .take(point_count)
                {
                    let clip_point = first.point(local_point);
                    let plane_distance = (clip_point.0 - plane_point.0).mul_add(
                        reference_normal.0,
                        (clip_point.1 - plane_point.1) * reference_normal.1,
                    );
                    let surface_b = (
                        (local.second_radius - plane_distance)
                            .mul_add(reference_normal.0, clip_point.0),
                        (local.second_radius - plane_distance)
                            .mul_add(reference_normal.1, clip_point.1),
                    );
                    let surface_a = (
                        (-local.first_radius).mul_add(reference_normal.0, clip_point.0),
                        (-local.first_radius).mul_add(reference_normal.1, clip_point.1),
                    );
                    let separation = (surface_a.0 - surface_b.0).mul_add(
                        reference_normal.0,
                        (surface_a.1 - surface_b.1) * reference_normal.1,
                    );
                    *output = (
                        -separation,
                        (surface_a.0 + surface_b.0) * 0.5_f32,
                        (surface_a.1 + surface_b.1) * 0.5_f32,
                    );
                }
                (-reference_normal.0, -reference_normal.1)
            }
        };
        let feature_ids = [
            self.feature_id,
            self.secondary.map(|point| point.feature_id).unwrap_or(0),
        ];
        let point = |index: usize| ContactPoint {
            penetration: f64::from(refreshed[index].0),
            point_x: f64::from(refreshed[index].1),
            point_y: f64::from(refreshed[index].2),
            feature_id: feature_ids[index],
        };
        let primary = point(0);
        Self {
            normal_x: f64::from(normal.0),
            normal_y: f64::from(normal.1),
            penetration: primary.penetration,
            point_x: primary.point_x,
            point_y: primary.point_y,
            feature_id: primary.feature_id,
            secondary: (point_count > 1).then(|| point(1)),
            position: local,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct ContactLocalManifold {
    pub(crate) manifold_type: ContactManifoldType,
    pub(crate) local_normal: (f32, f32),
    pub(crate) local_point: (f32, f32),
    pub(crate) local_points: [(f32, f32); 2],
    pub(crate) point_count: u8,
    pub(crate) first_radius: f32,
    pub(crate) second_radius: f32,
}

#[cfg(test)]
pub(crate) fn velocity_contact_points(
    first: &SceneObject,
    second: &SceneObject,
    manifold: ContactManifold,
) -> Vec<ContactPoint> {
    velocity_contact_points_for_states(
        ContactBodyState::capture(first, 0),
        ContactBodyState::capture(second, 0),
        manifold,
    )
}

#[cfg(test)]
pub(crate) fn velocity_contact_points_for_states(
    first: ContactBodyState,
    second: ContactBodyState,
    manifold: ContactManifold,
) -> Vec<ContactPoint> {
    let mut points = manifold.points();
    if points.len() != 2 {
        return points;
    }
    let first_inverse_mass = first.inverse_mass;
    let second_inverse_mass = second.inverse_mass;
    let inverse_mass_sum = first_inverse_mass + second_inverse_mass;
    let first_inverse_inertia = first.inverse_inertia;
    let second_inverse_inertia = second.inverse_inertia;
    let normal = (manifold.normal_x as f32, manifold.normal_y as f32);
    let first_center = first.center;
    let second_center = second.center;
    let inverse_normal_mass = |point: ContactPoint| {
        let first_radius = (
            point.point_x as f32 - first_center.0,
            point.point_y as f32 - first_center.1,
        );
        let second_radius = (
            point.point_x as f32 - second_center.0,
            point.point_y as f32 - second_center.1,
        );
        let first_lever = first_radius
            .0
            .mul_add(normal.1, -(first_radius.1 * normal.0));
        let second_lever = second_radius
            .0
            .mul_add(normal.1, -(second_radius.1 * normal.0));
        (
            (first_inverse_inertia * first_lever).mul_add(first_lever, inverse_mass_sum)
                + second_inverse_inertia * second_lever * second_lever,
            first_lever,
            second_lever,
        )
    };
    let (k11, first_lever_1, second_lever_1) = inverse_normal_mass(points[0]);
    let (k22, first_lever_2, second_lever_2) = inverse_normal_mass(points[1]);
    let k12 = inverse_mass_sum
        + first_inverse_inertia * first_lever_1 * first_lever_2
        + second_inverse_inertia * second_lever_1 * second_lever_2;
    let determinant = k11.mul_add(k22, -(k12 * k12));
    // InitVelocityConstraints in Purple keeps the second point only when
    // k11² < 1000 * det(K). This happens before WarmStart and before tangent
    // constraints, not inside the later normal block-solver branch.
    if (k11 * k11).partial_cmp(&(1_000.0_f32 * determinant)) != Some(std::cmp::Ordering::Less) {
        points.truncate(1);
    }
    points
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct ContactPoint {
    pub(crate) penetration: f64,
    pub(crate) point_x: f64,
    pub(crate) point_y: f64,
    pub(crate) feature_id: u32,
}

#[cfg(test)]
mod world_manifold_tests {
    use super::*;
    use crate::circle_circle_manifold_at_transforms;

    #[test]
    fn toi_refresh_reprojects_local_circle_witnesses_at_corrected_transforms() {
        let first = NativeToiTransform::IDENTITY;
        let second_before = NativeToiTransform {
            position: (1.5, 0.25),
            sine: 0.0,
            cosine: 1.0,
        };
        let second_after = NativeToiTransform {
            position: (1.65, 0.15),
            sine: 0.0,
            cosine: 1.0,
        };
        let manifold = circle_circle_manifold_at_transforms(
            (0.0, 0.0),
            1.0,
            first,
            (0.0, 0.0),
            1.0,
            second_before,
        )
        .unwrap();
        let refreshed = manifold.at_native_transforms(first, second_after);
        let direct = circle_circle_manifold_at_transforms(
            (0.0, 0.0),
            1.0,
            first,
            (0.0, 0.0),
            1.0,
            second_after,
        )
        .unwrap();

        assert_eq!(refreshed.normal_x.to_bits(), direct.normal_x.to_bits());
        assert_eq!(refreshed.normal_y.to_bits(), direct.normal_y.to_bits());
        assert_eq!(refreshed.point_x.to_bits(), direct.point_x.to_bits());
        assert_eq!(refreshed.point_y.to_bits(), direct.point_y.to_bits());
        assert_eq!(
            refreshed.penetration.to_bits(),
            direct.penetration.to_bits()
        );
        assert_eq!(refreshed.feature_id, manifold.feature_id);
    }
}
