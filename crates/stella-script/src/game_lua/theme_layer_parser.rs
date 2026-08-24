//! Theme layer-list traversal (`sub_10006B9A4`).

mod layer;
mod spawn;

use mlua::{Lua, Result as LuaResult, Value};

use crate::*;

#[derive(Debug, Clone, Copy, Default)]
struct ThemeLayerOverrides {
    velocity_x: Option<f32>,
    velocity_y: Option<f32>,
    offset_x: Option<f32>,
    offset_y: Option<f32>,
    world_x: Option<Option<f32>>,
    world_y: Option<Option<f32>>,
    world_width: Option<Option<f32>>,
    world_height: Option<Option<f32>>,
}

pub(crate) fn named_theme_layer_offsets(
    lua: &Lua,
    theme_name: &str,
    layer_field: &str,
) -> LuaResult<Vec<Option<f32>>> {
    let environment = game_environment(lua)?;
    let Value::Table(block_table) = environment.get::<Value>("blockTable")? else {
        return Ok(Vec::new());
    };
    let Value::Table(themes) = block_table.get::<Value>("themes")? else {
        return Ok(Vec::new());
    };
    let Value::Table(theme) = themes.get::<Value>(theme_name)? else {
        return Ok(Vec::new());
    };
    let Value::Table(layers) = theme.get::<Value>(layer_field)? else {
        return Ok(Vec::new());
    };

    let mut offsets = Vec::with_capacity(layers.raw_len());
    for index in 1..=layers.raw_len() {
        let offset = match layers.raw_get::<Value>(index)? {
            Value::Table(layer) => theme_table_f32(&layer, "offsetY")?,
            _ => None,
        };
        offsets.push(offset);
    }
    Ok(offsets)
}

pub(crate) fn parse_theme_layers(
    theme: &mlua::Table,
    field: &str,
    resources: &ResourceRuntime,
    random: &mut NativeParticleRandom,
    libc_random: &mut NativeLibcRandom,
) -> LuaResult<Vec<ThemeLayer>> {
    let Value::Table(definitions) = theme.get::<Value>(field)? else {
        return Ok(Vec::new());
    };
    let mut layers = Vec::with_capacity(definitions.raw_len());
    for (zero_based_index, definition) in definitions.sequence_values::<Value>().enumerate() {
        let Value::Table(definition) = definition? else {
            continue;
        };
        let definition_index = zero_based_index + 1;
        if let Some(parameters) = spawn::parse_spawn_parameters(&definition)? {
            if parameters.amount >= 1 {
                for _ in 0..parameters.amount {
                    let overrides =
                        spawn::sample_initial_overrides(&definition, &parameters, random)?;
                    if let Some(layer) = layer::parse_theme_layer(
                        &definition,
                        resources,
                        random,
                        libc_random,
                        definition_index,
                        overrides,
                        Some(parameters.clone()),
                    )? {
                        layers.push(layer);
                    }
                }
            }
        } else if let Some(layer) = layer::parse_theme_layer(
            &definition,
            resources,
            random,
            libc_random,
            definition_index,
            ThemeLayerOverrides::default(),
            None,
        )? {
            layers.push(layer);
        }
    }
    Ok(layers)
}
