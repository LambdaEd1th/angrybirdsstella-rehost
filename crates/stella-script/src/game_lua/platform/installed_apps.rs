//! Installed-iOS-application lookup worker and response delivery.

use std::{
    collections::BTreeSet,
    sync::{Arc, Mutex},
};

use mlua::{Function, Lua, MultiValue, Result as LuaResult, Table};

use crate::{RenderBridge, game_environment, native_required_string, runtime_error};

use super::sharing::fetch_http_200;

#[derive(Clone, Debug)]
pub(crate) struct InstalledAppsRuntime {
    /// GameLua+0xA8 is overwritten before every worker is constructed. The
    /// worker reads that shared string only after its thread starts.
    request_url: Arc<Mutex<String>>,
    /// GameLua+0xB0 plus the completion byte at +0xB8 form one slot, not an
    /// event queue. A later successful worker therefore replaces the body.
    response: Arc<Mutex<Option<Vec<u8>>>>,
    /// Shared platform application capability owner used by Purple's
    /// `sub_10053CC18` canOpenURL boundary.
    render: Arc<Mutex<RenderBridge>>,
}

impl InstalledAppsRuntime {
    fn new(render: Arc<Mutex<RenderBridge>>) -> Self {
        Self {
            request_url: Arc::new(Mutex::new(String::new())),
            response: Arc::new(Mutex::new(None)),
            render,
        }
    }

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

pub(super) fn install(
    lua: &Lua,
    globals: &Table,
    render: Arc<Mutex<RenderBridge>>,
) -> LuaResult<InstalledAppsRuntime> {
    let runtime = InstalledAppsRuntime::new(render);
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
    let offline_runtime = runtime.clone();
    globals.set(
        "checkInstalledAppsOffline",
        lua.create_function(move |lua, args: MultiValue| {
            let response = native_required_string(&args, 0, "checkInstalledAppsOffline")?;
            let schemes = offline_runtime
                .render
                .lock()
                .expect("render bridge lock poisoned")
                .installed_url_schemes
                .clone();
            let parsed = parse_response(response.as_bytes(), &schemes)?;
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

fn parse_response(
    response: &[u8],
    installed_url_schemes: &BTreeSet<String>,
) -> LuaResult<ParsedResponse> {
    let document: serde_json::Value =
        serde_json::from_slice(response).map_err(|_| runtime_error("Malformed response"))?;
    let object = document
        .as_object()
        .ok_or_else(|| runtime_error("Malformed response"))?;
    let ttl = required_i32(object, "ttl")?;
    let game_count = required_i32(object, "gameCount")?;

    // sub_100060CBC asks UIApplication whether every authored `scheme://` can
    // be opened and joins the matching display names in document order.
    let mut installed_names = Vec::new();
    for index in 0..game_count {
        let game = object
            .get(&format!("game_{index}"))
            .and_then(serde_json::Value::as_object)
            .filter(|game| !game.is_empty())
            .ok_or_else(|| runtime_error("Malformed response"))?;
        let name = game
            .get("name")
            .and_then(serde_json::Value::as_str)
            .ok_or_else(|| runtime_error("Malformed response"))?;
        let scheme = game
            .get("scheme")
            .and_then(serde_json::Value::as_str)
            .ok_or_else(|| runtime_error("Malformed response"))?;
        if can_open_url(&format!("{scheme}://"), installed_url_schemes) {
            installed_names.push(name);
        }
    }

    Ok(ParsedResponse {
        ttl,
        installed_names: installed_names.join(","),
    })
}

pub(crate) fn normalized_url_scheme(url_or_scheme: &str) -> Option<String> {
    let scheme = url_or_scheme
        .split_once(':')
        .map_or(url_or_scheme, |(scheme, _)| scheme);
    let mut chars = scheme.chars();
    let first = chars.next()?;
    if !first.is_ascii_alphabetic()
        || !chars.all(|character| {
            character.is_ascii_alphanumeric() || matches!(character, '+' | '-' | '.')
        })
    {
        return None;
    }
    Some(scheme.to_ascii_lowercase())
}

pub(crate) fn can_open_url(url: &str, installed_url_schemes: &BTreeSet<String>) -> bool {
    normalized_url_scheme(url).is_some_and(|scheme| installed_url_schemes.contains(&scheme))
}

/// Consume GameLua+0xB8 at its recovered location in `sub_10005E898`.
/// Purple clears the flag only after parsing and the three-argument callback
/// both succeed, so a malformed document or Lua error remains observable on
/// the next frame too.
pub(crate) fn dispatch_completion(lua: &Lua, runtime: &InstalledAppsRuntime) -> LuaResult<()> {
    let Some(response) = runtime.response() else {
        return Ok(());
    };
    let schemes = runtime
        .render
        .lock()
        .expect("render bridge lock poisoned")
        .installed_url_schemes
        .clone();
    let parsed = parse_response(&response, &schemes)?;
    let callback = game_environment(lua)?.get::<Function>("setInstalledApps")?;
    let raw_response = lua.create_string(&response)?;
    callback.call::<()>((parsed.installed_names, parsed.ttl, raw_response))?;
    runtime.clear_response();
    Ok(())
}
