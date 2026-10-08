use std::{
    collections::{HashMap, HashSet},
    sync::{Arc, Weak, mpsc},
};

use anyhow::{Result, anyhow};
use image::RgbaImage;
use stella_assets::surface_format::SurfaceFormat;
use winit::window::Window;

use super::*;

mod frame;
mod program;
mod renderer;
mod resources;

#[cfg(test)]
mod reference;

use frame::{DrawUniform, GpuVertex, PreparedDraw, PreparedOperation};
use program::{NativeProgram, native_sprite_program};

const GAME_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;
const WHITE_TEXTURE: &str = "<stella-white>";
/// Pixels owned by one native share call, independent of subsequent draws,
/// presentation overlays and drawable resizes. Rows are top-down RGBA8.
pub(crate) struct ScreenshotShareCapture {
    pub(crate) request: ScreenshotShareRequest,
    pub(crate) resolution: GameResolution,
    pub(crate) rgba: Vec<u8>,
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
    texture_leases: HashMap<String, Arc<()>>,
    current_clip: Option<[i32; 4]>,
    current_projection: Option<TextProjection3D>,
    current_raw_vertices: bool,
    current_vertex_depth: f32,
    gpu_region_trace: Option<bool>,
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
    instance: wgpu::Instance,
    adapter: wgpu::Adapter,
    surface_window: Option<Arc<Window>>,
    surface: Option<wgpu::Surface<'static>>,
    surface_config: Option<wgpu::SurfaceConfiguration>,
    surface_recovery: Option<renderer::surface_acquisition::SurfaceRecovery>,
    device: wgpu::Device,
    device_state: renderer::device_state::DeviceState,
    queue: wgpu::Queue,
    resolution: GameResolution,
    game_texture: wgpu::Texture,
    game_view: wgpu::TextureView,
    screenshot_shares: Vec<ScreenshotShareCapture>,
    capture_pipeline: wgpu::RenderPipeline,
    capture_layout: wgpu::BindGroupLayout,
    capture_bind_group: wgpu::BindGroup,
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
    texture_lifetimes: HashMap<String, Weak<()>>,
    blit_pipeline: Option<wgpu::RenderPipeline>,
    blit_bind_group: Option<wgpu::BindGroup>,
    blit_layout: Option<wgpu::BindGroupLayout>,
    blit_sampler: Option<wgpu::Sampler>,
    window_overlay: Option<renderer::window_overlay::WindowOverlay>,
}

#[cfg(test)]
mod tests;
