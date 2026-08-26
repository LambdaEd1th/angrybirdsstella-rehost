//! `b2BroadPhase::UpdatePairs<b2ContactManager>` at `0x10086BDB0`.

use crate::*;

impl RenderBridge {
    pub(crate) fn find_new_broad_phase_contacts(&mut self) {
        let moved = std::mem::take(&mut self.moved_proxy_ids);
        let mut pairs = BTreeMap::new();
        for proxy_id in moved {
            let Some(fat) = self.dynamic_tree.proxy_aabb(proxy_id) else {
                continue;
            };
            let Some((name, fixture)) = self.dynamic_tree.proxy_user_data(proxy_id).cloned() else {
                continue;
            };
            for other_proxy in self.dynamic_tree.query(fat) {
                if proxy_id == other_proxy {
                    continue;
                }
                let Some((other_name, other_fixture)) =
                    self.dynamic_tree.proxy_user_data(other_proxy).cloned()
                else {
                    continue;
                };
                if name == other_name {
                    continue;
                }
                let proxy_pair = if proxy_id <= other_proxy {
                    (proxy_id, other_proxy)
                } else {
                    (other_proxy, proxy_id)
                };
                let contact_key = if name <= other_name {
                    (name.clone(), other_name, fixture, other_fixture)
                } else {
                    (other_name, name.clone(), other_fixture, fixture)
                };
                pairs.entry(proxy_pair).or_insert(contact_key);
            }
        }
        for (_, key) in pairs {
            let allowed = self
                .scene
                .get(&key.0)
                .zip(self.scene.get(&key.1))
                .is_some_and(|(first, second)| {
                    (first.dynamic_body || second.dynamic_body)
                        && Self::native_objects_should_collide(first, second)
                        && !self.joints.values().any(|joint| {
                            joint.is_physical
                                && !joint.collide_connected
                                && ((joint.first == key.0 && joint.second == key.1)
                                    || (joint.first == key.1 && joint.second == key.0))
                        })
                });
            if allowed && self.broad_phase_contacts.insert(key.clone()) {
                let order = self.allocate_physics_creation_order();
                self.insert_native_contact_order(key, order);
            }
        }
        let stale = self
            .contact_creation_order
            .keys()
            .filter(|key| {
                !self.broad_phase_contacts.contains(*key)
                    && !self.active_contacts.contains_key(*key)
            })
            .cloned()
            .collect::<Vec<_>>();
        for key in stale {
            self.remove_native_contact_order(&key);
        }
    }
}
