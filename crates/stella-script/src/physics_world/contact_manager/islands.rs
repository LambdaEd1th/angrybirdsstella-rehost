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
        let mut contact_keys = self
            .contact_manifolds
            .keys()
            .filter(|key| self.active_contacts.get(*key) == Some(&false))
            .cloned()
            .collect::<Vec<_>>();
        contact_keys.sort_unstable_by(|left, right| {
            self.contact_creation_order
                .get(right)
                .copied()
                .unwrap_or(0)
                .cmp(&self.contact_creation_order.get(left).copied().unwrap_or(0))
                .then_with(|| left.cmp(right))
        });
        let mut joints = self
            .joints
            .values()
            .filter(|joint| joint.is_physical)
            .cloned()
            .collect::<Vec<_>>();
        joints.sort_unstable_by(|left, right| {
            right
                .physics_creation_order
                .cmp(&left.physics_creation_order)
                .then_with(|| left.name.cmp(&right.name))
        });
        // b2Body owns contact/joint edge lists.  Reconstruct those lists once
        // in the same creation order instead of rescanning every world edge
        // for every body visited by the DFS.
        let mut contact_edges = BTreeMap::<String, Vec<usize>>::new();
        for (index, key) in contact_keys.iter().enumerate() {
            contact_edges.entry(key.0.clone()).or_default().push(index);
            contact_edges.entry(key.1.clone()).or_default().push(index);
        }
        let mut joint_edges = BTreeMap::<String, Vec<usize>>::new();
        for (index, joint) in joints.iter().enumerate() {
            joint_edges
                .entry(joint.first.clone())
                .or_default()
                .push(index);
            joint_edges
                .entry(joint.second.clone())
                .or_default()
                .push(index);
        }
        let mut seeds = self
            .scene
            .iter()
            .filter(|(_, object)| {
                object.moves_during_step()
                    && object.active
                    && object.motion_started
                    && !object.sleeping
            })
            .map(|(name, _)| name.clone())
            .collect::<Vec<_>>();
        seeds.sort_unstable_by(|left, right| {
            self.scene[right]
                .physics_creation_order
                .cmp(&self.scene[left].physics_creation_order)
                .then_with(|| left.cmp(right))
        });

        let mut visited_non_static = BTreeSet::new();
        let mut selected_contacts = BTreeSet::new();
        let mut selected_joints = BTreeSet::new();
        let mut islands = Vec::new();
        for seed in seeds {
            if !visited_non_static.insert(seed.clone()) {
                continue;
            }
            let mut stack = vec![seed];
            let mut island_seen = BTreeSet::new();
            let mut island = SolverIsland::default();
            while let Some(name) = stack.pop() {
                if !island_seen.insert(name.clone()) {
                    continue;
                }
                let Some(object) = self.scene.get_mut(&name) else {
                    continue;
                };
                if !object.active {
                    continue;
                }
                if object.moves_during_step() && object.sleeping {
                    object.wake();
                }
                island.bodies.push(name.clone());

                // b2World::Solve adds a static endpoint to the island body
                // array, then stops traversal through it. Its island flag is
                // cleared after Solve so another island may share it.
                if !object.moves_during_step() {
                    continue;
                }

                for &edge_index in contact_edges.get(&name).into_iter().flatten() {
                    let key = &contact_keys[edge_index];
                    let other = if key.0 == name {
                        Some(&key.1)
                    } else if key.1 == name {
                        Some(&key.0)
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
                    if selected_contacts.insert(key.clone()) {
                        island.contacts.push(key.clone());
                    }
                    if other_object.moves_during_step() {
                        if visited_non_static.insert(other.clone()) {
                            stack.push(other.clone());
                        }
                    } else if !island_seen.contains(other) {
                        stack.push(other.clone());
                    }
                }

                for &edge_index in joint_edges.get(&name).into_iter().flatten() {
                    let joint = &joints[edge_index];
                    let other = if joint.first == name {
                        Some(&joint.second)
                    } else if joint.second == name {
                        Some(&joint.first)
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
                    if selected_joints.insert(joint.name.clone()) {
                        island.joints.push(joint.name.clone());
                    }
                    if other_object.moves_during_step() {
                        if visited_non_static.insert(other.clone()) {
                            stack.push(other.clone());
                        }
                    } else if !island_seen.contains(other) {
                        stack.push(other.clone());
                    }
                }
            }
            if !island.bodies.is_empty() {
                islands.push(island);
            }
        }

        self.velocity_contacts = selected_contacts
            .iter()
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
        self.solver_islands = islands;
    }
}
