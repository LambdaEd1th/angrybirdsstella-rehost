//! Analytics service table, whose native methods are intentional void sinks.

use crate::*;

pub(super) fn install(lua: &Lua, globals: &mlua::Table) -> LuaResult<()> {
    let analytics_native = lua.create_table()?;
    for method in ["logTimerEvent", "logEvent"] {
        analytics_native.set(
            method,
            lua.create_function(move |_, args: MultiValue| {
                native_required_string(&args, 0, method)?;
                Ok(MultiValue::new())
            })?,
        )?;
    }
    analytics_native.set(
        "logEventWithParam",
        lua.create_function(|_, args: MultiValue| {
            for index in 0..3 {
                native_required_string(&args, index, "logEventWithParam")?;
            }
            Ok(MultiValue::new())
        })?,
    )?;
    analytics_native.set(
        "logEventWithParams",
        lua.create_function(|_, args: MultiValue| {
            for index in 0..2 {
                native_required_string(&args, index, "logEventWithParams")?;
            }
            Ok(MultiValue::new())
        })?,
    )?;
    globals.set("Analytics", analytics_native)?;
    Ok(())
}
