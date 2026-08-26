use crate::*;

impl RenderBridge {
    pub(crate) fn joint_pending_native_destruction(&self, name: &str) -> bool {
        self.pending_native_joint_destructions
            .iter()
            .any(|pending| pending == name)
    }

    pub(crate) fn attached_joint_names(&self, object: &str) -> Vec<String> {
        self.joints
            .values()
            .filter(|joint| {
                !self.joint_pending_native_destruction(&joint.name)
                    && (joint.first == object || joint.second == object)
            })
            .map(|joint| joint.name.clone())
            .collect()
    }

    fn native_attached_joint_names(&self, object: &str) -> Vec<String> {
        let mut names = self
            .native_joint_world_order
            .iter()
            .rev()
            .filter_map(|(_, name)| self.joints.get(name))
            .filter(|joint| joint.first == object || joint.second == object)
            .map(|joint| joint.name.clone())
            .collect::<Vec<_>>();
        names.extend(
            self.joints
                .values()
                .filter(|joint| {
                    !joint.is_physical && (joint.first == object || joint.second == object)
                })
                .map(|joint| joint.name.clone()),
        );
        names
    }

    pub(crate) fn flag_contacts_for_filtering_between(&mut self, first: &str, second: &str) {
        self.contact_filter_dirty.extend(
            self.broad_phase_contacts
                .iter()
                .filter(|(contact_first, contact_second, _, _)| {
                    (contact_first == first && contact_second == second)
                        || (contact_first == second && contact_second == first)
                })
                .cloned(),
        );
    }

    /// Mirror `b2World::DestroyJoint` at `sub_10086E27C`: physical joint
    /// destruction wakes both bodies, unlinks the constraint, and flags any
    /// surviving contacts between a formerly non-colliding pair. Metadata
    /// destruction links never enter the Box2D path.
    pub(crate) fn destroy_native_joint(&mut self, name: &str) -> Option<PhysicsJoint> {
        // `sub_100062474` is the b2DestructionListener cleanup path. If an
        // explicitly destroyed body/joint reaches Box2D before GameLua's
        // frame-tail drain, erase the stale queued jointData copy as well.
        self.pending_native_joint_destructions
            .retain(|pending| pending != name);
        self.destroy_native_joint_now(name)
    }

    fn destroy_native_joint_now(&mut self, name: &str) -> Option<PhysicsJoint> {
        let joint = self.joints.remove(name)?;
        self.native_joint_world_order
            .remove(&joint.physics_creation_order);
        if joint.is_physical {
            for object_name in [&joint.first, &joint.second] {
                if let Some(object) = self.scene.get_mut(object_name) {
                    object.wake();
                }
            }
            if !joint.collide_connected {
                self.flag_contacts_for_filtering_between(&joint.first, &joint.second);
            }
        }
        Some(joint)
    }

    /// Drain GameLua's pending `jointData` vector in reverse insertion order.
    /// Native `sub_10005E898` performs this after Lua update and particles,
    /// once `b2World::Step` is no longer locked.
    pub(crate) fn drain_pending_native_joint_destructions(&mut self) -> Vec<String> {
        let mut destroyed = Vec::with_capacity(self.pending_native_joint_destructions.len());
        while let Some(name) = self.pending_native_joint_destructions.pop() {
            if self.destroy_native_joint_now(&name).is_some() {
                destroyed.push(name);
            }
        }
        destroyed
    }

    /// Remove every joint attached to `object` and reproduce type 5's native
    /// destruction-link side effect. A one-way link propagates end1 -> end2;
    /// an ordinary link propagates in either direction.
    pub(crate) fn destroy_attached_joint(&mut self, object: &str, name: &str) -> bool {
        let Some(joint) = self
            .joints
            .get(name)
            .filter(|joint| joint.first == object || joint.second == object)
            .cloned()
        else {
            return false;
        };
        if joint.joint_type == 5 && !joint.is_physical {
            let target = if joint.first == object {
                Some(joint.second.as_str())
            } else if !joint.one_way_destroy {
                Some(joint.first.as_str())
            } else {
                None
            };
            if let Some(target) = target
                && target != object
                && self.scene.contains_key(target)
            {
                let timer = joint.destroy_timer.max(0.0);
                self.pending_object_destructions
                    .entry(target.to_owned())
                    .and_modify(|current| *current = current.min(timer))
                    .or_insert(timer);
            }
        }
        self.destroy_native_joint(name).is_some()
    }

    pub(crate) fn detach_object_joints(&mut self, object: &str) {
        // b2World::DestroyBody owns every native edge, including a breakable
        // joint already absent from GameLua's logical vector but still queued
        // for frame-tail destruction.
        for name in self.native_attached_joint_names(object) {
            self.destroy_attached_joint(object, &name);
        }
    }

    pub(crate) fn handle_joint_limit_boundary(&mut self, name: &str, stop: bool) -> Option<f64> {
        if self.joint_pending_native_destruction(name) {
            return None;
        }
        let joint = self.joints.get(name)?.clone();
        if joint.joint_type != 3 || !joint.motor_enabled || !joint.limits_enabled {
            return None;
        }
        let first = self.scene.get(&joint.first)?;
        let second = self.scene.get(&joint.second)?;
        let angle = second.angle - first.angle - joint.rest_angle;
        let speed = joint.motor_speed.unwrap_or(0.0);
        let reached = (speed > 0.0 && angle >= joint.upper_limit)
            || (speed < 0.0 && angle <= joint.lower_limit);
        if !reached {
            return None;
        }
        let new_speed = if stop { 0.0 } else { -speed };
        if let Some(joint) = self.joints.get_mut(name) {
            joint.motor_speed = Some(new_speed);
        }
        for object_name in [&joint.first, &joint.second] {
            if let Some(object) = self.scene.get_mut(object_name) {
                object.motion_started = true;
                object.wake();
            }
        }
        Some(new_speed)
    }

    pub(crate) fn break_joints_attached_to(
        &mut self,
        object: &str,
        collision_force: f64,
    ) -> Vec<String> {
        if !collision_force.is_finite() || collision_force <= 0.0 {
            return Vec::new();
        }
        let mut broken = self
            .joints
            .values()
            .filter(|joint| {
                !self.joint_pending_native_destruction(&joint.name)
                    && joint.breakable
                    && collision_force > joint.break_force
                    && (joint.first == object || joint.second == object)
            })
            .map(|joint| (joint.physics_creation_order, joint.name.clone()))
            .collect::<Vec<_>>();
        // GameLua stores jointData in a vector and `std::remove_if` scans it
        // from begin to end. The Rust map's lexical name order is unrelated.
        broken.sort_unstable_by_key(|(creation_order, _)| *creation_order);
        let broken = broken.into_iter().map(|(_, name)| name).collect::<Vec<_>>();
        for name in &broken {
            // RemovePredicate at `0x10007BC94` calls `sub_10007384C`, which
            // copies the complete 48-byte jointData record into GameLua's
            // pending vector. The b2Joint remains linked to b2World here.
            self.pending_native_joint_destructions.push(name.clone());
        }
        broken
    }
}
