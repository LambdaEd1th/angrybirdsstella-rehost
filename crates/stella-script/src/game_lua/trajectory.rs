//! BirdSimulation predictor and AimStream math recovered from Purple.

use mlua::{Lua, Result as LuaResult, Value};

use crate::{SceneObject, game_environment, native_fcvtzs_f32, value_number};

/// One of the two native 0x38-byte flight-trail records beginning at
/// GameLua+0x558. These records are independent from the simulation trajectory
/// vector at GameLua+0x590 and from AimStream.
#[derive(Debug, Clone, Default)]
pub(crate) struct NativeTrajectoryBuffer {
    pub(crate) points: Vec<(f64, f64)>,
    pub(crate) puff: Option<(f64, f64)>,
    pub(crate) normal_sprite: String,
    pub(crate) special_sprite: String,
}

/// AimStream::StreamParticle is exactly three packed float32 values: the
/// Catmull-Rom path parameter, rotation and per-particle render scale.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct NativeAimParticle {
    pub(crate) path_parameter: f32,
    pub(crate) angle: f32,
    pub(crate) scale: f32,
}

/// Rovio's trajectory predictor does not invoke the ordinary Box2D island
/// solver. `sub_10086F6AC` is a custom one-body step which integrates the
/// supplied body without contacts, while retaining Box2D's damping and
/// maximum translation/rotation guards.
pub(crate) fn step_native_trajectory_body(
    object: &mut SceneObject,
    world_gravity: (f32, f32),
    step: f32,
) {
    // FCMP/B.LE also rejects an unordered (NaN) step.
    if !matches!(
        step.partial_cmp(&0.0_f32),
        Some(std::cmp::Ordering::Greater)
    ) {
        return;
    }

    let inverse_mass = object.inverse_mass as f32;
    let acceleration_x = inverse_mass.mul_add(object.force_x as f32, world_gravity.0);
    let acceleration_y = inverse_mass.mul_add(object.force_y as f32, world_gravity.1);
    let mut velocity_x = acceleration_x.mul_add(step, object.velocity_x as f32);
    let mut velocity_y = acceleration_y.mul_add(step, object.velocity_y as f32);

    let angular_step = (object.inverse_inertia() as f32) * step;
    let mut angular_velocity =
        angular_step.mul_add(object.torque as f32, object.angular_velocity as f32);
    let linear_drag = (-(object.linear_damping as f32))
        .mul_add(step, 1.0_f32)
        .clamp(0.0_f32, 1.0_f32);
    velocity_x *= linear_drag;
    velocity_y *= linear_drag;
    let angular_drag = (-(object.angular_damping as f32))
        .mul_add(step, 1.0_f32)
        .clamp(0.0_f32, 1.0_f32);
    angular_velocity *= angular_drag;

    let translation_x = velocity_x * step;
    let translation_y = velocity_y * step;
    let translation_squared = translation_x.mul_add(translation_x, translation_y * translation_y);
    const MAX_TRANSLATION_SQUARED: f32 = f32::from_bits(0x3cd1_b717); // 0.0256f
    const MAX_TRANSLATION: f32 = f32::from_bits(0x3e23_d70a); // 0.16f
    if translation_squared > MAX_TRANSLATION_SQUARED {
        let scale = MAX_TRANSLATION / translation_squared.sqrt();
        velocity_x *= scale;
        velocity_y *= scale;
    }

    let rotation = angular_velocity * step;
    const MAX_ROTATION_SQUARED: f32 = f32::from_bits(0x401d_e9e7);
    const MAX_ROTATION: f32 = f32::from_bits(0x3fc9_0fdb); // pi/2
    if rotation * rotation > MAX_ROTATION_SQUARED {
        let scale = MAX_ROTATION / rotation.abs();
        angular_velocity *= scale;
    }

    object.velocity_x = f64::from(velocity_x);
    object.velocity_y = f64::from(velocity_y);
    object.angular_velocity = f64::from(angular_velocity);
    object.apply_native_trajectory_velocity_step(step, velocity_x, velocity_y, angular_velocity);
}

pub(crate) fn native_trajectory_settings(lua: &Lua) -> LuaResult<(f32, i32, f32, i32)> {
    let environment = game_environment(lua)?;
    // GameLua+0x408/+0x420 is the persistent LuaObject named `objects`.
    // Both sub_100032970 and sub_10004B8EC resolve currentTimeStep from that
    // table, not from the global environment. GameScene deliberately selects
    // 1/90 while a bird is aimed and 1/30 otherwise.
    let objects = environment.get::<mlua::Table>("objects").ok();
    let current_time_step = objects
        .as_ref()
        .and_then(|objects| objects.get::<Value>("currentTimeStep").ok())
        .as_ref()
        .and_then(value_number)
        .map_or(f32::from_bits(0x3cea_0ea1), |value| value as f32);
    let world_attributes = environment.get::<mlua::Table>("worldAttributes").ok();
    let attribute_number = |name: &str, fallback: f32| {
        world_attributes
            .as_ref()
            .and_then(|attributes| attributes.get::<Value>(name).ok())
            .as_ref()
            .and_then(value_number)
            .map_or(fallback, |value| value as f32)
    };
    // Level initialization at sub_100066978 narrows both integer fields with
    // FCVTZS and retains the time multiplier as float32.
    let iterations = native_fcvtzs_f32(attribute_number("simulationIterations", 50.0_f32));
    let time_step_multiplier = attribute_number("simulationTimeStepMultiplier", 3.0_f32);
    let point_sampler =
        native_fcvtzs_f32(attribute_number("simulationStorePointsSampler", 1.0_f32));
    Ok((
        current_time_step,
        iterations,
        time_step_multiplier,
        point_sampler,
    ))
}

pub(crate) fn native_aim_stream_settings(lua: &Lua) -> LuaResult<(f32, f32)> {
    let environment = game_environment(lua)?;
    let world_attributes = environment.get::<mlua::Table>("worldAttributes").ok();
    let attribute = |name: &str, fallback: f32| {
        world_attributes
            .as_ref()
            .and_then(|attributes| attributes.get::<Value>(name).ok())
            .as_ref()
            .and_then(value_number)
            .map_or(fallback, |value| value as f32)
    };
    // AimStream's constructor stores 0.6f/4.0f. Level initialization at
    // sub_100066A6C replaces them with the two worldAttributes float32s (the
    // shipped table uses 0.6f/5.0f), resets both vectors and deactivates it.
    Ok((
        attribute("simulationAimSpawnTime", 0.6_f32),
        attribute("simulationAimSpeed", 4.0_f32),
    ))
}

pub(crate) fn native_aim_stream_point(
    control_points: &[(f64, f64)],
    path_parameter: f32,
) -> Option<(f32, f32)> {
    if control_points.len() < 4 || !path_parameter.is_finite() || path_parameter < 0.0 {
        return None;
    }
    let segment = path_parameter.trunc() as usize;
    if segment + 3 >= control_points.len() {
        return None;
    }
    let amount = path_parameter.fract();
    let p0 = (
        control_points[segment].0 as f32,
        control_points[segment].1 as f32,
    );
    let p1 = (
        control_points[segment + 1].0 as f32,
        control_points[segment + 1].1 as f32,
    );
    let p2 = (
        control_points[segment + 2].0 as f32,
        control_points[segment + 2].1 as f32,
    );
    let p3 = (
        control_points[segment + 3].0 as f32,
        control_points[segment + 3].1 as f32,
    );
    let amount_squared = amount * amount;
    let amount_cubed = amount.powf(3.0_f32);
    let interpolate = |p0: f32, p1: f32, p2: f32, p3: f32| {
        let linear = amount * (p2 - p0);
        let base = p1.mul_add(2.0_f32, linear);
        let mut quadratic = p1 * -5.0_f32;
        quadratic = p0.mul_add(2.0_f32, quadratic);
        quadratic = p2.mul_add(4.0_f32, quadratic);
        quadratic -= p3;
        let through_quadratic = amount_squared.mul_add(quadratic, base);
        let cubic = (-p2).mul_add(3.0_f32, p1.mul_add(3.0_f32, -p0)) + p3;
        cubic.mul_add(amount_cubed, through_quadratic) * 0.5_f32
    };
    Some((
        interpolate(p0.0, p1.0, p2.0, p3.0),
        interpolate(p0.1, p1.1, p2.1, p3.1),
    ))
}
