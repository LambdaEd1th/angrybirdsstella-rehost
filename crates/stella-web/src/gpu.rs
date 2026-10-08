//! Shared native frame preparation with a browser-visible binary packet.

use super::*;
use std::sync::Weak;
use stella_assets::surface_format::SurfaceFormat;

#[allow(unused_imports)]
#[path = "../../stella-app/src/gpu/frame/mod.rs"]
mod frame;
#[path = "../../stella-app/src/gpu/program.rs"]
mod program;
use frame::{DrawUniform, GpuVertex, PreparedDraw, PreparedOperation};
use program::{NativeProgram, native_sprite_program};
mod retirement;

const WHITE_TEXTURE: &str = "<stella-white>";

pub(super) fn clear_scissor(edges: Option<[i32; 4]>, resolution: GameResolution) -> [u32; 4] {
    frame::native_scissor(edges, resolution)
}

#[derive(Default)]
pub(crate) struct PreparedFrame {
    resolution: GameResolution,
    vertices: Vec<GpuVertex>,
    uniforms: Vec<DrawUniform>,
    draws: Vec<PreparedDraw>,
    texture_pairs: Vec<(String, String)>,
    operations: Vec<PreparedOperation>,
    capture_formats: HashMap<String, SurfaceFormat>,
    required_textures: HashSet<String>,
    texture_leases: HashMap<String, Arc<()>>,
    transient_textures: HashMap<String, Arc<TextureAsset>>,
    retired_textures: HashSet<String>,
    current_clip: Option<[i32; 4]>,
    current_projection: Option<TextProjection3D>,
    current_raw_vertices: bool,
    current_vertex_depth: f32,
    gpu_region_trace: Option<bool>,
}

impl PreparedFrame {
    pub(super) fn packet(
        &self,
        assets: &mut AssetCatalog,
        uploaded: &mut HashSet<String>,
    ) -> Result<serde_json::Value> {
        let mut textures = Vec::new();
        for name in &self.required_textures {
            if uploaded.contains(name) || name.starts_with("<capture-generation:") {
                continue;
            }
            let texture = if let Some(texture) = self.transient_textures.get(name) {
                texture.as_ref()
            } else {
                assets.texture(name)?
            };
            textures.push(json!({"name": name, "width": texture.width(), "height": texture.height(), "pointer": texture.image.as_raw().as_ptr() as usize, "length": texture.image.as_raw().len()}));
            uploaded.insert(name.clone());
        }
        let operations = self.operations.iter().map(|operation| match operation {
            PreparedOperation::Capture(name) => {
                uploaded.insert(name.clone());
                json!({"capture": name})
            },
            PreparedOperation::ScreenshotShare(request) => json!({"share": {
                "sequence": request.sequence, "filename": request.filename, "title": request.title
            }}),
            PreparedOperation::Draw(index) => {
                let draw = &self.draws[*index];
                let pair = &self.texture_pairs[draw.texture_pair];
                let program = match draw.program {
                    NativeProgram::Plain => 0, NativeProgram::PlainAlpha => 1,
                    NativeProgram::Sprite => 2, NativeProgram::SpriteAlpha => 3,
                    NativeProgram::SpriteAlphaMasked => 4,
                };
                json!({"first": draw.vertices.start, "count": draw.vertices.end - draw.vertices.start, "base": pair.0, "fill": pair.1, "program": program, "scissor": draw.scissor})
            }
        }).collect::<Vec<_>>();
        let capture_targets = self
            .operations
            .iter()
            .filter_map(|operation| match operation {
                PreparedOperation::Capture(name) => Some(name.clone()),
                PreparedOperation::Draw(_) | PreparedOperation::ScreenshotShare(_) => None,
            })
            .collect::<HashSet<_>>();
        let retired = retirement::retired_texture_names(
            &self.retired_textures,
            uploaded,
            &assets.file_images.lifetimes,
            &self.required_textures,
            &capture_targets,
        );
        for name in &retired {
            uploaded.remove(name);
        }
        Ok(
            json!({"vertices": {"pointer": self.vertices.as_ptr() as usize, "length": self.vertices.len() * std::mem::size_of::<GpuVertex>()}, "uniforms": {"pointer": self.uniforms.as_ptr() as usize, "count": self.uniforms.len()}, "textures": textures, "operations": operations, "retired": retired}),
        )
    }
}
