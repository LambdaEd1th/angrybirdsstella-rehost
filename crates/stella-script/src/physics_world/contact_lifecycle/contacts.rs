use crate::*;

impl RenderBridge {
    /// Destroy contacts attached to bodies whose fixture proxies have just
    /// been removed. `sub_10086B9B8` invokes the game's EndContact listener
    /// before unlinking a touching contact. That listener (`sub_1000653AC`)
    /// wakes both bodies and clears their sleep timers before dispatching the
    /// Lua callbacks. Each body's contact-edge list is head-inserted, so
    /// callbacks follow descending creation order.
    pub(crate) fn drain_contacts_for_invalidated_objects(
        &mut self,
        invalidated: &[String],
        clear_collision_velocities: bool,
    ) -> Vec<(String, String, bool)> {
        if invalidated.is_empty() {
            return Vec::new();
        }
        let invalidated = invalidated.iter().collect::<BTreeSet<_>>();
        let mut ending = self
            .active_contacts
            .keys()
            .filter(|(first, second, _, _)| {
                invalidated.contains(first) || invalidated.contains(second)
            })
            .cloned()
            .collect::<Vec<_>>();
        ending.sort_unstable_by(|left, right| {
            self.contact_creation_order
                .get(right)
                .copied()
                .unwrap_or(0)
                .cmp(&self.contact_creation_order.get(left).copied().unwrap_or(0))
                .then_with(|| left.cmp(right))
        });
        // 0x10086B9EC..0x10086B9F8 calls the listener before either contact
        // edge is unlinked. Its stores at 0x1000653DC..0x100065410 are the
        // reason a sleeping stack falls when Luca removes its glass support.
        // Native removeObject keeps the invalidated endpoint in `scene` until
        // its EndContact callbacks return. Non-callback host synchronization
        // paths may already have erased it, but every surviving endpoint must
        // still be woken.
        for key in &ending {
            self.wake_contact_bodies(key);
        }
        let exits = ending
            .iter()
            .filter_map(|key| {
                self.active_contacts
                    .get(key)
                    .copied()
                    .map(|sensor| (key.0.clone(), key.1.clone(), sensor))
            })
            .collect::<Vec<_>>();
        self.active_contacts.retain(|(first, second, _, _), _| {
            !invalidated.contains(first) && !invalidated.contains(second)
        });
        self.broad_phase_contacts.retain(|(first, second, _, _)| {
            !invalidated.contains(first) && !invalidated.contains(second)
        });
        self.contact_impulses.retain(|(first, second, _, _), _| {
            !invalidated.contains(first) && !invalidated.contains(second)
        });
        self.solver_contact_impulses
            .retain(|(first, second, _, _), _| {
                !invalidated.contains(first) && !invalidated.contains(second)
            });
        self.contact_velocity_bias
            .retain(|(first, second, _, _), _| {
                !invalidated.contains(first) && !invalidated.contains(second)
            });
        self.contact_creation_order
            .retain(|(first, second, _, _), _| {
                !invalidated.contains(first) && !invalidated.contains(second)
            });
        self.contact_filter_dirty.retain(|(first, second, _, _)| {
            !invalidated.contains(first) && !invalidated.contains(second)
        });
        self.contact_manifolds.retain(|(first, second, _, _), _| {
            !invalidated.contains(first) && !invalidated.contains(second)
        });
        self.velocity_contacts.retain(|(first, second, _, _), _| {
            !invalidated.contains(first) && !invalidated.contains(second)
        });
        self.position_contacts.retain(|(first, second, _, _), _| {
            !invalidated.contains(first) && !invalidated.contains(second)
        });
        if clear_collision_velocities {
            for name in invalidated {
                self.collision_velocities.remove(name.as_str());
            }
        }
        exits
    }

    /// Destroy only contacts attached to one fixture. The fixture indices in
    /// a ContactKey are creation-order indices; Dirt destroys the intrusive
    /// list head, so removing the final vector element never renumbers any
    /// surviving key. Contact edges themselves are also head-inserted and are
    /// therefore visited in descending creation order.
    pub(crate) fn drain_contacts_for_destroyed_fixture(
        &mut self,
        name: &str,
        fixture: usize,
    ) -> Vec<(String, String, bool)> {
        let attached = |key: &ContactKey| {
            (key.0 == name && key.2 == fixture) || (key.1 == name && key.3 == fixture)
        };
        let mut ending = self
            .active_contacts
            .keys()
            .filter(|key| attached(key))
            .cloned()
            .collect::<Vec<_>>();
        ending.sort_unstable_by(|left, right| {
            self.contact_creation_order
                .get(right)
                .copied()
                .unwrap_or(0)
                .cmp(&self.contact_creation_order.get(left).copied().unwrap_or(0))
                .then_with(|| left.cmp(right))
        });
        // DestroyFixture reaches the same ContactManager::Destroy listener
        // path as DestroyBody, while both endpoints are still live.
        for key in &ending {
            self.wake_contact_bodies(key);
        }
        let exits = ending
            .iter()
            .filter_map(|key| {
                self.active_contacts
                    .get(key)
                    .copied()
                    .map(|sensor| (key.0.clone(), key.1.clone(), sensor))
            })
            .collect::<Vec<_>>();
        self.active_contacts.retain(|key, _| !attached(key));
        self.broad_phase_contacts.retain(|key| !attached(key));
        self.contact_impulses.retain(|key, _| !attached(key));
        self.solver_contact_impulses.retain(|key, _| !attached(key));
        self.contact_velocity_bias.retain(|key, _| !attached(key));
        self.contact_creation_order.retain(|key, _| !attached(key));
        self.contact_filter_dirty.retain(|key| !attached(key));
        self.contact_manifolds.retain(|key, _| !attached(key));
        self.velocity_contacts.retain(|key, _| !attached(key));
        self.position_contacts.retain(|key, _| !attached(key));
        exits
    }

    /// Destroying a Box2D body invokes EndContact before fixture/body storage
    /// is released, so the Lua-side object records are still live while Purple
    /// dispatches `exitCollision`.
    pub(crate) fn drain_contacts_for_removed_objects(
        &mut self,
        removed: &[String],
    ) -> Vec<(String, String, bool)> {
        self.drain_contacts_for_invalidated_objects(removed, true)
    }
}
