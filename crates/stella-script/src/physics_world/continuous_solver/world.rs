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
            // Only the moving endpoint has a distinct start transform. Purple
            // passes the existing fixture pointers plus a compact b2Transform
            // here; no RenderObjectData/body clone is constructed.
            let first_start = sweep_starts
                .get(&key.0)
                .copied()
                .unwrap_or_else(|| NativeSweepStart::capture(first_end));
            let second_start = sweep_starts
                .get(&key.1)
                .copied()
                .unwrap_or_else(|| NativeSweepStart::capture(second_end));
            let first_end_transform = first_end.native_collision_transform();
            let second_end_transform = second_end.native_collision_transform();
            let start_touching = if dynamic_body == key.0 {
                first_end
                    .collision_fixture_manifold_at_transforms(
                        second_end,
                        key.2,
                        key.3,
                        first_end.native_collision_transform_at_sweep(
                            first_start.center,
                            first_start.angle,
                        ),
                        second_end_transform,
                    )
                    .is_some()
            } else {
                first_end
                    .collision_fixture_manifold_at_transforms(
                        second_end,
                        key.2,
                        key.3,
                        first_end_transform,
                        second_end.native_collision_transform_at_sweep(
                            second_start.center,
                            second_start.angle,
                        ),
                    )
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
            let impact_center = (
                center_delta.0.mul_add(alpha, start_center.0),
                center_delta.1.mul_add(alpha, start_center.1),
            );
            let impact_angle = angle_delta.mul_add(alpha, dynamic_start.angle);
            let (first_transform, second_transform) = if dynamic_body == key.0 {
                (
                    first_end.native_collision_transform_at_sweep(impact_center, impact_angle),
                    second_end_transform,
                )
            } else {
                (
                    first_end_transform,
                    second_end.native_collision_transform_at_sweep(impact_center, impact_angle),
                )
            };
            let Some(manifold) = first_end.collision_fixture_manifold_at_transforms(
                second_end,
                key.2,
                key.3,
                first_transform,
                second_transform,
            ) else {
                continue;
            };
            let event =
                Self::native_contact_event(&key, first_end, second_end, manifold, false, true);
            hits.push((
                world_alpha,
                alpha,
                key,
                dynamic_body,
                impact_center,
                impact_angle,
                manifold,
                event,
            ));
        }

        hits.sort_unstable_by(|left, right| {
            left.0
                .total_cmp(&right.0)
                .then_with(|| left.2.cmp(&right.2))
        });
        let (_, alpha, key, dynamic_body, impact_center, impact_angle, manifold, event) =
            hits.into_iter().next()?;
        *toi_state.counts.entry(key.clone()).or_insert(0) += 1;
        if let Some(object) = self.scene.get_mut(&dynamic_body) {
            object.set_native_sweep_transform(impact_center, impact_angle);
            object.wake();
        }
        self.active_contacts.insert(key.clone(), false);
        self.contact_manifolds.insert(key.clone(), manifold);
        self.contact_impulses.remove(&key);
        self.solver_contact_impulses.remove(&key);
        self.contact_velocity_bias.remove(&key);
        self.wake_contact_bodies(&key);
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
        let candidates = self
            .native_contact_world_order
            .iter()
            .rev()
            .map(|(_, key)| key)
            .filter(|candidate| {
                self.broad_phase_contacts.contains(*candidate)
                    && !self.active_contacts.contains_key(*candidate)
                    && (candidate.0 == dynamic_body || candidate.1 == dynamic_body)
            })
            .cloned()
            .collect::<Vec<_>>();
        for extra_key in candidates {
            let (extra_manifold, extra_event) = {
                let Some((extra_first, extra_second)) = self
                    .scene
                    .get(&extra_key.0)
                    .zip(self.scene.get(&extra_key.1))
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
                    || !Self::native_objects_should_collide(extra_first, extra_second)
                {
                    continue;
                }
                let Some(extra_manifold) =
                    extra_first.collision_fixture_manifold(extra_second, extra_key.2, extra_key.3)
                else {
                    continue;
                };
                let extra_event = Self::native_contact_event(
                    &extra_key,
                    extra_first,
                    extra_second,
                    extra_manifold,
                    false,
                    true,
                );
                (extra_manifold, extra_event)
            };
            self.active_contacts.insert(extra_key.clone(), false);
            self.contact_manifolds
                .insert(extra_key.clone(), extra_manifold);
            self.contact_impulses.remove(&extra_key);
            self.solver_contact_impulses.remove(&extra_key);
            self.contact_velocity_bias.remove(&extra_key);
            self.wake_contact_bodies(&extra_key);
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
