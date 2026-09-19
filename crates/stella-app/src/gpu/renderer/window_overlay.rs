//! Window-only native UI layer, outside Purple's game/capture framebuffer.
//!
//! showSkynestView: (0x1007717B0) appends its background and account view as
//! UIKit siblings above the EAGL view. This host compositor preserves that
//! ownership boundary without entering the Lua draw or capture resources.

use super::super::*;

pub(in crate::gpu) struct WindowOverlay {
    texture: wgpu::Texture,
    layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
    bind_group: wgpu::BindGroup,
    pipeline: wgpu::RenderPipeline,
}

impl GpuRenderer {
    #[cfg(test)]
    pub(crate) fn composite_window_overlay_for_test(&self, background: &RgbaImage) -> RgbaImage {
        tests::composite(self, background)
    }

    #[cfg(test)]
    pub(crate) fn has_window_overlay_for_test(&self) -> bool {
        self.window_overlay.is_some()
    }

    /// Replace the window UI with top-left-origin, premultiplied RGBA8 pixels.
    ///
    /// Call only when the host UI is dirty. Same-size updates reuse the GPU
    /// allocation, and None drops the layer. Supply physical window dimensions
    /// for pixel-exact output; another extent is scaled over the full window,
    /// independently of the game's letterbox viewport. Pixel channels use the
    /// same unorm interpretation as the ordinary game-target presentation.
    pub(crate) fn set_window_overlay(&mut self, image: Option<&RgbaImage>) -> Result<()> {
        let Some(image) = image else {
            self.window_overlay = None;
            return Ok(());
        };
        let (width, height) = image.dimensions();
        if width == 0 || height == 0 {
            return Err(anyhow!("window overlay must have a nonzero extent"));
        }
        let limit = self.device.limits().max_texture_dimension_2d;
        if width > limit || height > limit || width.checked_mul(4).is_none() {
            return Err(anyhow!(
                "window overlay extent {width}x{height} exceeds the GPU texture limit {limit}"
            ));
        }
        if let Some(overlay) = &mut self.window_overlay {
            if overlay.texture.width() != width || overlay.texture.height() != height {
                overlay.texture = create_texture(&self.device, width, height);
                overlay.bind_group = create_bind_group(
                    &self.device,
                    &overlay.layout,
                    &overlay.sampler,
                    &overlay.texture,
                );
            }
        } else {
            let format = self
                .surface_config
                .as_ref()
                .map_or(GAME_FORMAT, |config| config.format);
            self.window_overlay = Some(WindowOverlay::new(&self.device, width, height, format));
        }
        let overlay = self
            .window_overlay
            .as_ref()
            .expect("overlay was initialized");
        self.queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &overlay.texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            image.as_raw(),
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(width * 4),
                rows_per_image: Some(height),
            },
            wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
        );
        Ok(())
    }

    pub(super) fn encode_window_overlay(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        output_view: &wgpu::TextureView,
        width: u32,
        height: u32,
    ) {
        let Some(overlay) = &self.window_overlay else {
            return;
        };
        if width == 0 || height == 0 {
            return;
        }
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("Stella native window UI overlay"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: output_view,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Load,
                    store: wgpu::StoreOp::Store,
                },
            })],
            ..Default::default()
        });
        // This is a separate pass: no scene scissor, projection, alpha or
        // letterbox viewport can leak into the native modal's presentation.
        pass.set_viewport(0.0, 0.0, width as f32, height as f32, 0.0, 1.0);
        pass.set_scissor_rect(0, 0, width, height);
        pass.set_pipeline(&overlay.pipeline);
        pass.set_bind_group(0, &overlay.bind_group, &[]);
        pass.draw(0..3, 0..1);
    }
}

impl WindowOverlay {
    fn new(device: &wgpu::Device, width: u32, height: u32, format: wgpu::TextureFormat) -> Self {
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Stella window overlay texture layout"),
            entries: &[
                resources::texture_layout_entry(0),
                resources::sampler_layout_entry(1),
            ],
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Stella window overlay pipeline layout"),
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Stella window overlay shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("../../blit.wgsl").into()),
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Stella premultiplied native window UI compositor"),
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
                    blend: Some(wgpu::BlendState::PREMULTIPLIED_ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            multiview_mask: None,
            cache: None,
        });
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("Stella window overlay linear sampler"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            ..Default::default()
        });
        let texture = create_texture(device, width, height);
        let bind_group = create_bind_group(device, &layout, &sampler, &texture);
        Self {
            texture,
            layout,
            sampler,
            bind_group,
            pipeline,
        }
    }
}

fn create_texture(device: &wgpu::Device, width: u32, height: u32) -> wgpu::Texture {
    device.create_texture(&wgpu::TextureDescriptor {
        label: Some("Stella native window UI pixels"),
        size: wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: GAME_FORMAT,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    })
}

fn create_bind_group(
    device: &wgpu::Device,
    layout: &wgpu::BindGroupLayout,
    sampler: &wgpu::Sampler,
    texture: &wgpu::Texture,
) -> wgpu::BindGroup {
    device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("Stella native window UI binding"),
        layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(
                    &texture.create_view(&wgpu::TextureViewDescriptor::default()),
                ),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::Sampler(sampler),
            },
        ],
    })
}

#[cfg(test)]
mod tests;
