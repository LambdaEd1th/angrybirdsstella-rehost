//! Window letterbox presentation and headless RGBA readback.

use super::super::*;

impl GpuRenderer {
    pub(crate) fn resize_surface(&mut self, width: u32, height: u32) {
        if width == 0 || height == 0 {
            return;
        }
        if let (Some(surface), Some(config)) = (&self.surface, &mut self.surface_config)
            && (config.width != width || config.height != height)
        {
            config.width = width;
            config.height = height;
            surface.configure(&self.device, config);
        }
    }

    pub(crate) fn resize_game_target(&mut self, resolution: GameResolution) {
        if resolution == self.resolution {
            return;
        }
        let (game_texture, game_view) =
            super::initialization::target::create_game_texture(&self.device, resolution);
        self.game_texture = game_texture;
        self.game_view = game_view;
        self.resolution = resolution;

        let capture_names = self
            .textures
            .keys()
            .filter(|name| name.starts_with("<capture:"))
            .cloned()
            .collect::<Vec<_>>();
        for name in capture_names {
            self.textures.insert(
                name.clone(),
                super::super::resources::create_capture_texture(&self.device, &name, resolution),
            );
        }
        self.texture_bind_groups.clear();
        if let (Some(layout), Some(sampler)) = (&self.blit_layout, &self.blit_sampler) {
            self.blit_bind_group =
                Some(self.device.create_bind_group(&wgpu::BindGroupDescriptor {
                    label: Some("Stella resized game-target blit bind group"),
                    layout,
                    entries: &[
                        wgpu::BindGroupEntry {
                            binding: 0,
                            resource: wgpu::BindingResource::TextureView(&self.game_view),
                        },
                        wgpu::BindGroupEntry {
                            binding: 1,
                            resource: wgpu::BindingResource::Sampler(sampler),
                        },
                    ],
                }));
        }
    }

    pub(crate) fn render_to_window(
        &mut self,
        assets: &AssetCatalog,
        frame: &PreparedFrame,
        background_color: [u8; 3],
        width: u32,
        height: u32,
    ) -> Result<()> {
        if width == 0 || height == 0 {
            return Ok(());
        }
        self.render_game(assets, frame, background_color)?;
        self.resize_surface(width, height);
        let surface = self
            .surface
            .as_ref()
            .ok_or_else(|| anyhow!("window renderer has no wgpu surface"))?;
        let output = match surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(output)
            | wgpu::CurrentSurfaceTexture::Suboptimal(output) => output,
            wgpu::CurrentSurfaceTexture::Lost | wgpu::CurrentSurfaceTexture::Outdated => {
                if let Some(config) = &self.surface_config {
                    surface.configure(&self.device, config);
                }
                match surface.get_current_texture() {
                    wgpu::CurrentSurfaceTexture::Success(output)
                    | wgpu::CurrentSurfaceTexture::Suboptimal(output) => output,
                    status => {
                        return Err(anyhow!("acquire reconfigured wgpu surface: {status:?}"));
                    }
                }
            }
            wgpu::CurrentSurfaceTexture::Timeout | wgpu::CurrentSurfaceTexture::Occluded => {
                return Ok(());
            }
            wgpu::CurrentSurfaceTexture::Validation => {
                return Err(anyhow!("wgpu surface acquisition validation error"));
            }
        };
        let view = output
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Stella surface blit encoder"),
            });
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Stella letterbox presentation pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                ..Default::default()
            });
            let scale = (width as f32 / self.resolution.width as f32)
                .min(height as f32 / self.resolution.height as f32);
            let viewport_width = self.resolution.width as f32 * scale;
            let viewport_height = self.resolution.height as f32 * scale;
            pass.set_viewport(
                (width as f32 - viewport_width) * 0.5,
                (height as f32 - viewport_height) * 0.5,
                viewport_width,
                viewport_height,
                0.0,
                1.0,
            );
            pass.set_pipeline(
                self.blit_pipeline
                    .as_ref()
                    .ok_or_else(|| anyhow!("window renderer has no blit pipeline"))?,
            );
            pass.set_bind_group(
                0,
                self.blit_bind_group
                    .as_ref()
                    .ok_or_else(|| anyhow!("window renderer has no blit bind group"))?,
                &[],
            );
            pass.draw(0..3, 0..1);
        }
        self.queue.submit([encoder.finish()]);
        self.queue.present(output);
        Ok(())
    }

    pub(crate) fn render_to_rgba(
        &mut self,
        assets: &AssetCatalog,
        frame: &PreparedFrame,
        background_color: [u8; 3],
    ) -> Result<Vec<u8>> {
        self.render_game(assets, frame, background_color)?;
        self.read_game_rgba()
    }

    /// Read the already-rendered game target. Screenshot sharing calls this
    /// after normal window presentation so it captures that exact frame
    /// without traversing Lua or issuing the scene draw a second time.
    pub(crate) fn read_game_rgba(&self) -> Result<Vec<u8>> {
        let unpadded_bytes_per_row = self.resolution.width * 4;
        let alignment = wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;
        let bytes_per_row = unpadded_bytes_per_row.div_ceil(alignment) * alignment;
        let buffer_size = u64::from(bytes_per_row) * u64::from(self.resolution.height);
        let readback = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Stella screenshot readback"),
            size: buffer_size,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Stella screenshot copy encoder"),
            });
        encoder.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo {
                texture: &self.game_texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::TexelCopyBufferInfo {
                buffer: &readback,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(bytes_per_row),
                    rows_per_image: Some(self.resolution.height),
                },
            },
            wgpu::Extent3d {
                width: self.resolution.width,
                height: self.resolution.height,
                depth_or_array_layers: 1,
            },
        );
        self.queue.submit([encoder.finish()]);
        let (sender, receiver) = mpsc::channel();
        readback.map_async(wgpu::MapMode::Read, .., move |result| {
            let _ = sender.send(result);
        });
        self.device
            .poll(wgpu::PollType::wait_indefinitely())
            .context("wait for Stella screenshot readback")?;
        receiver
            .recv()
            .context("receive Stella screenshot map result")?
            .context("map Stella screenshot buffer")?;
        let mapped = readback
            .get_mapped_range(..)
            .context("get mapped Stella screenshot bytes")?;
        let mut rgba = Vec::with_capacity(
            (u64::from(unpadded_bytes_per_row) * u64::from(self.resolution.height)) as usize,
        );
        for row in mapped.chunks_exact(bytes_per_row as usize) {
            rgba.extend_from_slice(&row[..unpadded_bytes_per_row as usize]);
        }
        drop(mapped);
        readback.unmap();
        // The EAGL drawable is presented as an opaque screen even though
        // glBlendFunc also evolves its unused alpha channel. PNG consumers do
        // use alpha, so normalize it to the visible framebuffer contract.
        for pixel in rgba.chunks_exact_mut(4) {
            pixel[3] = 255;
        }
        Ok(rgba)
    }
}
