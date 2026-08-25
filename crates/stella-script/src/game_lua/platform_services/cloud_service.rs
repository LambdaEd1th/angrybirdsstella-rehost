//! Native cloud-service announcement after the Lua dispatcher exists.

use crate::*;

pub(crate) fn announce_registrations(lua: &Lua) -> LuaResult<()> {
    // GameLua constructs these native services in this order. Their common
    // service interface publishes the lowercase name, and RovioCloudManager
    // responds to EID_CLOUD_SERVICE_REGISTERED by loading the corresponding
    // GameLua facade. Assets.lua retains the native publication separately as
    // `_G.Assets` and calls its loadFiles member through that explicit path.
    for (service_name, table_name) in [("social", "SocialManager"), ("assets", "Assets")] {
        announce_registration(lua, service_name, table_name)?;
    }
    Ok(())
}

fn announce_registration(lua: &Lua, service_name: &str, table_name: &str) -> LuaResult<()> {
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

    let Value::Table(native_service) = lua.globals().get::<Value>(table_name)? else {
        return Ok(());
    };
    if let Value::Function(enable) = native_service.get::<Value>("onEnableService")? {
        enable.call::<()>(())?;
    }
    Ok(())
}
