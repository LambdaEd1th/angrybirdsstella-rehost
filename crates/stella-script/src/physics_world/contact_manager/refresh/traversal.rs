//! `b2ContactManager::Collide` list traversal and live-map cleanup.

use crate::*;

impl RenderBridge {
    /// Begin the native `b2ContactManager::Collide` traversal. `AddPair`
    /// inserts at the list head, so Purple visits the newest contact first.
    /// The returned snapshot is deliberately only the contacts that existed
    /// when Collide began; callbacks cannot make a newly-created fixture join
    /// the traversal already in progress.
    pub(crate) fn begin_contact_manager_refresh(&mut self) -> Vec<ContactKey> {
        // World::Step services the new-fixture move buffer before Collide.
        // Later movement buffers are normally consumed at the end of the
        // preceding island solve.
        self.find_new_broad_phase_contacts();
        self.velocity_contacts.clear();
        self.solver_contact_impulses.clear();
        self.position_contacts.clear();
        self.solver_islands.clear();
        self.solver_synchronized_bodies.clear();
        self.native_contact_world_order
            .iter()
            .rev()
            .map(|(_, key)| key)
            .filter(|key| self.broad_phase_contacts.contains(*key))
            .cloned()
            .collect::<Vec<_>>()
    }

    pub(crate) fn finish_contact_manager_refresh(&mut self) {
        self.contact_impulses
            .retain(|pair, _| self.active_contacts.contains_key(pair));
        self.contact_velocity_bias
            .retain(|pair, _| self.active_contacts.contains_key(pair));
        let stale = self
            .contact_creation_order
            .keys()
            .filter(|pair| {
                !self.active_contacts.contains_key(*pair)
                    && !self.broad_phase_contacts.contains(*pair)
            })
            .cloned()
            .collect::<Vec<_>>();
        for pair in stale {
            self.remove_native_contact_order(&pair);
        }
        self.contact_filter_dirty
            .retain(|pair| self.broad_phase_contacts.contains(pair));
    }

    /// Batch wrapper used by focused physics tests. Production dispatches the
    /// listener after each individual `refresh_native_contact` return so Lua
    /// mutations are visible to the very next contact in native list order.
    #[cfg(test)]
    pub(crate) fn refresh_contacts(&mut self) -> Vec<ContactEvent> {
        let contact_keys = self.begin_contact_manager_refresh();
        let events = contact_keys
            .iter()
            .filter_map(|contact_key| self.refresh_native_contact(contact_key))
            .collect();
        self.finish_contact_manager_refresh();
        events
    }
}
