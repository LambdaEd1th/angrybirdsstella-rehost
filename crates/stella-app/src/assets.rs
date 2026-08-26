//! Recovered KA3D asset catalog, sprite geometry, and renderer-neutral transforms.

use super::*;

mod catalog;
mod sprite;
mod system_font;
mod text;
mod texture;
mod transform;

pub(super) use system_font::{
    SystemLabelPool, native_system_label_hash, native_system_label_horizontal_anchor,
    native_system_label_offset, native_system_label_vertical_anchor, rasterize_system_label,
};
pub(super) use texture::TextureAsset;

pub(super) use transform::{
    composite_child_transform, project_text_3d, render_command_transform, text_glyph_transform,
};

#[derive(Debug, Clone)]
pub(super) struct AtlasRegion {
    pub(super) texture: String,
    pub(super) sprite: SpriteRegion,
}

pub(super) struct AssetCatalog {
    pub(super) root: PathBuf,
    pub(super) font_root: PathBuf,
    pub(super) regions: HashMap<String, AtlasRegion>,
    pub(super) composites: HashMap<String, Vec<CompositePart>>,
    pub(super) masked_textures: HashMap<String, String>,
    pub(super) fonts: HashMap<String, BitmapFont>,
    pub(super) textures: HashMap<String, TextureAsset>,
    pub(super) system_labels: SystemLabelPool,
}

#[cfg(test)]
#[allow(dead_code)]
#[derive(Clone, Copy)]
pub(super) struct SpriteDrawOptions<'a> {
    pub(super) masked_texture: Option<(&'a str, f64)>,
    pub(super) masked_texture_matrix: Option<[f64; 6]>,
    pub(super) shader: Option<&'a SpriteShader>,
    pub(super) draw_size: Option<[f64; 2]>,
    pub(super) sprite_pivot: Option<[f64; 2]>,
}

#[derive(Debug, Clone, Copy)]
pub(super) struct SpriteTransform {
    pub(super) x: f32,
    pub(super) y: f32,
    pub(super) m00: f32,
    pub(super) m01: f32,
    pub(super) m10: f32,
    pub(super) m11: f32,
    pub(super) alpha: f32,
}
