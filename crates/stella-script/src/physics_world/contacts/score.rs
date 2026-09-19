//! The pre-callback snapshot and post-callback score phase of BeginContact.

use mlua::{Lua, Result as LuaResult, Table, Value};

use crate::*;

fn block_score_table(lua: &Lua) -> LuaResult<Table> {
    let environment = game_environment(lua)?;
    let score_table = required_score_table(environment.raw_get("scoreTable")?, "scoreTable")?;
    required_score_table(score_table.raw_get("blocks")?, "blocks")
}

fn required_score_table(value: Value, index: &str) -> LuaResult<Table> {
    match value {
        Value::Table(table) => Ok(table),
        value => Err(runtime_error(format!(
            "Tried to get a Lua table from index '{index}', but type was {}",
            value.type_name()
        ))),
    }
}

pub(crate) fn native_capture_block_collision_score(lua: &Lua) -> LuaResult<f32> {
    let blocks = block_score_table(lua)?;
    // 0x100063FC0 uses lua_tonumber without a numeric type precondition.
    Ok(native_lua51_number(&blocks.raw_get::<Value>("score")?).unwrap_or(0.0) as f32)
}

pub(crate) fn native_add_block_collision_score(
    lua: &Lua,
    previous_score: f32,
    score_damage: f64,
) -> LuaResult<()> {
    let score_damage = score_damage as f32;
    if score_damage.partial_cmp(&0.0) != Some(std::cmp::Ordering::Greater) {
        return Ok(());
    }
    // 0x100064DB0 reads the retained WorldAttributes *after* blockCollision.
    // Replacing the global table does not rebind the native LuaObject.
    let multiplier = match native_lua_object(lua, NativeLuaObject::WorldAttributes)? {
        Some(attributes) => {
            native_lua51_number(&attributes.raw_get::<Value>("scoreDamageMultiplier")?)
                .unwrap_or(0.0) as f32
        }
        None => 0.0,
    };
    let score = score_damage.floor() * (multiplier as i32) as f32;
    // Re-fetch the destination, but retain the old scalar captured before
    // damage/joint callbacks. Even an increment of zero must write/notify.
    let blocks = block_score_table(lua)?;
    blocks.raw_set("score", f64::from(previous_score + score))?;
    let environment = game_environment(lua)?;
    if let Value::Function(function) = environment.raw_get::<Value>("addScoreToBird")? {
        function.call::<()>(f64::from(score))?;
    }
    Ok(())
}
