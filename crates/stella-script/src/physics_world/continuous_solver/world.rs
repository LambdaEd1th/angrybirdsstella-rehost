//! `b2World::SolveTOI` candidate selection and contact-edge expansion.

use crate::*;

impl RenderBridge {
    /// Recover the otherwise invisible contact made by a fast dynamic body
    /// whose fixture is separated at both ends of the discrete step. Purple
    /// reaches this path from b2World::SolveTOI (`sub_10086EA54`) after the
    /// ordinary island solve has synchronized its swept broad-phase proxies.
    ///
    /// Candidate impact fractions come from the float32 GJK/separation-
    /// function port of `sub_100861B54`. Purple accepts a pair when at least
    /// one endpoint is dynamic and either endpoint is a bullet or non-dynamic;
    /// this covers ordinary dynamic/static and dynamic/kinematic contacts as
    /// well as bullet dynamic/dynamic contacts.
    pub(crate) fn advance_continuous_tunneling(
        &mut self,
        sweep_starts: &BTreeMap<String, NativeSweepStart>,
        sweep_alphas: &BTreeMap<String, f32>,
        toi_state: &mut NativeToiStepState,
    ) -> Option<Vec<(NativeToiContact, ContactEvent)>> {
        let candidate_keys = self
            .native_contact_world_order
            .iter()
            .rev()
            .map(|(_, key)| key)
            .filter(|key| self.broad_phase_contacts.contains(*key))
            .cloned()
            .collect::<Vec<_>>();
        let mut selected_world_alpha = 1.0_f32;
        let mut selected = None;
        for key in candidate_keys {
            if toi_state.counts.get(&key).copied().unwrap_or(0) > 8 {
                continue;
            }
            let Some((first_end, second_end)) = self.scene.get(&key.0).zip(self.scene.get(&key.1))
            else {
                continue;
            };
            if first_end.sensor
                || second_end.sensor
                || !first_end.active
                || !second_end.active
                || !Self::native_objects_should_collide(first_end, second_end)
            {
                continue;
            }
            let first_awake =
                first_end.moves_during_step() && first_end.motion_started && !first_end.sleeping;
            let second_awake =
                second_end.moves_during_step() && second_end.motion_started && !second_end.sleeping;
            if !first_awake && !second_awake {
                continue;
            }
            if !first_end.dynamic_body && !second_end.dynamic_body {
                continue;
            }
            if first_end.dynamic_body
                && second_end.dynamic_body
                && !first_end.bullet
                && !second_end.bullet
            {
                continue;
            }
            let first_start = sweep_starts
                .get(&key.0)
                .copied()
                .unwrap_or_else(|| NativeSweepStart::capture(first_end));
            let second_start = sweep_starts
                .get(&key.1)
                .copied()
                .unwrap_or_else(|| NativeSweepStart::capture(second_end));
            let first_alpha_0 = sweep_alphas.get(&key.0).copied().unwrap_or(0.0_f32);
            let second_alpha_0 = sweep_alphas.get(&key.1).copied().unwrap_or(0.0_f32);
            let alpha_0 = first_alpha_0.max(second_alpha_0);
            if alpha_0 >= 1.0_f32 {
                continue;
            }
            let first_start = if first_alpha_0 < alpha_0 {
                let advance = (alpha_0 - first_alpha_0) / (1.0_f32 - first_alpha_0);
                NativeSweep::between(first_start, first_end).advance_pose(advance)
            } else {
                first_start
            };
            let second_start = if second_alpha_0 < alpha_0 {
                let advance = (alpha_0 - second_alpha_0) / (1.0_f32 - second_alpha_0);
                NativeSweep::between(second_start, second_end).advance_pose(advance)
            } else {
                second_start
            };
            let first_delta = (
                first_end.native_world_center().0 - first_start.center.0,
                first_end.native_world_center().1 - first_start.center.1,
                first_end.angle as f32 - first_start.angle,
            );
            let second_delta = (
                second_end.native_world_center().0 - second_start.center.0,
                second_end.native_world_center().1 - second_start.center.1,
                second_end.angle as f32 - second_start.angle,
            );
            if first_delta == (0.0_f32, 0.0_f32, 0.0_f32)
                && second_delta == (0.0_f32, 0.0_f32, 0.0_f32)
            {
                continue;
            }

            let Some(proxy_a) = first_end.native_distance_proxy(key.2) else {
                continue;
            };
            let Some(proxy_b) = second_end.native_distance_proxy(key.3) else {
                continue;
            };
            let (world_alpha, alpha) =
                if let Some(world_alpha) = toi_state.cached_world_alphas.get(&key).copied() {
                    if world_alpha >= 1.0_f32 || alpha_0 >= 1.0_f32 {
                        continue;
                    }
                    (world_alpha, (world_alpha - alpha_0) / (1.0_f32 - alpha_0))
                } else {
                    let NativeToiOutput { state, alpha } = native_time_of_impact(
                        &proxy_a,
                        NativeSweep::between(first_start, first_end),
                        &proxy_b,
                        NativeSweep::between(second_start, second_end),
                    );
                    if state != NativeToiState::Touching {
                        toi_state.cached_world_alphas.insert(key.clone(), 1.0_f32);
                        continue;
                    }
                    let world_alpha = (1.0_f32 - alpha_0).mul_add(alpha, alpha_0);
                    toi_state
                        .cached_world_alphas
                        .insert(key.clone(), world_alpha);
                    (world_alpha, alpha)
                };
            let first_impact = NativeSweep::between(first_start, first_end).advance_pose(alpha);
            let second_impact = NativeSweep::between(second_start, second_end).advance_pose(alpha);
            let first_transform = first_end
                .native_collision_transform_at_sweep(first_impact.center, first_impact.angle);
            let second_transform = second_end
                .native_collision_transform_at_sweep(second_impact.center, second_impact.angle);
            let Some(manifold) = first_end.collision_fixture_manifold_at_transforms(
                second_end,
                key.2,
                key.3,
                first_transform,
                second_transform,
            ) else {
                continue;
            };
            let began = !self.active_contacts.contains_key(&key);
            let event =
                Self::native_contact_event(&key, first_end, second_end, manifold, false, began);
            let hit = (
                world_alpha,
                key,
                first_impact,
                second_impact,
                manifold,
                event,
            );
            // AddPair inserts at the native world-list head and SolveTOI
            // replaces its candidate only for a strictly smaller alpha. Ties
            // therefore retain the newest contact encountered first.
            if world_alpha < selected_world_alpha {
                selected_world_alpha = world_alpha;
                selected = Some(hit);
            }
        }
        let (world_alpha, key, first_impact, second_impact, manifold, event) = selected?;
        *toi_state.counts.entry(key.clone()).or_insert(0) += 1;
        for (name, impact) in [(&key.0, first_impact), (&key.1, second_impact)] {
            if let Some(object) = self.scene.get_mut(name) {
                object.set_native_sweep_transform(impact.center, impact.angle);
                if object.moves_during_step() {
                    object.wake();
                }
            }
        }
        self.active_contacts.insert(key.clone(), false);
        self.contact_manifolds.insert(key.clone(), manifold);
        if event.began {
            self.contact_impulses.remove(&key);
        }
        self.solver_contact_impulses.remove(&key);
        self.contact_velocity_bias.remove(&key);
        if event.began {
            self.wake_contact_bodies(&key);
        }
        Some(vec![(
            NativeToiContact {
                toi_bodies: (key.0.clone(), key.1.clone()),
                key,
                alpha: world_alpha,
                manifold,
            },
            event,
        )])
    }

    /// Continue the selected body's native contact-edge traversal one node at
    /// a time. Production dispatches each returned BeginContact before asking
    /// for the next node, so collision/filter mutations made by Lua affect
    /// the remainder of the same TOI island expansion.
    pub(crate) fn advance_next_toi_auxiliary_contact(
        &mut self,
        pending_toi_bodies: (&str, &str),
        alpha: f32,
        island_contacts: &[ContactKey],
        sweep_starts: &BTreeMap<String, NativeSweepStart>,
        sweep_alphas: &BTreeMap<String, f32>,
    ) -> Option<(NativeToiContact, ContactEvent)> {
        // sub_10086EA54 constructs its scratch island with 64 body slots and
        // 32 contact slots, then stops the contact-edge walk as soon as either
        // count reaches capacity. Every additional TOI body is introduced by
        // a contact, so the contact bound is the reachable limiting case.
        if island_contacts.len() >= 32 {
            return None;
        }
        // b2Island stores the two selected contact endpoints first, followed
        // by every newly reached endpoint. Rebuild that insertion order from
        // the pending contact array so callbacks may resume the native body /
        // contact-edge walk without retaining pointers across the Lua call.
        let mut island_body_set = BTreeSet::new();
        let mut island_bodies = Vec::new();
        for body in [pending_toi_bodies.0, pending_toi_bodies.1]
            .into_iter()
            .chain(
                island_contacts
                    .iter()
                    .flat_map(|contact| [contact.0.as_str(), contact.1.as_str()]),
            )
        {
            if island_body_set.insert(body.to_owned()) {
                island_bodies.push(body.to_owned());
            }
        }
        let candidates = island_bodies
            .iter()
            .filter(|body| {
                self.scene
                    .get(*body)
                    .is_some_and(|object| object.dynamic_body)
            })
            .flat_map(|root| {
                self.native_contact_world_order
                    .iter()
                    .rev()
                    .map(|(_, key)| key)
                    .filter(|candidate| {
                        self.broad_phase_contacts.contains(*candidate)
                            && !island_contacts.contains(*candidate)
                            && (candidate.0 == *root || candidate.1 == *root)
                    })
                    .cloned()
                    .map(|key| (root.clone(), key))
            })
            .collect::<Vec<_>>();
        for (root, extra_key) in candidates {
            let Some(root_object) = self.scene.get(&root) else {
                continue;
            };
            let other_name = if extra_key.0 == root {
                extra_key.1.clone()
            } else {
                extra_key.0.clone()
            };
            let Some(other_object) = self.scene.get(&other_name) else {
                continue;
            };
            if other_object.dynamic_body && !root_object.bullet && !other_object.bullet {
                continue;
            }
            let other_in_island = island_body_set.contains(&other_name);
            let restore_pose = (!other_in_island && other_object.moves_during_step())
                .then(|| NativeSweepStart::capture(other_object));
            let impact_pose = restore_pose.map(|restore| {
                let start = sweep_starts.get(&other_name).copied().unwrap_or(restore);
                let old_alpha = sweep_alphas.get(&other_name).copied().unwrap_or(0.0_f32);
                if old_alpha < alpha {
                    let advance = (alpha - old_alpha) / (1.0_f32 - old_alpha);
                    NativeSweep::between(start, other_object).advance_pose(advance)
                } else {
                    restore
                }
            });
            if let Some(impact) = impact_pose
                && let Some(other) = self.scene.get_mut(&other_name)
            {
                other.set_native_sweep_transform(impact.center, impact.angle);
            }
            let contact = {
                let Some((extra_first, extra_second)) = self
                    .scene
                    .get(&extra_key.0)
                    .zip(self.scene.get(&extra_key.1))
                else {
                    if let Some(restore) = restore_pose
                        && let Some(other) = self.scene.get_mut(&other_name)
                    {
                        other.set_native_sweep_transform(restore.center, restore.angle);
                    }
                    continue;
                };
                if extra_first.sensor
                    || extra_second.sensor
                    || !extra_first.active
                    || !extra_second.active
                    || !Self::native_objects_should_collide(extra_first, extra_second)
                {
                    None
                } else {
                    extra_first
                        .collision_fixture_manifold(extra_second, extra_key.2, extra_key.3)
                        .map(|extra_manifold| {
                            let began = !self.active_contacts.contains_key(&extra_key);
                            let extra_event = Self::native_contact_event(
                                &extra_key,
                                extra_first,
                                extra_second,
                                extra_manifold,
                                false,
                                began,
                            );
                            (extra_manifold, extra_event)
                        })
                }
            };
            let Some((extra_manifold, extra_event)) = contact else {
                if let Some(restore) = restore_pose
                    && let Some(other) = self.scene.get_mut(&other_name)
                {
                    other.set_native_sweep_transform(restore.center, restore.angle);
                }
                continue;
            };
            self.active_contacts.insert(extra_key.clone(), false);
            self.contact_manifolds
                .insert(extra_key.clone(), extra_manifold);
            if extra_event.began {
                self.contact_impulses.remove(&extra_key);
            }
            self.solver_contact_impulses.remove(&extra_key);
            self.contact_velocity_bias.remove(&extra_key);
            if extra_event.began {
                self.wake_contact_bodies(&extra_key);
            }
            if !other_in_island
                && let Some(other) = self.scene.get_mut(&other_name)
                && other.moves_during_step()
            {
                other.wake();
            }
            return Some((
                NativeToiContact {
                    toi_bodies: (
                        pending_toi_bodies.0.to_owned(),
                        pending_toi_bodies.1.to_owned(),
                    ),
                    key: extra_key,
                    alpha,
                    manifold: extra_manifold,
                },
                extra_event,
            ));
        }
        None
    }
}
