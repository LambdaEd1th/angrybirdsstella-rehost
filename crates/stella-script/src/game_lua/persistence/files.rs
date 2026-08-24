//! GameLua save/load members `sub_10004B394`, `sub_10004B6D0`, and `sub_10004B880`.

use std::{fs, path::Path, path::PathBuf, sync::Arc};

use mlua::{Lua, LuaSerdeExt, MultiValue, Result as LuaResult, Table, Value};

use crate::{
    app_data_path, game_environment, legacy_json_app_data_path, native_required_boolean,
    native_required_string, prepare_lua_chunk, resolve_data_file, runtime_error,
};

use super::serialization::serialize_table;

pub(crate) fn install_persistent_save(
    lua: &Lua,
    globals: &Table,
    data_root: &Arc<PathBuf>,
) -> LuaResult<()> {
    let root = Arc::clone(data_root);
    globals.set(
        "savePersistentLuaFile",
        lua.create_function(move |lua, args: MultiValue| {
            let file_name = native_required_string(&args, 0, "savePersistentLuaFile")?;
            let table_name = native_required_string(&args, 1, "savePersistentLuaFile")?;
            let environment = game_environment(lua)?;
            let value = environment.get::<Value>(table_name)?;
            let destination = app_data_path(&root, &file_name).map_err(runtime_error)?;
            write_saved_lua_table(&destination, value, true)?;
            Ok(())
        })?,
    )
}

pub(crate) fn install_table_files(
    lua: &Lua,
    globals: &Table,
    data_root: &Arc<PathBuf>,
) -> LuaResult<()> {
    let load_root = Arc::clone(data_root);
    globals.set(
        "loadTableFromFile",
        lua.create_function(move |lua, args: MultiValue| {
            let file_name = native_required_string(&args, 0, "loadTableFromFile")?;
            let table_name = native_required_string(&args, 1, "loadTableFromFile")?;
            let path = app_data_path(&load_root, &file_name)
                .ok()
                .filter(|path| path.is_file())
                .or_else(|| legacy_json_app_data_path(&load_root, &file_name))
                .or_else(|| resolve_data_file(&load_root, &file_name).ok())
                .ok_or_else(|| runtime_error(format!("saved Lua file not found: {file_name}")))?;
            let value = load_saved_lua_table(lua, &path)?;
            let environment = game_environment(lua)?;
            environment.set(table_name.as_str(), value.clone())?;
            lua.globals().set(table_name, value)?;
            Ok(())
        })?,
    )?;

    let save_root = Arc::clone(data_root);
    globals.set(
        "saveLuaFile",
        lua.create_function(move |lua, args: MultiValue| {
            let file_name = native_required_string(&args, 0, "saveLuaFile")?;
            let table_name = native_required_string(&args, 1, "saveLuaFile")?;
            let persistent = native_required_boolean(&args, 2, "saveLuaFile")?;
            let destination = app_data_path(&save_root, &file_name).map_err(runtime_error)?;
            let value = game_environment(lua)?.get::<Value>(table_name.as_str())?;
            write_saved_lua_table(&destination, value, persistent)?;
            Ok(())
        })?,
    )
}

pub(crate) fn load_saved_lua_table(lua: &Lua, path: &Path) -> LuaResult<Table> {
    let bytes = decode_persistent_lua(fs::read(path).map_err(runtime_error)?);

    // Builds before the native serializer was recovered wrote JSON. Keep that
    // representation readable so existing local saves remain usable.
    if let Ok(json) = serde_json::from_slice::<serde_json::Value>(&bytes) {
        return match lua.to_value(&json)? {
            Value::Table(table) => Ok(table),
            _ => Err(runtime_error("saved Lua root is not a table")),
        };
    }

    let prepared = prepare_lua_chunk(&bytes).map_err(runtime_error)?;
    let environment = lua.create_table()?;
    lua.load(&prepared)
        .set_name(path.to_string_lossy())
        .set_environment(environment.clone())
        .exec()?;
    Ok(environment)
}

pub(crate) fn decode_persistent_lua(bytes: Vec<u8>) -> Vec<u8> {
    stella_assets::decrypt_persistent_lua(&bytes)
        .ok()
        .filter(|decoded| {
            decoded.starts_with(&stella_assets::lua::LUA_SIGNATURE)
                || std::str::from_utf8(decoded).is_ok()
        })
        .unwrap_or(bytes)
}

pub(crate) fn write_saved_lua_table(path: &Path, value: Value, persistent: bool) -> LuaResult<()> {
    let Value::Table(table) = value else {
        return Err(runtime_error("saved Lua value is not a table"));
    };
    let mut output = serialize_table(&table)?;
    if persistent {
        output = stella_assets::encrypt_persistent_lua(&output);
    }
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(runtime_error)?;
    }
    fs::write(path, output).map_err(runtime_error)
}
