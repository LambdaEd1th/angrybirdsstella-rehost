//! ResourceManager width/height queries and atlas submission.
//!
//! This models `sub_10045CD14`, `sub_10045CD60`, `sub_10045C144`, and the
//! atlas branch `sub_100467AF0` used by `drawBoxNative`.

use crate::*;

pub(super) const H_RIGHT: u32 = 2;
pub(super) const V_BOTTOM: u32 = 2;

pub(super) fn width(sprite: Option<&str>, resources: &ResourceRuntime) -> f32 {
    sprite
        .and_then(|name| resources.active_geometry(name))
        .map_or(0.0, |bounds| bounds.width() as f32)
}

pub(super) fn height(sprite: Option<&str>, resources: &ResourceRuntime) -> f32 {
    sprite
        .and_then(|name| resources.active_geometry(name))
        .map_or(0.0, |bounds| bounds.height() as f32)
}

pub(super) fn command(
    sprite: Option<&str>,
    rect: [f32; 4],
    horizontal_anchor: u32,
    vertical_anchor: u32,
    resources: &ResourceRuntime,
    data_root: &Path,
) -> Option<RenderCommand> {
    let sprite = sprite?;
    let bounds = resources.active_geometry(sprite)?;
    let bound_region = resources.active_atlas_catalog_region(sprite, data_root)?;
    let [x, y, width, height] = rect;
    let native_width = bounds.width() as f32;
    let native_height = bounds.height() as f32;
    if native_width == 0.0 || native_height == 0.0 {
        return None;
    }
    let x = match horizontal_anchor {
        1 => x - native_width * 0.5_f32,
        H_RIGHT => x - native_width,
        _ => x,
    };
    let y = match vertical_anchor {
        1 => y - native_height * 0.5_f32,
        V_BOTTOM => y - native_height,
        _ => y,
    };
    Some(RenderCommand {
        order: 0,
        sprite: sprite.to_owned(),
        texture: None,
        texture_scale: 1.0,
        masked_texture_binding: None,
        bound_region: Some(bound_region),
        bound_composite: None,
        shader: None,
        clip_holes: Vec::new(),
        dirt: None,
        x: f64::from(x - bounds.min_x as f32),
        y: f64::from(y - bounds.min_y as f32),
        state: RenderState {
            draw_size: Some([f64::from(width), f64::from(height)]),
            ..RenderState::default()
        },
        world_space: true,
    })
}
