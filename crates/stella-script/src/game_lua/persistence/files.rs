//! GameLua save/load members `sub_10004B394`, `sub_10004B6D0`, and `sub_10004B880`.

use std::{fs, path::Path, path::PathBuf, sync::Arc};

use mlua::{Lua, LuaSerdeExt, MultiValue, Result as LuaResult, Table, Value};

use crate::{
    app_data_path, game_environment, legacy_json_app_data_path, native_required_boolean,
    native_required_string, prepare_lua_chunk, resolve_data_file, runtime_error,
};

use super::messages::queue_persistent_load_message;
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
            let app_path = app_data_path(&load_root, &file_name).map_err(runtime_error)?;
            let path = app_path
                .is_file()
                .then_some(app_path.clone())
                .or_else(|| legacy_json_app_data_path(&load_root, &file_name))
                .or_else(|| resolve_data_file(&load_root, &file_name).ok())
                .unwrap_or(app_path);
            let value = load_persistent_lua_table(lua, &path, &file_name)?;
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
    evaluate_saved_lua_table(lua, path, &bytes, lua.create_table()?)
}

/// GameLua's persistent table reader (10005B7D4), distinct from level loading.
pub(crate) fn load_persistent_lua_table(
    lua: &Lua,
    path: &Path,
    file_name: &str,
) -> LuaResult<Table> {
    // The LuaObject is retained before opening the stream. Neither native
    // catch replaces it or rolls back fields assigned before a runtime error.
    let environment = lua.create_table()?;
    let bytes = match fs::read(path) {
        Ok(bytes) => decode_persistent_lua(bytes),
        Err(error) => {
            queue_persistent_load_message(lua, format!("File Created:{file_name}"));
            if error.kind() != std::io::ErrorKind::NotFound {
                eprintln!("persistent Lua open failed ({file_name}): {error}");
            }
            return Ok(environment);
        }
    };
    match evaluate_saved_lua_table(lua, path, &bytes, environment.clone()) {
        Ok(table) => Ok(table),
        Err(error) => {
            // 10005B940..BB28 queues this exact filename-only message and
            // resumes stream cleanup/table return. Retain detailed host
            // diagnostics too, including when release Lua suppresses warnings.
            let message = format!("Persistent file loading failed {file_name}");
            eprintln!("{message}: {error}");
            queue_persistent_load_message(lua, message);
            Ok(environment)
        }
    }
}

fn evaluate_saved_lua_table(
    lua: &Lua,
    path: &Path,
    bytes: &[u8],
    environment: Table,
) -> LuaResult<Table> {
    // Builds before the native serializer was recovered wrote JSON. Keep that
    // representation readable so existing local saves remain usable.
    if let Ok(json) = serde_json::from_slice::<serde_json::Value>(bytes) {
        return match lua.to_value(&json)? {
            Value::Table(table) => Ok(table),
            _ => Err(runtime_error("saved Lua root is not a table")),
        };
    }

    let prepared = prepare_lua_chunk(bytes).map_err(runtime_error)?;
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
