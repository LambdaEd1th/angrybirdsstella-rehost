//! `b2World::SolveTOI` candidate selection and contact-edge expansion.

use crate::*;

impl RenderBridge {
    /// Recover the otherwise invisible contact made by a fast dynamic body
    /// whose fixture is separated at both ends of the discrete step. Purple
    /// reaches this path from b2World::SolveTOI (`sub_10086EA54`) after the
    /// ordinary island solve has synchronized its swept broad-phase proxies.
    ///
    /// Purple's body constructors set `bullet = false` and expose no setter,
    /// so the shipped candidate rule reaches only non-bullet dynamic/static
    /// pairs. Candidate impact fractions come from the float32
    /// GJK/separation-function port of `sub_100861B54`.
    pub(crate) fn advance_continuous_tunneling(
        &mut self,
        sweep_starts: &BTreeMap<String, NativeSweepStart>,
        sweep_alphas: &BTreeMap<String, f32>,
        toi_state: &mut NativeToiStepState,
    ) -> Option<Vec<(NativeToiContact, ContactEvent)>> {
        let mut hits = Vec::new();
        for key in self.broad_phase_contacts.iter().cloned() {
            if self.active_contacts.contains_key(&key) {
                continue;
            }
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
            let dynamic_body = match (
                first_end.dynamic_body,
                second_end.dynamic_body,
                first_end.moves_during_step(),
                second_end.moves_during_step(),
            ) {
                (true, false, true, false) => key.0.clone(),
                (false, true, false, true) => key.1.clone(),
                _ => continue,
            };
            // Only the moving endpoint has a distinct start transform.  Use
            // one temporary collision object for the start-overlap test;
            // the persistent snapshot itself stays the same compact b2Sweep
            // state used by Purple instead of cloning every render object.
            let first_start = sweep_starts
                .get(&key.0)
                .copied()
                .unwrap_or_else(|| NativeSweepStart::capture(first_end));
            let second_start = sweep_starts
                .get(&key.1)
                .copied()
                .unwrap_or_else(|| NativeSweepStart::capture(second_end));
            let start_touching = if dynamic_body == key.0 {
                let mut dynamic_start = first_end.clone();
                dynamic_start.set_native_sweep_transform(first_start.center, first_start.angle);
                dynamic_start
                    .collision_fixture_manifold(second_end, key.2, key.3)
                    .is_some()
            } else {
                let mut dynamic_start = second_end.clone();
                dynamic_start.set_native_sweep_transform(second_start.center, second_start.angle);
                first_end
                    .collision_fixture_manifold(&dynamic_start, key.2, key.3)
                    .is_some()
            };
            if start_touching {
                continue;
            }

            let dynamic_start = if dynamic_body == key.0 {
                first_start
            } else {
                second_start
            };
            let dynamic_end = if dynamic_body == key.0 {
                first_end
            } else {
                second_end
            };
            let start_center = dynamic_start.center;
            let end_center = dynamic_end.native_world_center();
            let center_delta = (end_center.0 - start_center.0, end_center.1 - start_center.1);
            let angle_delta = dynamic_end.angle as f32 - dynamic_start.angle;
            if center_delta.0 == 0.0_f32 && center_delta.1 == 0.0_f32 && angle_delta == 0.0_f32 {
                continue;
            }

            let objects_at = |alpha: f32| {
                let mut first = first_end.clone();
                let mut second = second_end.clone();
                let body = if dynamic_body == key.0 {
                    &mut first
                } else {
                    &mut second
                };
                body.set_native_sweep_transform(
                    (
                        center_delta.0.mul_add(alpha, start_center.0),
                        center_delta.1.mul_add(alpha, start_center.1),
                    ),
                    angle_delta.mul_add(alpha, dynamic_start.angle),
                );
                (first, second)
            };

            let Some(proxy_a) = first_end.native_distance_proxy(key.2) else {
                continue;
            };
            let Some(proxy_b) = second_end.native_distance_proxy(key.3) else {
                continue;
            };
            let alpha_0 = sweep_alphas.get(&dynamic_body).copied().unwrap_or(0.0_f32);
            let (world_alpha, alpha) =
                if let Some(world_alpha) = toi_state.cached_world_alphas.get(&key).copied() {
                    if world_alpha >= 1.0_f32 || alpha_0 >= 1.0_f32 {
                        continue;
                    }
                    (world_alpha, (world_alpha - alpha_0) / (1.0_f32 - alpha_0))
                } else {
                    let Some(alpha) = native_time_of_impact(
                        &proxy_a,
                        NativeSweep::between(first_start, first_end),
                        &proxy_b,
                        NativeSweep::between(second_start, second_end),
                    ) else {
                        toi_state.cached_world_alphas.insert(key.clone(), 1.0_f32);
                        continue;
                    };
                    let world_alpha = (1.0_f32 - alpha_0).mul_add(alpha, alpha_0);
                    toi_state
                        .cached_world_alphas
                        .insert(key.clone(), world_alpha);
                    (world_alpha, alpha)
                };
            let (first, second) = objects_at(alpha);
            let Some(manifold) = first.collision_fixture_manifold(&second, key.2, key.3) else {
                continue;
            };
            hits.push((
                world_alpha,
                alpha,
                key,
                dynamic_body,
                first,
                second,
                manifold,
            ));
        }

        hits.sort_unstable_by(|left, right| {
            left.0
                .total_cmp(&right.0)
                .then_with(|| left.2.cmp(&right.2))
        });
        let (_, alpha, key, dynamic_body, first, second, manifold) = hits.into_iter().next()?;
        *toi_state.counts.entry(key.clone()).or_insert(0) += 1;
        let transform = if dynamic_body == key.0 {
            &first
        } else {
            &second
        };
        if let Some(object) = self.scene.get_mut(&dynamic_body) {
            object.set_native_sweep_transform(
                transform.native_world_center(),
                transform.angle as f32,
            );
            object.wake();
        }
        self.active_contacts.insert(key.clone(), false);
        self.contact_manifolds.insert(key.clone(), manifold);
        self.contact_impulses.remove(&key);
        self.solver_contact_impulses.remove(&key);
        self.contact_velocity_bias.remove(&key);
        self.wake_contact_bodies(&key);
        let event = Self::native_contact_event(&key, &first, &second, manifold, false, true);
        Some(vec![(
            NativeToiContact {
                key,
                dynamic_body,
                alpha,
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
        dynamic_body: &str,
        alpha: f32,
    ) -> Option<(NativeToiContact, ContactEvent)> {
        let mut candidates = self
            .broad_phase_contacts
            .iter()
            .filter(|candidate| {
                !self.active_contacts.contains_key(*candidate)
                    && (candidate.0 == dynamic_body || candidate.1 == dynamic_body)
            })
            .cloned()
            .collect::<Vec<_>>();
        candidates.sort_unstable_by(|left, right| {
            self.contact_creation_order
                .get(right)
                .copied()
                .unwrap_or(0)
                .cmp(&self.contact_creation_order.get(left).copied().unwrap_or(0))
                .then_with(|| left.cmp(right))
        });
        for extra_key in candidates {
            let Some((extra_first, extra_second)) = self
                .scene
                .get(&extra_key.0)
                .cloned()
                .zip(self.scene.get(&extra_key.1).cloned())
            else {
                continue;
            };
            let other_is_static = if extra_key.0 == dynamic_body {
                !extra_second.moves_during_step()
            } else {
                !extra_first.moves_during_step()
            };
            if !other_is_static
                || extra_first.sensor
                || extra_second.sensor
                || !extra_first.active
                || !extra_second.active
                || !Self::native_objects_should_collide(&extra_first, &extra_second)
            {
                continue;
            }
            let Some(extra_manifold) =
                extra_first.collision_fixture_manifold(&extra_second, extra_key.2, extra_key.3)
            else {
                continue;
            };
            self.active_contacts.insert(extra_key.clone(), false);
            self.contact_manifolds
                .insert(extra_key.clone(), extra_manifold);
            self.contact_impulses.remove(&extra_key);
            self.solver_contact_impulses.remove(&extra_key);
            self.contact_velocity_bias.remove(&extra_key);
            self.wake_contact_bodies(&extra_key);
            let extra_event = Self::native_contact_event(
                &extra_key,
                &extra_first,
                &extra_second,
                extra_manifold,
                false,
                true,
            );
            return Some((
                NativeToiContact {
                    key: extra_key,
                    dynamic_body: dynamic_body.to_owned(),
                    alpha,
                    manifold: extra_manifold,
                },
                extra_event,
            ));
        }
        None
    }
}
