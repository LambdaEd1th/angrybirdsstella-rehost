//! AnimationWrapper shader-table decoding.

use std::collections::BTreeMap;

use mlua::{Result as LuaResult, Value};

use crate::SpriteShader;

fn native_shader_number(value: Value) -> Option<f64> {
    let value = match value {
        Value::Integer(value) => value as f64,
        Value::Number(value) => value,
        Value::String(value) => value
            .to_str()
            .ok()
            .and_then(|value| value.trim().parse::<f64>().ok())?,
        _ => return None,
    };
    Some(f64::from(value as f32))
}

pub(crate) fn lua_vector4(value: Value) -> LuaResult<[f64; 4]> {
    let table = match value {
        Value::Table(table) => table,
        _ => return Ok([1.0; 4]),
    };
    let mut vector = [1.0; 4];
    for (index, channel) in vector.iter_mut().enumerate() {
        *channel = native_shader_number(table.raw_get::<Value>(index + 1)?).unwrap_or(1.0);
    }
    Ok(vector)
}

pub(crate) fn sprite_shader_from_lua(
    table: mlua::Table,
    cache: &mut BTreeMap<String, SpriteShader>,
) -> LuaResult<SpriteShader> {
    let name = table.get::<String>("name").unwrap_or_default();
    // sub_10006CB08 installs a newly constructed shader in the shared cache
    // before it reads `params`, then mutates that cached object in place.
    let shader = cache.entry(name.clone()).or_insert_with(|| SpriteShader {
        name,
        ..SpriteShader::default()
    });
    let params = table.get::<mlua::Table>("params")?;
    for parameter in params.sequence_values::<mlua::Table>() {
        let parameter = parameter?;
        let name = parameter.get::<String>("name").unwrap_or_default();
        let parameter_type = parameter.get::<String>("type").unwrap_or_default();
        let value = parameter.get::<Value>("value")?;
        if parameter_type == "vector" {
            if name == "DIFFUSEC" {
                shader.diffuse = lua_vector4(value)?;
            }
            continue;
        }
        let value = native_shader_number(value).unwrap_or(0.0);
        match (name.as_str(), value) {
            ("LIGHTNESS", value) => shader.lightness = value,
            ("SATURATION", value) => shader.saturation = value,
            ("HIGHLIGHT", value) => shader.highlight = value,
            _ => {}
        }
    }
    Ok(shader.clone())
}
