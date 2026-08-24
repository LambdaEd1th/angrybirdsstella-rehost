//! Cached 0x80-byte `ParticleSystemData` parsed by `sub_10008E524`.

use crate::*;

const DEGREES_TO_RADIANS: f32 = f32::from_bits(0x3c8e_fa35);

#[derive(Debug, Clone)]
pub(crate) struct ParticleDefinition {
    pub(crate) sprites: Vec<String>,
    pub(crate) min_velocity: f32,
    pub(crate) max_velocity: f32,
    pub(crate) min_angular_velocity: f32,
    pub(crate) max_angular_velocity: f32,
    pub(crate) min_scale_begin: f32,
    pub(crate) max_scale_begin: f32,
    pub(crate) min_scale_end: f32,
    pub(crate) max_scale_end: f32,
    pub(crate) lifetime: f32,
    pub(crate) gravity_x: f32,
    pub(crate) gravity_y: f32,
    pub(crate) min_emitter_angle: f32,
    pub(crate) max_emitter_angle: f32,
    pub(crate) min_angle: f32,
    pub(crate) max_angle: f32,
    pub(crate) emitter_scale_x: f32,
    pub(crate) emitter_scale_y: f32,
    pub(crate) area_width: f32,
    pub(crate) area_height: f32,
    pub(crate) amount: i32,
    pub(crate) ignore_limits: bool,
    pub(crate) use_spawner_angle: bool,
    pub(crate) animate_over_lifetime: bool,
    pub(crate) background: bool,
}

impl ParticleDefinition {
    pub(crate) fn parse(definition: &mlua::Table, ignore_limits: bool) -> LuaResult<Self> {
        let number = |name| -> LuaResult<f32> {
            Ok(table_required_number(definition, name, "native_addParticlesWithMode")? as f32)
        };
        let boolean = |name| match definition.get::<Value>(name) {
            Ok(Value::Boolean(value)) => value,
            _ => false,
        };
        let optional_number = |name, fallback| {
            definition
                .get::<f64>(name)
                .map(|value| value as f32)
                .unwrap_or(fallback)
        };

        // The cache record is populated in this same order by
        // `sub_10008E524`: all unguarded accesses below are strict.
        let amount = native_fcvtzs_f32(optional_number("amount", 0.0));
        let gravity_x = number("gravityX")?;
        let gravity_y = number("gravityY")?;
        let min_velocity = number("minVel")?;
        let max_velocity = number("maxVel")?;
        let min_angular_velocity = number("minAngleVel")?;
        let max_angular_velocity = number("maxAngleVel")?;
        let min_scale_begin = number("minScaleBegin")?;
        let max_scale_begin = number("maxScaleBegin")?;
        let min_scale_end = number("minScaleEnd")?;
        let max_scale_end = number("maxScaleEnd")?;
        let min_emitter_angle = number("minAngleEmitter")? * DEGREES_TO_RADIANS;
        let max_emitter_angle = number("maxAngleEmitter")? * DEGREES_TO_RADIANS;
        let min_angle = number("minAngle")? * DEGREES_TO_RADIANS;
        let max_angle = number("maxAngle")? * DEGREES_TO_RADIANS;
        let lifetime = number("lifeTime")?;
        let area_width = optional_number("areaW", 0.0);
        let area_height = optional_number("areaH", 0.0);
        let emitter_scale_x = optional_number("emitAreaScaleX", 1.0);
        let emitter_scale_y = optional_number("emitAreaScaleY", 1.0);
        let animate_over_lifetime = definition
            .get::<String>("animation")
            .is_ok_and(|animation| animation == "lifeTime");
        let Value::Table(sprite_table) = definition.get::<Value>("sprites")? else {
            return Err(runtime_error(
                "native_addParticlesWithMode table field sprites must be table",
            ));
        };
        let mut sprites = Vec::with_capacity(sprite_table.raw_len());
        for index in 1..=sprite_table.raw_len() {
            let Value::String(sprite) = sprite_table.raw_get::<Value>(index)? else {
                return Err(runtime_error(format!(
                    "native_addParticlesWithMode sprites entry #{index} must be string"
                )));
            };
            sprites.push(sprite.to_str()?.to_owned());
        }

        Ok(Self {
            sprites,
            min_velocity,
            max_velocity,
            min_angular_velocity,
            max_angular_velocity,
            min_scale_begin,
            max_scale_begin,
            min_scale_end,
            max_scale_end,
            lifetime,
            gravity_x,
            gravity_y,
            min_emitter_angle,
            max_emitter_angle,
            min_angle,
            max_angle,
            emitter_scale_x,
            emitter_scale_y,
            area_width,
            area_height,
            amount,
            ignore_limits,
            use_spawner_angle: boolean("useAngleFromSpawner"),
            animate_over_lifetime,
            background: boolean("background"),
        })
    }
}
