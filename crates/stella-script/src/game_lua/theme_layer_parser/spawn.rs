//! `spawnParameters` expansion in the native layer-list facade
//! (`sub_10006B9A4`).

use mlua::{Result as LuaResult, Value};

use super::ThemeLayerOverrides;
use crate::*;

pub(super) fn parse_spawn_parameters(
    definition: &mlua::Table,
) -> LuaResult<Option<ThemeSpawnParameters>> {
    let Value::Table(parameters) = definition.get::<Value>("spawnParameters")? else {
        return Ok(None);
    };
    let amount = native_fcvtzs_f32(parameters.get::<f64>("amount").unwrap_or(0.0) as f32);
    let x_speed_variance = parameters.get::<f64>("xSpeedVariance").unwrap_or(0.0) as f32;
    let y_speed_variance = parameters.get::<f64>("ySpeedVariance").unwrap_or(0.0) as f32;
    let area = match parameters.get::<Value>("area")? {
        Value::Table(area) => ThemeSpawnArea {
            screen_x: area.get::<f64>("screenX").unwrap_or(0.0) as f32,
            screen_y: area.get::<f64>("screenY").unwrap_or(0.0) as f32,
            screen_width: area.get::<f64>("screenW").unwrap_or(0.0) as f32,
            screen_height: area.get::<f64>("screenH").unwrap_or(0.0) as f32,
            world_x: optional_world_coordinate(&area, "worldX")?,
            world_y: optional_world_coordinate(&area, "worldY")?,
            world_width: optional_world_coordinate(&area, "worldW")?,
            world_height: optional_world_coordinate(&area, "worldH")?,
        },
        _ => ThemeSpawnArea::default(),
    };
    Ok(Some(ThemeSpawnParameters {
        amount,
        x_speed_variance,
        y_speed_variance,
        area,
    }))
}

pub(super) fn sample_initial_overrides(
    definition: &mlua::Table,
    parameters: &ThemeSpawnParameters,
    random: &mut NativeParticleRandom,
) -> LuaResult<ThemeLayerOverrides> {
    // 0x10006C3F4..0x10006C518 consumes four samples per expanded layer in
    // X velocity, Y velocity, X position, Y position order. Velocity
    // variance is multiplied in double precision and narrowed before the
    // float32 addition; screen coordinates use double FMADD then narrow.
    let base_velocity_x = definition.get::<f64>("velX").unwrap_or(0.0) as f32;
    let velocity_x_variation = (random.next() * f64::from(parameters.x_speed_variance)) as f32;
    let velocity_x = base_velocity_x + velocity_x_variation;

    let base_velocity_y = definition.get::<f64>("velY").unwrap_or(0.0) as f32;
    let velocity_y_variation = (random.next() * f64::from(parameters.y_speed_variance)) as f32;
    let velocity_y = base_velocity_y + velocity_y_variation;

    let area = parameters.area;
    let screen_x_origin = f64::from(area.screen_width).mul_add(-0.5_f64, f64::from(area.screen_x));
    let offset_x = random
        .next()
        .mul_add(f64::from(area.screen_width), screen_x_origin) as f32;
    let screen_y_origin = f64::from(area.screen_height).mul_add(-0.5_f64, f64::from(area.screen_y));
    let offset_y = random
        .next()
        .mul_add(f64::from(area.screen_height), screen_y_origin) as f32;

    Ok(ThemeLayerOverrides {
        velocity_x: Some(velocity_x),
        velocity_y: Some(velocity_y),
        offset_x: Some(offset_x),
        offset_y: Some(offset_y),
        world_x: Some(area.world_x),
        world_y: Some(area.world_y),
        world_width: Some(area.world_width),
        world_height: Some(area.world_height),
    })
}

fn optional_world_coordinate(table: &mlua::Table, field: &str) -> LuaResult<Option<f32>> {
    Ok(table
        .get::<Value>(field)
        .ok()
        .as_ref()
        .and_then(native_lua51_number)
        .map(|value| value as f32)
        .filter(|value| value.to_bits() != f32::MAX.to_bits()))
}
