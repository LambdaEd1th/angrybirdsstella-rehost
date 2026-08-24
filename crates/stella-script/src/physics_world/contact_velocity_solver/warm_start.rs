//! `b2ContactSolver::WarmStart` cached impulse application.

use crate::*;

impl RenderBridge {
    pub(crate) fn warm_start_island_contact_constraints(&mut self, contact_keys: &[ContactKey]) {
        for pair in contact_keys {
            let Some(manifold) = self.velocity_contacts.get(pair).copied() else {
                continue;
            };
            let Some(first) = self
                .scene
                .get(&pair.0)
                .map(|object| ContactBodyState::capture(object, pair.2))
            else {
                continue;
            };
            let Some(second) = self
                .scene
                .get(&pair.1)
                .map(|object| ContactBodyState::capture(object, pair.3))
            else {
                continue;
            };
            let impulse = self
                .solver_contact_impulses
                .get(pair)
                .copied()
                .unwrap_or_default();
            self.apply_cached_contact_impulse(&pair.0, &pair.1, first, second, manifold, impulse);
        }
    }

    fn apply_cached_contact_impulse(
        &mut self,
        first_name: &str,
        second_name: &str,
        first: ContactBodyState,
        second: ContactBodyState,
        manifold: ContactManifold,
        impulse: CachedContactImpulse,
    ) {
        let first_inverse_mass = first.inverse_mass;
        let second_inverse_mass = second.inverse_mass;
        let first_inverse_inertia = first.inverse_inertia;
        let second_inverse_inertia = second.inverse_inertia;
        let normal = (manifold.normal_x as f32, manifold.normal_y as f32);
        let tangent = (normal.1, -normal.0);
        let first_center = first.center;
        let second_center = second.center;
        for (index, point) in manifold.points().into_iter().enumerate() {
            let (normal_impulse, tangent_impulse) = impulse.point(index);
            let normal_impulse = normal_impulse as f32;
            let tangent_impulse = tangent_impulse as f32;
            let total_impulse = (
                normal_impulse.mul_add(normal.0, tangent_impulse * tangent.0),
                normal_impulse.mul_add(normal.1, tangent_impulse * tangent.1),
            );
            let first_radius = (
                point.point_x as f32 - first_center.0,
                point.point_y as f32 - first_center.1,
            );
            let second_radius = (
                point.point_x as f32 - second_center.0,
                point.point_y as f32 - second_center.1,
            );
            if let Some(object) = self.scene.get_mut(first_name) {
                let cross = first_radius
                    .0
                    .mul_add(total_impulse.1, -(first_radius.1 * total_impulse.0));
                object.velocity_x =
                    f64::from(object.velocity_x as f32 - first_inverse_mass * total_impulse.0);
                object.velocity_y =
                    f64::from(object.velocity_y as f32 - first_inverse_mass * total_impulse.1);
                object.angular_velocity =
                    f64::from(object.angular_velocity as f32 - first_inverse_inertia * cross);
            }
            if let Some(object) = self.scene.get_mut(second_name) {
                let cross = second_radius
                    .0
                    .mul_add(total_impulse.1, -(second_radius.1 * total_impulse.0));
                object.velocity_x =
                    f64::from(object.velocity_x as f32 + second_inverse_mass * total_impulse.0);
                object.velocity_y =
                    f64::from(object.velocity_y as f32 + second_inverse_mass * total_impulse.1);
                object.angular_velocity =
                    f64::from(object.angular_velocity as f32 + second_inverse_inertia * cross);
            }
        }
    }
}
