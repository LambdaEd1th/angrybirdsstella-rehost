//! World island graph assembly and island-wide sleep.

use crate::*;

mod sleep;

impl RenderBridge {
    /// Assemble the awake Box2D islands after synchronous contact callbacks
    /// have returned. `sub_10086E634` seeds every active awake non-static
    /// body, walks touching non-sensor contacts and active physical joints,
    /// and wakes each connected non-static body before `b2Island::Solve`.
    /// Static bodies terminate traversal, so two dynamic islands resting on
    /// the same platform are not accidentally merged.
    pub(crate) fn assemble_box2d_islands(&mut self) {
        let mut islands = std::mem::take(&mut self.solver_islands);
        islands.clear();
        let mut synchronized_bodies = std::mem::take(&mut self.solver_synchronized_bodies);
        synchronized_bodies.clear();
        {
            // Purple walks the persistent b2ContactEdge/b2JointEdge lists
            // owned by each body. The Rust bridge maintains those lists at
            // contact/joint creation and destruction, so island assembly does
            // not rebuild a host-side graph or compare endpoint names merely
            // to find a body's edges.
            let mut visited_non_static = BTreeSet::<u64>::new();
            let mut selected_contacts = BTreeSet::<u64>::new();
            let mut selected_joints = BTreeSet::<u64>::new();
            let mut stack = Vec::<(u64, &str)>::new();
            // b2World::Solve starts at the intrusive body-list head and
            // follows b2Body+0x68. Reverse creation-index iteration is that
            // exact order; it does not first copy or sort a seed-name vector.
            for (&seed_id, seed) in self.native_body_world_order.iter().rev() {
                let Some(seed_object) = self.scene.get(seed) else {
                    continue;
                };
                if !seed_object.moves_during_step()
                    || !seed_object.active
                    || !seed_object.motion_started
                    || seed_object.sleeping
                    || !visited_non_static.insert(seed_id)
                {
                    continue;
                }
                stack.clear();
                stack.push((seed_id, seed.as_str()));
                let mut island_seen = BTreeSet::<u64>::new();
                let mut island = SolverIsland::default();
                while let Some((body_id, name)) = stack.pop() {
                    if !island_seen.insert(body_id) {
                        continue;
                    }
                    let Some(object) = self.scene.get(name) else {
                        continue;
                    };
                    if !object.active {
                        continue;
                    }
                    island.bodies.push(name.to_owned());

                    // A static endpoint is appended, then terminates this DFS.
                    // Native clears its island bit after Solve so another
                    // dynamic island may share the same platform.
                    if !object.moves_during_step() {
                        continue;
                    }

                    for &contact_order in self
                        .native_body_contact_edges
                        .get(&body_id)
                        .into_iter()
                        .flatten()
                        .rev()
                    {
                        let Some(key) = self.native_contact_world_order.get(&contact_order) else {
                            continue;
                        };
                        if !self.contact_manifolds.contains_key(key)
                            || self.active_contacts.get(key) != Some(&false)
                        {
                            continue;
                        }
                        let other = if key.0 == name {
                            Some(key.1.as_str())
                        } else if key.1 == name {
                            Some(key.0.as_str())
                        } else {
                            None
                        };
                        let Some(other) = other else {
                            continue;
                        };
                        let Some(other_object) = self.scene.get(other) else {
                            continue;
                        };
                        if !other_object.active {
                            continue;
                        }
                        if selected_contacts.insert(contact_order) {
                            island.contacts.push(key.clone());
                        }
                        let other_id = other_object.physics_creation_order;
                        if other_object.moves_during_step() {
                            if visited_non_static.insert(other_id) {
                                stack.push((other_id, other));
                            }
                        } else if !island_seen.contains(&other_id) {
                            stack.push((other_id, other));
                        }
                    }

                    for &joint_order in self
                        .native_body_joint_edges
                        .get(&body_id)
                        .into_iter()
                        .flatten()
                        .rev()
                    {
                        let Some(joint) = self
                            .native_joint_world_order
                            .get(&joint_order)
                            .and_then(|name| self.joints.get(name))
                        else {
                            continue;
                        };
                        let other = if joint.first == name {
                            Some(joint.second.as_str())
                        } else if joint.second == name {
                            Some(joint.first.as_str())
                        } else {
                            None
                        };
                        let Some(other) = other else {
                            continue;
                        };
                        let Some(other_object) = self.scene.get(other) else {
                            continue;
                        };
                        if !other_object.active {
                            continue;
                        }
                        if selected_joints.insert(joint_order) {
                            island.joints.push(joint.name.clone());
                        }
                        let other_id = other_object.physics_creation_order;
                        if other_object.moves_during_step() {
                            if visited_non_static.insert(other_id) {
                                stack.push((other_id, other));
                            }
                        } else if !island_seen.contains(&other_id) {
                            stack.push((other_id, other));
                        }
                    }
                }
                if !island.bodies.is_empty() {
                    islands.push(island);
                }
            }

            synchronized_bodies.extend(
                self.native_body_world_order
                    .iter()
                    .rev()
                    .filter(|(order, _)| visited_non_static.contains(*order))
                    .map(|(_, name)| name.clone()),
            );
        }

        // The graph above is deliberately immutable. Wake selected live
        // non-static bodies after its borrowed pointer view expires and before
        // the first island solve; no callback or solver work can observe a
        // different order at this boundary.
        for name in islands.iter().flat_map(|island| &island.bodies) {
            if let Some(object) = self.scene.get_mut(name)
                && object.moves_during_step()
                && object.sleeping
            {
                object.wake();
            }
        }

        self.velocity_contacts = islands
            .iter()
            .flat_map(|island| &island.contacts)
            .filter_map(|key| {
                self.contact_manifolds
                    .get(key)
                    .copied()
                    .map(|manifold| (key.clone(), manifold))
            })
            .collect();
        // b2ContactSolver snapshots local manifold witnesses during island
        // construction, before gravity integration and body movement.
        self.position_contacts = self
            .velocity_contacts
            .iter()
            .filter_map(|(key, manifold)| {
                self.scene
                    .get(&key.0)
                    .zip(self.scene.get(&key.1))
                    .map(|(first, second)| {
                        (
                            key.clone(),
                            PositionContactConstraint::from_manifold(first, second, *manifold),
                        )
                    })
            })
            .collect();
        self.solver_synchronized_bodies = synchronized_bodies;
        self.solver_islands = islands;
    }
}
