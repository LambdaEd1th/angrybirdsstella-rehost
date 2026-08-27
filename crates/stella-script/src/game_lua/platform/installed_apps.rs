//! Installed-iOS-application lookup worker and response delivery.

use std::sync::{Arc, Mutex};

use mlua::{Function, Lua, MultiValue, Result as LuaResult, Table};

use crate::{game_environment, native_required_string, runtime_error};

use super::sharing::fetch_http_200;

#[derive(Clone, Debug, Default)]
pub(crate) struct InstalledAppsRuntime {
    /// GameLua+0xA8 is overwritten before every worker is constructed. The
    /// worker reads that shared string only after its thread starts.
    request_url: Arc<Mutex<String>>,
    /// GameLua+0xB0 plus the completion byte at +0xB8 form one slot, not an
    /// event queue. A later successful worker therefore replaces the body.
    response: Arc<Mutex<Option<Vec<u8>>>>,
}

impl InstalledAppsRuntime {
    fn response(&self) -> Option<Vec<u8>> {
        self.response
            .lock()
            .expect("installed-app response lock poisoned")
            .clone()
    }

    fn clear_response(&self) {
        *self
            .response
            .lock()
            .expect("installed-app response lock poisoned") = None;
    }
}

pub(super) fn install(lua: &Lua, globals: &Table) -> LuaResult<InstalledAppsRuntime> {
    let runtime = InstalledAppsRuntime::default();
    let online_runtime = runtime.clone();
    globals.set(
        "checkInstalledAppsOnline",
        lua.create_function(move |_, args: MultiValue| {
            let url = native_required_string(&args, 0, "checkInstalledAppsOnline")?;
            *online_runtime
                .request_url
                .lock()
                .expect("installed-app URL lock poisoned") = url;

            let worker_runtime = online_runtime.clone();
            std::thread::Builder::new()
                .name("stella-installed-apps".to_owned())
                .spawn(move || {
                    let url = worker_runtime
                        .request_url
                        .lock()
                        .expect("installed-app URL lock poisoned")
                        .clone();
                    let Some(response) = fetch_http_200(&url) else {
                        return;
                    };
                    *worker_runtime
                        .response
                        .lock()
                        .expect("installed-app response lock poisoned") = Some(response);
                })
                .map_err(|_| runtime_error("Creating thread failed"))?;
            Ok(())
        })?,
    )?;
    globals.set(
        "checkInstalledAppsOffline",
        lua.create_function(|lua, args: MultiValue| {
            let response = native_required_string(&args, 0, "checkInstalledAppsOffline")?;
            let parsed = parse_response(response.as_bytes())?;
            let callback = game_environment(lua)?.get::<Function>("setInstalledAppsOffline")?;
            callback.call::<()>(parsed.installed_names)?;
            Ok(())
        })?,
    )?;
    Ok(runtime)
}

struct ParsedResponse {
    ttl: i32,
    installed_names: String,
}

fn required_i32(object: &serde_json::Map<String, serde_json::Value>, key: &str) -> LuaResult<i32> {
    object
        .get(key)
        .and_then(serde_json::Value::as_i64)
        .and_then(|value| i32::try_from(value).ok())
        .ok_or_else(|| runtime_error("Malformed response"))
}

fn parse_response(response: &[u8]) -> LuaResult<ParsedResponse> {
    let document: serde_json::Value =
        serde_json::from_slice(response).map_err(|_| runtime_error("Malformed response"))?;
    let object = document
        .as_object()
        .ok_or_else(|| runtime_error("Malformed response"))?;
    let ttl = required_i32(object, "ttl")?;
    let game_count = required_i32(object, "gameCount")?;

    // sub_100060CBC asks UIApplication whether every authored `scheme://` can
    // be opened and joins the matching display names. A desktop rehost has no
    // iOS application registry, but it must still validate the complete JSON
    // shape before returning the empty native list.
    for index in 0..game_count {
        let game = object
            .get(&format!("game_{index}"))
            .and_then(serde_json::Value::as_object)
            .filter(|game| !game.is_empty())
            .ok_or_else(|| runtime_error("Malformed response"))?;
        for key in ["name", "scheme"] {
            game.get(key)
                .and_then(serde_json::Value::as_str)
                .ok_or_else(|| runtime_error("Malformed response"))?;
        }
    }

    Ok(ParsedResponse {
        ttl,
        installed_names: String::new(),
    })
}

/// Consume GameLua+0xB8 at its recovered location in `sub_10005E898`.
/// Purple clears the flag only after parsing and the three-argument callback
/// both succeed, so a malformed document or Lua error remains observable on
/// the next frame too.
pub(crate) fn dispatch_completion(lua: &Lua, runtime: &InstalledAppsRuntime) -> LuaResult<()> {
    let Some(response) = runtime.response() else {
        return Ok(());
    };
    let parsed = parse_response(&response)?;
    let callback = game_environment(lua)?.get::<Function>("setInstalledApps")?;
    let raw_response = lua.create_string(&response)?;
    callback.call::<()>((parsed.installed_names, parsed.ttl, raw_response))?;
    runtime.clear_response();
    Ok(())
}
