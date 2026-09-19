//! Shared per-layer body of ThemeManager's world-relative refresh
//! (`sub_10009A894`). Both a draw pass and an animation-coordinate wrap can
//! invoke this member.

use crate::*;

#[derive(Debug, Clone, Copy)]
pub(crate) struct ThemeWorldOffsetContext {
    pub(crate) current_scale: f32,
    pub(crate) screen_left: f32,
    pub(crate) screen_top: f32,
    pub(crate) screen_width: f32,
    pub(crate) screen_height: f32,
    pub(crate) reference_x: f32,
    pub(crate) reference_y: f32,
}

pub(crate) fn refresh_theme_layer_world_offset(
    layer: &mut ThemeLayer,
    limits: ThemeWorldLimits,
    context: ThemeWorldOffsetContext,
    random: &mut NativeParticleRandom,
) {
    let screen_right = context.screen_left + context.screen_width / context.current_scale;
    let screen_bottom = context.screen_top + context.screen_height / context.current_scale;
    let left = limits
        .left
        .map_or(context.screen_left, |value| value.min(context.screen_left));
    let right = limits
        .right
        .map_or(screen_right, |value| value.max(screen_right));
    let top = limits
        .top
        .map_or(context.screen_top, |value| value.min(context.screen_top));
    let bottom = limits
        .bottom
        .map_or(screen_bottom, |value| value.max(screen_bottom));
    let width = right - left;
    let height = bottom - top;

    if let Some(world_x) = layer.world_x {
        let anchored = width.mul_add(world_x as f32, left);
        layer.offset_x = f64::from(anchored - context.reference_x);
    }
    if let Some(world_y) = layer.world_y {
        let anchored = height.mul_add(world_y as f32, top);
        set_native_offset_y(layer, anchored - context.reference_y);
    }
    if let Some(world_width) = layer.world_width {
        let sample = random.next() as f32;
        let random_factor = (world_width as f32) * (0.5_f32 - sample);
        layer.offset_x = f64::from(width.mul_add(random_factor, layer.offset_x as f32));
    }
    if let Some(world_height) = layer.world_height {
        let sample = random.next() as f32;
        let random_factor = (world_height as f32) * (0.5_f32 - sample);
        let offset_y = height.mul_add(random_factor, native_offset_y(layer));
        set_native_offset_y(layer, offset_y);
    }
}

pub(crate) fn native_offset_y(layer: &ThemeLayer) -> f32 {
    match layer.offset_y {
        ThemeVerticalOffset::Pixels(value) => value as f32,
        ThemeVerticalOffset::Top | ThemeVerticalOffset::Bottom => {
            layer.resolved_offset_y.unwrap_or(0.0) as f32
        }
    }
}

pub(crate) fn set_native_offset_y(layer: &mut ThemeLayer, value: f32) {
    match &mut layer.offset_y {
        ThemeVerticalOffset::Pixels(offset) => *offset = f64::from(value),
        ThemeVerticalOffset::Top | ThemeVerticalOffset::Bottom => {
            layer.resolved_offset_y = Some(f64::from(value));
        }
    }
}
