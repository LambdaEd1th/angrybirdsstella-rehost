//! wgpu boundary regressions split by recovered Purple render responsibility.

use super::*;
use stella_assets::surface_format::SurfaceFormat;

mod batch;
mod capture;
mod device_loss;
mod file_images;
mod geometry;
mod native_images;
mod program;
mod sharing;
mod sprites;
mod text;

/// Raw attachment samples, independent of the production screenshot readback.
/// Used for both presentation targets and call-prefix screenshot references.
pub(super) fn read_texture(renderer: &GpuRenderer, texture: &wgpu::Texture) -> RgbaImage {
    let (width, height) = (texture.width(), texture.height());
    let pitch = (width * 4).div_ceil(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT)
        * wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;
    let buffer = renderer.device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("independent GPU test readback"),
        size: u64::from(pitch) * u64::from(height),
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut encoder = renderer.device.create_command_encoder(&Default::default());
    encoder.copy_texture_to_buffer(
        texture.as_image_copy(),
        wgpu::TexelCopyBufferInfo {
            buffer: &buffer,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(pitch),
                rows_per_image: Some(height),
            },
        },
        texture.size(),
    );
    renderer.queue.submit([encoder.finish()]);
    let (sender, receiver) = mpsc::channel();
    buffer.map_async(wgpu::MapMode::Read, .., move |result| {
        sender.send(result).unwrap();
    });
    renderer
        .device
        .poll(wgpu::PollType::wait_indefinitely())
        .unwrap();
    receiver.recv().unwrap().unwrap();
    let mapped = buffer.get_mapped_range(..).unwrap();
    let pixels = mapped
        .chunks_exact(pitch as usize)
        .flat_map(|row| row[..(width * 4) as usize].iter().copied())
        .collect();
    drop(mapped);
    buffer.unmap();
    let mut image = RgbaImage::from_raw(width, height, pixels).unwrap();
    if matches!(
        texture.format(),
        wgpu::TextureFormat::Bgra8Unorm | wgpu::TextureFormat::Bgra8UnormSrgb
    ) {
        for pixel in image.pixels_mut() {
            pixel.0.swap(0, 2);
        }
    }
    image
}

impl GpuRenderer {
    /// Exercise wgpu's actual loss callback, after completing submitted work.
    pub(crate) fn destroy_device_for_test(&self) {
        self.device
            .poll(wgpu::PollType::wait_indefinitely())
            .unwrap();
        self.device.destroy();
    }
}

fn alpha_texture(width: u32, height: u32) -> TextureAsset {
    TextureAsset::new(
        RgbaImage::from_pixel(width, height, image::Rgba([255; 4])),
        SurfaceFormat::A8B8G8R8,
    )
}

#[test]
fn persistent_gpu_streams_reuse_capacity_and_grow_only_when_needed() {
    let mut renderer = GpuRenderer::headless(GameResolution::default()).unwrap();
    let initial_storage = renderer.draw_storage_capacity;
    let initial_vertices = renderer.vertex_capacity;

    renderer
        .ensure_stream_capacity(initial_storage / 2, initial_vertices / 2)
        .unwrap();
    assert_eq!(renderer.draw_storage_capacity, initial_storage);
    assert_eq!(renderer.vertex_capacity, initial_vertices);

    renderer
        .ensure_stream_capacity(initial_storage + 1, initial_vertices + 1)
        .unwrap();
    assert_eq!(renderer.draw_storage_capacity, initial_storage * 2);
    assert_eq!(renderer.vertex_capacity, initial_vertices * 2);

    renderer
        .ensure_stream_capacity(initial_storage, initial_vertices)
        .unwrap();
    assert_eq!(renderer.draw_storage_capacity, initial_storage * 2);
    assert_eq!(renderer.vertex_capacity, initial_vertices * 2);
}
