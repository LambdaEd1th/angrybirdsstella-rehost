//! Water/buoyancy member at `sub_10005E404`.

use crate::*;

use super::force_dispatch::native_hypot;

/// This member is used for every active non-gravitation sensor and for type-2
/// sensors explicitly marked as water.
pub(super) fn apply_native_water_sensor(
    sensor: &SceneObject,
    object: &mut SceneObject,
    game_world_scale: f32,
    water_force_multiplier: f32,
    bird_water_drag: f32,
    object_water_drag: f32,
) {
    let minimum = sensor.sensor_minimum_force as f32;
    let maximum = sensor.sensor_maximum_force as f32;
    let sensor_x = sensor.x as f32;
    let sensor_y = sensor.y as f32;
    let object_x = object.x as f32;
    let object_y = object.y as f32;
    let sensor_is_circle = matches!(sensor.collision_shape, CollisionShape::Circle { .. });
    let object_is_circle = matches!(object.collision_shape, CollisionShape::Circle { .. });
    let (object_width, object_height) = object.native_shape_dimensions();

    let mut radius = sensor.sensor_radius as f32;
    let mut direction_x = sensor_x - object_x;
    let mut direction_y = sensor_y - object_y;
    let distance = native_hypot(direction_x, direction_y);
    let mut submerged = radius.min(distance);
    if distance >= f32::EPSILON {
        let inverse_distance = 1.0 / distance;
        direction_x *= inverse_distance;
        direction_y *= inverse_distance;
    }

    if !sensor_is_circle {
        let (_, sensor_height) = sensor.native_shape_dimensions();
        radius = sensor_height;
        submerged = (sensor_y + sensor_height * 0.5_f32 - object_y)
            .max(0.0)
            .min(sensor_height);
        direction_x = 0.0;
        direction_y = 1.0;
    }
    if sensor.bubble {
        direction_x = 0.0;
        direction_y = 1.0;
        submerged = 0.0;
    }

    let density_delta = object.water_density as f32 - sensor.water_density as f32;
    let force_curve = (minimum - maximum).mul_add(submerged / radius, maximum);
    let mut buoyancy = (object.native_body_mass() * 0.1_f32) * (force_curve * density_delta);
    let launched_bird = object.controllable && (object.time_since_collision as f32) < 0.0;

    let (mut point_offset_x, mut point_offset_y) = (0.0_f32, 0.0_f32);
    if launched_bird {
        buoyancy *= water_force_multiplier;
    } else {
        let edge = radius.min(game_world_scale * 1.5_f32);
        if submerged > radius - edge {
            let ratio = (radius - submerged) / edge;
            buoyancy *= ratio;
            let angle = object.angle as f32;
            let mut sine = f64::from(angle).sin() as f32;
            let mut cosine = f64::from(angle).cos() as f32;
            if object_width < object_height {
                let rotated = angle + std::f32::consts::PI * 0.5_f32;
                sine = f64::from(rotated).sin() as f32;
                cosine = f64::from(rotated).cos() as f32;
            }
            let projection = direction_y.mul_add(sine, direction_x * cosine);
            let remainder = 1.0_f32 - ratio;
            point_offset_x = remainder * (cosine * projection);
            point_offset_y = remainder * (sine * projection);
        }
    }

    if object.keep_orientation {
        let rotated = f64::from(object.angle as f32) + f64::from(std::f32::consts::PI) * -0.5_f64;
        point_offset_x = (rotated.cos() * 0.25_f64) as f32;
        point_offset_y = (rotated.sin() * 0.25_f64) as f32;
    }

    let point_scale = if object_is_circle {
        game_world_scale
    } else {
        let wide_scale = game_world_scale * ((object_width / object_height) * 0.25_f32);
        let tall_scale = game_world_scale * ((object_height / object_width) * 0.25_f32);
        if object_width >= object_height {
            wide_scale
        } else {
            tall_scale
        }
    };

    let velocity_projection = direction_y.mul_add(
        object.velocity_y as f32,
        direction_x * object.velocity_x as f32,
    );
    if velocity_projection > -(game_world_scale * 15.0_f32) || launched_bird {
        object.apply_native_force_at(
            (direction_x * buoyancy, direction_y * buoyancy),
            (
                point_offset_x.mul_add(point_scale, object_x),
                point_offset_y.mul_add(point_scale, object_y),
            ),
        );
    }

    let drag = if launched_bird {
        bird_water_drag
    } else {
        object_water_drag
    };
    let drag_mass = object.native_body_mass() * drag;
    object.apply_native_force_at(
        (
            -(drag_mass * object.velocity_x as f32),
            -(drag_mass * object.velocity_y as f32),
        ),
        (object_x, object_y),
    );
}
