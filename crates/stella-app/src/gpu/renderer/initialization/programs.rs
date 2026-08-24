//! wgpu equivalents of GL_Context's independently cached 2D program family.

use super::super::super::resources::{create_sprite_pipeline, premultiplied_blend, straight_blend};

pub(super) struct NativePrograms {
    pub(super) plain: wgpu::RenderPipeline,
    pub(super) plain_alpha: wgpu::RenderPipeline,
    pub(super) sprite: wgpu::RenderPipeline,
    pub(super) sprite_alpha: wgpu::RenderPipeline,
    pub(super) sprite_alpha_masked: wgpu::RenderPipeline,
}

pub(super) fn create(
    device: &wgpu::Device,
    storage_layout: &wgpu::BindGroupLayout,
    texture_layout: &wgpu::BindGroupLayout,
) -> NativePrograms {
    let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("Stella native 2D program layout"),
        bind_group_layouts: &[Some(storage_layout), Some(texture_layout)],
        immediate_size: 0,
    });
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("Reverse-aligned Purple 2D shader family"),
        source: wgpu::ShaderSource::Wgsl(include_str!("../../../gpu.wgsl").into()),
    });
    NativePrograms {
        plain: create_sprite_pipeline(
            device,
            &layout,
            &shader,
            wgpu::BlendState::REPLACE,
            "Purple 2d-vertexcolor program",
        ),
        plain_alpha: create_sprite_pipeline(
            device,
            &layout,
            &shader,
            straight_blend(),
            "Purple 2d-vertexcolor-alpha program",
        ),
        sprite: create_sprite_pipeline(
            device,
            &layout,
            &shader,
            wgpu::BlendState::REPLACE,
            "Purple 2d-sprite program",
        ),
        sprite_alpha: create_sprite_pipeline(
            device,
            &layout,
            &shader,
            premultiplied_blend(),
            "Purple 2d-sprite-alpha program",
        ),
        sprite_alpha_masked: create_sprite_pipeline(
            device,
            &layout,
            &shader,
            straight_blend(),
            "Purple 2d-sprite-alpha-masked program",
        ),
    }
}
