use crate::*;

impl RenderBridge {
    pub(crate) fn native_collision_force(
        &self,
        attacker: &str,
        event: &ContactEvent,
        factors: CollisionForceFactors,
    ) -> f64 {
        let (attacker_mass, attacker_x, attacker_y, target_mass, target_x, target_y) =
            if attacker == event.first {
                (
                    event.first_mass,
                    event.first_velocity_x,
                    event.first_velocity_y,
                    event.second_mass,
                    event.second_velocity_x,
                    event.second_velocity_y,
                )
            } else if attacker == event.second {
                (
                    event.second_mass,
                    event.second_velocity_x,
                    event.second_velocity_y,
                    event.first_mass,
                    event.first_velocity_x,
                    event.first_velocity_y,
                )
            } else {
                return 0.0;
            };
        if !attacker_mass.is_finite() || !target_mass.is_finite() {
            return 0.0;
        }
        let force_damage = factors.force_damage_multiplier as f32;
        let attacker_scale = (factors.damage_multiplier * factors.powerup_damage_multiplier) as f32;
        let attacker_mass = attacker_mass as f32;
        let target_mass = target_mass as f32;
        let attacker_x = attacker_x as f32;
        let attacker_y = attacker_y as f32;
        let target_x = target_x as f32;
        let target_y = target_y as f32;

        // sub_100062520+0x11A8..0x11E4 performs these operations as floats:
        // |forceDamage * (m_target*v_target - multiplier*m_attacker*v_attacker)| / 10.
        // GameLua+0x524 is initialized to the literal 10.0 by sub_10002C274.
        // velocityMultiplier is loaded by the same branch, but is used only
        // for the post-destruction velocity path at 0x1000645A8.
        let attacker_factor = force_damage * attacker_scale * attacker_mass;
        let target_factor = force_damage * target_mass;
        let delta_x = target_x * target_factor - attacker_x * attacker_factor;
        let delta_y = target_y * target_factor - attacker_y * attacker_factor;
        f64::from((delta_x * delta_x + delta_y * delta_y).sqrt() / 10.0)
    }

    pub(crate) fn apply_pending_collision_velocities(&mut self) {
        // sub_10005E898 walks GameLua+0x730 immediately after each Box2D
        // step, resolves every name through the live object map, applies the
        // stored b2Vec2 with b2Body::SetLinearVelocity, then empties the map.
        let pending = std::mem::take(&mut self.collision_velocities);
        for (name, (velocity_x, velocity_y)) in pending {
            let Some(object) = self.scene.get_mut(&name) else {
                continue;
            };
            if !object.dynamic_body && !object.kinematic_body {
                continue;
            }
            let velocity_x_f32 = velocity_x as f32;
            let velocity_y_f32 = velocity_y as f32;
            // sub_10005E898 uses one packed FMUL followed by FADDP; both
            // squared lanes round before the addition (there is no FMA).
            if velocity_x_f32 * velocity_x_f32 + velocity_y_f32 * velocity_y_f32 > 0.0_f32 {
                object.motion_started = true;
                object.wake();
            }
            object.velocity_x = velocity_x;
            object.velocity_y = velocity_y;
        }
    }

    pub(crate) fn native_symmetric_collision_force(
        &self,
        event: &ContactEvent,
        force_damage_multiplier: f64,
    ) -> f64 {
        let force_damage = force_damage_multiplier as f32;
        let first_mass = event.first_mass as f32;
        let second_mass = event.second_mass as f32;
        let first_velocity_x = event.first_velocity_x as f32;
        let first_velocity_y = event.first_velocity_y as f32;
        let second_velocity_x = event.second_velocity_x as f32;
        let second_velocity_y = event.second_velocity_y as f32;
        let first_scale = force_damage * first_mass;
        let second_scale = force_damage * second_mass;
        let delta_x = second_velocity_x * second_scale - first_velocity_x * first_scale;
        let delta_y = second_velocity_y * second_scale - first_velocity_y * first_scale;
        f64::from((delta_x * delta_x + delta_y * delta_y).sqrt() / 10.0_f32)
    }

    pub(crate) fn native_two_controllable_collision_force(&self, event: &ContactEvent) -> f64 {
        let first_velocity_x = event.first_velocity_x as f32;
        let first_velocity_y = event.first_velocity_y as f32;
        let second_velocity_x = event.second_velocity_x as f32;
        let second_velocity_y = event.second_velocity_y as f32;
        let first_speed_squared =
            first_velocity_x * first_velocity_x + first_velocity_y * first_velocity_y;
        let second_speed_squared =
            second_velocity_x * second_velocity_x + second_velocity_y * second_velocity_y;
        let momentum = if first_speed_squared > second_speed_squared {
            (event.first_mass as f32) * first_speed_squared.sqrt()
        } else {
            (event.second_mass as f32) * second_speed_squared.sqrt()
        };
        f64::from(momentum / 10.0_f32)
    }

    pub(crate) fn trigger_native_contact_bounce(
        &mut self,
        event: &ContactEvent,
        collision_force: f64,
    ) {
        let both_controllable = self
            .scene
            .get(&event.first)
            .is_some_and(|object| object.controllable)
            && self
                .scene
                .get(&event.second)
                .is_some_and(|object| object.controllable);
        for name in [&event.first, &event.second] {
            if let Some(object) = self.scene.get_mut(name)
                && (!both_controllable || !object.not_collided)
            {
                // P_NOT_COLLIDED is consulted only by the native
                // two-controllable-object collision branch.
                object.trigger_native_bounce(collision_force);
            }
        }
    }
}
