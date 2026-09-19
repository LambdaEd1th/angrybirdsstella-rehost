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
            Value::String(name) => match native_lua_object(lua, NativeLuaObject::BlockTable)? {
                Some(block_table) => match block_table.get::<Value>("damageFactors")? {
                    Value::Table(definitions) => {
                        match definitions.get::<Value>(name.to_string_lossy())? {
                            Value::Table(definition) => Some(definition),
                            _ => None,
                        }
                    }
                    _ => None,
                },
                _ => None,
            },
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
    let Some(world_attributes) = native_lua_object(lua, NativeLuaObject::WorldAttributes)? else {
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
    entry.raw_set("strength", strength)?;
    let dead_blocks = match native_lua_object(lua, NativeLuaObject::DeadBlocks)? {
        Some(table) => table,
        None => {
            // Pre-boot/unit runtimes may invoke the native damage bridge
            // without passing through loadLevelImpl. Preserve the previous
            // safe bootstrap there, then retain the concrete identity exactly
            // as a real level load would.
            let environment = game_environment(lua)?;
            let table = lua.create_table()?;
            environment.set("deadBlocks", table.clone())?;
            retain_native_lua_object(lua, NativeLuaObject::DeadBlocks, Some(&table))?;
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
    native_apply_scaled_collision_damage(lua, target, collision_force as f32, 1.0)
}

pub(crate) fn native_apply_scaled_collision_damage(
    lua: &Lua,
    target: &str,
    base_force: f32,
    damage_multiplier: f32,
) -> LuaResult<NativeDamageResult> {
    let world = object_world(lua)?;
    let Value::Table(entry) = world.raw_get::<Value>(target)? else {
        return Ok(NativeDamageResult::default());
    };
    let ignore_all_damage = matches!(
        entry.raw_get::<Value>("ignoreAllDamage")?,
        Value::Boolean(true)
    );
    if ignore_all_damage {
        // sub_100062520 treats an ignored hit as having entered the damage
        // branch, while preserving strength and reporting zero damage.
        return Ok(NativeDamageResult {
            attempted: true,
            ..NativeDamageResult::default()
        });
    }

    // The threshold uses a separately rounded FMUL, but the eventual raw
    // damage uses FNMSUB at 0x100064938/B64: multiplier*base - defence.
    let collision_force = damage_multiplier * base_force;
    let defence =
        native_lua51_number(&entry.raw_get::<Value>("defence")?).map(|value| value as f32);
    // With a numeric defence, the native FCMP/B.GE only takes the damage
    // branch for an ordered >= comparison. An absent defence skips this
    // comparison entirely and contributes zero to the subtraction.
    if defence.is_some_and(|defence| {
        !matches!(
            collision_force.partial_cmp(&defence),
            Some(std::cmp::Ordering::Equal | std::cmp::Ordering::Greater)
        )
    }) {
        return Ok(NativeDamageResult::default());
    }
    let damage = damage_multiplier.mul_add(base_force, -defence.unwrap_or(0.0));
    let reported_damage = damage.floor();
    let mut result = NativeDamageResult {
        attempted: true,
        reported_damage: f64::from(reported_damage),
        applied_damage: 0.0,
        ..NativeDamageResult::default()
    };
    let old_strength =
        native_lua51_number(&entry.raw_get::<Value>("strength")?).unwrap_or(0.0) as f32;
    let new_strength = old_strength - reported_damage;
    entry.raw_set("strength", f64::from(new_strength))?;
    result.previous_strength = f64::from(old_strength);
    result.remaining_strength = f64::from(new_strength);
    result.destroyed = new_strength <= 0.0;
    // sub_100062520 floors only the strength deduction. Its block-score
    // accumulator receives the full fractional damage while the block lives.
    result.score_damage = Some(if new_strength > 0.0 {
        damage
    } else {
        old_strength
    });
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
