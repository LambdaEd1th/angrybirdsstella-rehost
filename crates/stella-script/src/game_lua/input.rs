//! Per-frame keyboard and pointer edge buffers exposed by `GameLua`.
//!
//! Purple keeps these queries next to its native GameLua lifecycle bridge: the
//! public key maps and the compact `g_*` event arrays are deliberately not the
//! same representation.

use crate::*;

pub(crate) const NATIVE_FRAME_KEYS: [&str; 5] = [
    "LBUTTON",
    "KEY_BACK",
    "KEY_MENU",
    "VOLUME_UP",
    "VOLUME_DOWN",
];

/// `sub_10005E898` writes all five entries on every frame, including false
/// entries for keys whose platform bytes are clear.
pub(crate) fn publish_native_key_state(lua: &Lua) -> LuaResult<()> {
    let environment = game_environment(lua)?;
    for name in ["keyPressed", "keyReleased", "keyHold"] {
        let Value::Table(table) = environment.get::<Value>(name)? else {
            continue;
        };
        for key in NATIVE_FRAME_KEYS {
            if matches!(table.raw_get::<Value>(key)?, Value::Nil) {
                table.raw_set(key, false)?;
            }
        }
    }
    Ok(())
}

pub(crate) fn trace_input_tables(environment: &mlua::Table, phase: &str) -> LuaResult<()> {
    let read = |name: &str| -> (bool, bool, bool) {
        let Ok(Value::Table(table)) = environment.get::<Value>(name) else {
            return (false, false, false);
        };
        (
            table.get::<bool>("LBUTTON").unwrap_or(false),
            table.get::<bool>(1).unwrap_or(false),
            table.raw_len() > 0,
        )
    };
    let key_pressed = read("keyPressed");
    let global_pressed = read("g_keyPressed");
    let key_released = read("keyReleased");
    let global_released = read("g_keyReleased");
    if key_pressed.0 || global_pressed.0 || key_released.0 || global_released.0 {
        eprintln!(
            "input-tables {phase} keyPressed={key_pressed:?} g_keyPressed={global_pressed:?} keyReleased={key_released:?} g_keyReleased={global_released:?}"
        );
    }
    Ok(())
}

pub(crate) fn install_input_queries(lua: &Lua) -> LuaResult<()> {
    let environment = game_environment(lua)?;
    for (function_name, table_name) in [
        ("isKeyPressed", "g_keyPressed"),
        ("isKeyHold", "g_keyHold"),
        ("isKeyReleased", "g_keyReleased"),
    ] {
        environment.set(
            function_name,
            lua.create_function(move |lua, key: Value| {
                let environment = game_environment(lua)?;
                let Value::Table(table) = environment.get::<Value>(table_name)? else {
                    return Ok(false);
                };
                Ok(match table.raw_get::<Value>(key)? {
                    Value::Nil => false,
                    Value::Boolean(value) => value,
                    Value::Integer(value) => value != 0,
                    Value::Number(value) => value != 0.0,
                    _ => true,
                })
            })?,
        )?;
    }
    Ok(())
}

pub(crate) fn set_input_flag(
    lua: &Lua,
    environment: &mlua::Table,
    name: &str,
    key: Value,
    value: bool,
) -> LuaResult<()> {
    let table = match environment.get::<Value>(name)? {
        Value::Table(table) => table,
        _ => lua.create_table()?,
    };
    table.raw_set(key.clone(), value)?;
    // Only native `g_*` buffers carry the compact event list. Publishing its
    // numeric entry in Lua's key map makes MenuManager mistake mouse button 1
    // for a keyboard event and skip pointer delegation for that frame.
    if name.starts_with("g_") {
        if value {
            table.raw_set(1, key)?;
        } else {
            table.raw_set(1, Value::Nil)?;
        }
    } else {
        table.raw_set(1, Value::Nil)?;
    }
    environment.set(name, table.clone())?;
    lua.globals().set(name, table)?;
    Ok(())
}

pub(crate) fn clear_input_edges(lua: &Lua) -> LuaResult<()> {
    let environment = game_environment(lua)?;
    for name in ["keyPressed", "keyReleased"] {
        let Value::Table(table) = environment.get::<Value>(name)? else {
            continue;
        };
        let keys = table
            .clone()
            .pairs::<Value, Value>()
            .map(|pair| pair.map(|(key, _)| key))
            .collect::<LuaResult<Vec<_>>>()?;
        for key in keys {
            table.raw_set(key, false)?;
        }
    }
    for name in [
        "g_keyPressed",
        "g_keyPressedNotBlocked",
        "g_keyReleased",
        "g_keyReleasedNotBlocked",
    ] {
        let Value::Table(table) = environment.get::<Value>(name)? else {
            continue;
        };
        let keys = table
            .clone()
            .pairs::<Value, Value>()
            .map(|pair| pair.map(|(key, _)| key))
            .collect::<LuaResult<Vec<_>>>()?;
        for key in keys {
            table.raw_set(key, Value::Nil)?;
        }
    }
    Ok(())
}
