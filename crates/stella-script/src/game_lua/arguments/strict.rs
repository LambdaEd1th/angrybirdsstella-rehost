//! Strict generated-binding adapters such as `sub_100088D24` and `sub_10008962C`.

use std::ffi::{CStr, c_char};

use mlua::{MultiValue, Result as LuaResult, Value};

use super::{native_integer, value_number_at};
use crate::runtime_error;

fn native_string_error(index: usize, function: &str) -> mlua::Error {
    runtime_error(format!(
        "bad argument #{} to '{function}' (string expected)",
        index + 1
    ))
}

pub(crate) fn native_required_string(
    values: &MultiValue,
    index: usize,
    function: &str,
) -> LuaResult<String> {
    Ok(native_required_borrowed_string(values, index, function)?.to_owned())
}

/// Borrow the C-string view used by generated adapters such as
/// `sub_100084398` and `sub_1000859F4`. Purple obtains the exact Lua STRING
/// pointer through `sub_1005285CC`, measures it with `strlen`, and only then
/// assigns the bytes to `std::string`; embedded NUL bytes therefore terminate
/// every generated string argument.
pub(crate) fn native_required_borrowed_string<'a>(
    values: &'a MultiValue,
    index: usize,
    function: &str,
) -> LuaResult<&'a str> {
    let Some(Value::String(value)) = values.iter().nth(index) else {
        return Err(native_string_error(index, function));
    };

    let pointer = value.to_pointer().cast::<c_char>();
    if pointer.is_null() {
        return Err(native_string_error(index, function));
    }

    // SAFETY: `MultiValue` owns the LuaString registry reference for the
    // returned lifetime. Lua strings are immutable/non-moving, and mlua's
    // LuaString::to_pointer uses lua_tostring to expose that stable payload.
    // CStr intentionally reproduces Purple's following strlen boundary.
    let bytes = unsafe { CStr::from_ptr(pointer) }.to_bytes();
    std::str::from_utf8(bytes).map_err(|_| native_string_error(index, function))
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
