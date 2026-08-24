//! Tangent, scalar-normal and two-point block velocity solves.

use crate::*;

impl RenderBridge {
    pub(crate) fn solve_contact_velocity_constraint(
        &mut self,
        pair: &ContactKey,
        first: ContactBodyState,
        second: ContactBodyState,
        manifold: ContactManifold,
    ) -> f64 {
        let first_inverse_mass = first.inverse_mass;
        let second_inverse_mass = second.inverse_mass;
        let inverse_mass_sum = first_inverse_mass + second_inverse_mass;
        if inverse_mass_sum <= 0.0_f32 {
            return 0.0;
        }
        let first_inverse_inertia = first.inverse_inertia;
        let second_inverse_inertia = second.inverse_inertia;
        let normal = (manifold.normal_x as f32, manifold.normal_y as f32);
        let tangent = (normal.1, -normal.0);
        let points = velocity_contact_points_for_states(first, second, manifold);
        // Native b2ContactSolver constraints retain two signed body indices
        // and use them for every tangent/normal access in all iterations.
        // Resolve our string-addressable scene names once per constraint pass.
        let body_indices = self
            .contact_velocity_cache
            .as_ref()
            .and_then(|cache| cache.body_indices((&pair.0, &pair.1)));
        let bodies = ContactVelocityBodies {
            names: (&pair.0, &pair.1),
            indices: body_indices,
            first,
            second,
        };
        let mut cached = self
            .solver_contact_impulses
            .get(pair)
            .copied()
            .unwrap_or_default()
            .aligned_to(&points);
        let velocity_bias = if let Some(bias) = self.contact_velocity_bias.get(pair).copied() {
            bias
        } else {
            let mut bias = [0.0_f32; 2];
            for (index, point) in points.iter().copied().enumerate() {
                let relative = self.relative_contact_velocity(
                    &pair.0,
                    &pair.1,
                    body_indices,
                    first,
                    second,
                    point,
                );
                let normal_velocity = relative.0.mul_add(normal.0, relative.1 * normal.1);
                if normal_velocity < -1.0_f32 {
                    let restitution = first.restitution.max(second.restitution);
                    bias[index] = -restitution * normal_velocity;
                }
            }
            self.contact_velocity_bias.insert(pair.clone(), bias);
            bias
        };
        let friction = (first.friction * second.friction).sqrt();
        let first_center = first.center;
        let second_center = second.center;

        // sub_100864164..204 walks both points and solves their tangent
        // constraints before entering the scalar/two-point normal branch.
        for (index, point) in points.iter().copied().enumerate() {
            let first_radius = (
                point.point_x as f32 - first_center.0,
                point.point_y as f32 - first_center.1,
            );
            let second_radius = (
                point.point_x as f32 - second_center.0,
                point.point_y as f32 - second_center.1,
            );
            let first_tangent_lever = first_radius
                .0
                .mul_add(tangent.1, -(first_radius.1 * tangent.0));
            let second_tangent_lever = second_radius
                .0
                .mul_add(tangent.1, -(second_radius.1 * tangent.0));
            let tangent_inverse_mass = (first_inverse_inertia * first_tangent_lever)
                .mul_add(first_tangent_lever, inverse_mass_sum)
                + second_inverse_inertia * second_tangent_lever * second_tangent_lever;
            if tangent_inverse_mass <= 0.0_f32 {
                continue;
            }
            let relative = self.relative_contact_velocity(
                &pair.0,
                &pair.1,
                body_indices,
                first,
                second,
                point,
            );
            let tangent_velocity = relative.0.mul_add(tangent.0, relative.1 * tangent.1);
            let (old_normal, old_tangent) = cached.point(index);
            let old_normal = old_normal as f32;
            let old_tangent = old_tangent as f32;
            let friction_limit = friction * old_normal;
            let new_tangent = (old_tangent - tangent_velocity / tangent_inverse_mass)
                .clamp(-friction_limit, friction_limit);
            let delta = new_tangent - old_tangent;
            cached.set_point(index, f64::from(old_normal), f64::from(new_tangent));
            if delta != 0.0_f32 {
                self.apply_contact_velocity_impulse(
                    bodies,
                    point,
                    (delta * tangent.0, delta * tangent.1),
                );
            }
        }

        let normal_inverse_mass = |point: ContactPoint| {
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

        if points.len() == 2 {
            let (k11, first_lever_1, second_lever_1) = normal_inverse_mass(points[0]);
            let (k22, first_lever_2, second_lever_2) = normal_inverse_mass(points[1]);
            let k12 = inverse_mass_sum
                + first_inverse_inertia * first_lever_1 * first_lever_2
                + second_inverse_inertia * second_lever_1 * second_lever_2;
            let determinant = k11.mul_add(k22, -(k12 * k12));
            // The initialization path recovered before sub_1008640D0 uses
            // Box2D's 1000:1 condition-number guard and drops point two when
            // the 2x2 matrix would be unstable.
            if k11 * k11 < 1_000.0_f32 * determinant {
                let a = [cached.normal as f32, cached.secondary_normal as f32];
                let relative_1 = self.relative_contact_velocity(
                    &pair.0,
                    &pair.1,
                    body_indices,
                    first,
                    second,
                    points[0],
                );
                let relative_2 = self.relative_contact_velocity(
                    &pair.0,
                    &pair.1,
                    body_indices,
                    first,
                    second,
                    points[1],
                );
                let mut b = [
                    relative_1.0.mul_add(normal.0, relative_1.1 * normal.1) - velocity_bias[0],
                    relative_2.0.mul_add(normal.0, relative_2.1 * normal.1) - velocity_bias[1],
                ];
                b[0] -= k11.mul_add(a[0], k12 * a[1]);
                b[1] -= k12.mul_add(a[0], k22 * a[1]);

                let inverse_determinant = determinant.recip();
                let both = [
                    -(k22 * b[0] - k12 * b[1]) * inverse_determinant,
                    -(-k12 * b[0] + k11 * b[1]) * inverse_determinant,
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
                    self.apply_contact_block_velocity_impulse(
                        bodies,
                        [points[0], points[1]],
                        normal,
                        deltas,
                    );
                }
            }
        } else {
            for (index, point) in points.iter().copied().enumerate() {
                let (effective_inverse_mass, _, _) = normal_inverse_mass(point);
                if effective_inverse_mass <= 0.0_f32 {
                    continue;
                }
                let relative = self.relative_contact_velocity(
                    &pair.0,
                    &pair.1,
                    body_indices,
                    first,
                    second,
                    point,
                );
                let normal_velocity = relative.0.mul_add(normal.0, relative.1 * normal.1);
                let (old_normal, tangent_impulse) = cached.point(index);
                let old_normal = old_normal as f32;
                let new_normal = (old_normal
                    - (normal_velocity - velocity_bias[index]) / effective_inverse_mass)
                    .max(0.0_f32);
                let delta = new_normal - old_normal;
                cached.set_point(index, f64::from(new_normal), tangent_impulse);
                if delta != 0.0_f32 {
                    self.apply_contact_velocity_impulse(
                        bodies,
                        point,
                        (delta * normal.0, delta * normal.1),
                    );
                }
            }
        }

        self.solver_contact_impulses.insert(pair.clone(), cached);
        f64::from((cached.normal as f32).max(cached.secondary_normal as f32))
    }
}
