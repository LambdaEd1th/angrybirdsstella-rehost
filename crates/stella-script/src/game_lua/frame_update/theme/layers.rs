//! One background or foreground ThemeManager pass (`sub_10009B8B4`).

use crate::game_lua::theme_world_offsets::{native_offset_y, set_native_offset_y};
use crate::*;

pub(super) fn advance_theme_manager_pass(
    bridge: &mut RenderBridge,
    foreground: bool,
    delta: f32,
    bindings: Option<(&ResourceRuntime, &Path)>,
) {
    // ThemeManager::update stores S1/S2 at +0x54/+0x58 before touching the
    // selected layer pass. Both native calls receive the same filtered pair.
    bridge.theme_camera.effect_x = bridge.accelerometer_filtered[0];
    bridge.theme_camera.effect_y = bridge.accelerometer_filtered[1];
    bridge.theme_camera.y = if foreground {
        0.0
    } else {
        bridge.theme_camera.saved_y
    };
    let camera = bridge.theme_camera;
    let world_limits = camera.world_limits;
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
        // Native updates the complete layer+0x40 value with one FMADD.
        // Splitting a resolved anchor and its motion changes float rounding
        // and incorrectly carries old motion through a later refresh.
        let offset_y = offset_y_step.mul_add(parallax_velocity, native_offset_y(layer));
        set_native_offset_y(layer, offset_y);

        wrap_moving_layer(layer, camera, bridge.resolution_camera_scale, world_context);
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
    set_native_offset_y(layer, offset_y);
    layer.world_x = area.world_x.map(f64::from);
    layer.world_y = area.world_y.map(f64::from);
    layer.world_width = area.world_width.map(f64::from);
    layer.world_height = area.world_height.map(f64::from);
    refresh_theme_layer_world_offset(layer, world_limits, world_context, random);
}

fn wrap_moving_layer(
    layer: &mut ThemeLayer,
    camera: ThemeCameraReference,
    end_scale: f32,
    world_context: ThemeWorldOffsetContext,
) {
    if (layer.velocity_x as f32) == 0.0 && (layer.velocity_y as f32) == 0.0 {
        return;
    }

    // 0x10009B9B4..0x10009B9D4 expands the current screen-to-world rectangle
    // with the four level limits cached by ThemeManager's draw. A moving
    // reference tile wraps only after its previous actual draw center leaves
    // this complete union, not merely the current 1024x768 viewport.
    let current_scale = world_context.current_scale;
    let world_limits = camera.world_limits;
    let screen_right = world_context.screen_left + world_context.screen_width / current_scale;
    let screen_bottom = world_context.screen_top + world_context.screen_height / current_scale;
    let union_left = world_limits
        .left
        .map_or(world_context.screen_left, |value| {
            value.min(world_context.screen_left)
        });
    let union_right = world_limits
        .right
        .map_or(screen_right, |value| value.max(screen_right));
    let union_top = world_limits.top.map_or(world_context.screen_top, |value| {
        value.min(world_context.screen_top)
    });
    let union_bottom = world_limits
        .bottom
        .map_or(screen_bottom, |value| value.max(screen_bottom));
    let union_width = union_right - union_left;
    let union_height = union_bottom - union_top;

    let z_distance = layer.z_distance as f32;
    let parallax_scale =
        native_theme_parallax_scale(camera.current_scale, end_scale, camera.scale, z_distance);
    // ResourceManager's four geometry accessors are retained in signed
    // 16-bit layer slots at +0x58/+0x5A.
    let width = f32::from(layer.geometry.width() as i16);
    if (layer.velocity_x as f32) != 0.0 {
        let authored_step = width * layer.scale_x as f32;
        let half_width =
            ((width * ((layer.scale_x as f32) * parallax_scale)) / camera.current_scale) * 0.5_f32;
        let tile_count = native_fcvtzs_f32((union_width / authored_step) * camera.scale);
        let shift = authored_step.mul_add(tile_count as f32, authored_step);
        let center = layer.cached_draw_world_x;
        let offset = layer.offset_x as f32;
        if center - half_width > union_right && (layer.velocity_x as f32) > 0.0 {
            layer.offset_x = f64::from(offset - shift);
            return;
        } else if center + half_width < union_left && (layer.velocity_x as f32) < 0.0 {
            layer.offset_x = f64::from(offset + shift);
            return;
        }
    }

    if (layer.velocity_y as f32) == 0.0 {
        return;
    }
    let height = f32::from(layer.geometry.height() as i16);
    let authored_step = height * layer.scale_y as f32;
    let half_height =
        ((((layer.scale_y as f32) * parallax_scale) * height) / camera.current_scale) * 0.5_f32;
    let tile_count = native_fcvtzs_f32((union_height / authored_step) * camera.scale_y);
    let shift = authored_step.mul_add(tile_count as f32, authored_step);
    let center = layer.cached_draw_world_y;
    if center + half_height < union_top && (layer.velocity_y as f32) < 0.0 {
        shift_layer_y(layer, shift);
    } else if center - half_height > union_bottom && (layer.velocity_y as f32) > 0.0 {
        shift_layer_y(layer, -shift);
    }
}

fn shift_layer_y(layer: &mut ThemeLayer, shift: f32) {
    set_native_offset_y(layer, native_offset_y(layer) + shift);
}
