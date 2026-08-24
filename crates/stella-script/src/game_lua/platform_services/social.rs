//! Native SocialManager registration and the disconnected Facebook backend.

use crate::*;

pub(super) fn install(lua: &Lua, globals: &mlua::Table) -> LuaResult<()> {
    let social = lua.create_table()?;
    social.set(
        "native_isConnectedToSocialNetwork",
        // SocialManager::isConnected at sub_1000C032C checks whether the
        // active social-network pointer at +0xA0 is non-null. The offline
        // host never creates that provider, but the SocialManager service and
        // its Lua table still exist.
        lua.create_function(|_, _: MultiValue| Ok(false))?,
    )?;
    for method in [
        "native_connectToSocialNetwork",
        "native_getFriendsProgress",
        "native_unloadAllAvatars",
    ] {
        social.set(method, lua.create_function(|_, _: MultiValue| Ok(()))?)?;
    }
    social.set(
        "native_postScores",
        lua.create_function(|_, args: MultiValue| {
            native_required_string(&args, 0, "native_postScores")?;
            native_required_number(&args, 1, "native_postScores")?;
            native_required_string(&args, 2, "native_postScores")?;
            Ok(())
        })?,
    )?;
    social.set(
        "native_fetchLeaderboard",
        lua.create_function(|_, args: MultiValue| {
            native_required_string(&args, 0, "native_fetchLeaderboard")?;
            native_required_string(&args, 1, "native_fetchLeaderboard")?;
            Ok(())
        })?,
    )?;
    for method in [
        "native_setProgress",
        "native_loadAvatar",
        "native_unloadAvatar",
    ] {
        social.set(
            method,
            lua.create_function(move |_, args: MultiValue| {
                native_required_string(&args, 0, method)?;
                Ok(())
            })?,
        )?;
    }
    social.set(
        "native_getSocialNetworkName",
        // The bound member at sub_1000C2AB0 constructs this literal even
        // while disconnected. SocialBar uses it to choose Facebook rather
        // than the Sina Weibo presentation.
        lua.create_function(|_, _: MultiValue| Ok("facebook"))?,
    )?;
    social.set(
        "native_getFriendAccountId",
        lua.create_function(|_, args: MultiValue| {
            native_required_string(&args, 0, "native_getFriendAccountId")?;
            // sub_1000C2ADC returns an empty string when there is no active
            // provider/friend record.
            Ok("")
        })?,
    )?;
    social.set(
        "native_getLocalUserAccountId",
        // sub_1000C2E40 returns an empty string without a local user object.
        lua.create_function(|_, _: MultiValue| Ok(""))?,
    )?;
    social.set(
        "native_getFriends",
        // sub_1000C2E98 always pushes one array table; disconnected state
        // simply leaves it empty.
        lua.create_function(|lua, _: MultiValue| lua.create_table())?,
    )?;
    globals.set("SocialManager", social)?;
    Ok(())
}

/// Announce the native service after the Lua cloud dispatcher has installed
/// its listener. In Purple the C++ RovioCloudManager owns this lifecycle: its
/// SocialManager constructor registers the native table first, then the
/// `EID_CLOUD_SERVICE_REGISTERED` event loads the script-side facade.
pub(crate) fn announce_registration(lua: &Lua) -> LuaResult<()> {
    let environment = game_environment(lua)?;
    let Value::Table(cloud_manager) = environment.get::<Value>("RovioCloudManager")? else {
        return Ok(());
    };
    if let Value::Function(is_available) = cloud_manager.get::<Value>("isServiceAvailable")?
        && is_available.call::<bool>("social")?
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
    event.set("serviceName", "social")?;
    notify.call::<()>((event_manager, event))?;

    let Value::Table(native_social) = lua.globals().get::<Value>("SocialManager")? else {
        return Ok(());
    };
    if let Value::Function(enable) = native_social.get::<Value>("onEnableService")? {
        enable.call::<()>(())?;
    }
    Ok(())
}
