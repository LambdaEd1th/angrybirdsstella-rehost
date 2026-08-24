//! Strict table-field readers used by recovered hand-written members.

use mlua::{Result as LuaResult, Value};

use crate::runtime_error;

pub(crate) fn table_required_number(
    table: &mlua::Table,
    field: &str,
    function: &str,
) -> LuaResult<f64> {
    match table.get::<Value>(field)? {
        Value::Integer(value) => Ok(value as f64),
        Value::Number(value) => Ok(value),
        _ => Err(runtime_error(format!(
            "{function} table field {field} must be number"
        ))),
    }
}

pub(crate) fn table_required_string(
    table: &mlua::Table,
    field: &str,
    function: &str,
) -> LuaResult<String> {
    match table.get::<Value>(field)? {
        Value::String(value) => Ok(value.to_str()?.to_owned()),
        _ => Err(runtime_error(format!(
            "{function} table field {field} must be string"
        ))),
    }
}
