//! wgpu boundary regressions split by recovered Purple render responsibility.

use super::*;
use stella_assets::surface_format::SurfaceFormat;

mod batch;
mod geometry;
mod program;
mod sprites;
mod text;

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
