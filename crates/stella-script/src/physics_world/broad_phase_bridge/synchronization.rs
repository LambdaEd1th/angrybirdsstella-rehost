//! Fixture synchronization through `0x10086CB74` and MoveProxy.

use crate::*;

impl RenderBridge {
    /// b2Body::SetTransform synchronizes only the fixtures attached to that
    /// body before draining the shared broad-phase move buffer.
    pub(crate) fn sync_native_body_broad_phase(&mut self, name: &str) {
        self.synchronize_native_body_proxies(name);
        self.find_new_broad_phase_contacts();
    }

    pub(crate) fn sync_native_broad_phase(&mut self) {
        let names = self
            .scene
            .iter()
            .filter(|(_, object)| object.active)
            .map(|(name, _)| name.clone())
            .collect::<Vec<_>>();
        for name in names {
            self.synchronize_native_body_proxies(&name);
        }
        self.find_new_broad_phase_contacts();
    }

    fn synchronize_native_body_proxies(&mut self, name: &str) {
        let Some(object) = self.scene.get(name) else {
            return;
        };
        let proxy_ids = object.fixture_proxy_ids.clone();
        let current_aabbs = object.collision_fixture_aabbs();
        let current_position = (object.x as f32, object.y as f32);
        let old_position = self
            .proxy_body_positions
            .get(name)
            .copied()
            .unwrap_or(current_position);
        let displacement = (
            current_position.0 - old_position.0,
            current_position.1 - old_position.1,
        );
        for (fixture, (proxy_id, current)) in proxy_ids.into_iter().zip(current_aabbs).enumerate() {
            let Some(proxy_id) = proxy_id else {
                continue;
            };
            let key = (name.to_owned(), fixture);
            let old = self
                .fixture_tight_aabbs
                .get(&key)
                .copied()
                .unwrap_or(current);
            let swept = (
                old.0.min(current.0),
                old.1.min(current.1),
                old.2.max(current.2),
                old.3.max(current.3),
            );
            if self.dynamic_tree.move_proxy(proxy_id, swept, displacement) {
                let next = self
                    .dynamic_tree
                    .proxy_aabb(proxy_id)
                    .expect("moved dynamic-tree proxy must remain a live leaf");
                self.fixture_fat_aabbs.insert(key.clone(), next);
                self.moved_proxy_ids.insert(proxy_id);
            }
            self.fixture_tight_aabbs.insert(key, current);
        }
        self.proxy_body_positions
            .insert(name.to_owned(), current_position);
    }
}
