//! `b2ContactSolver::StoreImpulses` manifold cache publication.

use crate::*;

impl RenderBridge {
    pub(crate) fn store_island_contact_impulses(&mut self, contact_keys: &[ContactKey]) {
        for pair in contact_keys {
            if let Some(impulse) = self.solver_contact_impulses.get(pair).copied() {
                self.contact_impulses.insert(pair.clone(), impulse);
            }
        }
    }
}
