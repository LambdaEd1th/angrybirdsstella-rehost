//! Ordered copies of the RGBA game target, excluding the presentation layer.

use super::super::*;

pub(super) struct GameReadback {
    buffer: wgpu::Buffer,
    resolution: GameResolution,
    bytes_per_row: u32,
}

impl GpuRenderer {
    pub(super) fn encode_game_readback(&self, encoder: &mut wgpu::CommandEncoder) -> GameReadback {
        let alignment = wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;
        let bytes_per_row = (self.resolution.width * 4).div_ceil(alignment) * alignment;
        let buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Stella ordered screenshot readback"),
            size: u64::from(bytes_per_row) * u64::from(self.resolution.height),
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        encoder.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo {
                texture: &self.game_texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::TexelCopyBufferInfo {
                buffer: &buffer,
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
        GameReadback {
            buffer,
            resolution: self.resolution,
            bytes_per_row,
        }
    }

    /// The copy has already been submitted at the native call's draw position.
    /// Unlike display screenshots, PNG shares preserve the drawable's alpha.
    pub(super) fn finish_game_readback(&self, readback: GameReadback) -> Result<Vec<u8>> {
        let (sender, receiver) = mpsc::channel();
        readback
            .buffer
            .map_async(wgpu::MapMode::Read, .., move |result| {
                let _ = sender.send(result);
            });
        self.device
            .poll(wgpu::PollType::wait_indefinitely())
            .context("wait for Stella screenshot readback")?;
        self.device_state.check()?;
        receiver
            .recv()
            .context("receive Stella screenshot map result")?
            .context("map Stella screenshot buffer")?;
        let mapped = readback
            .buffer
            .get_mapped_range(..)
            .context("get mapped Stella screenshot bytes")?;
        let row_bytes = readback.resolution.width as usize * 4;
        let mut rgba = Vec::with_capacity(row_bytes * readback.resolution.height as usize);
        for row in mapped.chunks_exact(readback.bytes_per_row as usize) {
            rgba.extend_from_slice(&row[..row_bytes]);
        }
        drop(mapped);
        readback.buffer.unmap();
        Ok(rgba)
    }

    pub(crate) fn take_screenshot_shares(&mut self) -> Vec<ScreenshotShareCapture> {
        std::mem::take(&mut self.screenshot_shares)
    }
}
