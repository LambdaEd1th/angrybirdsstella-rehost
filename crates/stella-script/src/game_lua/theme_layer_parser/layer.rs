//! One native theme-layer record constructor (`sub_10006855C`).

use std::collections::BTreeSet;

use mlua::{Result as LuaResult, Value};

use super::ThemeLayerOverrides;
use crate::*;

const MISSING_SPRITE_GEOMETRY: SpriteGeometry = SpriteGeometry {
    min_x: 0.0,
    min_y: 0.0,
    max_x: 0.0,
    max_y: 0.0,
};

pub(super) fn parse_theme_layer(
    definition: &mlua::Table,
    resources: &ResourceRuntime,
    random: &mut NativeParticleRandom,
    libc_random: &mut NativeLibcRandom,
    definition_index: usize,
    overrides: ThemeLayerOverrides,
    spawn_parameters: Option<ThemeSpawnParameters>,
) -> LuaResult<Option<ThemeLayer>> {
    let animation_frames = parse_animation_frames(definition)?;
    let Some(mut sprite) = animation_frames.first().cloned() else {
        return Ok(None);
    };

    // sub_10006855C asks the resource manager independently for left, right,
    // top and bottom. sub_10045CD14/60/AC/F8 all return zero when lookup fails;
    // the native record therefore has a zero rectangle, not a substitute tile.
    let animation_geometries = animation_frames
        .iter()
        .map(|sprite| {
            resources
                .active_geometry(sprite)
                .unwrap_or(MISSING_SPRITE_GEOMETRY)
        })
        .collect::<Vec<_>>();
    let mut geometry = animation_geometries[0];
    let sprite_is_table = matches!(definition.get::<Value>("sprite")?, Value::Table(_));
    let animation_speed_is_number = definition
        .get::<Value>("animationSpeed")
        .ok()
        .as_ref()
        .and_then(native_lua51_number)
        .is_some();
    if sprite_is_table && !animation_speed_is_number {
        // sub_100069004 selects a static table-valued sprite with the same
        // libc rand stream exposed by math.random. Animated tables retain
        // their first frame and consume no libc sample here.
        let selected = libc_random.next_word() as usize % animation_frames.len();
        sprite = animation_frames[selected].clone();
        geometry = animation_geometries[selected];
    }
    let offset_y = match definition.get::<Value>("offsetY")? {
        Value::String(value) if value.as_bytes() == b"top" => ThemeVerticalOffset::Top,
        Value::String(value) if value.as_bytes() == b"bottom" => ThemeVerticalOffset::Bottom,
        value => ThemeVerticalOffset::Pixels(f64::from(value_number(&value).unwrap_or(0.0) as f32)),
    };
    let uniform_scale = definition.get::<f64>("scale").unwrap_or(1.0) as f32;
    let scale_x = definition
        .get::<f64>("scaleX")
        .map(|value| value as f32)
        .unwrap_or(uniform_scale);
    let scale_y = definition
        .get::<f64>("scaleY")
        .map(|value| value as f32)
        .unwrap_or(uniform_scale);
    let flags = parse_flags(definition)?;
    let native_flags = native_layer_flags(&flags);
    // sub_10006855C parses all four values as float32. xSpeedAdd/ySpeedAdd
    // are folded into the layer velocities before the 0x130-byte record is
    // inserted (0x10006ABF4 and 0x10006ACAC).
    let velocity_x = overrides
        .velocity_x
        .unwrap_or(definition.get::<f64>("velX").unwrap_or(0.0) as f32)
        + definition.get::<f64>("xSpeedAdd").unwrap_or(0.0) as f32;
    let velocity_y = overrides
        .velocity_y
        .unwrap_or(definition.get::<f64>("velY").unwrap_or(0.0) as f32)
        + definition.get::<f64>("ySpeedAdd").unwrap_or(0.0) as f32;
    let relative_x = optional_f32(definition, "relativeX")?;
    let relative_y = optional_f32(definition, "relativeY")?;
    let world_x = overrides
        .world_x
        .unwrap_or(optional_f32(definition, "worldX")?);
    let world_y = overrides
        .world_y
        .unwrap_or(optional_f32(definition, "worldY")?);
    let world_width = overrides
        .world_width
        .unwrap_or(optional_f32(definition, "worldW")?);
    let world_height = overrides
        .world_height
        .unwrap_or(optional_f32(definition, "worldH")?);
    let (animation_timeline_definition, animation_timeline) =
        parse_animation_timeline(definition, random)?;

    Ok(Some(ThemeLayer {
        geometry,
        sprite,
        animation_frames,
        animation_geometries,
        animation_delay: f64::from(definition.get::<f64>("animationSpeed").unwrap_or(0.0) as f32),
        animation_timeline,
        animation_timeline_definition,
        animation_timer: 0.0,
        animation_frame: 0,
        definition_index,
        particles: definition
            .get::<Value>("particles")
            .ok()
            .as_ref()
            .and_then(native_lua51_string)
            .filter(|name| !name.is_empty()),
        spawn_interval: definition
            .get::<f64>("spawnInterval")
            .map(|value| value as f32)
            .unwrap_or(-1.0),
        spawner_id: definition
            .get::<Value>("spawnerId")
            .ok()
            .as_ref()
            .and_then(native_lua51_number)
            .map(|value| native_fcvtzs_f32(value as f32))
            .unwrap_or(0),
        spawn_parameters,
        position_x: f64::from(definition.get::<f64>("posX").unwrap_or(0.0) as f32),
        position_y: f64::from(definition.get::<f64>("posY").unwrap_or(0.0) as f32),
        offset_x: f64::from(
            overrides
                .offset_x
                .unwrap_or(definition.get::<f64>("offsetX").unwrap_or(0.0) as f32),
        ),
        offset_y: overrides
            .offset_y
            .map(|value| ThemeVerticalOffset::Pixels(f64::from(value)))
            .unwrap_or(offset_y),
        resolved_offset_y: None,
        scale_x: f64::from(scale_x),
        scale_y: f64::from(scale_y),
        // sub_10006855C stores these authored values in float32 slots. The
        // native defaults are parallaxSpeed=1, zDistance=0, scaleSpeed=1,
        // angleMult=0, xMult=0, and yMult=1.
        parallax_speed: f64::from(definition.get::<f64>("parallaxSpeed").unwrap_or(1.0) as f32),
        z_distance: f64::from(definition.get::<f64>("zDistance").unwrap_or(0.0) as f32),
        scale_speed: f64::from(definition.get::<f64>("scaleSpeed").unwrap_or(1.0) as f32),
        angle_multiplier: f64::from(definition.get::<f64>("angleMult").unwrap_or(0.0) as f32),
        x_multiplier: f64::from(definition.get::<f64>("xMult").unwrap_or(0.0) as f32),
        y_multiplier: f64::from(definition.get::<f64>("yMult").unwrap_or(1.0) as f32),
        alpha: f64::from(definition.get::<f64>("alpha").unwrap_or(1.0) as f32),
        min_alpha: f64::from(definition.get::<f64>("minAlpha").unwrap_or(1.0) as f32),
        max_alpha: f64::from(definition.get::<f64>("maxAlpha").unwrap_or(1.0) as f32),
        repeat_x: flags.contains("H_REPEAT"),
        repeat_y: flags.contains("V_REPEAT"),
        repeat_left_only: flags.contains("REPEAT_LEFT_ONLY"),
        repeat_right_only: flags.contains("REPEAT_RIGHT_ONLY"),
        native_flags,
        relative_x: relative_x.map(f64::from),
        relative_y: relative_y.map(f64::from),
        world_x: world_x.map(f64::from),
        world_y: world_y.map(f64::from),
        world_width: world_width.map(f64::from),
        world_height: world_height.map(f64::from),
        velocity_x: f64::from(velocity_x),
        velocity_y: f64::from(velocity_y),
        motion_y: 0.0,
    }))
}

fn optional_f32(definition: &mlua::Table, field: &str) -> LuaResult<Option<f32>> {
    Ok(definition
        .get::<Value>(field)
        .ok()
        .as_ref()
        .and_then(native_lua51_number)
        .map(|value| value as f32)
        .filter(|value| value.to_bits() != f32::MAX.to_bits()))
}

fn parse_animation_timeline(
    definition: &mlua::Table,
    random: &mut NativeParticleRandom,
) -> LuaResult<(Vec<ThemeAnimationTimelineEntry>, Vec<f32>)> {
    let Value::Table(timeline) = definition.get::<Value>("animationTimeline")? else {
        return Ok((Vec::new(), Vec::new()));
    };
    let mut definitions = Vec::with_capacity(timeline.raw_len());
    let mut sampled = Vec::with_capacity(timeline.raw_len());
    for index in 1..=timeline.raw_len() {
        let entry = match timeline.raw_get::<Value>(index)? {
            Value::Table(range) => ThemeAnimationTimelineEntry {
                base: range
                    .raw_get::<Value>(1)
                    .ok()
                    .as_ref()
                    .and_then(native_lua51_number)
                    .unwrap_or(0.0) as f32,
                variance: range
                    .raw_get::<Value>(2)
                    .ok()
                    .as_ref()
                    .and_then(native_lua51_number)
                    .unwrap_or(0.0) as f32,
                uses_random: true,
            },
            value => ThemeAnimationTimelineEntry {
                base: native_lua51_number(&value).unwrap_or(0.0) as f32,
                variance: 0.0,
                uses_random: false,
            },
        };
        let value = if entry.uses_random {
            (random.next() as f32).mul_add(entry.variance, entry.base)
        } else {
            entry.base
        };
        definitions.push(entry);
        sampled.push(value);
    }
    Ok((definitions, sampled))
}

fn parse_animation_frames(definition: &mlua::Table) -> LuaResult<Vec<String>> {
    match definition.get::<Value>("sprite")? {
        Value::String(sprite) => Ok(vec![sprite.to_str()?.to_owned()]),
        Value::Table(sprites) => sprites
            .sequence_values::<Value>()
            .map_while(|value| match value {
                Ok(Value::String(sprite)) => Some(sprite.to_str().map(|sprite| sprite.to_owned())),
                Ok(_) => None,
                Err(error) => Some(Err(error)),
            })
            .collect(),
        _ => Ok(Vec::new()),
    }
}

fn parse_flags(definition: &mlua::Table) -> LuaResult<BTreeSet<String>> {
    match definition.get::<Value>("flags")? {
        Value::Table(flags) => Ok(flags
            .sequence_values::<Value>()
            .filter_map(Result::ok)
            .filter_map(|value| value_string(&value))
            .collect()),
        _ => Ok(BTreeSet::new()),
    }
}

fn native_layer_flags(flags: &BTreeSet<String>) -> u32 {
    // sub_100069D0C..0x100069E18 in the 304-byte record constructor.
    flags.iter().fold(0_u32, |bits, flag| {
        bits | match flag.as_str() {
            "OVERSTRETCH_ANCHOR_V" => 0x001,
            "V_REPEAT" => 0x002,
            "H_REPEAT" => 0x004,
            "REFRESH_ANIMATION_TIMELINE" => 0x008,
            "REFRESH_ANIMATION_COORDINATES" => 0x010,
            "ANCHOR_V" => 0x020,
            "PREVENT_STRETCH_SCALE" => 0x080,
            "REPEAT_LEFT_ONLY" => 0x100,
            "REPEAT_RIGHT_ONLY" => 0x200,
            _ => 0,
        }
    })
}
