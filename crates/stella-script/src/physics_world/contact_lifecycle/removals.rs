use crate::*;

impl RenderBridge {
    /// Retire every object owned by the current native level. Purple performs
    /// this directly in `sub_100065D3C` before it opens the requested Lua
    /// level file, including a whole-tree erase of GameLua `+0x310`.
    pub(crate) fn clear_native_level_scene(&mut self) {
        // loadLevelImpl writes begin into the vector's end pointer at
        // 0x100066010, before objects from the next level can register.
        self.aiming_aid_force_sources.clear();
        let names = self.scene.keys().cloned().collect::<Vec<_>>();
        let mut removed = Vec::with_capacity(names.len());
        for name in names {
            removed.extend(self.remove_object_with_destroy_links(&name));
        }
        self.drain_contacts_for_removed_objects(&removed);
        self.scene_render_index.clear();
        self.native_sensor_overlaps.clear();
        self.inside_gravity_objects.clear();
        self.solver_islands.clear();
        self.solver_synchronized_bodies.clear();
        self.native_body_world_order.clear();
        self.native_body_contact_edges.clear();
        self.native_contact_body_orders.clear();
        self.native_body_joint_edges.clear();
        self.native_joint_body_orders.clear();
        self.contact_creation_order.clear();
        self.native_contact_world_order.clear();
        self.pending_object_destructions.clear();
        self.orphaned_native_bodies.clear();
        self.joints.clear();
        self.native_joint_world_order.clear();
        self.pending_native_joint_destructions.clear();
        self.orphaned_native_joints.clear();
        self.tracks.clear();
        self.object_world_identity = None;
    }

    /// Start using a Lua `objects.world` owner, retiring the complete native
    /// scene when the table itself has been replaced. Level restart performs
    /// that replacement and can recreate the same object names before the
    /// next host draw, so key-set synchronization alone cannot see it.
    pub(crate) fn synchronize_object_world_owner(&mut self, identity: usize) -> bool {
        let previous = self.object_world_identity.replace(identity);
        if previous.is_none() || previous == Some(identity) {
            return false;
        }

        self.clear_native_level_scene();
        self.object_world_identity = Some(identity);
        true
    }

    pub(crate) fn remove_object_with_destroy_links(&mut self, object: &str) -> Vec<String> {
        self.remove_one_object_with_destroy_links(object)
            .then(|| object.to_owned())
            .into_iter()
            .collect()
    }

    pub(crate) fn object_game_lua_record_removed(&self, name: &str) -> bool {
        self.orphaned_native_bodies.contains(name)
    }

    /// Resolve the name map exposed by GameLua rather than the backing body
    /// store used by the native solver. A locked DestroyBody orphan remains
    /// in `scene` solely so Box2D can finish the step; native name lookups no
    /// longer return its cleared RenderObjectData record.
    pub(crate) fn game_lua_object(&self, name: &str) -> Option<&SceneObject> {
        (!self.object_game_lua_record_removed(name))
            .then(|| self.scene.get(name))
            .flatten()
    }

    pub(crate) fn game_lua_object_mut(&mut self, name: &str) -> Option<&mut SceneObject> {
        if self.object_game_lua_record_removed(name) {
            return None;
        }
        self.scene.get_mut(name)
    }

    pub(crate) fn game_lua_object_exists(&self, name: &str) -> bool {
        self.game_lua_object(name).is_some()
    }

    /// Mirror the split lifetime produced by `sub_1000674FC` when
    /// b2World::DestroyBody sees e_locked. The native body remains linked to
    /// the world, broad phase, contacts and joints, but the subsequent
    /// removeObject code erases the RenderObjectData name from GameLua's
    /// render tree. Keeping the SceneObject resident supplies the native
    /// solver payload until the whole b2World is destroyed.
    pub(crate) fn orphan_native_body_after_locked_destroy(&mut self, name: &str) -> bool {
        if self.orphaned_native_bodies.contains(name) {
            return false;
        }
        let Some(object) = self.scene.get(name) else {
            return false;
        };
        if !object.has_physics_body() {
            return self.remove_one_object_with_destroy_links(name);
        }

        self.pending_object_destructions.remove(name);
        let z_bucket = native_fcvtzs_f32(object.z_order as f32);
        let sheet = native_scene_sheet_id(object);
        self.scene_render_index.erase_first(z_bucket, sheet, name);
        self.tracks.remove(name);
        self.orphaned_native_bodies.insert(name.to_owned());
        true
    }

    /// Remove one object without recursively consuming zero-delay destruction
    /// links. The Lua host uses this boundary to run Purple's per-joint
    /// callbacks before the attached native joints are destroyed.
    pub(crate) fn remove_one_object_with_destroy_links(&mut self, name: &str) -> bool {
        self.pending_object_destructions.remove(name);
        self.orphaned_native_bodies.remove(name);
        self.remove_object_broad_phase_proxy_state(name);
        let Some(object) = self.scene.remove(name) else {
            return false;
        };
        if object.body_allocation_slot.is_some() {
            self.native_body_world_order
                .remove(&object.physics_creation_order);
            self.native_body_contact_edges
                .remove(&object.physics_creation_order);
            self.native_body_joint_edges
                .remove(&object.physics_creation_order);
        }
        let z_bucket = native_fcvtzs_f32(object.z_order as f32);
        let sheet = native_scene_sheet_id(&object);
        self.scene_render_index.erase_first(z_bucket, sheet, name);
        if let Some(slot) = object.body_allocation_slot {
            self.release_body_allocation_slot(slot);
        }
        self.tracks.remove(name);
        self.detach_object_joints(name);
        true
    }

    pub(crate) fn take_ready_object_destructions(&mut self, delta: f64) -> Vec<String> {
        if delta.is_finite() && delta > 0.0 {
            for timer in self.pending_object_destructions.values_mut() {
                *timer -= delta;
            }
        }
        let ready = self
            .pending_object_destructions
            .iter()
            .filter_map(|(name, timer)| (*timer <= 0.0).then_some(name.clone()))
            .collect::<Vec<_>>();
        for name in &ready {
            self.pending_object_destructions.remove(name);
        }
        ready
    }
}
