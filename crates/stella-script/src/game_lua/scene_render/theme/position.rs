//! ThemeManager's camera-relative layer transform (`sub_10009CEB0`).

use crate::*;

pub(super) struct ThemeLayerTransform {
    pub(super) scale_x: f64,
    pub(super) scale_y: f64,
    /// World-space pair returned in S0/S1 by `sub_10009CEB0`. Purple keeps
    /// repeating in this coordinate space and projects every candidate only
    /// immediately before culling/submission.
    pub(super) native_world: [f32; 2],
}

pub(super) fn layer_transform(
    bridge: &RenderBridge,
    layer: &ThemeLayer,
    foreground: bool,
) -> ThemeLayerTransform {
    let current_scale = bridge.theme_camera.current_scale;
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
    // `FNMSUB Sd, Sn, Sm, Sa` at 0x10009CF3C/0x10009CF50 computes
    // `Sn * Sm - Sa`: Purple first moves the authored pivot to the sprite's
    // geometric center, then asks ResourceManager to draw that center using
    // HCENTER/VCENTER. The later anchor operation cancels this term for an
    // unscaled, unprojected sprite, but both halves are required for native
    // parallax and repeat traversal.
    let centered_x = width.mul_add(0.5_f32, -pivot_x);
    let centered_y = height.mul_add(0.5_f32, -pivot_y);
    let local_x = ((layer.offset_x as f32) + centered_x) / reference_scale;

    let camera_delta_x = bridge.theme_camera.screen_x - bridge.theme_camera.x;
    let camera_delta_y = bridge.theme_camera.screen_y - bridge.theme_camera.y;
    let ratio = current_scale / end_scale;
    let one_minus_z = 1.0_f32 - z_distance;

    let base_x =
        local_x.mul_add(one_minus_z, z_distance * (local_x / ratio)) + bridge.theme_camera.x;
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

    let x_world = base_x + (effect_x + x_delta);
    // layer+0x40 is always numeric. Strings start at zero and refresh writes
    // their resolved value; relativeY is materialized once by the lazy prelude.
    let offset_y = crate::game_lua::theme_world_offsets::native_offset_y(layer);
    let local_y = (offset_y + centered_y) / reference_scale;
    let base_y = z_distance.mul_add(local_y / ratio, local_y * one_minus_z)
        + orientation_y
        + bridge.theme_camera.y;
    let y_world = base_y + (y_delta + effect_y);

    ThemeLayerTransform {
        scale_x: f64::from(scale_x),
        scale_y: f64::from(scale_y),
        native_world: [x_world, y_world],
    }
}
