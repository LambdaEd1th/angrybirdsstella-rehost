//! Native world-list creation order and b2BlockAllocator body slots.

use crate::*;

impl RenderBridge {
    pub(crate) fn allocate_physics_creation_order(&mut self) -> u64 {
        let order = self.next_physics_creation_order;
        self.next_physics_creation_order = self.next_physics_creation_order.wrapping_add(1);
        order
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
