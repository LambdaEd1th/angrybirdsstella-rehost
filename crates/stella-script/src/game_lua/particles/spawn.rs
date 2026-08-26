//! Cached-definition emitter member `sub_10008E524`.

use mlua::{Lua, Result as LuaResult};

use crate::*;

use super::ParticleDefinition;

const SPAWN_FUNCTION: &str = "native_addParticlesWithMode";

#[derive(Debug, Clone)]
pub(crate) struct ParticleSpawnQuery {
    definition_name: String,
    requested_amount: i32,
    x: f32,
    y: f32,
    width: f32,
    height: f32,
    angle: f32,
    requested_mode: i32,
    z: f32,
    theme_layer_index: i32,
    ignore_time_multiplier: bool,
}

/// Fully resolved input to the native 0x68-byte `ParticleData` constructor.
///
/// ThemeParticleSystem keeps a copied Lua query in its 0x58-byte Spawner and
/// invokes the normal Particles virtual add member whenever its timer expires.
/// Rust cannot retain an `mlua::Table` inside the cross-thread render bridge,
/// so resolve the immutable numeric/string payload once while preserving the
/// same cached `ParticleSystemData` ownership for each particle-system
/// instance.
#[derive(Debug, Clone)]
pub(crate) struct ParticleEmitter {
    definition: ParticleDefinition,
    requested_amount: i32,
    x: f32,
    y: f32,
    emitter_width: f32,
    emitter_height: f32,
    spawner_angle: f32,
    min_emitter_angle: f32,
    max_emitter_angle: f32,
    min_velocity: f32,
    max_velocity: f32,
    min_angle: f32,
    max_angle: f32,
    min_angular_velocity: f32,
    max_angular_velocity: f32,
    min_scale_begin: f32,
    max_scale_begin: f32,
    min_scale_end: f32,
    max_scale_end: f32,
    lifetime: f32,
    mode: i32,
    z: f32,
    theme_layer_index: i32,
    ignore_time_multiplier: bool,
}

fn query_number(query: &mlua::Table, name: &str, fallback: f32) -> f32 {
    query
        .get::<f64>(name)
        .map(|value| value as f32)
        .unwrap_or(fallback)
}

fn optional_bool(table: &mlua::Table, name: &str) -> Option<bool> {
    match table.get::<Value>(name).ok()? {
        Value::Boolean(value) => Some(value),
        _ => None,
    }
}

impl ParticleSpawnQuery {
    fn parse(query: &mlua::Table) -> LuaResult<Self> {
        let definition_name = table_required_string(query, "definitionName", SPAWN_FUNCTION)?;
        let requested_amount = native_fcvtzs_f32(query.get::<f64>("amount").unwrap_or(0.0) as f32);
        let x = table_required_number(query, "x", SPAWN_FUNCTION)? as f32;
        let y = table_required_number(query, "y", SPAWN_FUNCTION)? as f32;
        let width = table_required_number(query, "w", SPAWN_FUNCTION)? as f32;
        let height = table_required_number(query, "h", SPAWN_FUNCTION)? as f32;
        let angle = table_required_number(query, "angle", SPAWN_FUNCTION)? as f32;
        let z = query
            .get::<f64>("z")
            .map(|value| value as f32)
            .unwrap_or(0.0);
        let theme_layer_index = query
            .get::<f64>("themeLayerIndex")
            .map(|value| native_fcvtzs_f32(value as f32))
            .unwrap_or(-1);
        // Unlike most overrides, mode is read directly with the strict
        // numeric accessor before the definition cache is consulted.
        let requested_mode =
            native_fcvtzs_f32(table_required_number(query, "mode", SPAWN_FUNCTION)? as f32);
        let ignore_time_multiplier = optional_bool(query, "ignoreDeltaTimeMultiplier")
            .unwrap_or(matches!(requested_mode, 3 | 4));
        Ok(Self {
            definition_name,
            requested_amount,
            x,
            y,
            width,
            height,
            angle,
            requested_mode,
            z,
            theme_layer_index,
            ignore_time_multiplier,
        })
    }
}

fn load_definition(lua: &Lua, name: &str) -> LuaResult<mlua::Table> {
    let environment = game_environment(lua)?;
    let Value::Table(particle_table) = environment.get::<Value>("particleTable")? else {
        return Err(runtime_error(
            "native_addParticlesWithMode particleTable must be table",
        ));
    };
    let Value::Table(definitions) = particle_table.get::<Value>("particles")? else {
        return Err(runtime_error(
            "native_addParticlesWithMode particleTable.particles must be table",
        ));
    };
    let Value::Table(definition) = definitions.get::<Value>(name)? else {
        return Err(runtime_error(format!(
            "native_addParticlesWithMode definition {name} must be table"
        )));
    };
    Ok(definition)
}

fn cached_definition(
    lua: &Lua,
    definitions: &mut BTreeMap<String, ParticleDefinition>,
    query: &mlua::Table,
    name: &str,
) -> LuaResult<ParticleDefinition> {
    if let Some(definition) = definitions.get(name) {
        return Ok(definition.clone());
    }
    let definition = load_definition(lua, name)?;

    // `sub_10008E524` stores this flag in the cached 0x80-byte definition.
    // Consequently only the first use of a definition observes a query-level
    // override; later calls reuse the original value with every other field.
    let reference_ignore_limits = definition
        .get::<mlua::Table>("reference")
        .ok()
        .and_then(|reference| optional_bool(&reference, "ignoreLimits"))
        .unwrap_or(false);
    let ignore_limits = optional_bool(query, "ignoreLimits").unwrap_or(reference_ignore_limits);
    let definition = ParticleDefinition::parse(&definition, ignore_limits)?;
    definitions.insert(name.to_owned(), definition.clone());
    Ok(definition)
}

pub(crate) fn prepare_particle_emitter(
    lua: &Lua,
    definitions: &mut BTreeMap<String, ParticleDefinition>,
    query: &mlua::Table,
) -> LuaResult<ParticleEmitter> {
    // `sub_10008E524` consumes every base emitter field before its cache
    // lookup. Parse them first so malformed calls fail even when the named
    // definition does not exist or has already been cached.
    let spawn = ParticleSpawnQuery::parse(query)?;
    let definition = cached_definition(lua, definitions, query, &spawn.definition_name)?;
    let min_emitter_angle = query_number(query, "minAngleEmitter", definition.min_emitter_angle);
    let max_emitter_angle = query_number(query, "maxAngleEmitter", definition.max_emitter_angle);
    let min_velocity = query_number(query, "minVel", definition.min_velocity);
    let max_velocity = query_number(query, "maxVel", definition.max_velocity);
    let min_angle = query_number(query, "minAngle", definition.min_angle);
    let max_angle = query_number(query, "maxAngle", definition.max_angle);
    let min_angular_velocity = query_number(query, "minAngleVel", definition.min_angular_velocity);
    let max_angular_velocity = query_number(query, "maxAngleVel", definition.max_angular_velocity);
    let min_scale_begin = query_number(query, "minScaleBegin", definition.min_scale_begin);
    let max_scale_begin = query_number(query, "maxScaleBegin", definition.max_scale_begin);
    let min_scale_end = query_number(query, "minScaleEnd", definition.min_scale_end);
    let max_scale_end = query_number(query, "maxScaleEnd", definition.max_scale_end);
    let lifetime = query_number(query, "lifeTime", definition.lifetime);
    let mode = if spawn.requested_mode != 0 {
        spawn.requested_mode
    } else if definition.background {
        2
    } else {
        1
    };

    Ok(ParticleEmitter {
        emitter_width: spawn.width + definition.area_width,
        emitter_height: spawn.height + definition.area_height,
        definition,
        requested_amount: spawn.requested_amount,
        x: spawn.x,
        y: spawn.y,
        spawner_angle: spawn.angle,
        min_emitter_angle,
        max_emitter_angle,
        min_velocity,
        max_velocity,
        min_angle,
        max_angle,
        min_angular_velocity,
        max_angular_velocity,
        min_scale_begin,
        max_scale_begin,
        min_scale_end,
        max_scale_end,
        lifetime,
        mode,
        z: spawn.z,
        theme_layer_index: spawn.theme_layer_index,
        ignore_time_multiplier: spawn.ignore_time_multiplier,
    })
}

pub(crate) fn emit_particles(
    particles: &mut Vec<Particle>,
    random: &mut NativeParticleRandom,
    emitter: &ParticleEmitter,
    limit_particle_count: usize,
    bindings: Option<(&ResourceRuntime, &Path)>,
) {
    let definition = &emitter.definition;
    if definition.sprites.is_empty() {
        return;
    }

    // Amounts narrow through f32 then truncate. Explicit zero selects the
    // cached definition amount rather than requesting an empty burst.
    let amount = if emitter.requested_amount != 0 {
        emitter.requested_amount
    } else {
        definition.amount
    };
    let mut amount = amount;
    let native_limit_sum = |amount: i32| {
        (i64::try_from(limit_particle_count).unwrap_or(i64::MAX) + i64::from(amount)) as u64
    };
    if native_limit_sum(amount) >= 61 && !definition.ignore_limits && amount >= 2 {
        amount /= 2;
    }
    if native_limit_sum(amount) >= 1_001 && !definition.ignore_limits {
        amount = 1_000_i32.saturating_sub(i32::try_from(limit_particle_count).unwrap_or(i32::MAX));
    }
    if amount < 1 {
        return;
    }

    for _ in 0..amount as usize {
        let offset_x = (((random.next() as f32) - 0.5_f32) * emitter.emitter_width)
            * definition.emitter_scale_x;
        let offset_y = (((random.next() as f32) - 0.5_f32) * emitter.emitter_height)
            * definition.emitter_scale_y;
        let (sine, cosine) = emitter.spawner_angle.sin_cos();
        let mut velocity_angle = (emitter.max_emitter_angle - emitter.min_emitter_angle)
            .mul_add(random.next() as f32, emitter.min_emitter_angle);
        if definition.use_spawner_angle {
            velocity_angle += emitter.spawner_angle;
        }
        let velocity = (emitter.max_velocity - emitter.min_velocity)
            .mul_add(random.next() as f32, emitter.min_velocity);
        let angle_base = if definition.use_spawner_angle {
            emitter.spawner_angle + emitter.min_angle
        } else {
            emitter.min_angle
        };
        let angle =
            (emitter.max_angle - emitter.min_angle).mul_add(random.next() as f32, angle_base);
        let angular_velocity = (emitter.max_angular_velocity - emitter.min_angular_velocity)
            .mul_add(random.next() as f32, emitter.min_angular_velocity);
        let scale_begin = (emitter.max_scale_begin - emitter.min_scale_begin)
            .mul_add(random.next() as f32, emitter.min_scale_begin);
        let scale_end = (emitter.max_scale_end - emitter.min_scale_end)
            .mul_add(random.next() as f32, emitter.min_scale_end);
        let sprite = if definition.animate_over_lifetime {
            definition.sprites[0].clone()
        } else {
            let sprite_index = (random.next() * definition.sprites.len() as f64) as usize;
            definition.sprites[sprite_index.min(definition.sprites.len() - 1)].clone()
        };
        let (velocity_sine, velocity_cosine) = velocity_angle.sin_cos();
        let mut particle = Particle {
            sprite: sprite.into(),
            sprites: definition.sprites.clone(),
            bound_region: None,
            bound_composite: None,
            x: offset_x.mul_add(cosine, (-offset_y).mul_add(sine, emitter.x)),
            y: offset_x.mul_add(sine, offset_y.mul_add(cosine, emitter.y)),
            velocity_x: velocity * velocity_cosine,
            velocity_y: velocity * velocity_sine,
            gravity_x: definition.gravity_x,
            gravity_y: definition.gravity_y,
            angle,
            angular_velocity,
            scale_begin,
            scale_end,
            current_scale: scale_begin,
            elapsed: 0.0,
            lifetime: emitter.lifetime,
            animation_frame: 0,
            animate_over_lifetime: definition.animate_over_lifetime,
            mode: emitter.mode,
            z: emitter.z,
            theme_layer_index: emitter.theme_layer_index,
            ignore_time_multiplier: emitter.ignore_time_multiplier,
        };
        if let Some((resources, data_root)) = bindings {
            particle.bind_sprite(resources, data_root);
        }
        particles.push(particle);
    }
}

pub(crate) fn spawn_particles(
    lua: &Lua,
    bridge: &mut RenderBridge,
    query: &mlua::Table,
    resources: &ResourceRuntime,
    data_root: &Path,
) -> LuaResult<()> {
    let emitter = prepare_particle_emitter(lua, &mut bridge.particle_system.definitions, query)?;
    let limit_particle_count = bridge.particle_system.particles.len();
    emit_particles(
        &mut bridge.particle_system.particles,
        &mut bridge.particle_random,
        &emitter,
        limit_particle_count,
        Some((resources, data_root)),
    );
    Ok(())
}
