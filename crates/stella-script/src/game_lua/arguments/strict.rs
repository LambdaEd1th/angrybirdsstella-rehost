//! Strict generated-binding adapters such as `sub_100088D24` and `sub_10008962C`.

use mlua::{MultiValue, Result as LuaResult, Value};

use super::{native_integer, value_number_at, value_string};
use crate::runtime_error;

pub(crate) fn native_required_string(
    values: &MultiValue,
    index: usize,
    function: &str,
) -> LuaResult<String> {
    values
        .iter()
        .nth(index)
        .and_then(value_string)
        .ok_or_else(|| {
            runtime_error(format!(
                "bad argument #{} to '{function}' (string expected)",
                index + 1
            ))
        })
}

pub(crate) fn native_required_boolean(
    values: &MultiValue,
    index: usize,
    function: &str,
) -> LuaResult<bool> {
    match values.iter().nth(index) {
        Some(Value::Boolean(value)) => Ok(*value),
        _ => Err(runtime_error(format!(
            "bad argument #{} to '{function}' (boolean expected)",
            index + 1
        ))),
    }
}

pub(crate) fn native_required_integer(
    values: &MultiValue,
    index: usize,
    function: &str,
) -> LuaResult<i64> {
    values
        .iter()
        .nth(index)
        .and_then(native_integer)
        .ok_or_else(|| {
            runtime_error(format!(
                "bad argument #{} to '{function}' (integer expected)",
                index + 1
            ))
        })
}

pub(crate) fn native_required_number(
    values: &MultiValue,
    index: usize,
    function: &str,
) -> LuaResult<f64> {
    value_number_at(values, index).ok_or_else(|| {
        runtime_error(format!(
            "bad argument #{} to '{function}' (number expected)",
            index + 1
        ))
    })
}

pub(crate) fn native_required_table(
    values: &MultiValue,
    index: usize,
    function: &str,
) -> LuaResult<mlua::Table> {
    match values.iter().nth(index) {
        Some(Value::Table(table)) => Ok(table.clone()),
        _ => Err(runtime_error(format!(
            "bad argument #{} to '{function}' (table expected)",
            index + 1
        ))),
    }
}
