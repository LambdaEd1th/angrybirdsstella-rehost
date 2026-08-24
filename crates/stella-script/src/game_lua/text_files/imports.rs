//! Lua-visible text/string, JSON import and bundle-copy adapters.

use std::{
    fs,
    path::{Component, Path, PathBuf},
    sync::Arc,
};

use mlua::{Error as LuaError, Lua, LuaSerdeExt, MultiValue, Result as LuaResult, Table, Value};

use super::{paths::resolve_text_table, pipeline::load_text_bytes};
use crate::{
    game_environment, native_required_boolean, native_required_string, prepare_lua_chunk,
    resolve_data_file, runtime_error,
};

pub(crate) fn install_data_imports(
    lua: &Lua,
    globals: &Table,
    data_root: &Arc<PathBuf>,
) -> LuaResult<()> {
    globals.set(
        "importJSONToLuaTable",
        lua.create_function(move |lua, args: MultiValue| {
            let document = native_required_string(&args, 0, "importJSONToLuaTable")?;
            let table_name = native_required_string(&args, 1, "importJSONToLuaTable")?;
            let environment = game_environment(lua)?;
            let target = match environment.get::<Value>(table_name.as_str())? {
                Value::Table(table) => table,
                _ => {
                    return Err(runtime_error(format!(
                        "Tried to get a Lua table from index '{table_name}'"
                    )));
                }
            };
            let json: serde_json::Value = serde_json::from_str(&document)
                .map_err(|error| LuaError::RuntimeError(error.to_string()))?;
            let converted = lua.to_value(&json)?;
            let Value::Table(source) = converted else {
                return Err(runtime_error("JSON root must be a table"));
            };
            for pair in source.pairs::<Value, Value>() {
                let (key, value) = pair?;
                target.raw_set(key, value)?;
            }
            Ok(())
        })?,
    )?;

    let root = Arc::clone(data_root);
    globals.set(
        "native_loadTextFileToLuaTable",
        lua.create_function(move |lua, args: MultiValue| {
            let requested = native_required_string(&args, 0, "native_loadTextFileToLuaTable")?;
            let encrypted = native_required_boolean(&args, 1, "native_loadTextFileToLuaTable")?;
            let parse_json = optional_boolean(&args, 2, "native_loadTextFileToLuaTable")?;
            let decompress = optional_boolean(&args, 3, "native_loadTextFileToLuaTable")?;
            let alternate_key = optional_boolean(&args, 4, "native_loadTextFileToLuaTable")?;
            let bytes = load_text_bytes(&root, &requested, encrypted, alternate_key, decompress)
                .map_err(runtime_error)?;

            if bytes.is_empty() {
                return Ok(Value::Nil);
            }
            if parse_json {
                let json: serde_json::Value = serde_json::from_slice(&bytes)
                    .map_err(|error| LuaError::RuntimeError(error.to_string()))?;
                return lua.to_value(&json);
            }

            // `sub_10052AE8C` passes the complete byte span to Lua 5.1's
            // loadbuffer, executes the compiled chunk and returns its first
            // value in the GameLua environment.
            let prepared = prepare_lua_chunk(&bytes).map_err(runtime_error)?;
            lua.load(&prepared)
                .set_name("")
                .set_environment(game_environment(lua)?)
                .eval::<Value>()
        })?,
    )?;

    let bundle_copy_root = Arc::clone(data_root);
    globals.set(
        "copyFileFromBundleToAppData",
        lua.create_function(move |_, args: MultiValue| {
            let source = native_required_string(&args, 0, "copyFileFromBundleToAppData")?;
            let destination = native_required_string(&args, 1, "copyFileFromBundleToAppData")?;
            let source = resolve_data_file(&bundle_copy_root, &source)
                .ok()
                .or_else(|| resolve_text_table(&bundle_copy_root, &source))
                .ok_or_else(|| runtime_error(format!("bundle file not found: {source}")))?;
            let relative = Path::new(destination.trim_start_matches('/'));
            if relative.components().any(|component| {
                matches!(
                    component,
                    Component::ParentDir | Component::RootDir | Component::Prefix(_)
                )
            }) {
                return Err(runtime_error("unsafe app-data destination"));
            }
            let app_data = bundle_copy_root
                .parent()
                .unwrap_or(bundle_copy_root.as_path())
                .join("appdata");
            let destination = app_data.join(relative);
            if let Some(parent) = destination.parent() {
                fs::create_dir_all(parent).map_err(runtime_error)?;
            }
            fs::copy(source, destination).map_err(runtime_error)?;
            Ok(())
        })?,
    )?;

    Ok(())
}

pub(crate) fn install_string_loader(
    lua: &Lua,
    globals: &Table,
    data_root: &Arc<PathBuf>,
) -> LuaResult<()> {
    let root = Arc::clone(data_root);
    globals.set(
        "loadTextFileToString",
        lua.create_function(move |lua, args: MultiValue| {
            let requested = native_required_string(&args, 0, "loadTextFileToString")?;
            let encrypted = native_required_boolean(&args, 1, "loadTextFileToString")?;
            let alternate_key = native_required_boolean(&args, 2, "loadTextFileToString")?;
            let decompress = native_required_boolean(&args, 3, "loadTextFileToString")?;
            let bytes = load_text_bytes(&root, &requested, encrypted, alternate_key, decompress)
                .map_err(runtime_error)?;
            lua.create_string(&bytes)
        })?,
    )?;
    Ok(())
}

fn optional_boolean(values: &MultiValue, index: usize, function: &str) -> LuaResult<bool> {
    match values.iter().nth(index) {
        None => Ok(false),
        Some(Value::Boolean(value)) => Ok(*value),
        _ => Err(runtime_error(format!(
            "bad argument #{} to '{function}' (boolean expected)",
            index + 1
        ))),
    }
}
