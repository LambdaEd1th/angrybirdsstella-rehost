//! Shared native frame preparation with a browser-visible binary packet.

use super::*;
use bytemuck::{Pod, Zeroable};
use std::ops::Range;
use stella_assets::surface_format::SurfaceFormat;

#[allow(unused_imports)]
#[path = "../../stella-app/src/gpu/frame/mod.rs"]
mod frame;
#[path = "../../stella-app/src/gpu/program.rs"]
mod program;
use program::{NativeProgram, native_sprite_program};

const WHITE_TEXTURE: &str = "<stella-white>";

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct GpuVertex {
    position: [f32; 2],
    uv: [f32; 2],
    source: [f32; 2],
    clip_position: [f32; 4],
    draw_index: u32,
    padding: u32,
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct DrawUniform {
    header: [f32; 4],
    diffuse: [f32; 4],
    params: [f32; 4],
    fill: [f32; 4],
}

struct PreparedDraw {
    vertices: Range<u32>,
    texture_pair: usize,
    program: NativeProgram,
    scissor: Option<[u32; 4]>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum PreparedOperation {
    Draw(usize),
    Capture(String),
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
    transient_textures: HashMap<String, Arc<TextureAsset>>,
    retired_textures: HashSet<String>,
    current_clip: Option<[i32; 4]>,
    current_projection: Option<TextProjection3D>,
    current_raw_vertices: bool,
    current_vertex_depth: f32,
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
            PreparedOperation::Capture(name) => json!({"capture": name}),
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
        for name in &self.retired_textures {
            uploaded.remove(name);
        }
        Ok(
            json!({"vertices": {"pointer": self.vertices.as_ptr() as usize, "length": self.vertices.len() * std::mem::size_of::<GpuVertex>()}, "uniforms": {"pointer": self.uniforms.as_ptr() as usize, "count": self.uniforms.len()}, "textures": textures, "operations": operations, "retired": self.retired_textures}),
        )
    }
}
