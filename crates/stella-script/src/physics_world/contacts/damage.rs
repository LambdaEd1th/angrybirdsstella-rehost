//! Collision-force factors, damage application and block-score propagation.

use mlua::{Lua, Result as LuaResult, Value};

use crate::*;

pub(crate) fn native_collision_force_factors(
    lua: &Lua,
    attacker: &str,
    target: &str,
) -> LuaResult<CollisionForceFactors> {
    let mut factors = CollisionForceFactors {
        force_damage_multiplier: native_force_damage_multiplier(lua)?,
        ..CollisionForceFactors::default()
    };

    let world = object_world(lua)?;
    let attacker_entry = match world.raw_get::<Value>(attacker)? {
        Value::Table(table) => Some(table),
        _ => None,
    };
    let target_entry = match world.raw_get::<Value>(target)? {
        Value::Table(table) => Some(table),
        _ => None,
    };
    let target_material = match target_entry
        .as_ref()
        .map(|entry| entry.get::<Value>("material"))
        .transpose()?
    {
        Some(Value::String(value)) => value.to_string_lossy(),
        _ => String::new(),
    };

    if let Some(entry) = attacker_entry {
        if let Some(value) = value_number(&entry.get::<Value>("powerupDamageMultiplier")?) {
            factors.powerup_damage_multiplier = value;
        }
        let damage_factors = match entry.get::<Value>("damageFactors")? {
            Value::String(name) => {
                let environment = game_environment(lua)?;
                match environment.get::<Value>("blockTable")? {
                    Value::Table(block_table) => {
                        match block_table.get::<Value>("damageFactors")? {
                            Value::Table(definitions) => {
                                match definitions.get::<Value>(name.to_string_lossy())? {
                                    Value::Table(definition) => Some(definition),
                                    _ => None,
                                }
                            }
                            _ => None,
                        }
                    }
                    _ => None,
                }
            }
            _ => None,
        };
        if let Some(damage_factors) = damage_factors {
            for (field, destination) in [
                ("damageMultiplier", &mut factors.damage_multiplier),
                ("velocityMultiplier", &mut factors.velocity_multiplier),
            ] {
                if let Value::Table(values) = damage_factors.get::<Value>(field)?
                    && !target_material.is_empty()
                    && let Some(value) =
                        value_number(&values.get::<Value>(target_material.as_str())?)
                {
                    *destination = value;
                }
            }
        }
    }
    Ok(factors)
}

pub(crate) fn native_force_damage_multiplier(lua: &Lua) -> LuaResult<f64> {
    let environment = game_environment(lua)?;
    let Value::Table(world_attributes) = environment.get::<Value>("worldAttributes")? else {
        return Ok(1.0);
    };
    Ok(value_number(&world_attributes.get::<Value>("forceDamageMultiplier")?).unwrap_or(1.0))
}

pub(crate) fn native_object_flag(lua: &Lua, name: &str, field: &str) -> LuaResult<bool> {
    let world = object_world(lua)?;
    let Value::Table(entry) = world.raw_get::<Value>(name)? else {
        return Ok(false);
    };
    Ok(value_bool(&entry.get::<Value>(field)?).unwrap_or(false))
}

/// Publish a natively destroyed object through the same Lua damage queue used
/// by ordinary collision damage. GameLua's per-object expiry pass writes a
/// float zero to `objects.world[name].strength` and stores the retained object
/// table in `deadBlocks[name]`; the next `removeBlocks` pass owns component
/// disposal and the eventual `removeObject` call.
pub(crate) fn native_queue_dead_block(lua: &Lua, name: &str, strength: f64) -> LuaResult<bool> {
    let world = object_world(lua)?;
    let Value::Table(entry) = world.raw_get::<Value>(name)? else {
        return Ok(false);
    };
    entry.set("strength", strength)?;
    let environment = game_environment(lua)?;
    let dead_blocks = match environment.get::<Value>("deadBlocks")? {
        Value::Table(table) => table,
        _ => {
            let table = lua.create_table()?;
            environment.set("deadBlocks", table.clone())?;
            table
        }
    };
    dead_blocks.raw_set(name, entry)?;
    Ok(true)
}

/// Prepare the synchronous work performed by Purple's native contact
/// listener. Keeping this separate from the island solver makes the
/// ContactManager::Collide timing explicit: damage, bounce state, joint
/// breakage and the Lua callback payloads are all decided from the pre-force
/// body velocities captured in `ContactEvent`.
pub(crate) fn native_apply_collision_damage(
    lua: &Lua,
    target: &str,
    collision_force: f64,
) -> LuaResult<NativeDamageResult> {
    let world = object_world(lua)?;
    let Value::Table(entry) = world.raw_get::<Value>(target)? else {
        return Ok(NativeDamageResult::default());
    };
    let ignore_all_damage = value_bool(&entry.get::<Value>("ignoreAllDamage")?).unwrap_or(false);
    if ignore_all_damage {
        // sub_100062520 treats an ignored hit as having entered the damage
        // branch, while preserving strength and reporting zero damage.
        return Ok(NativeDamageResult {
            attempted: true,
            ..NativeDamageResult::default()
        });
    }

    let defence = value_number(&entry.get::<Value>("defence")?).unwrap_or(0.0) as f32;
    let collision_force = collision_force as f32;
    if collision_force < defence {
        return Ok(NativeDamageResult::default());
    }
    let reported_damage = (collision_force - defence).floor();
    let mut result = NativeDamageResult {
        attempted: true,
        reported_damage: f64::from(reported_damage),
        applied_damage: 0.0,
        ..NativeDamageResult::default()
    };
    if reported_damage <= 0.0 {
        return Ok(result);
    }

    let old_strength = value_number(&entry.get::<Value>("strength")?).unwrap_or(0.0) as f32;
    let new_strength = old_strength - reported_damage;
    entry.set("strength", f64::from(new_strength))?;
    result.previous_strength = f64::from(old_strength);
    result.remaining_strength = f64::from(new_strength);
    result.destroyed = new_strength <= 0.0;
    result.applied_damage = f64::from(if new_strength > 0.0 {
        reported_damage
    } else {
        old_strength.max(0.0)
    });
    if new_strength <= 0.0 {
        native_queue_dead_block(lua, target, f64::from(new_strength))?;
    }
    Ok(result)
}

pub(crate) fn native_add_block_collision_score(lua: &Lua, score_damage: f64) -> LuaResult<()> {
    let score_damage = score_damage as f32;
    if score_damage <= 0.0 {
        return Ok(());
    }
    let environment = game_environment(lua)?;
    let multiplier = match environment.get::<Value>("worldAttributes")? {
        Value::Table(attributes) => {
            value_number(&attributes.get::<Value>("scoreDamageMultiplier")?).unwrap_or(1.0)
        }
        _ => value_number(&environment.get::<Value>("scoreDamageMultiplier")?).unwrap_or(1.0),
    } as f32;
    let score = score_damage.floor() * (multiplier as i32) as f32;
    if score == 0.0 {
        return Ok(());
    }

    if let Value::Table(score_table) = environment.get::<Value>("scoreTable")?
        && let Value::Table(blocks) = score_table.get::<Value>("blocks")?
    {
        let old_score = value_number(&blocks.get::<Value>("score")?).unwrap_or(0.0) as f32;
        blocks.set("score", f64::from(old_score + score))?;
    }
    if let Value::Function(function) = environment.get::<Value>("addScoreToBird")? {
        function.call::<()>(f64::from(score))?;
    }
    Ok(())
}
