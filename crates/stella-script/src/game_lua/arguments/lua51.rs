//! Coercions used only by hand-written Lua 5.1 C-API members.

use mlua::Value;

/// Lua 5.1's direct `lua_isnumber`/`lua_tonumber` pair also accepts numeric
/// strings. Generated adapters use `arguments::strict` instead.
pub(crate) fn native_lua51_number(value: &Value) -> Option<f64> {
    match value {
        Value::Integer(value) => Some(*value as f64),
        Value::Number(value) => Some(*value),
        Value::String(value) => parse_number(value.as_bytes().as_ref()),
        _ => None,
    }
}

fn parse_number(bytes: &[u8]) -> Option<f64> {
    let text = std::str::from_utf8(bytes).ok()?.trim();
    if text.is_empty() {
        return None;
    }
    if let Ok(number) = text.parse::<f64>() {
        return Some(number);
    }

    // luaO_str2d falls back to hexadecimal integer parsing if strtod does
    // not consume an 0x-prefixed value.
    let (sign, unsigned) = match text.as_bytes().first() {
        Some(b'+') => (1.0, &text[1..]),
        Some(b'-') => (-1.0, &text[1..]),
        _ => (1.0, text),
    };
    let digits = unsigned
        .strip_prefix("0x")
        .or_else(|| unsigned.strip_prefix("0X"))?;
    (!digits.is_empty())
        .then(|| u64::from_str_radix(digits, 16).ok())
        .flatten()
        .map(|number| sign * number as f64)
}

/// Lua 5.1's `lua_isstring`/`lua_tolstring` accepts strings and numbers.
pub(crate) fn native_lua51_string(value: &Value) -> Option<String> {
    match value {
        Value::String(value) => Some(value.to_string_lossy()),
        Value::Integer(value) => Some(value.to_string()),
        Value::Number(value) => Some(value.to_string()),
        _ => None,
    }
}
