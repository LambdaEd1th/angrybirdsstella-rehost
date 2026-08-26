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
            // Purple walks the stable contact/joint edge pointers owned by each
            // body and appends only selected pointers to b2Island scratch
            // arrays. Borrow the equivalent world records here: copying the
            // complete contact/joint graph (and every endpoint string) is not
            // part of `sub_10086E634`.
            let contact_keys = self
                .native_contact_world_order
                .iter()
                .rev()
                .map(|(_, key)| key)
                .filter(|key| {
                    self.contact_manifolds.contains_key(*key)
                        && self.active_contacts.get(*key) == Some(&false)
                })
                .collect::<Vec<_>>();
            let joints = self
                .native_joint_world_order
                .iter()
                .rev()
                .filter_map(|(_, name)| self.joints.get(name))
                .collect::<Vec<_>>();
            // b2Body owns contact/joint edge lists. Reconstruct the pointer
            // topology once in native creation order with borrowed name keys.
            let mut contact_edges = BTreeMap::<&str, Vec<usize>>::new();
            for (index, key) in contact_keys.iter().enumerate() {
                contact_edges.entry(key.0.as_str()).or_default().push(index);
                contact_edges.entry(key.1.as_str()).or_default().push(index);
            }
            let mut joint_edges = BTreeMap::<&str, Vec<usize>>::new();
            for (index, joint) in joints.iter().enumerate() {
                joint_edges
                    .entry(joint.first.as_str())
                    .or_default()
                    .push(index);
                joint_edges
                    .entry(joint.second.as_str())
                    .or_default()
                    .push(index);
            }

            let mut visited_non_static = BTreeSet::<&str>::new();
            let mut selected_contacts = vec![false; contact_keys.len()];
            let mut selected_joints = vec![false; joints.len()];
            let mut stack = Vec::<&str>::new();
            // b2World::Solve starts at the intrusive body-list head and
            // follows b2Body+0x68. Reverse creation-index iteration is that
            // exact order; it does not first copy or sort a seed-name vector.
            for (_, seed) in self.native_body_world_order.iter().rev() {
                let Some(seed_object) = self.scene.get(seed) else {
                    continue;
                };
                if !seed_object.moves_during_step()
                    || !seed_object.active
                    || !seed_object.motion_started
                    || seed_object.sleeping
                    || !visited_non_static.insert(seed.as_str())
                {
                    continue;
                }
                stack.clear();
                stack.push(seed.as_str());
                let mut island_seen = BTreeSet::<&str>::new();
                let mut island = SolverIsland::default();
                while let Some(name) = stack.pop() {
                    if !island_seen.insert(name) {
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

                    for &edge_index in contact_edges.get(name).into_iter().flatten() {
                        let key = contact_keys[edge_index];
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
                        if !selected_contacts[edge_index] {
                            selected_contacts[edge_index] = true;
                            island.contacts.push(key.clone());
                        }
                        if other_object.moves_during_step() {
                            if visited_non_static.insert(other) {
                                stack.push(other);
                            }
                        } else if !island_seen.contains(other) {
                            stack.push(other);
                        }
                    }

                    for &edge_index in joint_edges.get(name).into_iter().flatten() {
                        let joint = joints[edge_index];
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
                        if !selected_joints[edge_index] {
                            selected_joints[edge_index] = true;
                            island.joints.push(joint.name.clone());
                        }
                        if other_object.moves_during_step() {
                            if visited_non_static.insert(other) {
                                stack.push(other);
                            }
                        } else if !island_seen.contains(other) {
                            stack.push(other);
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
                    .filter(|(_, name)| visited_non_static.contains(name.as_str()))
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
