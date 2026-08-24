//! GameLua table fallback and object-script environments.

use std::path::Path;

use mlua::{Error as LuaError, Lua, Result as LuaResult, Value};

use super::chunks::{execute_script, execute_script_in_with_options};

pub(crate) fn load_script_to_object(
    lua: &Lua,
    data_root: &Path,
    requested: &str,
    parent: Option<mlua::Table>,
    child_name: Option<&str>,
) -> LuaResult<()> {
    load_script_to_object_with_options(lua, data_root, requested, parent, child_name, true, false)
}

pub(crate) fn load_script_to_object_with_options(
    lua: &Lua,
    data_root: &Path,
    requested: &str,
    parent: Option<mlua::Table>,
    child_name: Option<&str>,
    decrypt_persistent: bool,
    decompress: bool,
) -> LuaResult<()> {
    let Some(parent) = parent else {
        return execute_script(lua, data_root, requested);
    };
    if parent.metatable().is_none() {
        install_table_fallback(lua, &parent, game_environment(lua)?)?;
    }

    let environment = if let Some(name) = child_name.filter(|name| !name.is_empty()) {
        let child = match parent.raw_get::<Value>(name)? {
            Value::Table(table) => table,
            _ => lua.create_table()?,
        };
        install_table_fallback(lua, &child, parent.clone())?;
        // GameLua::loadLuaFileToObject (sub_10005761C) publishes the owning
        // GameLua object as an actual child field before running the chunk.
        // A metatable-only lookup is observably different to rawget/save code.
        child.raw_set("gamelua", game_environment(lua)?)?;
        parent.set(name, child.clone())?;
        child
    } else {
        parent
    };

    execute_script_in_with_options(
        lua,
        data_root,
        requested,
        environment,
        decrypt_persistent,
        decompress,
    )
}

pub(crate) fn install_global_fallback(lua: &Lua, environment: &mlua::Table) -> LuaResult<()> {
    install_table_fallback(lua, environment, lua.globals())
}

pub(crate) fn install_table_fallback(
    lua: &Lua,
    environment: &mlua::Table,
    fallback: mlua::Table,
) -> LuaResult<()> {
    let metatable = environment.metatable().unwrap_or(lua.create_table()?);
    metatable.set("__index", fallback)?;
    environment.set_metatable(Some(metatable))?;
    Ok(())
}

pub(crate) fn game_environment(lua: &Lua) -> LuaResult<mlua::Table> {
    match lua.globals().get::<Value>("gamelua")? {
        Value::Table(table) => Ok(table),
        _ => Err(LuaError::RuntimeError(
            "gamelua environment is not installed".to_owned(),
        )),
    }
}
