//! `b2ContactSolver::InitializeVelocityConstraints` state construction.

use crate::*;

impl RenderBridge {
    /// Seed every currently touching, non-sensor fixture pair before
    /// InitializeVelocityConstraints. New Box2D contacts have zero cached
    /// impulses, but they still need restitution bias computed from the same
    /// pre-warm island velocity snapshot as persistent contacts.
    #[cfg(test)]
    pub(crate) fn seed_contact_velocity_constraints(&mut self) {
        let contact_keys = self.velocity_contacts.keys().cloned().collect::<Vec<_>>();
        self.seed_island_contact_velocity_constraints(&contact_keys);
    }

    pub(crate) fn seed_island_contact_velocity_constraints(&mut self, contact_keys: &[ContactKey]) {
        for constraint in contact_keys.iter().cloned() {
            self.contact_impulses.entry(constraint).or_default();
        }
    }

    #[cfg(test)]
    pub(crate) fn begin_contact_step(&mut self) {
        let contact_keys = self.velocity_contacts.keys().cloned().collect::<Vec<_>>();
        self.begin_island_contact_step(&contact_keys);
    }

    pub(crate) fn begin_island_contact_step(&mut self, contact_keys: &[ContactKey]) {
        self.apply_warm_start_debug_overrides();
        let constraints = contact_keys
            .iter()
            .filter_map(|pair| {
                self.velocity_contacts
                    .get(pair)
                    .copied()
                    .map(|manifold| (pair.clone(), manifold))
            })
            .collect::<Vec<_>>();
        self.initialize_contact_velocity_constraints(&constraints, true);
        // Purple calls the independent 0x100863FAC member only after
        // InitializeVelocityConstraints has completed for every constraint.
        self.warm_start_island_contact_constraints(contact_keys);
    }

    pub(crate) fn begin_toi_contact_step(&mut self, constraints: &[(ContactKey, ContactManifold)]) {
        // b2Island::SolveTOI constructs a solver with warmStarting=false but
        // still initializes effective masses and restitution velocity bias.
        self.initialize_contact_velocity_constraints(constraints, false);
    }

    fn apply_warm_start_debug_overrides(&mut self) {
        if std::env::var_os("STELLA_DISABLE_CONTACT_WARM_START").is_some() {
            for impulse in self.contact_impulses.values_mut() {
                impulse.normal = 0.0;
                impulse.tangent = 0.0;
                impulse.secondary_normal = 0.0;
                impulse.secondary_tangent = 0.0;
            }
        }
        if std::env::var_os("STELLA_DISABLE_CONTACT_NORMAL_WARM_START").is_some() {
            for impulse in self.contact_impulses.values_mut() {
                impulse.normal = 0.0;
                impulse.secondary_normal = 0.0;
            }
        }
        if std::env::var_os("STELLA_DISABLE_CONTACT_TANGENT_WARM_START").is_some() {
            for impulse in self.contact_impulses.values_mut() {
                impulse.tangent = 0.0;
                impulse.secondary_tangent = 0.0;
            }
        }
    }

    fn initialize_contact_velocity_constraints(
        &mut self,
        constraints: &[(ContactKey, ContactManifold)],
        warm_starting: bool,
    ) {
        self.contact_velocity_bias.clear();
        self.contact_velocity_constraints.clear();
        // InitializeVelocityConstraints computes every point's restitution
        // bias before the separate WarmStart member mutates any island body.
        for (pair, manifold) in constraints.iter().cloned() {
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
            if first.sensor
                || second.sensor
                || !(first.participates_in_velocity_solve()
                    || second.participates_in_velocity_solve())
            {
                continue;
            }

            let mut points = manifold.points();
            let normal = (manifold.normal_x as f32, manifold.normal_y as f32);
            let tangent = (normal.1, -normal.0);
            let inverse_mass_sum = first.inverse_mass + second.inverse_mass;
            let mut native_points = [NativeContactVelocityPoint::default(); 2];
            let mut velocity_bias = [0.0_f32; 2];
            let restitution = first.restitution.max(second.restitution);
            for (index, point) in points.iter().copied().take(2).enumerate() {
                let first_radius = (
                    point.point_x as f32 - first.center.0,
                    point.point_y as f32 - first.center.1,
                );
                let second_radius = (
                    point.point_x as f32 - second.center.0,
                    point.point_y as f32 - second.center.1,
                );
                let first_normal_lever = first_radius
                    .0
                    .mul_add(normal.1, -(first_radius.1 * normal.0));
                let second_normal_lever = second_radius
                    .0
                    .mul_add(normal.1, -(second_radius.1 * normal.0));
                let normal_inverse_mass = first
                    .inverse_inertia
                    .mul_add(first_normal_lever * first_normal_lever, inverse_mass_sum);
                let normal_inverse_mass = second.inverse_inertia.mul_add(
                    second_normal_lever * second_normal_lever,
                    normal_inverse_mass,
                );
                let normal_mass = if normal_inverse_mass > 0.0 {
                    normal_inverse_mass.recip()
                } else {
                    0.0
                };
                let first_tangent_lever = first_radius
                    .0
                    .mul_add(tangent.1, -(first_radius.1 * tangent.0));
                let second_tangent_lever = second_radius
                    .0
                    .mul_add(tangent.1, -(second_radius.1 * tangent.0));
                let tangent_inverse_mass = first
                    .inverse_inertia
                    .mul_add(first_tangent_lever * first_tangent_lever, inverse_mass_sum);
                let tangent_inverse_mass = second.inverse_inertia.mul_add(
                    second_tangent_lever * second_tangent_lever,
                    tangent_inverse_mass,
                );
                let tangent_mass = if tangent_inverse_mass > 0.0 {
                    tangent_inverse_mass.recip()
                } else {
                    0.0
                };
                let first_contact_velocity = (
                    (-first.angular_velocity).mul_add(first_radius.1, first.velocity.0),
                    first
                        .angular_velocity
                        .mul_add(first_radius.0, first.velocity.1),
                );
                let second_contact_velocity = (
                    (-second.angular_velocity).mul_add(second_radius.1, second.velocity.0),
                    second
                        .angular_velocity
                        .mul_add(second_radius.0, second.velocity.1),
                );
                let normal_velocity = (second_contact_velocity.0 - first_contact_velocity.0)
                    .mul_add(
                        normal.0,
                        (second_contact_velocity.1 - first_contact_velocity.1) * normal.1,
                    );
                let bias = if normal_velocity < -1.0_f32 {
                    -restitution * normal_velocity
                } else {
                    0.0
                };
                native_points[index] = NativeContactVelocityPoint {
                    first_radius,
                    second_radius,
                    normal_mass,
                    tangent_mass,
                    velocity_bias: bias,
                };
                velocity_bias[index] = bias;
            }

            let mut normal_k = (0.0_f32, 0.0_f32, 0.0_f32);
            let mut normal_mass = (0.0_f32, 0.0_f32, 0.0_f32);
            if points.len() == 2 {
                let normal_levers = native_points.map(|point| {
                    (
                        point
                            .first_radius
                            .0
                            .mul_add(normal.1, -(point.first_radius.1 * normal.0)),
                        point
                            .second_radius
                            .0
                            .mul_add(normal.1, -(point.second_radius.1 * normal.0)),
                    )
                });
                let first_weighted_1 = first.inverse_inertia * normal_levers[0].0;
                let second_weighted_1 = second.inverse_inertia * normal_levers[0].1;
                let k11 = normal_levers[0]
                    .0
                    .mul_add(first_weighted_1, inverse_mass_sum);
                let k11 = normal_levers[0].1.mul_add(second_weighted_1, k11);
                let k22 = first
                    .inverse_inertia
                    .mul_add(normal_levers[1].0 * normal_levers[1].0, inverse_mass_sum);
                let k22 = second
                    .inverse_inertia
                    .mul_add(normal_levers[1].1 * normal_levers[1].1, k22);
                let k12 = normal_levers[1]
                    .0
                    .mul_add(first_weighted_1, inverse_mass_sum);
                let k12 = normal_levers[1].1.mul_add(second_weighted_1, k12);
                let determinant = k11.mul_add(k22, -(k12 * k12));
                if k11 * k11 < 1_000.0_f32 * determinant {
                    normal_k = (k11, k12, k22);
                    let inverse_determinant = if determinant != 0.0 {
                        determinant.recip()
                    } else {
                        determinant
                    };
                    normal_mass = (
                        k22 * inverse_determinant,
                        -(inverse_determinant * k12),
                        k11 * inverse_determinant,
                    );
                } else {
                    points.truncate(1);
                    velocity_bias[1] = 0.0;
                }
            }

            // Box2D transfers cached impulses by b2ContactID, not by the
            // contact point's array slot. A changed feature starts at zero.
            let source = if warm_starting {
                self.contact_impulses
                    .get(&pair)
                    .copied()
                    .unwrap_or_default()
            } else {
                CachedContactImpulse::default()
            };
            self.solver_contact_impulses
                .insert(pair.clone(), source.aligned_to(&points));
            self.contact_velocity_constraints.insert(
                pair.clone(),
                NativeContactVelocityConstraint {
                    points: native_points,
                    normal,
                    normal_mass,
                    normal_k,
                    first,
                    second,
                    friction: (first.friction * second.friction).sqrt(),
                    point_count: points.len(),
                },
            );
            self.contact_velocity_bias.insert(pair, velocity_bias);
        }
    }
}
