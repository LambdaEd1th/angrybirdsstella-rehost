//! Diagnostic formatting kept outside the observable Lua coercion paths.

use mlua::{MultiValue, Value};

pub(crate) fn describe_value(value: &Value) -> String {
    match value {
        Value::Nil => "nil".to_owned(),
        Value::Boolean(value) => value.to_string(),
        Value::Integer(value) => value.to_string(),
        Value::Number(value) => value.to_string(),
        Value::String(value) => format!("{:?}", value.to_string_lossy()),
        Value::Table(_) => "<table>".to_owned(),
        Value::Function(_) => "<function>".to_owned(),
        Value::Thread(_) => "<thread>".to_owned(),
        Value::UserData(_) => "<userdata>".to_owned(),
        Value::LightUserData(_) => "<lightuserdata>".to_owned(),
        Value::Error(error) => format!("<error:{error}>"),
        Value::Other(_) => "<other>".to_owned(),
    }
}

pub(crate) fn trace_object_loader(name: &str, args: &MultiValue) {
    if std::env::var_os("STELLA_TRACE_LUA_LOADS").is_some() {
        let rendered = args
            .iter()
            .map(describe_value)
            .collect::<Vec<_>>()
            .join(", ");
        eprintln!("{name}({rendered})");
    }
}
