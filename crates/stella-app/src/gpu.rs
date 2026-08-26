use std::{
    collections::{HashMap, HashSet},
    ops::Range,
    sync::{Arc, mpsc},
};

use anyhow::{Result, anyhow};
use bytemuck::{Pod, Zeroable};
use image::RgbaImage;
use stella_assets::surface_format::SurfaceFormat;
use winit::window::Window;

use super::*;

mod frame;
mod program;
mod renderer;
mod resources;

use program::{NativeProgram, native_sprite_program};

const GAME_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;
const WHITE_TEXTURE: &str = "<stella-white>";
const MAX_HOLES: usize = 64;

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct GpuVertex {
    position: [f32; 2],
    uv: [f32; 2],
    source: [f32; 2],
    local: [f32; 2],
    clip_position: [f32; 2],
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
    holes: [[f32; 4]; MAX_HOLES],
}

impl Default for DrawUniform {
    fn default() -> Self {
        Self::zeroed()
    }
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
    required_textures: HashSet<String>,
    transient_textures: HashMap<String, Arc<TextureAsset>>,
    retired_textures: HashSet<String>,
    current_clip: Option<[i32; 4]>,
}

#[cfg(test)]
impl PreparedFrame {
    fn draw_texture_pair(&self, draw_index: usize) -> (&str, &str) {
        let pair = &self.texture_pairs[self.draws[draw_index].texture_pair];
        (&pair.0, &pair.1)
    }
}

struct GpuTexture {
    texture: wgpu::Texture,
    view: wgpu::TextureView,
}

pub(crate) struct GpuRenderer {
    _instance: wgpu::Instance,
    surface: Option<wgpu::Surface<'static>>,
    surface_config: Option<wgpu::SurfaceConfiguration>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    resolution: GameResolution,
    game_texture: wgpu::Texture,
    game_view: wgpu::TextureView,
    sprite_storage_layout: wgpu::BindGroupLayout,
    sprite_texture_layout: wgpu::BindGroupLayout,
    draw_storage_buffer: wgpu::Buffer,
    draw_storage_bind_group: wgpu::BindGroup,
    draw_storage_capacity: u64,
    vertex_buffer: wgpu::Buffer,
    vertex_capacity: u64,
    plain_program: wgpu::RenderPipeline,
    plain_alpha_program: wgpu::RenderPipeline,
    sprite_program: wgpu::RenderPipeline,
    sprite_alpha_program: wgpu::RenderPipeline,
    sprite_alpha_masked_program: wgpu::RenderPipeline,
    base_sampler: wgpu::Sampler,
    fill_sampler: wgpu::Sampler,
    textures: HashMap<String, GpuTexture>,
    texture_bind_groups: HashMap<(String, String), wgpu::BindGroup>,
    retired_textures: HashSet<String>,
    blit_pipeline: Option<wgpu::RenderPipeline>,
    blit_bind_group: Option<wgpu::BindGroup>,
    blit_layout: Option<wgpu::BindGroupLayout>,
    blit_sampler: Option<wgpu::Sampler>,
}

#[cfg(test)]
mod tests;
