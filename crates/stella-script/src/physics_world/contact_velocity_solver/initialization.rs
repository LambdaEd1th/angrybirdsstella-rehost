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

            // Box2D transfers cached impulses by b2ContactID, not by the
            // contact point's array slot. A changed feature starts at zero.
            let points = velocity_contact_points_for_states(first, second, manifold);
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

            let mut velocity_bias = [0.0_f32; 2];
            let restitution = first.restitution.max(second.restitution);
            let first_center = first.center;
            let second_center = second.center;
            let normal_x = manifold.normal_x as f32;
            let normal_y = manifold.normal_y as f32;
            for (index, point) in points.into_iter().enumerate() {
                let first_radius = (
                    point.point_x as f32 - first_center.0,
                    point.point_y as f32 - first_center.1,
                );
                let second_radius = (
                    point.point_x as f32 - second_center.0,
                    point.point_y as f32 - second_center.1,
                );
                let first_angular_velocity = first.angular_velocity;
                let second_angular_velocity = second.angular_velocity;
                let first_contact_velocity = (
                    (-first_angular_velocity).mul_add(first_radius.1, first.velocity.0),
                    first_angular_velocity.mul_add(first_radius.0, first.velocity.1),
                );
                let second_contact_velocity = (
                    (-second_angular_velocity).mul_add(second_radius.1, second.velocity.0),
                    second_angular_velocity.mul_add(second_radius.0, second.velocity.1),
                );
                let normal_velocity = (second_contact_velocity.0 - first_contact_velocity.0)
                    .mul_add(
                        normal_x,
                        (second_contact_velocity.1 - first_contact_velocity.1) * normal_y,
                    );
                velocity_bias[index] = if normal_velocity < -1.0_f32 {
                    -restitution * normal_velocity
                } else {
                    0.0_f32
                };
            }
            self.contact_velocity_bias.insert(pair, velocity_bias);
        }
    }
}
