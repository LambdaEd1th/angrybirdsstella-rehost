//! Native date and epoch-to-calendar members.

use std::time::{SystemTime, UNIX_EPOCH};

use mlua::{Lua, MultiValue, Result as LuaResult, Table, Value};

use crate::native_required_string;

pub(super) fn install_get_date(lua: &Lua, globals: &Table) -> LuaResult<()> {
    globals.set(
        "GetDate",
        lua.create_function(|_, _: MultiValue| {
            // sub_1000313FC truncates time_t to signed int32 before dividing;
            // sub_1000886D0 then converts that integer to float32 for Lua.
            let seconds = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs() as i32;
            Ok(f64::from((seconds / 3_600) as f32))
        })?,
    )?;
    Ok(())
}

pub(super) fn install_epoch_conversion(lua: &Lua, globals: &Table) -> LuaResult<()> {
    globals.set(
        "getTimeFromEpochSeconds",
        lua.create_function(|lua, args: MultiValue| {
            // sub_10005716C reads slot 1 through the strict string getter,
            // parses it with operator>>(long), and validates slot 2 as a
            // BOOLEAN only when that slot already has the BOOLEAN type.
            // Missing and non-boolean values therefore both select UTC.
            let seconds = parse_native_long(&native_required_string(
                &args,
                0,
                "getTimeFromEpochSeconds",
            )?);
            let local = matches!(args.iter().nth(1), Some(Value::Boolean(true)));
            let os: Table = lua.globals().get("os")?;
            let date: mlua::Function = os.get("date")?;
            let format = if local { "*t" } else { "!*t" };
            let source: Table = date.call((format, seconds))?;
            let result = lua.create_table()?;
            for name in ["year", "month", "day", "hour"] {
                result.set(name, source.get::<Value>(name)?)?;
            }
            result.set("minutes", source.get::<Value>("min")?)?;
            result.set("seconds", source.get::<Value>("sec")?)?;
            Ok(result)
        })?,
    )?;
    Ok(())
}

fn parse_native_long(input: &str) -> i64 {
    let trimmed = input.trim_start();
    let (sign, rest) = match trimmed.as_bytes().first() {
        Some(b'-') => (-1_i64, &trimmed[1..]),
        Some(b'+') => (1_i64, &trimmed[1..]),
        _ => (1_i64, trimmed),
    };
    let digits = rest
        .bytes()
        .take_while(u8::is_ascii_digit)
        .collect::<Vec<_>>();
    if digits.is_empty() {
        return 0;
    }
    let magnitude = std::str::from_utf8(&digits)
        .ok()
        .and_then(|value| value.parse::<u128>().ok())
        .unwrap_or(u128::MAX);
    if sign < 0 {
        if magnitude > i64::MAX as u128 {
            i64::MIN
        } else {
            -(magnitude as i64)
        }
    } else {
        magnitude.min(i64::MAX as u128) as i64
    }
}
