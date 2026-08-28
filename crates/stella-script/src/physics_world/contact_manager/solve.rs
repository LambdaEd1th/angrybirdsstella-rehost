//! Focused contact-manager solve adapters.

use crate::*;

impl RenderBridge {
    /// Solve the velocity constraints frozen by [`Self::refresh_contacts`]
    /// once. Box2D reuses this exact array for every Gauss-Seidel pass; the
    /// narrow phase and Begin/EndContact callbacks are not rerun per pass.
    #[cfg(test)]
    pub(crate) fn solve_contact_velocity_constraints_once(&mut self) -> BTreeMap<ContactKey, f64> {
        let contact_keys = self.velocity_contacts.keys().cloned().collect::<Vec<_>>();
        self.solve_island_contact_velocity_constraints_once(&contact_keys)
    }

    #[cfg(test)]
    pub(crate) fn solve_island_contact_velocity_constraints_once(
        &mut self,
        contact_keys: &[ContactKey],
    ) -> BTreeMap<ContactKey, f64> {
        self.begin_contact_velocity_cache(contact_keys);
        let impulse_values =
            self.solve_prepared_island_contact_velocity_constraint_values_once(contact_keys);
        self.commit_contact_velocity_cache();
        self.end_contact_velocity_cache();
        contact_keys.iter().cloned().zip(impulse_values).collect()
    }

    pub(crate) fn solve_prepared_island_contact_velocity_constraint_values_once(
        &mut self,
        contact_keys: &[ContactKey],
    ) -> Vec<f64> {
        let mut impulses = Vec::with_capacity(contact_keys.len());
        for key in contact_keys {
            // Sequential Gauss-Seidel deliberately reads the live body
            // velocities changed by every earlier fixture-pair constraint.
            let impulse = self.solve_contact_velocity_constraint(key);
            impulses.push(impulse);
        }
        impulses
    }

    /// One-pass helper retained for focused collision tests. The production
    /// world step calls refresh once, then invokes the velocity solver for all
    /// native iterations without refreshing the contact manager.
    #[cfg(test)]
    pub(crate) fn solve_contacts(&mut self) -> Vec<ContactEvent> {
        let mut events = self.refresh_contacts();
        self.assemble_box2d_islands();
        self.seed_contact_velocity_constraints();
        self.begin_contact_step();
        let impulses = self.solve_contact_velocity_constraints_once();
        let contact_keys = self.velocity_contacts.keys().cloned().collect::<Vec<_>>();
        self.store_island_contact_impulses(&contact_keys);
        for event in &mut events {
            let key = (
                event.first.clone(),
                event.second.clone(),
                event.first_fixture,
                event.second_fixture,
            );
            if let Some(impulse) = impulses.get(&key) {
                event.impulse = *impulse;
            }
        }
        events.retain(|event| event.began || event.ended || event.impulse > f64::EPSILON);
        events
    }
}
