//! Box2D contact position-constraint solve.

use crate::*;

impl RenderBridge {
    #[cfg(test)]
    pub(crate) fn solve_contact_positions(&mut self) -> bool {
        let contact_keys = self.position_contacts.keys().cloned().collect::<Vec<_>>();
        self.solve_island_contact_positions(&contact_keys)
    }

    pub(crate) fn solve_island_contact_positions(&mut self, contact_keys: &[ContactKey]) -> bool {
        let contacts = contact_keys
            .iter()
            .filter_map(|key| {
                self.position_contacts
                    .get(key)
                    .cloned()
                    .map(|constraint| (key.clone(), constraint))
            })
            .collect::<Vec<_>>();
        let mut minimum_separation = 0.0_f32;
        for ((first_name, second_name, _, _), constraint) in contacts {
            for point_index in 0..constraint.point_count() {
                // b2PositionSolverManifold::Initialize is called inside the
                // point loop. Rebuild transforms, normal, separation and
                // lever arms after every prior point impulse instead of
                // solving both points from one stale transform snapshot.
                let Some(first) = self.scene.get(&first_name).map(PositionBodyState::capture)
                else {
                    continue;
                };
                let Some(second) = self.scene.get(&second_name).map(PositionBodyState::capture)
                else {
                    continue;
                };
                let Some(point) = constraint.world_point_from_states(first, second, point_index)
                else {
                    continue;
                };
                let (normal_x, normal_y) = point.normal;
                let (point_x, point_y) = point.point;
                let separation = point.separation;
                minimum_separation = minimum_separation.min(separation);
                let first_inverse_mass = constraint.first_inverse_mass;
                let second_inverse_mass = constraint.second_inverse_mass;
                let first_inverse_inertia = constraint.first_inverse_inertia;
                let second_inverse_inertia = constraint.second_inverse_inertia;
                let first_center = first.center;
                let second_center = second.center;
                let first_radius_x = point_x - first_center.0;
                let first_radius_y = point_y - first_center.1;
                let second_radius_x = point_x - second_center.0;
                let second_radius_y = point_y - second_center.1;
                let first_normal_lever =
                    first_radius_x.mul_add(normal_y, -(first_radius_y * normal_x));
                let second_normal_lever =
                    second_radius_x.mul_add(normal_y, -(second_radius_y * normal_x));
                let effective_inverse_mass = (first_inverse_inertia * first_normal_lever)
                    .mul_add(first_normal_lever, first_inverse_mass + second_inverse_mass)
                    + second_inverse_inertia * second_normal_lever * second_normal_lever;
                if effective_inverse_mass <= 0.0_f32 {
                    continue;
                }

                // sub_1008646A4 exposes Purple's exact Box2D position
                // constants: 0.001 slop, 0.2 Baumgarte and 0.2 maximum
                // correction. It applies them independently to both points.
                let positional_error =
                    (0.2_f32 * (separation + 0.001_f32)).clamp(-0.2_f32, 0.0_f32);
                let correction = -positional_error / effective_inverse_mass;
                let impulse_x = correction * normal_x;
                let impulse_y = correction * normal_y;
                if let Some(object) = self.scene.get_mut(&first_name) {
                    object.apply_native_position_delta(
                        -first_inverse_mass * impulse_x,
                        -first_inverse_mass * impulse_y,
                        -first_inverse_inertia
                            * first_radius_x.mul_add(impulse_y, -(first_radius_y * impulse_x)),
                    );
                }
                if let Some(object) = self.scene.get_mut(&second_name) {
                    object.apply_native_position_delta(
                        second_inverse_mass * impulse_x,
                        second_inverse_mass * impulse_y,
                        second_inverse_inertia
                            * second_radius_x.mul_add(impulse_y, -(second_radius_y * impulse_x)),
                    );
                }
            }
        }
        // sub_1008646A4 returns minSeparation >= -3 * linearSlop.
        // Purple's decompiled solver embeds a 0.001 linear slop.
        minimum_separation >= -0.003_f32
    }
}
