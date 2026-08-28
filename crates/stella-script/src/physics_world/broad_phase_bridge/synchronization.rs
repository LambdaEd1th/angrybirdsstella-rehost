//! Fixture synchronization through `0x10086CB74` and MoveProxy.

use crate::*;

impl RenderBridge {
    /// b2Body::SetTransform synchronizes only the fixtures attached to that
    /// body before draining the shared broad-phase move buffer.
    pub(crate) fn sync_native_body_broad_phase(&mut self, name: &str) {
        self.synchronize_native_body_proxies(name);
        self.find_new_broad_phase_contacts();
    }

    /// Synchronize exactly the bodies whose fixtures Box2D marked dirty while
    /// solving an island, then drain the shared broad-phase move buffer once.
    ///
    /// `b2World::Solve` and `b2World::SolveTOI` do not walk every active body:
    /// their post-solve loops visit only the non-static bodies retained in the
    /// corresponding island. Keeping that membership boundary is important
    /// both for native contact timing and for avoiding fixture/AABB work on a
    /// settled level.
    pub(crate) fn sync_native_broad_phase_bodies<'a>(
        &mut self,
        names: impl IntoIterator<Item = &'a str>,
    ) {
        for name in names {
            self.synchronize_native_body_proxies(name);
        }
        self.find_new_broad_phase_contacts();
    }

    /// Full-world synchronization is useful only for focused broad-phase
    /// tests that directly mutate native body fields without going through a
    /// binding or solver. Production paths retain Box2D's island boundary.
    #[cfg(test)]
    pub(crate) fn sync_native_broad_phase(&mut self) {
        let names = self
            .scene
            .iter()
            .filter(|(_, object)| object.active)
            .map(|(name, _)| name.clone())
            .collect::<Vec<_>>();
        self.sync_native_broad_phase_bodies(names.iter().map(String::as_str));
    }

    fn synchronize_native_body_proxies(&mut self, name: &str) {
        let RenderBridge {
            scene,
            body_proxy_states,
            dynamic_tree,
            moved_proxy_ids,
            ..
        } = self;
        let Some(object) = scene.get(name) else {
            return;
        };
        let current_position = (object.x as f32, object.y as f32);
        let Some(state) = body_proxy_states.get_mut(name) else {
            return;
        };
        let old_position = state.position;
        let displacement = (
            current_position.0 - old_position.0,
            current_position.1 - old_position.1,
        );
        for (fixture, proxy_id) in object.fixture_proxy_ids.iter().copied().enumerate() {
            let Some(proxy_id) = proxy_id else {
                continue;
            };
            let Some(current) = object.collision_fixture_aabb(fixture) else {
                continue;
            };
            let old = state.tight_aabbs.get(fixture).copied().unwrap_or(current);
            let swept = (
                old.0.min(current.0),
                old.1.min(current.1),
                old.2.max(current.2),
                old.3.max(current.3),
            );
            if dynamic_tree.move_proxy(proxy_id, swept, displacement) {
                let next = dynamic_tree
                    .proxy_aabb(proxy_id)
                    .expect("moved dynamic-tree proxy must remain a live leaf");
                if state.fat_aabbs.len() <= fixture {
                    state.fat_aabbs.resize(fixture + 1, next);
                }
                state.fat_aabbs[fixture] = next;
                moved_proxy_ids.insert(proxy_id);
            }
            if state.tight_aabbs.len() <= fixture {
                state.tight_aabbs.resize(fixture + 1, current);
            }
            state.tight_aabbs[fixture] = current;
        }
        state.position = current_position;
    }
}
