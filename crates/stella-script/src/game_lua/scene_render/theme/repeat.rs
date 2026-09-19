//! ThemeManager repeat helpers (`sub_10009C4C4` / `sub_10009CA0C`).

use super::position::ThemeLayerTransform;
use crate::*;

pub(super) fn tile_positions(
    bridge: &RenderBridge,
    layer: &ThemeLayer,
    transform: &ThemeLayerTransform,
) -> Vec<(f64, f64)> {
    native_tile_positions(bridge, layer, transform, transform.native_world)
}

/// Native `sub_10009BDB4` retains the reference position and every repeat
/// step as float32 world coordinates. `sub_100067A04` projects each candidate
/// independently, avoiding the drift caused by accumulating screen-space
/// doubles (and matching the eventual float32 GL/wgpu transform exactly).
fn native_tile_positions(
    bridge: &RenderBridge,
    layer: &ThemeLayer,
    transform: &ThemeLayerTransform,
    [base_x, base_y]: [f32; 2],
) -> Vec<(f64, f64)> {
    let current_scale = bridge.world_scale as f32;
    if current_scale == 0.0 || !current_scale.is_finite() {
        return Vec::new();
    }

    let scale_x = transform.scale_x as f32;
    let scale_y = transform.scale_y as f32;
    let width_world = (((layer.geometry.width() as f32) * scale_x) / current_scale).abs();
    let height_world = (((layer.geometry.height() as f32) * scale_y) / current_scale).abs();
    // Invalid authored scales must not trap the host in a non-progressing
    // repeat loop. Do not substitute another coordinate system.
    if !width_world.is_finite() || !height_world.is_finite() {
        return Vec::new();
    }
    let width_screen = width_world * current_scale;
    let height_screen = height_world * current_scale;

    // Four calls to screenToWorld (`sub_10006853C`) fill manager
    // +0x90/+0x94/+0xA0/+0xA4 before the layer loop.
    let world_left = bridge.top_left_x as f32;
    let world_right = (bridge.screen_width as f32) / current_scale + world_left;
    let world_top = bridge.top_left_y as f32;
    let world_bottom = (bridge.screen_height as f32) / current_scale + world_top;

    let project = |x: f32, y: f32| {
        (
            (x - bridge.top_left_x as f32) * current_scale,
            (y - bridge.top_left_y as f32) * current_scale,
        )
    };
    let visible = |x: f32, y: f32| {
        let (x, y) = project(x, y);
        let x = f64::from(x);
        let y = f64::from(y);
        let half_width = f64::from(width_screen) * 0.5_f64;
        let half_height = f64::from(height_screen) * 0.5_f64;
        x - half_width <= f64::from(bridge.screen_width)
            && x + half_width >= 0.0
            && y + half_height >= 0.0
            && y - half_height <= f64::from(bridge.screen_height)
    };
    let append = |positions: &mut Vec<(f64, f64)>, x: f32, y: f32| {
        if visible(x, y) {
            let (x, y) = project(x, y);
            positions.push((f64::from(x), f64::from(y)));
        }
    };
    let append_vertical = |positions: &mut Vec<(f64, f64)>, x: f32| {
        if !layer.repeat_y || height_world <= f32::EPSILON {
            return;
        }

        let half_height = f64::from(height_world) * 0.5_f64;
        let mut y = base_y;
        if f64::from(y) + half_height > f64::from(world_top) {
            loop {
                y -= height_world;
                append(positions, x, y);
                if f64::from(y) + half_height <= f64::from(world_top) {
                    break;
                }
            }
        }

        let mut y = base_y;
        if f64::from(y) - half_height < f64::from(world_bottom) {
            loop {
                y += height_world;
                append(positions, x, y);
                if f64::from(y) - half_height >= f64::from(world_bottom) {
                    break;
                }
            }
        }
    };

    // Purple submits the reference, right columns, left columns, then the
    // reference column's vertical copies. Translucent repeats expose this
    // deliberately non-rectangular painter order.
    let mut positions = Vec::new();
    append(&mut positions, base_x, base_y);

    let horizontal_repeat = layer.repeat_x || layer.repeat_left_only || layer.repeat_right_only;
    if horizontal_repeat && width_world > f32::EPSILON {
        let half_width = f64::from(width_world) * 0.5_f64;
        if !layer.repeat_left_only && f64::from(base_x) - half_width < f64::from(world_right) {
            let mut x = base_x;
            loop {
                x += width_world;
                append(&mut positions, x, base_y);
                append_vertical(&mut positions, x);
                if f64::from(x) - half_width >= f64::from(world_right) {
                    break;
                }
            }
        }
        if !layer.repeat_right_only && f64::from(base_x) + half_width > f64::from(world_left) {
            let mut x = base_x;
            loop {
                x -= width_world;
                append(&mut positions, x, base_y);
                append_vertical(&mut positions, x);
                if f64::from(x) + half_width <= f64::from(world_left) {
                    break;
                }
            }
        }
    }
    append_vertical(&mut positions, base_x);
    positions
}
