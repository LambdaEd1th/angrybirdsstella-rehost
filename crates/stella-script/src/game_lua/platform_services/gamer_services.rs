//! FusionGamerServices/Game Center ownership and offline state.

use crate::*;

pub(super) fn install(lua: &Lua, globals: &mlua::Table) -> LuaResult<()> {
    let gamer_services = lua.create_table()?;
    gamer_services.set(
        "isSupported",
        // GameCenter::Impl construction (sub_10054C784) initializes the
        // process-wide availability byte to one. Only an asynchronous
        // GameKit authentication error with code 16 clears it.
        lua.create_function(|_, ()| Ok(true))?,
    )?;
    gamer_services.set(
        "getBackendName",
        // FusionGamerServices::getBackendName (sub_1000CA39C) returns this
        // literal independently of Game Center availability.
        lua.create_function(|_, ()| Ok("gamecenter"))?,
    )?;
    gamer_services.set(
        "isLocalPlayerAuthenticated",
        lua.create_function(|_, ()| Ok(false))?,
    )?;
    for method in ["login", "showAchievements", "showLeaderboards"] {
        gamer_services.set(method, lua.create_function(|_, ()| Ok(()))?)?;
    }
    gamer_services.set(
        "postAchievement",
        lua.create_function(|_, args: MultiValue| {
            // sub_1000CC2F4 uses the exact STRING-tag accessor
            // sub_1005285CC for slot one before the unavailable backend's
            // no-op member is invoked.
            native_required_string(&args, 0, "FusionGamerServices.postAchievement")?;
            Ok(())
        })?,
    )?;
    gamer_services.set(
        "postScore",
        lua.create_function(|_, args: MultiValue| {
            // sub_1000CC0C4 pairs that STRING accessor with the generated
            // exact NUMBER-tag accessor sub_10052859C for slot two.
            native_required_string(&args, 0, "FusionGamerServices.postScore")?;
            native_required_number(&args, 1, "FusionGamerServices.postScore")?;
            Ok(())
        })?,
    )?;
    globals.set("FusionGamerServices", gamer_services)?;
    Ok(())
}
