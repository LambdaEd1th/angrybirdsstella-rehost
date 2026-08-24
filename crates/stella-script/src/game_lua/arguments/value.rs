//! Non-throwing access to Lua values before a member-specific ABI is applied.

use mlua::{MultiValue, Value};

pub(crate) fn value_table(value: &Value) -> Option<mlua::Table> {
    match value {
        Value::Table(table) => Some(table.clone()),
        _ => None,
    }
}

pub(crate) fn value_string(value: &Value) -> Option<String> {
    match value {
        Value::String(value) => value.to_str().ok().map(|value| value.to_owned()),
        _ => None,
    }
}

pub(crate) fn native_integer(value: &Value) -> Option<i64> {
    match value {
        Value::Integer(value) => Some(*value),
        // Purple's VM has a distinct integer tag. Stock Lua 5.1 stores the
        // i64 handles returned by mlua as numbers, so retain only exactly
        // integral values at this compatibility boundary.
        Value::Number(value)
            if value.is_finite()
                && value.fract() == 0.0
                && *value >= i64::MIN as f64
                && *value < -(i64::MIN as f64) =>
        {
            Some(*value as i64)
        }
        _ => None,
    }
}

pub(crate) fn value_number(value: &Value) -> Option<f64> {
    match value {
        Value::Integer(value) => Some(*value as f64),
        Value::Number(value) => Some(*value),
        _ => None,
    }
}

pub(crate) fn value_number_at(values: &MultiValue, index: usize) -> Option<f64> {
    values.iter().nth(index).and_then(value_number)
}

pub(crate) fn value_bool(value: &Value) -> Option<bool> {
    match value {
        Value::Boolean(value) => Some(*value),
        Value::Integer(value) => Some(*value != 0),
        Value::Number(value) => Some(*value != 0.0),
        _ => None,
    }
}
