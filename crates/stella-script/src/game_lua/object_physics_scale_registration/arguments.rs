//! Live Lua values consumed by `sub_10004050C` before fixture replacement.

use crate::*;

#[derive(Clone, Copy)]
pub(super) struct FixtureCoefficients {
    pub(super) density: f64,
    pub(super) friction: f64,
    pub(super) restitution: f64,
}

pub(super) fn required_world_entry(lua: &Lua, name: &str) -> LuaResult<mlua::Table> {
    match object_world(lua)?.raw_get::<Value>(name)? {
        Value::Table(entry) => Ok(entry),
        _ => Err(runtime_error(format!(
            "setPhysicsScale objects.world entry missing: {name}"
        ))),
    }
}

pub(super) fn coefficients(entry: &mlua::Table) -> LuaResult<FixtureCoefficients> {
    Ok(FixtureCoefficients {
        density: required_f32(entry, "density")?,
        friction: required_f32(entry, "friction")?,
        restitution: required_f32(entry, "restitution")?,
    })
}

pub(super) fn required_f32(entry: &mlua::Table, field: &str) -> LuaResult<f64> {
    Ok(f64::from(
        table_required_number(entry, field, "setPhysicsScale")? as f32,
    ))
}

pub(super) fn circle_definition_scale(lua: &Lua, entry: &mlua::Table) -> LuaResult<f32> {
    let Ok(definition_name) = entry.get::<String>("definition") else {
        return Ok(1.0);
    };
    let Value::Table(blocks) = game_environment(lua)?.get::<Value>("blocks")? else {
        return Ok(1.0);
    };
    let Value::Table(definition) = blocks.raw_get::<Value>(definition_name)? else {
        return Ok(1.0);
    };
    match definition.get::<Value>("scale")? {
        Value::Nil => Ok(1.0),
        Value::Integer(value) => Ok(value as f32),
        Value::Number(value) => Ok(value as f32),
        _ => Err(runtime_error(
            "setPhysicsScale block definition field scale must be number",
        )),
    }
}
