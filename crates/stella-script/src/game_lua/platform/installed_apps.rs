//! Discontinued installed-iOS-application lookup service.

use mlua::{Function, Lua, MultiValue, Result as LuaResult, Table};

use crate::{game_environment, native_required_string, runtime_error};

pub(super) fn install(lua: &Lua, globals: &Table) -> LuaResult<()> {
    globals.set(
        "checkInstalledAppsOnline",
        lua.create_function(|_, args: MultiValue| {
            native_required_string(&args, 0, "checkInstalledAppsOnline")?;
            Ok(())
        })?,
    )?;
    globals.set(
        "checkInstalledAppsOffline",
        lua.create_function(|lua, args: MultiValue| {
            let response = native_required_string(&args, 0, "checkInstalledAppsOffline")?;
            let document: serde_json::Value =
                serde_json::from_str(&response).map_err(|_| runtime_error("Malformed response"))?;
            let object = document
                .as_object()
                .ok_or_else(|| runtime_error("Malformed response"))?;
            let _ttl = object
                .get("ttl")
                .and_then(serde_json::Value::as_i64)
                .ok_or_else(|| runtime_error("Malformed response"))?;
            let game_count = object
                .get("gameCount")
                .and_then(serde_json::Value::as_i64)
                .filter(|count| *count >= 0)
                .ok_or_else(|| runtime_error("Malformed response"))?;
            for index in 0..game_count {
                let game = object
                    .get(&format!("game_{index}"))
                    .and_then(serde_json::Value::as_object)
                    .ok_or_else(|| runtime_error("Malformed response"))?;
                for key in ["name", "scheme"] {
                    game.get(key)
                        .and_then(serde_json::Value::as_str)
                        .ok_or_else(|| runtime_error("Malformed response"))?;
                }
            }
            let callback = game_environment(lua)?.get::<Function>("setInstalledAppsOffline")?;
            callback.call::<()>("")?;
            Ok(())
        })?,
    )?;
    Ok(())
}
