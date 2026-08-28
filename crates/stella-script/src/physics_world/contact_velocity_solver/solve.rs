//! Tangent, scalar-normal and two-point block velocity solves.

use crate::*;

impl RenderBridge {
    pub(crate) fn solve_contact_velocity_constraint(&mut self, pair: &ContactKey) -> f64 {
        let Some(constraint) = self.contact_velocity_constraints.get(pair).copied() else {
            return 0.0;
        };
        let normal = constraint.normal;
        let tangent = (normal.1, -normal.0);
        let body_indices = self
            .contact_velocity_cache
            .as_ref()
            .and_then(|cache| cache.body_indices((&pair.0, &pair.1)));
        let bodies = ContactVelocityBodies {
            names: (&pair.0, &pair.1),
            indices: body_indices,
            first: constraint.first,
            second: constraint.second,
        };
        let mut cached = self
            .solver_contact_impulses
            .get(pair)
            .copied()
            .unwrap_or_default();

        // sub_100864164..204 walks both points and solves their tangent
        // constraints before entering the scalar/two-point normal branch.
        for (index, point) in constraint
            .points
            .into_iter()
            .take(constraint.point_count)
            .enumerate()
        {
            let relative = self.relative_contact_velocity(bodies, point);
            let tangent_velocity = relative.0.mul_add(tangent.0, relative.1 * tangent.1);
            let (old_normal, old_tangent) = cached.point(index);
            let old_normal = old_normal as f32;
            let old_tangent = old_tangent as f32;
            let friction_limit = constraint.friction * old_normal;
            let new_tangent = point
                .tangent_mass
                .mul_add(-tangent_velocity, old_tangent)
                .clamp(-friction_limit, friction_limit);
            let delta = new_tangent - old_tangent;
            cached.set_point(index, f64::from(old_normal), f64::from(new_tangent));
            self.apply_contact_velocity_impulse(
                bodies,
                point,
                (delta * tangent.0, delta * tangent.1),
            );
        }

        if constraint.point_count == 2 {
            let points = constraint.points;
            let (k11, k12, k22) = constraint.normal_k;
            let (m11, m12, m22) = constraint.normal_mass;
            let a = [cached.normal as f32, cached.secondary_normal as f32];
            let relative_1 = self.relative_contact_velocity(bodies, points[0]);
            let relative_2 = self.relative_contact_velocity(bodies, points[1]);
            let mut b = [
                relative_1.0.mul_add(normal.0, relative_1.1 * normal.1) - points[0].velocity_bias,
                relative_2.0.mul_add(normal.0, relative_2.1 * normal.1) - points[1].velocity_bias,
            ];
            b[0] -= k11.mul_add(a[0], k12 * a[1]);
            b[1] -= k12.mul_add(a[0], k22 * a[1]);

            let both = [
                -((m11 * b[0]) + (m12 * b[1])),
                -m12.mul_add(b[0], m22 * b[1]),
            ];
            let solution = if both[0] >= 0.0_f32 && both[1] >= 0.0_f32 {
                Some(both)
            } else {
                let first_only = -b[0] / k11;
                if first_only >= 0.0_f32 && k12.mul_add(first_only, b[1]) >= 0.0_f32 {
                    Some([first_only, 0.0_f32])
                } else {
                    let second_only = -b[1] / k22;
                    if second_only >= 0.0_f32 && k12.mul_add(second_only, b[0]) >= 0.0_f32 {
                        Some([0.0_f32, second_only])
                    } else if b[0] >= 0.0_f32 && b[1] >= 0.0_f32 {
                        Some([0.0_f32, 0.0_f32])
                    } else {
                        None
                    }
                }
            };
            if let Some(solution) = solution {
                let deltas = [solution[0] - a[0], solution[1] - a[1]];
                for (index, normal_impulse) in solution.into_iter().enumerate() {
                    let (_, tangent_impulse) = cached.point(index);
                    cached.set_point(index, f64::from(normal_impulse), tangent_impulse);
                }
                self.apply_contact_block_velocity_impulse(bodies, points, normal, deltas);
            }
        } else if constraint.point_count == 1 {
            let point = constraint.points[0];
            let relative = self.relative_contact_velocity(bodies, point);
            let normal_velocity = relative.0.mul_add(normal.0, relative.1 * normal.1);
            let (old_normal, tangent_impulse) = cached.point(0);
            let old_normal = old_normal as f32;
            let new_normal = point
                .normal_mass
                .mul_add(point.velocity_bias - normal_velocity, old_normal)
                .max(0.0_f32);
            let delta = new_normal - old_normal;
            cached.set_point(0, f64::from(new_normal), tangent_impulse);
            self.apply_contact_velocity_impulse(
                bodies,
                point,
                (delta * normal.0, delta * normal.1),
            );
        }

        self.solver_contact_impulses.insert(pair.clone(), cached);
        f64::from((cached.normal as f32).max(cached.secondary_normal as f32))
    }
}
