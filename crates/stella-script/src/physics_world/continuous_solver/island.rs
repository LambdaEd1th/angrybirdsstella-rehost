//! Reduced `b2Island::SolveTOI` position/velocity/integration pass.

use crate::*;

impl RenderBridge {
    /// Solve the reduced TOI island after BeginContact has run. Purple passes
    /// a sub-step of `(1 - alpha) * dt`, disables warm starting and executes
    /// the same ten velocity iterations as the main step before integrating
    /// the remaining sweep.
    pub(crate) fn finish_continuous_tunneling(
        &mut self,
        contacts: &[NativeToiContact],
        step: f64,
        velocity_iterations: usize,
        max_translation: f64,
        max_rotation: f64,
    ) -> (
        BTreeMap<ContactKey, f64>,
        BTreeMap<String, NativeSweepStart>,
    ) {
        let mut impulses = BTreeMap::new();
        let mut sweep_starts = BTreeMap::new();
        let mut constraints = Vec::new();
        for contact in contacts {
            let Some((first, second)) = self
                .scene
                .get(&contact.key.0)
                .cloned()
                .zip(self.scene.get(&contact.key.1).cloned())
            else {
                continue;
            };
            if first.sensor
                || second.sensor
                || !first.active
                || !second.active
                || !Self::native_objects_should_collide(&first, &second)
            {
                continue;
            }
            self.contact_impulses.remove(&contact.key);
            self.solver_contact_impulses.remove(&contact.key);
            self.contact_velocity_bias.remove(&contact.key);
            let constraint =
                PositionContactConstraint::from_manifold(&first, &second, contact.manifold);
            constraints.push((contact, constraint));
            impulses.insert(contact.key.clone(), 0.0_f64);
        }
        for _ in 0..20 {
            let mut solved = true;
            for (contact, constraint) in &constraints {
                if !self.solve_toi_position_constraint(contact, constraint) {
                    solved = false;
                }
            }
            if solved {
                break;
            }
        }
        let velocity_constraints = constraints
            .iter()
            .map(|(contact, _)| (contact.key.clone(), contact.manifold))
            .collect::<Vec<_>>();
        self.begin_toi_contact_step(&velocity_constraints);
        let velocity_contact_keys = velocity_constraints
            .iter()
            .map(|(key, _)| key.clone())
            .collect::<Vec<_>>();
        self.begin_contact_velocity_cache(&velocity_contact_keys);
        for _ in 0..velocity_iterations {
            for (contact, _) in &constraints {
                let Some(first) = self
                    .scene
                    .get(&contact.key.0)
                    .map(|object| ContactBodyState::capture(object, contact.key.2))
                else {
                    continue;
                };
                let Some(second) = self
                    .scene
                    .get(&contact.key.1)
                    .map(|object| ContactBodyState::capture(object, contact.key.3))
                else {
                    continue;
                };
                let impulse = self.solve_contact_velocity_constraint(
                    &contact.key,
                    first,
                    second,
                    contact.manifold,
                );
                if let Some(maximum) = impulses.get_mut(&contact.key) {
                    *maximum = maximum.max(impulse);
                }
            }
        }
        self.commit_contact_velocity_cache();
        self.end_contact_velocity_cache();
        let mut bodies = BTreeSet::new();
        for (contact, _) in &constraints {
            if bodies.insert(contact.dynamic_body.clone())
                && let Some(object) = self.scene.get(&contact.dynamic_body)
            {
                sweep_starts.insert(
                    contact.dynamic_body.clone(),
                    NativeSweepStart::capture(object),
                );
            }
        }
        if let Some((first_contact, _)) = constraints.first() {
            let remaining_step = step * f64::from(1.0_f32 - first_contact.alpha);
            if remaining_step > 0.0 {
                for body in bodies {
                    self.integrate_island_positions(
                        std::slice::from_ref(&body),
                        remaining_step,
                        max_translation,
                        max_rotation,
                    );
                }
            }
        }
        (impulses, sweep_starts)
    }
}
