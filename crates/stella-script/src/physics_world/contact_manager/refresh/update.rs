//! `b2Contact::Update` touching/manifold transition for one live list node.

use crate::*;

impl RenderBridge {
    /// Advance exactly one contact from the native list. `sub_10086373C`
    /// updates the touching bit, wakes both bodies when it flips, then invokes
    /// BeginContact/EndContact before Collide advances to the next node. This
    /// method therefore mutates the live maps immediately and returns at most
    /// the one listener/solver record belonging to this list node.
    pub(crate) fn refresh_native_contact(
        &mut self,
        contact_key: &ContactKey,
    ) -> Option<ContactEvent> {
        if !self.broad_phase_contacts.contains(contact_key) {
            return None;
        }
        let Some((first, second)) = self
            .scene
            .get(&contact_key.0)
            .cloned()
            .zip(self.scene.get(&contact_key.1).cloned())
        else {
            self.broad_phase_contacts.remove(contact_key);
            self.active_contacts.remove(contact_key);
            self.contact_manifolds.remove(contact_key);
            self.contact_impulses.remove(contact_key);
            self.solver_contact_impulses.remove(contact_key);
            self.contact_velocity_bias.remove(contact_key);
            self.contact_creation_order.remove(contact_key);
            self.contact_filter_dirty.remove(contact_key);
            return None;
        };
        let first_awake = first.moves_during_step() && first.motion_started && !first.sleeping;
        let second_awake = second.moves_during_step() && second.motion_started && !second.sleeping;
        // ContactManager::Collide skips filter, fat-AABB and Contact::Update
        // together when neither non-static endpoint is awake. The old
        // touching bit and manifold consequently remain byte-for-byte live.
        if !first_awake && !second_awake {
            return None;
        }

        let filter_dirty = self.contact_filter_dirty.contains(contact_key);
        let filter_allowed = !filter_dirty
            || (Self::native_objects_should_collide(&first, &second)
                && !self.joints.values().any(|joint| {
                    joint.is_physical
                        && !joint.collide_connected
                        && ((joint.first == contact_key.0 && joint.second == contact_key.1)
                            || (joint.first == contact_key.1 && joint.second == contact_key.0))
                }));
        // The game-side filter at sub_100065488 runs in AddPair and reruns
        // for an existing contact only when Box2D's e_filterFlag is set.
        // Collision/material/group setters do not set it; joint topology does.
        // The native Collide loop reaches this branch only after its awake
        // gate, so a dirty contact shared by two sleeping bodies remains dirty.
        if filter_dirty && filter_allowed {
            self.contact_filter_dirty.remove(contact_key);
        }
        let allowed = first.active
            && second.active
            && (first.dynamic_body || second.dynamic_body)
            && filter_allowed;
        let fat_overlap = self
            .fixture_fat_aabbs
            .get(&(contact_key.0.clone(), contact_key.2))
            .zip(
                self.fixture_fat_aabbs
                    .get(&(contact_key.1.clone(), contact_key.3)),
            )
            .is_some_and(|(first_fat, second_fat)| {
                first_fat.0 <= second_fat.2
                    && second_fat.0 <= first_fat.2
                    && first_fat.1 <= second_fat.3
                    && second_fat.1 <= first_fat.3
            });
        if !allowed || !fat_overlap {
            self.broad_phase_contacts.remove(contact_key);
            self.contact_manifolds.remove(contact_key);
            self.contact_impulses.remove(contact_key);
            self.solver_contact_impulses.remove(contact_key);
            self.contact_velocity_bias.remove(contact_key);
            self.contact_creation_order.remove(contact_key);
            self.contact_filter_dirty.remove(contact_key);
            let sensor = self.active_contacts.remove(contact_key)?;
            return Some(Self::native_contact_end_event(contact_key, sensor));
        }

        let manifold = first.collision_fixture_manifold(&second, contact_key.2, contact_key.3);
        let was_touching = self.active_contacts.get(contact_key).copied();
        let Some(manifold) = manifold else {
            let sensor = was_touching?;
            self.active_contacts.remove(contact_key);
            self.contact_manifolds.remove(contact_key);
            self.contact_impulses.remove(contact_key);
            self.solver_contact_impulses.remove(contact_key);
            self.contact_velocity_bias.remove(contact_key);
            self.wake_contact_bodies(contact_key);
            return Some(Self::native_contact_end_event(contact_key, sensor));
        };

        let sensor = first.sensor || second.sensor;
        let began = was_touching.is_none();
        self.active_contacts.insert(contact_key.clone(), sensor);
        if sensor {
            self.contact_manifolds.remove(contact_key);
        } else {
            self.contact_manifolds.insert(contact_key.clone(), manifold);
        }
        if began {
            self.wake_contact_bodies(contact_key);
        }
        // A sensor produces only Begin/End records. Every touching solid
        // contact also needs a record so the island solver can attach its
        // PostSolve impulse without rerunning the narrow phase.
        (began || !sensor).then(|| {
            Self::native_contact_event(contact_key, &first, &second, manifold, sensor, began)
        })
    }
}
