//! ThemeManager's camera-relative layer transform (`sub_10009CEB0`).

use crate::*;

pub(super) struct ThemeLayerTransform {
    pub(super) x: f64,
    pub(super) y: f64,
    pub(super) scale_x: f64,
    pub(super) scale_y: f64,
    /// World-space pair returned in S0/S1 by `sub_10009CEB0`. Purple keeps
    /// repeating in this coordinate space and projects every candidate only
    /// immediately before culling/submission.
    pub(super) native_world: Option<[f32; 2]>,
}

pub(super) fn layer_transform(
    bridge: &RenderBridge,
    layer: &ThemeLayer,
    foreground: bool,
) -> ThemeLayerTransform {
    if !bridge.theme_camera.valid {
        return legacy_fixture_transform(bridge, layer);
    }

    let current_scale = bridge.world_scale as f32;
    let end_scale = bridge.resolution_camera_scale;
    let reference_scale = bridge.theme_camera.scale;
    let z_distance = layer.z_distance as f32;
    let parallax_scale =
        native_theme_parallax_scale(current_scale, end_scale, reference_scale, z_distance);
    let scale_x = (layer.scale_x as f32) * parallax_scale;
    let scale_y = (layer.scale_y as f32) * parallax_scale;

    // Geometry accessors used by Purple return width/height/pivot, stored at
    // layer+0x5A/+0x58/+0x5C/+0x5E.  SpriteGeometry uses pivot-relative
    // bounds, so `-min` reconstructs the authored pivot exactly.
    let width = layer.geometry.width() as f32;
    let height = layer.geometry.height() as f32;
    let pivot_x = -(layer.geometry.min_x as f32);
    let pivot_y = -(layer.geometry.min_y as f32);
    let centered_x = (-width).mul_add(0.5_f32, pivot_x);
    let centered_y = (-height).mul_add(0.5_f32, pivot_y);
    let local_x = ((layer.offset_x as f32) + centered_x) / reference_scale;

    let camera_x =
        (bridge.top_left_x as f32) + ((bridge.screen_width as f32) * 0.5_f32) / current_scale;
    let camera_y =
        (bridge.top_left_y as f32) + ((bridge.screen_height as f32) * 0.5_f32) / current_scale;
    let camera_delta_x = camera_x - bridge.theme_camera.x;
    let camera_delta_y = camera_y - bridge.theme_camera.y;
    let ratio = current_scale / end_scale;
    let one_minus_z = 1.0_f32 - z_distance;

    let base_x = z_distance.mul_add(local_x / ratio, local_x * one_minus_z) + bridge.theme_camera.x;
    // Shipped portrait gameplay reports orientation zero.  Preserve the
    // native slot and arithmetic so this remains correct when orientation is
    // later wired to a non-zero platform value.
    let orientation_y = if layer.native_flags & 0x1 != 0 && bridge.theme_camera.orientation > 0.0 {
        (-0.5_f32 * bridge.theme_camera.orientation) / current_scale
    } else {
        0.0
    };

    // ANCHOR_V selects the full camera delta. Bit 0x40 is an internal
    // horizontal equivalent which no 1.1.6 authored theme sets. With that
    // bit clear, 0x10009CF94 includes the constructor's xMult field.
    let x_delta = if layer.native_flags & 0x40 != 0 {
        camera_delta_x
    } else {
        (z_distance + layer.x_multiplier as f32) * camera_delta_x
    };
    let y_delta = if layer.native_flags & 0x20 != 0 {
        camera_delta_y
    } else {
        z_distance * camera_delta_y
    };
    let effect_x = (z_distance * (bridge.theme_camera.effect_x * 25.0_f32)) / current_scale;
    let effect_y = if foreground {
        0.0
    } else {
        (z_distance * (bridge.theme_camera.effect_y * -25.0_f32)) / current_scale
    };

    let x_world = base_x + x_delta + effect_x;
    let x = (x_world - bridge.top_left_x as f32) * current_scale;
    let numeric_offset_y = if let Some(relative_y) = layer.relative_y {
        Some(native_theme_relative_y_offset(
            relative_y as f32,
            bridge.screen_height as f32,
            bridge.theme_camera.original_scale_ratio,
        ))
    } else {
        match layer.offset_y {
            ThemeVerticalOffset::Pixels(offset_y) => Some(offset_y as f32),
            ThemeVerticalOffset::Top | ThemeVerticalOffset::Bottom => layer
                .resolved_offset_y
                .map(|offset_y| (offset_y as f32) + layer.motion_y as f32),
        }
    };
    let (y_world, y) = match numeric_offset_y {
        Some(offset_y) => {
            let local_y = (offset_y + centered_y) / reference_scale;
            let base_y = z_distance.mul_add(local_y / ratio, local_y * one_minus_z)
                + orientation_y
                + bridge.theme_camera.y;
            let y_world = base_y + y_delta + effect_y;
            (
                y_world,
                (y_world - bridge.top_left_y as f32) * current_scale,
            )
        }
        None => {
            let y = (match layer.offset_y {
                // A symbolic layer can be drawn before the script's refresh call.
                // Purple's shipped flow refreshes first; retain a deterministic
                // pre-refresh fallback for isolated host integrations.
                ThemeVerticalOffset::Top => -(layer.geometry.min_y as f32) * scale_y,
                ThemeVerticalOffset::Bottom => {
                    bridge.screen_height as f32 - (layer.geometry.max_y as f32) * scale_y
                }
                ThemeVerticalOffset::Pixels(_) => unreachable!(),
            }) + layer.motion_y as f32;
            // `sub_10006853C`: screen / scale + topLeft. This fallback is not
            // reached by shipped levels, but retaining float32 projection
            // keeps repeat traversal deterministic for an early host draw.
            (y / current_scale + bridge.top_left_y as f32, y)
        }
    };

    ThemeLayerTransform {
        x: f64::from(x),
        y: f64::from(y),
        scale_x: f64::from(scale_x),
        scale_y: f64::from(scale_y),
        native_world: Some([x_world, y_world]),
    }
}

/// Isolated binding tests construct themes without running the native level
/// refresh.  Retain their historical fixture coordinate system, while every
/// shipped level takes the exact camera path above.
fn legacy_fixture_transform(bridge: &RenderBridge, layer: &ThemeLayer) -> ThemeLayerTransform {
    let parallax_scale = ((bridge.world_scale * (1.0 - layer.z_distance)
        + bridge.max_world_scale * layer.z_distance)
        / 20.0)
        .max(0.01);
    let scale_x = layer.scale_x * parallax_scale;
    let scale_y = layer.scale_y * parallax_scale;
    let y = match layer.offset_y {
        ThemeVerticalOffset::Pixels(offset) => f64::from(bridge.screen_height) * 0.5 + offset,
        ThemeVerticalOffset::Top if layer.resolved_offset_y.is_some() => {
            f64::from(bridge.screen_height) * 0.5 + layer.resolved_offset_y.unwrap()
        }
        ThemeVerticalOffset::Top => -layer.geometry.min_y * scale_y,
        ThemeVerticalOffset::Bottom if layer.resolved_offset_y.is_some() => {
            f64::from(bridge.screen_height) * 0.5 + layer.resolved_offset_y.unwrap()
        }
        ThemeVerticalOffset::Bottom => {
            f64::from(bridge.screen_height) - layer.geometry.max_y * scale_y
        }
    } + layer.motion_y;
    ThemeLayerTransform {
        x: f64::from(bridge.screen_width) * 0.5 + layer.offset_x,
        y,
        scale_x,
        scale_y,
        native_world: None,
    }
}
