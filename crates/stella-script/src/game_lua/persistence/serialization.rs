//! Executable Lua-table serializer family `sub_10052A020..sub_10052A8FC`.

use std::collections::BTreeSet;

use mlua::{Result as LuaResult, Table, Value};

use crate::runtime_error;

pub(crate) fn serialize_table(table: &Table) -> LuaResult<Vec<u8>> {
    let mut output = Vec::new();
    let mut table_stack = BTreeSet::new();
    serialize_table_fields(table, &mut output, 0, true, &mut table_stack)?;
    Ok(output)
}

fn serialize_table_fields(
    table: &Table,
    output: &mut Vec<u8>,
    indent: usize,
    top_level: bool,
    table_stack: &mut BTreeSet<usize>,
) -> LuaResult<()> {
    let identity = table.to_pointer() as usize;
    if !table_stack.insert(identity) {
        return Err(runtime_error("cyclic Lua tables cannot be saved"));
    }

    let result = (|| {
        let mut entries = table
            .clone()
            .pairs::<Value, Value>()
            .collect::<LuaResult<Vec<_>>>()?;

        if !top_level {
            let mut next_array_index = 1_i64;
            while let Some(position) = entries.iter().position(|(key, value)| {
                numeric_key(key) == Some(next_array_index) && value_is_serializable(value)
            }) {
                let (_, value) = entries.remove(position);
                write_indent(output, indent);
                serialize_value(&value, output, indent, table_stack)?;
                output.extend_from_slice(b",\n");
                next_array_index += 1;
            }
        }

        for (key, value) in entries {
            if !value_is_serializable(&value) || key_is_reserved(&key) {
                continue;
            }
            if top_level {
                let Value::String(key) = key else {
                    continue;
                };
                if !identifier_is_valid(key.as_bytes().as_ref()) {
                    continue;
                }
                output.extend_from_slice(key.as_bytes().as_ref());
            } else {
                write_indent(output, indent);
                match &key {
                    Value::String(key) if identifier_is_valid(key.as_bytes().as_ref()) => {
                        output.extend_from_slice(key.as_bytes().as_ref());
                    }
                    _ if key_is_serializable(&key) => {
                        output.push(b'[');
                        serialize_value(&key, output, indent, table_stack)?;
                        output.push(b']');
                    }
                    _ => continue,
                }
            }
            output.extend_from_slice(b" = ");
            serialize_value(&value, output, indent, table_stack)?;
            if top_level {
                output.push(b'\n');
            } else {
                output.extend_from_slice(b",\n");
            }
        }
        Ok(())
    })();

    table_stack.remove(&identity);
    result
}

fn serialize_value(
    value: &Value,
    output: &mut Vec<u8>,
    indent: usize,
    table_stack: &mut BTreeSet<usize>,
) -> LuaResult<()> {
    match value {
        Value::Boolean(value) => output.extend_from_slice(if *value { b"true" } else { b"false" }),
        Value::Integer(value) => output.extend_from_slice(value.to_string().as_bytes()),
        Value::Number(value) if value.is_nan() => output.extend_from_slice(b"0/0"),
        Value::Number(value) if value.is_infinite() && value.is_sign_positive() => {
            output.extend_from_slice(b"1/0");
        }
        Value::Number(value) if value.is_infinite() => output.extend_from_slice(b"-1/0"),
        Value::Number(value) => output.extend_from_slice(value.to_string().as_bytes()),
        Value::String(value) => write_string(output, value.as_bytes().as_ref()),
        Value::Table(table) => {
            output.extend_from_slice(b"{\n");
            serialize_table_fields(table, output, indent + 4, false, table_stack)?;
            write_indent(output, indent);
            output.push(b'}');
        }
        _ => output.extend_from_slice(b"nil"),
    }
    Ok(())
}

fn write_string(output: &mut Vec<u8>, bytes: &[u8]) {
    output.push(b'"');
    for (index, byte) in bytes.iter().copied().enumerate() {
        match byte {
            7 => output.extend_from_slice(b"\\a"),
            8 => output.extend_from_slice(b"\\b"),
            b'\t' => output.extend_from_slice(b"\\t"),
            b'\n' => output.extend_from_slice(b"\\n"),
            11 => output.extend_from_slice(b"\\v"),
            12 => output.extend_from_slice(b"\\f"),
            b'\r' => output.extend_from_slice(b"\\r"),
            b'\\' => output.extend_from_slice(b"\\\\"),
            b'"' => output.extend_from_slice(b"\\\""),
            b'\'' => output.extend_from_slice(b"\\'"),
            32..=126 => output.push(byte),
            _ if bytes.get(index + 1).is_some_and(u8::is_ascii_digit) => {
                output.extend_from_slice(format!("\\{byte:03}").as_bytes());
            }
            _ => output.extend_from_slice(format!("\\{byte}").as_bytes()),
        }
    }
    output.push(b'"');
}

fn write_indent(output: &mut Vec<u8>, indent: usize) {
    output.resize(output.len() + indent, b' ');
}

fn numeric_key(value: &Value) -> Option<i64> {
    match value {
        Value::Integer(value) if *value >= 1 => Some(*value),
        Value::Number(value)
            if value.is_finite()
                && *value >= 1.0
                && value.fract() == 0.0
                && *value <= i64::MAX as f64 =>
        {
            Some(*value as i64)
        }
        _ => None,
    }
}

fn value_is_serializable(value: &Value) -> bool {
    matches!(
        value,
        Value::Boolean(_)
            | Value::Integer(_)
            | Value::Number(_)
            | Value::String(_)
            | Value::Table(_)
    )
}

fn key_is_serializable(value: &Value) -> bool {
    matches!(
        value,
        Value::Boolean(_) | Value::Integer(_) | Value::Number(_) | Value::String(_)
    )
}

fn key_is_reserved(value: &Value) -> bool {
    matches!(value, Value::String(value) if matches!(value.as_bytes().as_ref(), b"_G" | b"this"))
}

fn identifier_is_valid(bytes: &[u8]) -> bool {
    let Some((&first, rest)) = bytes.split_first() else {
        return false;
    };
    if !(first == b'_' || first.is_ascii_alphabetic())
        || !rest
            .iter()
            .all(|byte| *byte == b'_' || byte.is_ascii_alphanumeric())
    {
        return false;
    }
    !matches!(
        bytes,
        b"and"
            | b"break"
            | b"do"
            | b"else"
            | b"elseif"
            | b"end"
            | b"false"
            | b"for"
            | b"function"
            | b"if"
            | b"in"
            | b"local"
            | b"nil"
            | b"not"
            | b"or"
            | b"repeat"
            | b"return"
            | b"then"
            | b"true"
            | b"until"
            | b"while"
    )
}
