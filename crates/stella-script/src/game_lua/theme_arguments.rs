//! Theme binding adapters and direct-Lua table coercions.

use mlua::{MultiValue, Result as LuaResult, Value};

use crate::*;

pub(crate) fn theme_required_string(
    values: &MultiValue,
    index: usize,
    function: &str,
) -> LuaResult<String> {
    values
        .iter()
        .nth(index)
        .and_then(value_string)
        .ok_or_else(|| runtime_error(format!("{function} argument {} must be string", index + 1)))
}

pub(crate) fn theme_required_f32(
    values: &MultiValue,
    index: usize,
    function: &str,
) -> LuaResult<f32> {
    values
        .iter()
        .nth(index)
        .and_then(value_number)
        .map(|value| value as f32)
        .ok_or_else(|| runtime_error(format!("{function} argument {} must be number", index + 1)))
}

pub(crate) fn theme_required_bool(
    values: &MultiValue,
    index: usize,
    function: &str,
) -> LuaResult<bool> {
    match values.iter().nth(index) {
        Some(Value::Boolean(value)) => Ok(*value),
        _ => Err(runtime_error(format!(
            "{function} argument {} must be boolean",
            index + 1
        ))),
    }
}

pub(crate) fn native_theme_layer(value: f32) -> usize {
    // GameLua first FCVTZS's the Lua float, converts it back to float for the
    // background/foreground split and finally FCVTZU's the selected offset.
    // Negative/non-finite input therefore addresses layer zero.
    native_fcvtzs_f32(value).max(0) as usize
}

pub(crate) fn theme_table_f32(table: &mlua::Table, field: &str) -> LuaResult<Option<f32>> {
    Ok(native_lua51_number(&table.get::<Value>(field)?).map(|value| value as f32))
}

pub(crate) fn theme_table_string(table: &mlua::Table, field: &str) -> LuaResult<Option<String>> {
    Ok(native_lua51_string(&table.get::<Value>(field)?))
}

pub(crate) fn theme_table_bool(table: &mlua::Table, field: &str) -> LuaResult<Option<bool>> {
    Ok(match table.get::<Value>(field)? {
        Value::Boolean(value) => Some(value),
        _ => None,
    })
}
