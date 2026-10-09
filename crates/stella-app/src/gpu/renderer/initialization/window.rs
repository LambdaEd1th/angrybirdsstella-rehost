//! Optional platform-surface configuration and fixed-target letterbox program.

use super::super::super::resources::{sampler_layout_entry, texture_layout_entry};
use super::super::super::*;

pub(super) struct WindowPresentation {
    pub(super) surface_config: Option<wgpu::SurfaceConfiguration>,
    pub(super) blit_pipeline: Option<wgpu::RenderPipeline>,
    pub(super) blit_bind_group: Option<wgpu::BindGroup>,
    pub(super) blit_layout: Option<wgpu::BindGroupLayout>,
    pub(super) blit_sampler: Option<wgpu::Sampler>,
}

pub(super) fn create(
    surface: Option<&wgpu::Surface<'static>>,
    adapter: &wgpu::Adapter,
    device: &wgpu::Device,
    game_view: &wgpu::TextureView,
    width: u32,
    height: u32,
) -> Result<WindowPresentation> {
    let Some(surface) = surface else {
        return Ok(WindowPresentation {
            surface_config: None,
            blit_pipeline: None,
            blit_bind_group: None,
            blit_layout: None,
            blit_sampler: None,
        });
    };
    let capabilities = surface.get_capabilities(adapter);
    let mut config = surface
        .get_default_config(adapter, width, height)
        .ok_or_else(|| anyhow!("wgpu surface has no default configuration"))?;
    configure_color_format(&mut config, &capabilities.formats)?;
    config.present_mode = wgpu::PresentMode::AutoVsync;
    config.desired_maximum_frame_latency = 2;
    super::super::configure_window_surface(surface, device, &config)?;
    Ok(create_blit_presentation(device, game_view, config))
}

pub(super) fn configure_color_format(
    config: &mut wgpu::SurfaceConfiguration,
    formats: &[wgpu::TextureFormat],
) -> Result<()> {
    config.format = formats
        .iter()
        .copied()
        .find(|format| !format.is_srgb())
        .or_else(|| formats.first().copied())
        .ok_or_else(|| anyhow!("wgpu surface exposes no formats"))?;
    // Game and host UI channels are already encoded. Keep the UNORM view
    // while declaring sRGB to the platform compositor independently.
    config.color_space = wgpu::SurfaceColorSpace::Srgb;
    let view_format = super::super::window_target_format(config.format);
    config.view_formats.clear();
    if view_format != config.format {
        // wgpu permits a surface view to differ only in sRGB interpretation.
        // Register the alias before configuration; both window pipelines and
        // the acquired texture view must use this same format.
        config.view_formats.push(view_format);
    }
    Ok(())
}

pub(super) fn create_blit_presentation(
    device: &wgpu::Device,
    game_view: &wgpu::TextureView,
    config: wgpu::SurfaceConfiguration,
) -> WindowPresentation {
    let format = super::super::window_target_format(config.format);

    let blit_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("Stella game-target blit layout"),
        entries: &[texture_layout_entry(0), sampler_layout_entry(1)],
    });
    let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("Stella blit pipeline layout"),
        bind_group_layouts: &[Some(&blit_layout)],
        immediate_size: 0,
    });
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("Stella game-target blit shader"),
        source: wgpu::ShaderSource::Wgsl(include_str!("../../../blit.wgsl").into()),
    });
    let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("Stella letterbox blit pipeline"),
        layout: Some(&pipeline_layout),
        vertex: wgpu::VertexState {
            module: &shader,
            entry_point: Some("blit_vertex"),
            compilation_options: Default::default(),
            buffers: &[],
        },
        primitive: wgpu::PrimitiveState::default(),
        depth_stencil: None,
        multisample: wgpu::MultisampleState::default(),
        fragment: Some(wgpu::FragmentState {
            module: &shader,
            entry_point: Some("blit_fragment"),
            compilation_options: Default::default(),
            targets: &[Some(wgpu::ColorTargetState {
                format,
                blend: None,
                write_mask: wgpu::ColorWrites::ALL,
            })],
        }),
        multiview_mask: None,
        cache: None,
    });
    let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
        label: Some("Stella letterbox linear sampler"),
        mag_filter: wgpu::FilterMode::Linear,
        min_filter: wgpu::FilterMode::Linear,
        ..Default::default()
    });
    let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("Stella game-target blit bind group"),
        layout: &blit_layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(game_view),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::Sampler(&sampler),
            },
        ],
    });
    WindowPresentation {
        surface_config: Some(config),
        blit_pipeline: Some(pipeline),
        blit_bind_group: Some(bind_group),
        blit_layout: Some(blit_layout),
        blit_sampler: Some(sampler),
    }
}
