//! One background or foreground ThemeManager pass (`sub_10009B8B4`).

use crate::*;

pub(super) fn advance_theme_manager_pass(
    bridge: &mut RenderBridge,
    foreground: bool,
    delta: f32,
    world_limits: ThemeWorldLimits,
    bindings: Option<(&ResourceRuntime, &Path)>,
) {
    // ThemeManager::update stores S1/S2 at +0x54/+0x58 before touching the
    // selected layer pass. Both native calls receive the same filtered pair.
    bridge.theme_camera.effect_x = bridge.accelerometer_filtered[0];
    bridge.theme_camera.effect_y = bridge.accelerometer_filtered[1];
    let world_scale = bridge.world_scale as f32;
    let max_world_scale = bridge.max_world_scale as f32;
    let (end_scale, reference_scale) = if bridge.theme_camera.valid {
        (bridge.resolution_camera_scale, bridge.theme_camera.scale)
    } else {
        (max_world_scale, 20.0_f32)
    };
    let screen_width = bridge.screen_width as f32;
    let screen_height = bridge.screen_height as f32;
    let world_context = ThemeWorldOffsetContext {
        current_scale: bridge.world_scale as f32,
        screen_left: bridge.top_left_x as f32,
        screen_top: bridge.top_left_y as f32,
        screen_width,
        screen_height,
        reference_x: bridge.theme_camera.x,
        reference_y: bridge.theme_camera.y,
    };
    let random = &mut bridge.particle_random;
    let background_particles = &mut bridge.theme_background_particles;
    let foreground_particles = &mut bridge.theme_foreground_particles;
    let layers = if foreground {
        &mut bridge.theme_foreground_layers
    } else {
        &mut bridge.theme_background_layers
    };

    for layer in layers {
        // 0x10009BA14..0x10009BA4C calls both ThemeParticleSystem members at
        // the head of every native layer iteration, before animation timers,
        // layer velocity, or wrapping are advanced.
        let layer_index = layer.definition_index as i32;
        background_particles.update_layer(layer_index, delta, random, bindings);
        foreground_particles.update_layer(layer_index, delta, random, bindings);
        advance_animation(layer, delta, world_limits, world_context, random);

        // Layer+0x2c/+0x30 feed the parallax offsets at +0x3c/+0x40.
        // GameLua's later member integrates the same velocities into the
        // distinct posX/posY pair at +0x34/+0x38.
        let parallax_velocity = 1.0_f32 - layer.z_distance as f32;
        let offset_x_step = (layer.velocity_x as f32) * delta;
        layer.offset_x = f64::from(offset_x_step.mul_add(parallax_velocity, layer.offset_x as f32));
        let offset_y_step = (layer.velocity_y as f32) * delta;
        match &mut layer.offset_y {
            ThemeVerticalOffset::Pixels(offset_y) => {
                *offset_y = f64::from(offset_y_step.mul_add(parallax_velocity, *offset_y as f32));
            }
            ThemeVerticalOffset::Top | ThemeVerticalOffset::Bottom => {
                layer.motion_y =
                    f64::from(offset_y_step.mul_add(parallax_velocity, layer.motion_y as f32));
            }
        }

        wrap_moving_layer(
            layer,
            world_scale,
            max_world_scale,
            end_scale,
            reference_scale,
            screen_width,
            screen_height,
        );
    }
}

fn advance_animation(
    layer: &mut ThemeLayer,
    delta: f32,
    world_limits: ThemeWorldLimits,
    world_context: ThemeWorldOffsetContext,
    random: &mut NativeParticleRandom,
) {
    let delay = layer
        .animation_timeline
        .get(layer.animation_frame)
        .copied()
        .unwrap_or(layer.animation_delay as f32);
    if delay <= 0.0 {
        return;
    }

    let mut timer = (layer.animation_timer as f32) + delta;
    if timer > delay {
        timer -= delay;
        if !layer.animation_frames.is_empty() {
            layer.animation_frame += 1;
            if layer.animation_frame >= layer.animation_frames.len() {
                layer.animation_frame = 0;
                refresh_animation_cycle(layer, world_limits, world_context, random);
            }
            layer.sprite = layer.animation_frames[layer.animation_frame].clone();
            layer.geometry = layer.animation_geometries[layer.animation_frame];
        }
    }
    layer.animation_timer = f64::from(timer);
    let alpha_mix = ((timer / delay) - 0.5_f32).abs() * 2.0_f32;
    let alpha =
        (1.0_f32 - alpha_mix).mul_add(layer.min_alpha as f32, alpha_mix * layer.max_alpha as f32);
    layer.alpha = f64::from(alpha);
}

fn refresh_animation_cycle(
    layer: &mut ThemeLayer,
    world_limits: ThemeWorldLimits,
    world_context: ThemeWorldOffsetContext,
    random: &mut NativeParticleRandom,
) {
    if layer.native_flags & 0x8 != 0 {
        // sub_100099C24 uses one-based source indices as raw float-vector
        // offsets. Its visible effect therefore leaves element zero intact,
        // writes sampled source entry N into destination N, and discards the
        // final one-past-end store while still consuming its random sample.
        for (source_index, entry) in layer
            .animation_timeline_definition
            .iter()
            .copied()
            .enumerate()
        {
            if !entry.uses_random {
                continue;
            }
            let sampled = (random.next() as f32).mul_add(entry.variance, entry.base);
            let destination_index = source_index + 1;
            if let Some(destination) = layer.animation_timeline.get_mut(destination_index) {
                *destination = sampled;
            }
        }
    }

    if layer.native_flags & 0x10 == 0 {
        return;
    }
    let Some(parameters) = layer.spawn_parameters.as_ref() else {
        return;
    };
    let area = parameters.area;
    let origin_x = area.screen_width.mul_add(-0.5_f32, area.screen_x);
    let offset_x = area.screen_width.mul_add(random.next() as f32, origin_x);
    let origin_y = area.screen_height.mul_add(-0.5_f32, area.screen_y);
    let offset_y = area.screen_height.mul_add(random.next() as f32, origin_y);
    layer.offset_x = f64::from(offset_x);
    layer.offset_y = ThemeVerticalOffset::Pixels(f64::from(offset_y));
    layer.resolved_offset_y = None;
    layer.world_x = area.world_x.map(f64::from);
    layer.world_y = area.world_y.map(f64::from);
    layer.world_width = area.world_width.map(f64::from);
    layer.world_height = area.world_height.map(f64::from);
    refresh_theme_layer_world_offset(layer, world_limits, world_context, random);
}

fn wrap_moving_layer(
    layer: &mut ThemeLayer,
    world_scale: f32,
    max_world_scale: f32,
    end_scale: f32,
    reference_scale: f32,
    screen_width: f32,
    screen_height: f32,
) {
    if (layer.velocity_x as f32) == 0.0 && (layer.velocity_y as f32) == 0.0 {
        return;
    }

    // sub_10009B8B4 keeps a moving reference tile close to the camera. It
    // shifts by one more whole tile than the viewport quotient.
    let z_distance = layer.z_distance as f32;
    let parallax_scale = if reference_scale == 20.0_f32 && end_scale == max_world_scale {
        (world_scale * (1.0_f32 - z_distance) + max_world_scale * z_distance) / 20.0_f32
    } else {
        native_theme_parallax_scale(world_scale, end_scale, reference_scale, z_distance)
    };
    let width = (layer.geometry.width() as f32 * layer.scale_x as f32 * parallax_scale).abs();
    if width > f32::EPSILON && (layer.velocity_x as f32) != 0.0 {
        let center = screen_width * 0.5_f32 + layer.offset_x as f32;
        let half_width = width * 0.5_f32;
        let tile_count = (screen_width / width).trunc();
        let shift = width.mul_add(tile_count, width);
        let offset = layer.offset_x as f32;
        if center - half_width > screen_width && (layer.velocity_x as f32) > 0.0 {
            layer.offset_x = f64::from(offset - shift);
        } else if center + half_width < 0.0 && (layer.velocity_x as f32) < 0.0 {
            layer.offset_x = f64::from(offset + shift);
        }
    }

    let height = (layer.geometry.height() as f32 * layer.scale_y as f32 * parallax_scale).abs();
    if height <= f32::EPSILON || (layer.velocity_y as f32) == 0.0 {
        return;
    }

    let anchored_y = match layer.offset_y {
        ThemeVerticalOffset::Pixels(offset) => screen_height * 0.5_f32 + offset as f32,
        ThemeVerticalOffset::Top => {
            -(layer.geometry.min_y as f32) * layer.scale_y as f32 * parallax_scale
                + layer.motion_y as f32
        }
        ThemeVerticalOffset::Bottom => {
            screen_height - (layer.geometry.max_y as f32) * layer.scale_y as f32 * parallax_scale
                + layer.motion_y as f32
        }
    };
    let half_height = height * 0.5_f32;
    let tile_count = (screen_height / height).trunc();
    let shift = height.mul_add(tile_count, height);
    if anchored_y + half_height < 0.0 && (layer.velocity_y as f32) < 0.0 {
        shift_layer_y(layer, shift);
    } else if anchored_y - half_height > screen_height && (layer.velocity_y as f32) > 0.0 {
        shift_layer_y(layer, -shift);
    }
}

fn shift_layer_y(layer: &mut ThemeLayer, shift: f32) {
    match &mut layer.offset_y {
        ThemeVerticalOffset::Pixels(offset) => {
            *offset = f64::from(*offset as f32 + shift);
        }
        ThemeVerticalOffset::Top | ThemeVerticalOffset::Bottom => {
            layer.motion_y = f64::from(layer.motion_y as f32 + shift);
        }
    }
}
