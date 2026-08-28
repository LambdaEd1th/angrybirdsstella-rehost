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
            let constraint = {
                let Some((first, second)) = self
                    .scene
                    .get(&contact.key.0)
                    .zip(self.scene.get(&contact.key.1))
                else {
                    continue;
                };
                if first.sensor
                    || second.sensor
                    || !first.active
                    || !second.active
                    || !Self::native_objects_should_collide(first, second)
                {
                    continue;
                }
                PositionContactConstraint::from_manifold(first, second, contact.manifold)
            };
            self.solver_contact_impulses.remove(&contact.key);
            self.contact_velocity_bias.remove(&contact.key);
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
            .filter_map(|(contact, _)| {
                let (first, second) = self
                    .scene
                    .get(&contact.key.0)
                    .zip(self.scene.get(&contact.key.1))?;
                Some((
                    contact.key.clone(),
                    contact.manifold.at_native_transforms(
                        first.native_collision_transform(),
                        second.native_collision_transform(),
                    ),
                ))
            })
            .collect::<Vec<_>>();
        self.begin_toi_contact_step(&velocity_constraints);
        let velocity_contact_keys = velocity_constraints
            .iter()
            .map(|(key, _)| key.clone())
            .collect::<Vec<_>>();
        self.begin_contact_velocity_cache(&velocity_contact_keys);
        for _ in 0..velocity_iterations {
            for (contact, _) in &constraints {
                let impulse = self.solve_contact_velocity_constraint(&contact.key);
                if let Some(maximum) = impulses.get_mut(&contact.key) {
                    *maximum = maximum.max(impulse);
                }
            }
        }
        self.commit_contact_velocity_cache();
        self.end_contact_velocity_cache();
        // sub_10086EA54 retains the current TOI island body array, integrates
        // it, then synchronizes fixtures only for entries whose body type is
        // dynamic before one FindNewContacts call. Preserve first island
        // occurrence here; a sorted set would erase that native order.
        let mut seen_bodies = BTreeSet::new();
        let mut bodies = Vec::new();
        for name in contacts
            .iter()
            .flat_map(|contact| [&contact.key.0, &contact.key.1])
        {
            if seen_bodies.insert(name.clone())
                && let Some(object) = self.scene.get(name)
            {
                if object.moves_during_step() {
                    sweep_starts.insert(name.clone(), NativeSweepStart::capture(object));
                }
                bodies.push(name.clone());
            }
        }
        if let Some((first_contact, _)) = constraints.first() {
            let remaining_step = step * f64::from(1.0_f32 - first_contact.alpha);
            if remaining_step > 0.0 {
                for body in &bodies {
                    self.integrate_island_positions(
                        std::slice::from_ref(body),
                        remaining_step,
                        max_translation,
                        max_rotation,
                    );
                }
            }
        }
        let dynamic_bodies = bodies
            .iter()
            .filter(|name| {
                self.scene
                    .get(*name)
                    .is_some_and(|object| object.dynamic_body)
            })
            .map(String::as_str)
            .collect::<Vec<_>>();
        self.sync_native_broad_phase_bodies(dynamic_bodies);
        (impulses, sweep_starts)
    }
}
