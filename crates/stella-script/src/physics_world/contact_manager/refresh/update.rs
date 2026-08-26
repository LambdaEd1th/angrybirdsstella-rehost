//! `b2Contact::Update` touching/manifold transition for one live list node.

use super::NativeContactEventKinematics;
use crate::*;

enum NativeContactUpdate {
    MissingEndpoint,
    Sleeping,
    Retired,
    Separated {
        clear_filter_flag: bool,
    },
    Touching {
        clear_filter_flag: bool,
        manifold: ContactManifold,
        sensor: bool,
        began: bool,
        event_kinematics: Option<NativeContactEventKinematics>,
    },
}

impl RenderBridge {
    /// Read one contact through the same stable fixture/body ownership used by
    /// `b2Contact::Update`. The returned value contains only state that must
    /// survive the immutable borrow before the live contact maps are changed.
    fn prepare_native_contact_update(&self, contact_key: &ContactKey) -> NativeContactUpdate {
        let Some(first) = self.scene.get(&contact_key.0) else {
            return NativeContactUpdate::MissingEndpoint;
        };
        let Some(second) = self.scene.get(&contact_key.1) else {
            return NativeContactUpdate::MissingEndpoint;
        };

        let first_awake = first.moves_during_step() && first.motion_started && !first.sleeping;
        let second_awake = second.moves_during_step() && second.motion_started && !second.sleeping;
        // ContactManager::Collide skips filter, fat-AABB and Contact::Update
        // together when neither non-static endpoint is awake. The old
        // touching bit and manifold consequently remain byte-for-byte live.
        if !first_awake && !second_awake {
            return NativeContactUpdate::Sleeping;
        }

        let filter_dirty = self.contact_filter_dirty.contains(contact_key);
        let filter_allowed = !filter_dirty
            || (Self::native_objects_should_collide(first, second)
                && !self.joints.values().any(|joint| {
                    joint.is_physical
                        && !joint.collide_connected
                        && ((joint.first == contact_key.0 && joint.second == contact_key.1)
                            || (joint.first == contact_key.1 && joint.second == contact_key.0))
                }));
        let allowed = first.active
            && second.active
            && (first.dynamic_body || second.dynamic_body)
            && filter_allowed;
        let fat_overlap = self
            .body_proxy_states
            .get(&contact_key.0)
            .and_then(|state| state.fat_aabbs.get(contact_key.2))
            .zip(
                self.body_proxy_states
                    .get(&contact_key.1)
                    .and_then(|state| state.fat_aabbs.get(contact_key.3)),
            )
            .is_some_and(|(first_fat, second_fat)| {
                first_fat.0 <= second_fat.2
                    && second_fat.0 <= first_fat.2
                    && first_fat.1 <= second_fat.3
                    && second_fat.1 <= first_fat.3
            });
        if !allowed || !fat_overlap {
            return NativeContactUpdate::Retired;
        }

        let clear_filter_flag = filter_dirty && filter_allowed;
        let was_touching = self.active_contacts.get(contact_key).copied();
        let Some(manifold) = first.collision_fixture_manifold(second, contact_key.2, contact_key.3)
        else {
            return NativeContactUpdate::Separated { clear_filter_flag };
        };
        let sensor = first.sensor || second.sensor;
        let began = was_touching.is_none();
        let event_kinematics =
            (began || !sensor).then(|| NativeContactEventKinematics::capture(first, second));
        NativeContactUpdate::Touching {
            clear_filter_flag,
            manifold,
            sensor,
            began,
            event_kinematics,
        }
    }

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
        match self.prepare_native_contact_update(contact_key) {
            NativeContactUpdate::MissingEndpoint => {
                self.broad_phase_contacts.remove(contact_key);
                self.active_contacts.remove(contact_key);
                self.contact_manifolds.remove(contact_key);
                self.contact_impulses.remove(contact_key);
                self.solver_contact_impulses.remove(contact_key);
                self.contact_velocity_bias.remove(contact_key);
                self.remove_native_contact_order(contact_key);
                self.contact_filter_dirty.remove(contact_key);
                None
            }
            NativeContactUpdate::Sleeping => None,
            NativeContactUpdate::Retired => {
                self.broad_phase_contacts.remove(contact_key);
                self.contact_manifolds.remove(contact_key);
                self.contact_impulses.remove(contact_key);
                self.solver_contact_impulses.remove(contact_key);
                self.contact_velocity_bias.remove(contact_key);
                self.remove_native_contact_order(contact_key);
                self.contact_filter_dirty.remove(contact_key);
                let sensor = self.active_contacts.remove(contact_key)?;
                Some(Self::native_contact_end_event(contact_key, sensor))
            }
            NativeContactUpdate::Separated { clear_filter_flag } => {
                // The native Collide loop reaches filter clearing only after
                // its awake gate, so two sleeping bodies retain a dirty flag.
                if clear_filter_flag {
                    self.contact_filter_dirty.remove(contact_key);
                }
                let sensor = self.active_contacts.remove(contact_key)?;
                self.contact_manifolds.remove(contact_key);
                self.contact_impulses.remove(contact_key);
                self.solver_contact_impulses.remove(contact_key);
                self.contact_velocity_bias.remove(contact_key);
                self.wake_contact_bodies(contact_key);
                Some(Self::native_contact_end_event(contact_key, sensor))
            }
            NativeContactUpdate::Touching {
                clear_filter_flag,
                manifold,
                sensor,
                began,
                event_kinematics,
            } => {
                if clear_filter_flag {
                    self.contact_filter_dirty.remove(contact_key);
                }
                self.active_contacts.insert(contact_key.clone(), sensor);
                if sensor {
                    self.contact_manifolds.remove(contact_key);
                } else {
                    self.contact_manifolds.insert(contact_key.clone(), manifold);
                }
                if began {
                    self.wake_contact_bodies(contact_key);
                }
                // A sensor produces only Begin/End records. Every touching
                // solid contact also needs a record for its PostSolve impulse.
                event_kinematics.map(|kinematics| {
                    Self::native_contact_event_from_kinematics(
                        contact_key,
                        manifold,
                        sensor,
                        began,
                        kinematics,
                    )
                })
            }
        }
    }
}
