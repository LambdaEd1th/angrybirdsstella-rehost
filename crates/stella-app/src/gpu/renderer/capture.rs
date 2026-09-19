//! GL_Context::capture (0x10059A160) copies the complete framebuffer using
//! GL_RGB and lower-left image origin. A texel-exact pass reproduces both in
//! wgpu without depending on current sprite scissor, blend, alpha or matrices.

use super::super::*;

pub(super) fn create_pipeline(
    device: &wgpu::Device,
) -> (wgpu::RenderPipeline, wgpu::BindGroupLayout) {
    let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("Stella RGB capture layout"),
        entries: &[resources::texture_layout_entry(0)],
    });
    let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("Stella RGB capture pipeline layout"),
        bind_group_layouts: &[Some(&layout)],
        immediate_size: 0,
    });
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("Stella bottom-up RGB capture shader"),
        source: wgpu::ShaderSource::Wgsl(
            r#"
                @group(0) @binding(0) var framebuffer: texture_2d<f32>;

                @vertex fn capture_vertex(@builtin(vertex_index) index: u32)
                    -> @builtin(position) vec4<f32> {
                    let corners = array<vec2<f32>, 3>(
                        vec2<f32>(-1.0, -1.0), vec2<f32>(3.0, -1.0),
                        vec2<f32>(-1.0, 3.0));
                    return vec4<f32>(corners[index], 0.0, 1.0);
                }

                @fragment fn capture_fragment(@builtin(position) position: vec4<f32>)
                    -> @location(0) vec4<f32> {
                    let size = textureDimensions(framebuffer);
                    let source = vec2<i32>(i32(position.x), i32(size.y) - 1 - i32(position.y));
                    return vec4<f32>(textureLoad(framebuffer, source, 0).rgb, 1.0);
                }
            "#
            .into(),
        ),
    });
    let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("Stella native RGB framebuffer capture"),
        layout: Some(&pipeline_layout),
        vertex: wgpu::VertexState {
            module: &shader,
            entry_point: Some("capture_vertex"),
            compilation_options: Default::default(),
            buffers: &[],
        },
        primitive: wgpu::PrimitiveState::default(),
        depth_stencil: None,
        multisample: wgpu::MultisampleState::default(),
        fragment: Some(wgpu::FragmentState {
            module: &shader,
            entry_point: Some("capture_fragment"),
            compilation_options: Default::default(),
            targets: &[Some(wgpu::ColorTargetState {
                format: GAME_FORMAT,
                blend: None,
                write_mask: wgpu::ColorWrites::ALL,
            })],
        }),
        multiview_mask: None,
        cache: None,
    });
    (pipeline, layout)
}

pub(super) fn bind_framebuffer(
    device: &wgpu::Device,
    layout: &wgpu::BindGroupLayout,
    view: &wgpu::TextureView,
) -> wgpu::BindGroup {
    device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("Stella RGB capture framebuffer"),
        layout,
        entries: &[wgpu::BindGroupEntry {
            binding: 0,
            resource: wgpu::BindingResource::TextureView(view),
        }],
    })
}
