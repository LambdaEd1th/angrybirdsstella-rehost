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
            self.unlink_native_contact_edges(previous);
        }
        if let Some((first, second)) = self
            .scene
            .get(&key.0)
            .zip(self.scene.get(&key.1))
            .map(|(first, second)| (first.physics_creation_order, second.physics_creation_order))
        {
            self.native_contact_body_orders
                .insert(order, (first, second));
            self.native_body_contact_edges
                .entry(first)
                .or_default()
                .push(order);
            self.native_body_contact_edges
                .entry(second)
                .or_default()
                .push(order);
        }
        self.native_contact_world_order.insert(order, key);
    }

    pub(crate) fn remove_native_contact_order(&mut self, key: &ContactKey) -> Option<u64> {
        let order = self.contact_creation_order.remove(key)?;
        self.native_contact_world_order.remove(&order);
        self.unlink_native_contact_edges(order);
        Some(order)
    }

    fn unlink_native_contact_edges(&mut self, order: u64) {
        let Some((first, second)) = self.native_contact_body_orders.remove(&order) else {
            return;
        };
        for body in [first, second] {
            if let Some(edges) = self.native_body_contact_edges.get_mut(&body) {
                edges.retain(|edge| *edge != order);
            }
        }
    }

    pub(crate) fn insert_native_joint_order(
        &mut self,
        order: u64,
        name: String,
        first: &str,
        second: &str,
    ) {
        if let Some((first, second)) = self
            .scene
            .get(first)
            .zip(self.scene.get(second))
            .map(|(first, second)| (first.physics_creation_order, second.physics_creation_order))
        {
            self.native_joint_body_orders.insert(order, (first, second));
            self.native_body_joint_edges
                .entry(first)
                .or_default()
                .push(order);
            self.native_body_joint_edges
                .entry(second)
                .or_default()
                .push(order);
        }
        self.native_joint_world_order.insert(order, name);
    }

    pub(crate) fn remove_native_joint_order(&mut self, order: u64) {
        self.native_joint_world_order.remove(&order);
        let Some((first, second)) = self.native_joint_body_orders.remove(&order) else {
            return;
        };
        for body in [first, second] {
            if let Some(edges) = self.native_body_joint_edges.get_mut(&body) {
                edges.retain(|edge| *edge != order);
            }
        }
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
