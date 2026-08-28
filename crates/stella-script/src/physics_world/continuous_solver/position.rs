//! `b2ContactSolver::SolveTOIPositionConstraints`.

use crate::*;

impl RenderBridge {
    pub(crate) fn solve_toi_position_constraint(
        &mut self,
        contact: &NativeToiContact,
        constraint: &PositionContactConstraint,
    ) -> bool {
        let mut minimum_separation = 0.0_f32;
        for point_index in 0..constraint.point_count() {
            let Some(first) = self
                .scene
                .get(&contact.key.0)
                .map(PositionBodyState::capture)
            else {
                continue;
            };
            let Some(second) = self
                .scene
                .get(&contact.key.1)
                .map(PositionBodyState::capture)
            else {
                continue;
            };
            let Some(point) = constraint.world_point_from_states(first, second, point_index) else {
                continue;
            };
            minimum_separation = minimum_separation.min(point.separation);
            let first_is_toi = contact.dynamic_body == contact.key.0;
            let second_is_toi = contact.dynamic_body == contact.key.1;
            let first_inverse_mass = if first_is_toi {
                constraint.first_inverse_mass
            } else {
                0.0_f32
            };
            let second_inverse_mass = if second_is_toi {
                constraint.second_inverse_mass
            } else {
                0.0_f32
            };
            let first_inverse_inertia = if first_is_toi {
                constraint.first_inverse_inertia
            } else {
                0.0_f32
            };
            let second_inverse_inertia = if second_is_toi {
                constraint.second_inverse_inertia
            } else {
                0.0_f32
            };
            let first_center = first.center;
            let second_center = second.center;
            let first_radius = (
                point.point.0 - first_center.0,
                point.point.1 - first_center.1,
            );
            let second_radius = (
                point.point.0 - second_center.0,
                point.point.1 - second_center.1,
            );
            let first_lever = first_radius
                .0
                .mul_add(point.normal.1, -(first_radius.1 * point.normal.0));
            let second_lever = second_radius
                .0
                .mul_add(point.normal.1, -(second_radius.1 * point.normal.0));
            let inverse_mass = native_position_effective_inverse_mass(
                first_inverse_mass,
                second_inverse_mass,
                first_inverse_inertia,
                second_inverse_inertia,
                first_lever,
                second_lever,
            );

            // sub_1008649BC is b2ContactSolver's TOI-only position pass:
            // 0.75 Baumgarte, 0.001 slop and 0.2 maximum correction.
            let impulse = native_position_correction(point.separation, 0.75_f32, inverse_mass);
            let impulse_vector = (impulse * point.normal.0, impulse * point.normal.1);
            if first_is_toi && let Some(object) = self.scene.get_mut(&contact.key.0) {
                object.apply_native_position_impulse(
                    first_inverse_mass,
                    (-impulse_vector.0, -impulse_vector.1),
                    first_inverse_inertia,
                    -native_position_cross(first_radius, impulse_vector),
                );
            }
            if second_is_toi && let Some(object) = self.scene.get_mut(&contact.key.1) {
                object.apply_native_position_impulse(
                    second_inverse_mass,
                    impulse_vector,
                    second_inverse_inertia,
                    native_position_cross(second_radius, impulse_vector),
                );
            }
        }
        minimum_separation >= -0.0015_f32
    }
}
