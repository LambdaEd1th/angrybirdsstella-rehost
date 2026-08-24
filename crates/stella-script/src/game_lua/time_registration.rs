//! Calendar/server-time Lua bindings.

use crate::*;

pub(crate) fn install(lua: &Lua, globals: &mlua::Table) -> LuaResult<()> {
    globals.set(
        "getCurrentTime",
        lua.create_function(|lua, _: MultiValue| current_time_table(lua))?,
    )?;
    let server_time = lua.create_table()?;
    server_time.set(
        "getServerTimeInUTC",
        lua.create_function(|lua, _: MultiValue| current_utc_time_table(lua))?,
    )?;
    server_time.set(
        "getServerTimeInLocalTimeZone",
        lua.create_function(|lua, _: MultiValue| current_time_table(lua))?,
    )?;
    // The offline rehost starts in the native unsynchronized state: offset
    // zero and status zero. Purple only changes those fields after its HTTP
    // time service returns, so synchronization has no state to mutate here.
    server_time.set(
        "synchronizeServerTime",
        lua.create_function(|_, _: MultiValue| Ok(()))?,
    )?;
    server_time.set(
        "getStatus",
        lua.create_function(|_, _: MultiValue| Ok("STATUS_OK"))?,
    )?;
    globals.set("ServerTime", server_time)?;
    globals.set(
        "addDurationToTime",
        lua.create_function(|lua, (source, duration): (mlua::Table, f32)| {
            add_duration_to_time_table(lua, &source, duration)
        })?,
    )?;
    globals.set(
        "getTimeDifference",
        lua.create_function(|lua, (first, second): (mlua::Table, mlua::Table)| {
            let difference =
                (time_table_seconds(lua, &first)? - time_table_seconds(lua, &second)?).abs() as u32;
            let result = lua.create_table()?;
            result.set("days", (difference / 86_400) as f32)?;
            result.set("hours", (difference / 3_600 % 24) as f32)?;
            result.set("minutes", (difference / 60 % 60) as f32)?;
            result.set("seconds", (difference % 60) as f32)?;
            Ok(result)
        })?,
    )?;
    globals.set(
        "getTimeDifferenceInSeconds",
        lua.create_function(|lua, (first, second): (mlua::Table, mlua::Table)| {
            Ok((time_table_seconds(lua, &first)? - time_table_seconds(lua, &second)?) as f32)
        })?,
    )?;
    Ok(())
}
