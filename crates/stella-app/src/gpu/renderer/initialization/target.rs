//! Drawable-sized target, samplers and shared native-program bind layouts.

use super::super::super::resources::{sampler_layout_entry, texture_layout_entry};
use super::super::super::*;

pub(super) struct NativeTarget {
    pub(super) game_texture: wgpu::Texture,
    pub(super) game_view: wgpu::TextureView,
    pub(super) base_sampler: wgpu::Sampler,
    pub(super) fill_sampler: wgpu::Sampler,
    pub(super) sprite_storage_layout: wgpu::BindGroupLayout,
    pub(super) sprite_texture_layout: wgpu::BindGroupLayout,
}

pub(in crate::gpu) fn create_game_texture(
    device: &wgpu::Device,
    resolution: GameResolution,
) -> (wgpu::Texture, wgpu::TextureView) {
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("Stella native-size game target"),
        size: wgpu::Extent3d {
            width: resolution.width,
            height: resolution.height,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: GAME_FORMAT,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT
            | wgpu::TextureUsages::TEXTURE_BINDING
            | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
    (texture, view)
}

#[cfg(test)]
pub(in crate::gpu) fn game_texture_size(texture: &wgpu::Texture) -> GameResolution {
    GameResolution {
        width: texture.width(),
        height: texture.height(),
    }
}

pub(super) fn create(device: &wgpu::Device, resolution: GameResolution) -> NativeTarget {
    let (game_texture, game_view) = create_game_texture(device, resolution);
    let base_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
        label: Some("Purple GL_LINEAR clamp sampler"),
        address_mode_u: wgpu::AddressMode::ClampToEdge,
        address_mode_v: wgpu::AddressMode::ClampToEdge,
        address_mode_w: wgpu::AddressMode::ClampToEdge,
        mag_filter: wgpu::FilterMode::Linear,
        min_filter: wgpu::FilterMode::Linear,
        mipmap_filter: wgpu::MipmapFilterMode::Nearest,
        ..Default::default()
    });
    let fill_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
        label: Some("Purple GL_LINEAR repeat sampler"),
        address_mode_u: wgpu::AddressMode::Repeat,
        address_mode_v: wgpu::AddressMode::Repeat,
        address_mode_w: wgpu::AddressMode::Repeat,
        mag_filter: wgpu::FilterMode::Linear,
        min_filter: wgpu::FilterMode::Linear,
        mipmap_filter: wgpu::MipmapFilterMode::Nearest,
        ..Default::default()
    });
    let sprite_storage_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("Stella sprite storage layout"),
        entries: &[wgpu::BindGroupLayoutEntry {
            binding: 0,
            visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Storage { read_only: true },
                has_dynamic_offset: false,
                min_binding_size: None,
            },
            count: None,
        }],
    });
    let sprite_texture_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("Stella sprite texture layout"),
        entries: &[
            texture_layout_entry(0),
            sampler_layout_entry(1),
            texture_layout_entry(2),
            sampler_layout_entry(3),
        ],
    });
    NativeTarget {
        game_texture,
        game_view,
        base_sampler,
        fill_sampler,
        sprite_storage_layout,
        sprite_texture_layout,
    }
}
