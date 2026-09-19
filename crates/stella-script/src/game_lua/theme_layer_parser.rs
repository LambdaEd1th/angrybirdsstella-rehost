//! Theme layer-list traversal (`sub_10006B9A4`).

mod layer;
mod spawn;

use mlua::{Lua, Result as LuaResult, Table, Value};

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

/// Reproduce the three throwing LuaObject table traversals shared by
/// setThemeOffsetY and native_setThemeFgLayerOffsetY.
pub(crate) fn required_named_theme_layers(
    lua: &Lua,
    theme_name: &str,
    layer_field: &str,
) -> LuaResult<Table> {
    let block_table = native_lua_object(lua, NativeLuaObject::BlockTable)?;
    let themes = required_index_table(
        match block_table {
            Some(block_table) => block_table.raw_get::<Value>("themes")?,
            None => Value::Nil,
        },
        "themes",
    )?;
    let theme = required_index_table(themes.raw_get::<Value>(theme_name)?, theme_name)?;
    required_index_table(theme.raw_get::<Value>(layer_field)?, layer_field)
}

pub(crate) fn required_named_theme_layer(layers: &Table, index: usize) -> LuaResult<Table> {
    // setThemeOffsetY calls the throwing integer-index LuaObject helper
    // sub_100070444 once for every live native layer. It writes each layer
    // immediately, so a later short/non-table entry preserves earlier writes.
    required_index_table(layers.raw_get::<Value>(index)?, &index.to_string())
}

fn required_index_table(value: Value, index: &str) -> LuaResult<Table> {
    match value {
        Value::Table(table) => Ok(table),
        value => Err(runtime_error(format!(
            "Tried to get a Lua table from index '{index}', but type was {}",
            value.type_name()
        ))),
    }
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
