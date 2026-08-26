//! Native world-list creation order and b2BlockAllocator body slots.

use crate::*;

impl RenderBridge {
    pub(crate) fn allocate_physics_creation_order(&mut self) -> u64 {
        let order = self.next_physics_creation_order;
        self.next_physics_creation_order = self.next_physics_creation_order.wrapping_add(1);
        order
    }

    pub(crate) fn insert_native_contact_order(&mut self, key: ContactKey, order: u64) {
        if let Some(previous) = self.contact_creation_order.insert(key.clone(), order) {
            self.native_contact_world_order.remove(&previous);
        }
        self.native_contact_world_order.insert(order, key);
    }

    pub(crate) fn remove_native_contact_order(&mut self, key: &ContactKey) -> Option<u64> {
        let order = self.contact_creation_order.remove(key)?;
        self.native_contact_world_order.remove(&order);
        Some(order)
    }

    pub(crate) fn allocate_body_allocation_slot(&mut self) -> u64 {
        self.free_body_allocation_slots.pop().unwrap_or_else(|| {
            let slot = self.next_body_allocation_slot;
            self.next_body_allocation_slot = self.next_body_allocation_slot.wrapping_add(1);
            slot
        })
    }

    pub(crate) fn release_body_allocation_slot(&mut self, slot: u64) {
        // b2BlockAllocator::Free writes the previous size-class free-list head
        // into the released block and makes that block the new head.
        self.free_body_allocation_slots.push(slot);
    }
}
