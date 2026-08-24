//! Synchronous Purple BeginContact/EndContact event preparation.

use mlua::{Lua, Result as LuaResult};

use crate::*;

pub(crate) fn prepare_native_contact_callbacks(
    lua: &Lua,
    bridge: &mut RenderBridge,
    contact_events: &[ContactEvent],
) -> LuaResult<(Vec<NativeContactCallback>, Vec<String>)> {
    let mut broken_joints = Vec::new();
    let mut contact_callbacks = Vec::new();
    let force_damage_multiplier = native_force_damage_multiplier(lua)?;
    for event in contact_events {
        if event.ended {
            contact_callbacks.push(NativeContactCallback::Exit {
                first: event.first.clone(),
                second: event.second.clone(),
                sensor: event.sensor,
            });
            continue;
        }
        if event.sensor {
            if event.began {
                contact_callbacks.push(NativeContactCallback::Enter {
                    first: event.first.clone(),
                    second: event.second.clone(),
                });
            }
            continue;
        }
        // sub_100062520 is Purple's BeginContact callback. Damage, bounce and
        // Lua dispatch occur once when the pair begins.
        if !event.began {
            continue;
        }
        let first_controllable = bridge
            .scene
            .get(&event.first)
            .is_some_and(|object| object.controllable);
        let second_controllable = bridge
            .scene
            .get(&event.second)
            .is_some_and(|object| object.controllable);
        if first_controllable && second_controllable {
            let force = bridge.native_two_controllable_collision_force(event);
            bridge.trigger_native_contact_bounce(event, force);
            contact_callbacks.push(NativeContactCallback::Bird {
                first: event.first.clone(),
                second: event.second.clone(),
                force,
                damage: 0.0,
                point_x: event.point_x,
                point_y: event.point_y,
                normal_x: event.normal_x,
                normal_y: event.normal_y,
            });
        } else if first_controllable || second_controllable {
            // A one-controllable collision is reordered so that birdCollision
            // receives the bird first.
            let (attacker, target) = if first_controllable {
                (&event.first, &event.second)
            } else {
                (&event.second, &event.first)
            };
            let factors = native_collision_force_factors(lua, attacker, target)?;
            let force = bridge.native_collision_force(attacker, event, factors);
            bridge.trigger_native_contact_bounce(event, force);
            let target_accepts_collision = bridge
                .scene
                .get(target)
                .is_some_and(|object| object.dynamic_body || object.block_collision_enabled);
            let damage = if target_accepts_collision {
                native_apply_collision_damage(lua, target, force)?
            } else {
                NativeDamageResult::default()
            };
            if damage.destroyed && force > 0.0 {
                let use_legacy_collision_path =
                    native_object_flag(lua, attacker, "useLegacyCollisionPath")?;
                let (attacker_mass, velocity_x, velocity_y) = if attacker == &event.first {
                    (
                        event.first_mass,
                        event.first_velocity_x,
                        event.first_velocity_y,
                    )
                } else {
                    (
                        event.second_mass,
                        event.second_velocity_x,
                        event.second_velocity_y,
                    )
                };
                let collision_force = force as f32;
                let velocity_factor = if use_legacy_collision_path {
                    let denominator = (attacker_mass as f32) * collision_force;
                    ((damage.remaining_strength as f32 / denominator) * 10.0_f32 * -1.75_f32)
                        .min(1.0_f32)
                } else {
                    ((factors.velocity_multiplier as f32)
                        * ((collision_force - damage.previous_strength as f32) / collision_force))
                        .min(1.0_f32)
                };
                let velocity_x = f64::from((velocity_x as f32) * velocity_factor);
                let velocity_y = f64::from((velocity_y as f32) * velocity_factor);
                if use_legacy_collision_path {
                    if let Some(object) = bridge.scene.get_mut(attacker) {
                        object.velocity_x = velocity_x;
                        object.velocity_y = velocity_y;
                        object.motion_started = true;
                        object.wake();
                    }
                } else {
                    bridge
                        .collision_velocities
                        .insert(attacker.clone(), (velocity_x, velocity_y));
                }
            }
            if target_accepts_collision {
                extend_unique_joint_names(
                    &mut broken_joints,
                    bridge.break_joints_attached_to(target, force),
                );
            }
            if let Some(object) = bridge.scene.get_mut(attacker)
                && object.time_since_collision < 0.0
            {
                object.time_since_collision = 0.0;
            }
            contact_callbacks.push(NativeContactCallback::Bird {
                first: attacker.clone(),
                second: target.clone(),
                force,
                damage: damage.applied_damage.floor(),
                point_x: event.point_x,
                point_y: event.point_y,
                normal_x: event.normal_x,
                normal_y: event.normal_y,
            });
        } else {
            // Block/block contacts use one symmetric base force, followed by
            // asymmetric material damage multipliers.
            let force = bridge.native_symmetric_collision_force(event, force_damage_multiplier);
            bridge.trigger_native_contact_bounce(event, force);
            let second_attacks_first =
                native_collision_force_factors(lua, &event.second, &event.first)?;
            let first_attacks_second =
                native_collision_force_factors(lua, &event.first, &event.second)?;
            let first_damage = native_apply_collision_damage(
                lua,
                &event.first,
                f64::from((force as f32) * (second_attacks_first.damage_multiplier as f32)),
            )?;
            let second_damage = native_apply_collision_damage(
                lua,
                &event.second,
                f64::from((force as f32) * (first_attacks_second.damage_multiplier as f32)),
            )?;
            extend_unique_joint_names(
                &mut broken_joints,
                bridge.break_joints_attached_to(&event.first, force),
            );
            extend_unique_joint_names(
                &mut broken_joints,
                bridge.break_joints_attached_to(&event.second, force),
            );
            let first_scores = !bridge
                .scene
                .get(&event.first)
                .is_some_and(|object| object.ignores_score);
            let second_scores = !bridge
                .scene
                .get(&event.second)
                .is_some_and(|object| object.ignores_score);
            contact_callbacks.push(NativeContactCallback::Block {
                first: event.first.clone(),
                second: event.second.clone(),
                force,
                damaged: first_damage.attempted || second_damage.attempted,
                second_damage: second_damage.reported_damage,
                point_x: event.point_x,
                point_y: event.point_y,
                normal_x: event.normal_x,
                normal_y: event.normal_y,
                score_damage: if first_scores {
                    first_damage.applied_damage
                } else {
                    0.0
                } + if second_scores {
                    second_damage.applied_damage
                } else {
                    0.0
                },
            });
        }
    }
    Ok((contact_callbacks, broken_joints))
}

fn extend_unique_joint_names(target: &mut Vec<String>, names: Vec<String>) {
    for name in names {
        if !target.contains(&name) {
            target.push(name);
        }
    }
}
