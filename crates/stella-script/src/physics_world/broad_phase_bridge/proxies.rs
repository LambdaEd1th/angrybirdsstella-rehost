//! Fixture proxy creation/destruction and body active-state ownership.

use crate::*;

impl RenderBridge {
    pub(crate) fn release_fixture_proxy_ids(&mut self, proxy_ids: &mut [Option<i32>]) {
        // b2Body's intrusive fixture list is head-inserted, so deactivation
        // and body destruction visit fixtures in reverse creation order.
        for proxy_id in proxy_ids.iter_mut().rev() {
            if let Some(proxy_id) = proxy_id.take() {
                self.moved_proxy_ids.remove(&proxy_id);
                self.dynamic_tree.destroy_proxy(proxy_id);
            }
        }
    }

    pub(crate) fn set_object_active_state(&mut self, name: &str, active: bool) {
        let Some((was_active, fixture_count)) = self
            .scene
            .get(name)
            .map(|object| (object.active, object.collision_shape.fixture_count()))
        else {
            return;
        };
        if was_active == active {
            return;
        }
        if active {
            if let Some(object) = self.scene.get_mut(name) {
                object.fixture_proxy_ids = vec![None; fixture_count];
                object.active = true;
            }
            self.install_object_broad_phase_proxies(name, true);
        } else {
            self.remove_object_broad_phase_proxy_state(name);
            if let Some(object) = self.scene.get_mut(name) {
                object.fixture_proxy_ids = vec![None; fixture_count];
                object.active = false;
            }
        }
    }

    pub(crate) fn remove_object_broad_phase_proxy_state(&mut self, name: &str) {
        let mut proxy_ids = self
            .scene
            .get_mut(name)
            .map(|object| std::mem::take(&mut object.fixture_proxy_ids))
            .unwrap_or_default();
        self.release_fixture_proxy_ids(&mut proxy_ids);
        self.fixture_tight_aabbs
            .retain(|(object, _), _| object != name);
        self.fixture_fat_aabbs
            .retain(|(object, _), _| object != name);
        self.proxy_body_positions.remove(name);
        self.broad_phase_contacts
            .retain(|(first, second, _, _)| first != name && second != name);
        self.contact_filter_dirty
            .retain(|(first, second, _, _)| first != name && second != name);
    }

    pub(crate) fn install_object_broad_phase_proxies(
        &mut self,
        name: &str,
        reverse_fixture_list: bool,
    ) {
        self.remove_object_broad_phase_proxy_state(name);
        let Some(object) = self.scene.get(name) else {
            return;
        };
        if !object.active {
            return;
        }
        let aabbs = object.collision_fixture_aabbs();
        let position = (object.x as f32, object.y as f32);
        self.proxy_body_positions.insert(name.to_owned(), position);
        let mut proxy_ids = vec![None; aabbs.len()];
        let fixtures = if reverse_fixture_list {
            (0..aabbs.len()).rev().collect::<Vec<_>>()
        } else {
            (0..aabbs.len()).collect::<Vec<_>>()
        };
        for fixture in fixtures {
            let tight = aabbs[fixture];
            let proxy_id = self
                .dynamic_tree
                .create_proxy(tight, (name.to_owned(), fixture));
            proxy_ids[fixture] = Some(proxy_id);
            self.fixture_tight_aabbs
                .insert((name.to_owned(), fixture), tight);
            let fat = self
                .dynamic_tree
                .proxy_aabb(proxy_id)
                .expect("new dynamic-tree proxy must be a live leaf");
            self.fixture_fat_aabbs
                .insert((name.to_owned(), fixture), fat);
            self.moved_proxy_ids.insert(proxy_id);
        }
        if let Some(object) = self.scene.get_mut(name) {
            object.fixture_proxy_ids = proxy_ids;
        }
    }

    /// Release one proxy after b2Body::DestroyFixture has unlinked the fixture
    /// and destroyed all of its contacts.
    pub(crate) fn release_object_fixture_proxy(
        &mut self,
        name: &str,
        fixture: usize,
        proxy_id: Option<i32>,
    ) {
        if let Some(proxy_id) = proxy_id {
            self.moved_proxy_ids.remove(&proxy_id);
            self.dynamic_tree.destroy_proxy(proxy_id);
        }
        let key = (name.to_owned(), fixture);
        self.fixture_tight_aabbs.remove(&key);
        self.fixture_fat_aabbs.remove(&key);
        self.broad_phase_contacts.retain(|contact| {
            !((contact.0 == name && contact.2 == fixture)
                || (contact.1 == name && contact.3 == fixture))
        });
        self.contact_filter_dirty.retain(|contact| {
            !((contact.0 == name && contact.2 == fixture)
                || (contact.1 == name && contact.3 == fixture))
        });
        let has_proxy = self
            .scene
            .get(name)
            .is_some_and(|object| object.fixture_proxy_ids.iter().any(Option::is_some));
        if !has_proxy {
            self.proxy_body_positions.remove(name);
        }
    }

    /// Create the proxy for one newly appended fixture while preserving every
    /// preceding DestroyFixture/CreateFixture free-list and move-buffer effect.
    pub(crate) fn install_object_fixture_proxy(&mut self, name: &str, fixture: usize) {
        let Some((active, tight, position)) = self.scene.get(name).and_then(|object| {
            let tight = object.collision_fixture_aabbs().get(fixture).copied()?;
            Some((object.active, tight, (object.x as f32, object.y as f32)))
        }) else {
            return;
        };
        if !active {
            return;
        }
        let proxy_id = self
            .dynamic_tree
            .create_proxy(tight, (name.to_owned(), fixture));
        self.fixture_tight_aabbs
            .insert((name.to_owned(), fixture), tight);
        let fat = self
            .dynamic_tree
            .proxy_aabb(proxy_id)
            .expect("new dynamic-tree proxy must be a live leaf");
        self.fixture_fat_aabbs
            .insert((name.to_owned(), fixture), fat);
        self.moved_proxy_ids.insert(proxy_id);
        self.proxy_body_positions.insert(name.to_owned(), position);
        if let Some(object) = self.scene.get_mut(name) {
            if object.fixture_proxy_ids.len() <= fixture {
                object.fixture_proxy_ids.resize(fixture + 1, None);
            }
            object.fixture_proxy_ids[fixture] = Some(proxy_id);
        }
    }
}
