//! Native cloud-service announcement after the Lua dispatcher exists.

use crate::*;

pub(crate) fn announce_registrations(lua: &Lua) -> LuaResult<()> {
    // sub_1000AF9C0 registers all nine services in this exact order. The
    // service-name vtable leaves return `analytics` (sub_1000AB6BC), `push`
    // (sub_1000A0188) and `time` (sub_1000BD540) for the three services that
    // surrounded the previously recovered six. RemoteNotificationsService is
    // event-only and publishes no Lua table; the other services expose their
    // native tables before the common registration callback runs. Assets.lua
    // retains the native publication separately as `_G.Assets`.
    for (service_name, table_name) in [
        ("analytics", Some("Analytics")),
        ("push", None),
        ("identityLevel2", Some("SkynestAccount")),
        ("storage", Some("SkynestStorage")),
        ("ads", Some("RovioAds")),
        ("channel", Some("RovioChannel")),
        ("assets", Some("Assets")),
        ("social", Some("SocialManager")),
        ("time", Some("ServerTime")),
    ] {
        if service_name == "channel" {
            // The native cloud manager's availability callback reaches
            // RovioChannel::onEnableService (sub_1000AE354) before the Lua
            // facade observes the service registration.
            super::channel::enable_service(lua)?;
        }
        announce_registration(lua, service_name, table_name)?;
    }
    // ServerTimeImpl starts a request from its native constructor and posts
    // EID_SERVER_TIME_SYNCHRONIZED on the application thread after success.
    // The local clock is the authoritative offline source, but the completion
    // event must still cross the same post-bootstrap boundary.
    crate::game_lua::time_registration::complete_initial_sync(lua)?;
    load_late_ads_facade(lua)?;
    // The native account constructor starts an automatic login independently
    // of service announcement. Complete its offline equivalent only after all
    // facades and event listeners exist, matching the asynchronous provider
    // completion boundary and preventing a permanent ConnectionScreen.
    super::skynest_account::complete_initial_login(lua)?;
    Ok(())
}

fn load_late_ads_facade(lua: &Lua) -> LuaResult<()> {
    let environment = game_environment(lua)?;
    if !matches!(environment.get::<Value>("adSystem")?, Value::Nil) {
        return Ok(());
    }
    let Value::Function(load_lua_file) = environment.get::<Value>("loadLuaFile")? else {
        return Ok(());
    };
    let Value::String(common_script_path) = environment.get::<Value>("commonScriptPath")? else {
        return Ok(());
    };
    let script = format!("{}/cloud/ads/Ads.lua", common_script_path.to_string_lossy());
    // Purple has every native service registered before RovioCloudManager.lua
    // executes, so its top-level ads availability branch loads this chunk.
    // Rust announces after the dispatcher exists; reproduce that already-
    // available startup branch once after publishing the late `ads` event.
    load_lua_file.call::<()>(script)
}

fn announce_registration(lua: &Lua, service_name: &str, table_name: Option<&str>) -> LuaResult<()> {
    let environment = game_environment(lua)?;
    let Value::Table(cloud_manager) = environment.get::<Value>("RovioCloudManager")? else {
        return Ok(());
    };
    if let Value::Function(is_available) = cloud_manager.get::<Value>("isServiceAvailable")?
        && is_available.call::<bool>(service_name)?
    {
        return Ok(());
    }
    let Value::Table(event_manager) = environment.get::<Value>("eventManager")? else {
        return Ok(());
    };
    let Value::Table(events) = environment.get::<Value>("events")? else {
        return Ok(());
    };
    let Value::Function(notify) = event_manager.get::<Value>("notify")? else {
        return Ok(());
    };
    let event = lua.create_table()?;
    event.set("id", events.get::<Value>("EID_CLOUD_SERVICE_REGISTERED")?)?;
    event.set("serviceName", service_name)?;
    notify.call::<()>((event_manager, event))?;

    let Some(table_name) = table_name else {
        return Ok(());
    };
    let Value::Table(native_service) = lua.globals().get::<Value>(table_name)? else {
        return Ok(());
    };
    if let Value::Function(enable) = native_service.get::<Value>("onEnableService")? {
        enable.call::<()>(())?;
    }
    Ok(())
}
