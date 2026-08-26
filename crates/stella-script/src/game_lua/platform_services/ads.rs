//! Rovio Ads table and the native null-provider branch.

use crate::*;

pub(super) fn install(lua: &Lua, globals: &mlua::Table) -> LuaResult<()> {
    let ads = lua.create_table()?;

    for method in [
        "refresh",
        "addPlacement",
        "addPlacementNative",
        "hide",
        "click",
    ] {
        ads.set(
            method,
            lua.create_function(move |_, args: MultiValue| {
                native_required_string(&args, 0, &format!("RovioAds.{method}"))?;
                // The members at sub_1000A939C..sub_1000A9420 all skip the
                // provider call while the native pointer at +0x48 is null.
                Ok(())
            })?,
        )?;
    }

    ads.set(
        "addPlacementWithGeometry",
        lua.create_function(|_, args: MultiValue| {
            native_required_string(&args, 0, "RovioAds.addPlacementWithGeometry")?;
            for index in 1..=4 {
                native_required_number(&args, index, "RovioAds.addPlacementWithGeometry")?;
            }
            Ok(())
        })?,
    )?;
    ads.set(
        "show",
        lua.create_function(|_, args: MultiValue| {
            native_required_string(&args, 0, "RovioAds.show")?;
            // sub_1000A93E8 returns false before consulting an ad placement
            // when the provider pointer is null.
            Ok(false)
        })?,
    )?;
    for method in ["trackConversion", "startSession"] {
        ads.set(
            method,
            lua.create_function(|_, _: MultiValue| {
                // The original commands return no values. Network conversion
                // tracking has no meaningful endpoint on the offline host.
                Ok(())
            })?,
        )?;
    }

    globals.set("RovioAds", ads)?;
    Ok(())
}
