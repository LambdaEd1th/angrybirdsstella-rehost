//! Recursive Lua table copying used by Purple's fixed-schema level exporter.

use std::collections::BTreeSet;

use mlua::{Lua, Result as LuaResult, Value};

use crate::*;

#[derive(Clone, Copy)]
pub(super) enum NativeLevelFieldKind {
    Boolean,
    Number,
    String,
    Table,
}

pub(super) fn native_copy_level_field(
    lua: &Lua,
    source: &mlua::Table,
    destination: &mlua::Table,
    field: &str,
    kind: NativeLevelFieldKind,
    error_context: Option<(&str, &str)>,
) -> LuaResult<()> {
    let value = source.raw_get::<Value>(field)?;
    let copied = match kind {
        NativeLevelFieldKind::Boolean => match value {
            Value::Boolean(value) => Some(Value::Boolean(value)),
            _ => None,
        },
        NativeLevelFieldKind::Number => {
            native_lua51_number(&value).map(|value| Value::Number(f64::from(value as f32)))
        }
        NativeLevelFieldKind::String => match native_lua51_string(&value) {
            Some(value) => Some(Value::String(lua.create_string(value)?)),
            None => None,
        },
        NativeLevelFieldKind::Table => match value {
            Value::Table(table) => Some(Value::Table(native_clone_level_table(
                lua,
                &table,
                false,
                &mut BTreeSet::new(),
                error_context,
            )?)),
            _ => None,
        },
    };
    if let Some(value) = copied {
        destination.raw_set(field, value)?;
    }
    Ok(())
}

pub(super) fn native_set_required_level_number(
    source: &mlua::Table,
    destination: &mlua::Table,
    field: &str,
) -> LuaResult<()> {
    let value = native_lua51_number(&source.raw_get::<Value>(field)?).unwrap_or(0.0);
    destination.raw_set(field, f64::from(value as f32))
}

pub(super) fn native_set_required_level_string(
    source: &mlua::Table,
    destination: &mlua::Table,
    field: &str,
) -> LuaResult<()> {
    let value = native_lua51_string(&source.raw_get::<Value>(field)?).unwrap_or_default();
    destination.raw_set(field, value)
}

pub(super) fn native_copy_required_level_table(
    lua: &Lua,
    source: &mlua::Table,
    destination: &mlua::Table,
    field: &str,
) -> LuaResult<()> {
    let table = match source.raw_get::<Value>(field)? {
        Value::Table(table) => {
            native_clone_level_table(lua, &table, false, &mut BTreeSet::new(), None)?
        }
        _ => lua.create_table()?,
    };
    destination.raw_set(field, table)
}

pub(super) fn native_level_editable_value(
    lua: &Lua,
    value: &Value,
    error_context: Option<(&str, &str)>,
) -> LuaResult<Value> {
    if let Some(value) = native_lua51_number(value) {
        return Ok(Value::Number(f64::from(value as f32)));
    }
    match value {
        Value::Boolean(value) => Ok(Value::Boolean(*value)),
        Value::String(value) => Ok(Value::String(value.clone())),
        Value::Table(table) => Ok(Value::Table(native_clone_level_table(
            lua,
            table,
            true,
            &mut BTreeSet::new(),
            error_context,
        )?)),
        _ => Err(native_level_unsupported_attribute(error_context)),
    }
}

fn native_clone_level_table(
    lua: &Lua,
    source: &mlua::Table,
    strict: bool,
    table_stack: &mut BTreeSet<usize>,
    error_context: Option<(&str, &str)>,
) -> LuaResult<mlua::Table> {
    let identity = source.to_pointer() as usize;
    if !table_stack.insert(identity) {
        return Err(native_level_unsupported_attribute(error_context));
    }
    let result = (|| {
        let output = lua.create_table()?;
        for pair in source.clone().pairs::<Value, Value>() {
            let (key, value) = pair?;
            let key = match key {
                Value::Boolean(value) => Some(Value::Boolean(value)),
                Value::Integer(value) => Some(Value::Number(f64::from(value as f32))),
                Value::Number(value) => Some(Value::Number(f64::from(value as f32))),
                Value::String(value) => Some(Value::String(value)),
                _ => None,
            };
            let Some(key) = key else {
                if strict {
                    return Err(native_level_unsupported_attribute(error_context));
                }
                continue;
            };
            let value = match value {
                Value::Boolean(value) => Some(Value::Boolean(value)),
                Value::Integer(value) => Some(Value::Number(f64::from(value as f32))),
                Value::Number(value) => Some(Value::Number(f64::from(value as f32))),
                Value::String(value) => Some(Value::String(value)),
                Value::Table(table) => Some(Value::Table(native_clone_level_table(
                    lua,
                    &table,
                    strict,
                    table_stack,
                    error_context,
                )?)),
                Value::Nil => None,
                _ => {
                    if strict {
                        return Err(native_level_unsupported_attribute(error_context));
                    }
                    None
                }
            };
            if let Some(value) = value {
                output.raw_set(key, value)?;
            }
        }
        Ok(output)
    })();
    table_stack.remove(&identity);
    result
}

fn native_level_unsupported_attribute(context: Option<(&str, &str)>) -> mlua::Error {
    let (attribute, block) = context.unwrap_or(("?", "?"));
    runtime_error(format!(
        "Attribute {attribute} of block {block} can't be saved because it's of an unsupported type"
    ))
}

pub(super) fn native_lua51_truthy(value: &Value) -> bool {
    !matches!(value, Value::Nil | Value::Boolean(false))
}
