//! GameLua and ServerTime table conversion helpers.

use mlua::{Function, Lua, Result as LuaResult, Value};

pub(crate) fn current_time_table(lua: &Lua) -> LuaResult<mlua::Table> {
    date_table(lua, "*t")
}

pub(crate) fn utc_time_table_from_seconds(lua: &Lua, seconds: f64) -> LuaResult<mlua::Table> {
    date_table_from_seconds(lua, "!*t", seconds)
}

fn date_table(lua: &Lua, format: &str) -> LuaResult<mlua::Table> {
    let os: mlua::Table = lua.globals().get("os")?;
    let date: Function = os.get("date")?;
    let source: mlua::Table = date.call(format)?;
    let result = lua.create_table()?;
    copy_date_fields_as_native_floats(&source, &result)?;
    Ok(result)
}

/// Reproduces `sub_10005D700`: Purple reads all calendar values through its
/// single-precision Lua number bridge, requires Y/M/D, defaults H/M/S to zero,
/// and leaves the zero-initialized `tm_isdst` set to standard time.
pub(crate) fn time_table_seconds(lua: &Lua, source: &mlua::Table) -> LuaResult<f64> {
    let normalized = lua.create_table()?;
    normalized.set("year", required_native_integer(source, "year")?)?;
    normalized.set("month", required_native_integer(source, "month")?)?;
    normalized.set("day", required_native_integer(source, "day")?)?;
    normalized.set("hour", optional_native_integer(source, "hour")?)?;
    normalized.set("min", optional_native_integer(source, "minutes")?)?;
    normalized.set("sec", optional_native_integer(source, "seconds")?)?;
    normalized.set("isdst", false)?;
    os_time(lua, normalized)
}

/// Reproduces `sub_100056D68`: unlike the difference helper, every source
/// field is mandatory and `tm_isdst` is -1 so `mktime` determines DST after
/// adding the single-precision duration to `tm_sec`.
pub(crate) fn add_duration_to_time_table(
    lua: &Lua,
    source: &mlua::Table,
    duration: f32,
) -> LuaResult<mlua::Table> {
    let normalized = lua.create_table()?;
    normalized.set("year", required_native_integer(source, "year")?)?;
    normalized.set("month", required_native_integer(source, "month")?)?;
    normalized.set("day", required_native_integer(source, "day")?)?;
    normalized.set("hour", required_native_integer(source, "hour")?)?;
    normalized.set("min", required_native_integer(source, "minutes")?)?;
    let seconds = source.get::<f32>("seconds")? + duration;
    normalized.set("sec", seconds as i32)?;

    // Lua 5.1's os.time is the same local mktime bridge used by Purple. An
    // absent isdst field maps to tm_isdst=-1, matching the native member.
    let seconds = os_time(lua, normalized)?;
    time_table_from_seconds(lua, seconds)
}

fn os_time(lua: &Lua, source: mlua::Table) -> LuaResult<f64> {
    let os: mlua::Table = lua.globals().get("os")?;
    let time: Function = os.get("time")?;
    match time.call::<Value>(source)? {
        Value::Integer(value) => Ok(value as f64),
        Value::Number(value) => Ok(value),
        // Lua's os.time maps mktime(-1) to nil, whereas Purple uses the -1
        // time_t directly. Keeping that value preserves difftime behavior.
        Value::Nil => Ok(-1.0),
        value => Err(mlua::Error::FromLuaConversionError {
            from: value.type_name(),
            to: "time_t".to_owned(),
            message: Some("os.time returned a non-numeric value".to_owned()),
        }),
    }
}

pub(crate) fn time_table_from_seconds(lua: &Lua, seconds: f64) -> LuaResult<mlua::Table> {
    date_table_from_seconds(lua, "*t", seconds)
}

fn date_table_from_seconds(lua: &Lua, format: &str, seconds: f64) -> LuaResult<mlua::Table> {
    let os: mlua::Table = lua.globals().get("os")?;
    let date: Function = os.get("date")?;
    let source: mlua::Table = date.call((format, seconds))?;
    let result = lua.create_table()?;
    copy_date_fields_as_native_floats(&source, &result)?;
    Ok(result)
}

fn required_native_integer(source: &mlua::Table, name: &str) -> LuaResult<i32> {
    Ok(source.get::<f32>(name)? as i32)
}

fn optional_native_integer(source: &mlua::Table, name: &str) -> LuaResult<i32> {
    Ok(source.get::<Option<f32>>(name)?.unwrap_or(0.0) as i32)
}

fn copy_date_fields_as_native_floats(source: &mlua::Table, result: &mlua::Table) -> LuaResult<()> {
    for name in ["year", "month", "day", "hour"] {
        result.set(name, source.get::<f32>(name)?)?;
    }
    result.set("minutes", source.get::<f32>("min")?)?;
    result.set("seconds", source.get::<f32>("sec")?)?;
    Ok(())
}
