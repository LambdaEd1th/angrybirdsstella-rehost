//! Window letterbox presentation and headless RGBA readback.

use super::super::*;

impl GpuRenderer {
    pub(crate) fn resize_surface(&mut self, width: u32, height: u32) -> Result<()> {
        self.check_device()?;
        if width == 0 || height == 0 {
            return Ok(());
        }
        if let (Some(surface), Some(config)) = (&self.surface, &mut self.surface_config)
            && (config.width != width || config.height != height)
        {
            config.width = width;
            config.height = height;
            if self.surface_recovery.is_none() {
                surface.configure(&self.device, config);
            }
        }
        self.device_state.check()
    }

    pub(crate) fn resize_game_target(&mut self, resolution: GameResolution) -> Result<()> {
        self.check_device()?;
        if resolution == self.resolution {
            return Ok(());
        }
        let (game_texture, game_view) =
            super::initialization::target::create_game_texture(&self.device, resolution);
        self.game_texture = game_texture;
        self.game_view = game_view;
        self.resolution = resolution;

        // A captured Image owns its allocation independently of the current
        // drawable. Recreate only the framebuffer source binding; retained
        // captures keep both their pixel content and original dimensions.
        self.capture_bind_group =
            super::capture::bind_framebuffer(&self.device, &self.capture_layout, &self.game_view);
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
        self.device_state.check()
    }

    /// Present the completed game target without replaying its command stream.
    pub(crate) fn present_to_window(&mut self, width: u32, height: u32) -> Result<()> {
        self.present_window_acquiring(width, height, |renderer| {
            let surface = renderer
                .surface
                .as_ref()
                .ok_or_else(|| anyhow!("window renderer has no wgpu surface"))?;
            Ok(surface.get_current_texture())
        })
    }

    #[cfg(test)]
    pub(crate) fn present_window_with_acquire_for_test(
        &mut self,
        width: u32,
        height: u32,
        mut acquire: impl FnMut(&wgpu::Surface<'static>) -> wgpu::CurrentSurfaceTexture,
    ) -> Result<()> {
        self.present_window_acquiring(width, height, |renderer| {
            let surface = renderer
                .surface
                .as_ref()
                .ok_or_else(|| anyhow!("window renderer has no wgpu surface"))?;
            Ok(acquire(surface))
        })
    }

    fn present_window_acquiring(
        &mut self,
        width: u32,
        height: u32,
        acquire: impl FnMut(&mut Self) -> Result<wgpu::CurrentSurfaceTexture>,
    ) -> Result<()> {
        self.check_device()?;
        if width == 0 || height == 0 {
            return Ok(());
        }
        self.resize_surface(width, height)?;
        let pending = self.surface_recovery.take();
        let frame = super::surface_acquisition::acquire_window_frame(
            self,
            pending,
            acquire,
            |renderer, recovery| renderer.recover_window_surface(recovery, width, height),
        )?;
        self.surface_recovery = frame.pending_recovery;
        let Some(output) = frame.texture else {
            return self.device_state.check();
        };
        self.device_state.check()?;
        let view = super::window_target_view(&output.texture);
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Stella surface blit encoder"),
            });
        self.encode_window_game(&mut encoder, &view, width, height)?;
        // Skynest's native UIView is a sibling above the EAGL view, not part
        // of its drawable. Composite only onto the acquired window surface.
        self.encode_window_overlay(&mut encoder, &view, width, height);
        self.queue.submit([encoder.finish()]);
        self.device_state.check()?;
        if let Some(window) = &self.surface_window {
            // Wayland ties its next redraw callback to the actual present.
            window.pre_present_notify();
        }
        self.queue.present(output);
        self.device_state.check()
    }

    pub(super) fn encode_window_game(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        view: &wgpu::TextureView,
        width: u32,
        height: u32,
    ) -> Result<()> {
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Stella letterbox presentation pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view,
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

    /// Read the already-rendered target for an opaque display screenshot.
    /// Native sharing uses ordered copies with the original RGBA alpha.
    pub(crate) fn read_game_rgba(&self) -> Result<Vec<u8>> {
        self.check_device()?;
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Stella display screenshot copy encoder"),
            });
        let readback = self.encode_game_readback(&mut encoder);
        self.queue.submit([encoder.finish()]);
        let mut rgba = self.finish_game_readback(readback)?;
        // The EAGL drawable is presented as an opaque screen even though
        // glBlendFunc also evolves its unused alpha channel. PNG consumers do
        // use alpha, so normalize it to the visible framebuffer contract.
        for pixel in rgba.as_chunks_mut::<4>().0 {
            pixel[3] = 255;
        }
        Ok(rgba)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn device_loss_during_acquisition_stops_surface_recovery() {
        let size = GameResolution::new(8, 4).unwrap();
        let mut renderer = GpuRenderer::headless(size).unwrap();
        let mut acquisitions = 0;
        let result = renderer.present_window_acquiring(8, 4, |renderer| {
            acquisitions += 1;
            renderer.destroy_device_for_test();
            Ok(wgpu::CurrentSurfaceTexture::Lost)
        });
        assert_eq!(
            result.unwrap_err().to_string(),
            "wgpu device lost (Destroyed)"
        );
        assert_eq!(acquisitions, 1);
        assert_eq!(renderer.resolution, size);
    }
}
