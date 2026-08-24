//! `GameLua::applySensorForces` dispatch and gravity branch (`sub_10005DE90`).

use crate::*;

use super::water::apply_native_water_sensor;

pub(crate) fn native_hypot(x: f32, y: f32) -> f32 {
    x.mul_add(x, y * y).sqrt()
}

/// Reimplementation of GameLua::applySensorForces at sub_10005DE90.
///
/// The original body, fixture and RenderObjectData state is float32. Keep the
/// calculations and accumulator writes at that precision even though the
/// portable solver stores its public state as f64.
pub(crate) fn apply_native_sensor_forces(
    bridge: &mut RenderBridge,
    sensor_name: &str,
    object_name: &str,
) {
    let Some(sensor) = bridge.scene.get(sensor_name).cloned() else {
        return;
    };
    let gravity_force_multiplier = bridge.gravity_force_multiplier as f32;
    let water_force_multiplier = bridge.water_force_multiplier as f32;
    let bird_water_drag = bridge.bird_water_drag as f32;
    let object_water_drag = bridge.object_water_drag as f32;
    let game_world_scale = bridge.game_world_scale as f32;
    let Some(object) = bridge.scene.get_mut(object_name) else {
        return;
    };

    apply_native_sensor_forces_to_object(
        &sensor,
        object,
        gravity_force_multiplier,
        water_force_multiplier,
        bird_water_drag,
        object_water_drag,
        game_world_scale,
    );
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn apply_native_sensor_forces_to_object(
    sensor: &SceneObject,
    object: &mut SceneObject,
    gravity_force_multiplier: f32,
    water_force_multiplier: f32,
    bird_water_drag: f32,
    object_water_drag: f32,
    game_world_scale: f32,
) {
    let mask = sensor.sensor_gravity_mask;
    if (mask >= 0 && (object.gravity_category & (mask | i32::MIN)) == 0)
        || !sensor.sensor_active
        || !object.dynamic_body
    {
        return;
    }

    if sensor.sensor_type == 2 && !sensor.is_water {
        apply_native_gravity_sensor(sensor, object, gravity_force_multiplier);
    } else {
        apply_native_water_sensor(
            sensor,
            object,
            game_world_scale,
            water_force_multiplier,
            bird_water_drag,
            object_water_drag,
        );
    }
}

fn apply_native_gravity_sensor(
    sensor: &SceneObject,
    object: &mut SceneObject,
    gravity_force_multiplier: f32,
) {
    let minimum = sensor.sensor_minimum_force as f32;
    let maximum = sensor.sensor_maximum_force as f32;
    let radius = sensor.sensor_radius as f32;
    let sensor_x = sensor.x as f32;
    let sensor_y = sensor.y as f32;
    let object_x = object.x as f32;
    let object_y = object.y as f32;

    let (mut direction_x, mut direction_y, falloff) = if radius > -1.0 {
        let delta_x = sensor_x - object_x;
        let delta_y = sensor_y - object_y;
        let distance = native_hypot(delta_x, delta_y);
        (delta_x, delta_y, (maximum - minimum) * (distance / radius))
    } else {
        let angle = sensor.angle as f32;
        let sine = f64::from(angle).sin() as f32;
        let cosine = f64::from(angle).cos() as f32;
        let height = sensor.sensor_height as f32;
        let half_height = f64::from(height) * 0.5;
        let reference_x = (f64::from(sensor_x) - f64::from(sine) * half_height) as f32;
        let reference_y = (f64::from(sensor_y) + f64::from(cosine) * half_height) as f32;
        let distance = native_hypot(reference_x - object_x, reference_y - object_y);
        (-sine, cosine, (maximum - minimum) * (distance / height))
    };

    let length = native_hypot(direction_x, direction_y);
    if length >= f32::EPSILON {
        let inverse_length = 1.0 / length;
        direction_x *= inverse_length;
        direction_y *= inverse_length;
    }
    let mut magnitude = (maximum - falloff) * (object.native_body_mass() * 0.1_f32);

    if object.controllable {
        let collision_time = object.time_since_collision as f32;
        if collision_time < 0.0 {
            magnitude *= gravity_force_multiplier;
        } else {
            let factor = collision_time.mul_add(-1.3_f32, gravity_force_multiplier);
            // FCSEL S10,S10,S0,LE keeps the unscaled value while factor <= 1.
            if matches!(
                factor.partial_cmp(&1.0),
                Some(std::cmp::Ordering::Greater) | None
            ) {
                magnitude *= factor;
            }
        }
    } else if object.revert_gravity {
        magnitude *= -0.1_f32;
    } else if object.revert_gravity_with_multiplier {
        let speed = native_hypot(object.velocity_x as f32, object.velocity_y as f32);
        if speed >= object.revert_gravity_max_velocity as f32 {
            return;
        }
        magnitude *= object.revert_gravity_force as f32;
    }

    object.apply_native_force_at(
        (direction_x * magnitude, direction_y * magnitude),
        (object_x, object_y),
    );
}
