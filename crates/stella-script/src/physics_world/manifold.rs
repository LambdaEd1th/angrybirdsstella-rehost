//! Contact manifold records and velocity-point conditioning.

use crate::ContactBodyState;
#[cfg(test)]
use crate::SceneObject;

#[derive(Debug, Clone, Copy)]
pub(crate) enum ContactManifoldType {
    Circles,
    FaceFirst,
    FaceSecond,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct ContactManifold {
    pub(crate) manifold_type: ContactManifoldType,
    pub(crate) normal_x: f64,
    pub(crate) normal_y: f64,
    pub(crate) penetration: f64,
    pub(crate) point_x: f64,
    pub(crate) point_y: f64,
    pub(crate) feature_id: u32,
    pub(crate) secondary: Option<ContactPoint>,
}

impl ContactManifold {
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
