//! Window compositor pixel checks without a native window or surface.

use super::*;
use image::Rgba;

fn catalog() -> AssetCatalog {
    AssetCatalog {
        root: Default::default(),
        font_root: Default::default(),
        regions: HashMap::new(),
        composites: HashMap::new(),
        masked_textures: HashMap::new(),
        fonts: HashMap::new(),
        textures: HashMap::new(),
        system_labels: Default::default(),
        captures: Default::default(),
    }
}

fn read_texture(renderer: &GpuRenderer, texture: &wgpu::Texture) -> RgbaImage {
    let (width, height) = (texture.width(), texture.height());
    let pitch = (width * 4).div_ceil(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT)
        * wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;
    let buffer = renderer.device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("window overlay test readback"),
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
    RgbaImage::from_raw(width, height, pixels).unwrap()
}

pub(super) fn composite(renderer: &GpuRenderer, background: &RgbaImage) -> RgbaImage {
    let (width, height) = background.dimensions();
    let target = renderer.device.create_texture(&wgpu::TextureDescriptor {
        label: Some("window overlay test substitute surface"),
        size: wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: GAME_FORMAT,
        usage: wgpu::TextureUsages::COPY_DST
            | wgpu::TextureUsages::COPY_SRC
            | wgpu::TextureUsages::RENDER_ATTACHMENT,
        view_formats: &[],
    });
    renderer.queue.write_texture(
        target.as_image_copy(),
        background.as_raw(),
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(width * 4),
            rows_per_image: Some(height),
        },
        target.size(),
    );
    let mut encoder = renderer.device.create_command_encoder(&Default::default());
    renderer.encode_window_overlay(
        &mut encoder,
        &target.create_view(&Default::default()),
        width,
        height,
    );
    renderer.queue.submit([encoder.finish()]);
    read_texture(renderer, &target)
}

#[test]
fn premultiplied_window_overlay_covers_letterbox_without_modifying_game_or_capture() {
    let size = GameResolution {
        width: 4,
        height: 4,
    };
    let mut renderer = GpuRenderer::headless(size).unwrap();
    let assets = catalog();
    let mut frame = PreparedFrame {
        resolution: size,
        ..Default::default()
    };
    renderer
        .render_offscreen(&assets, &frame, [0, 0, 255])
        .unwrap();
    let game_before = renderer.read_game_rgba().unwrap();
    let texture_count = renderer.textures.len();
    let mut layer = RgbaImage::from_pixel(4, 8, Rgba([128, 0, 0, 128]));
    layer.put_pixel(0, 0, Rgba([0, 255, 0, 255]));
    layer.put_pixel(3, 7, Rgba([0, 0, 0, 0]));
    renderer.set_window_overlay(Some(&layer)).unwrap();
    assert_eq!(renderer.textures.len(), texture_count);

    // Stand-in window: 4x4 blue game centered in 4x8 black letterboxing.
    let background = RgbaImage::from_fn(4, 8, |_, y| {
        if (2..6).contains(&y) {
            Rgba([0, 0, 255, 255])
        } else {
            Rgba([0, 0, 0, 255])
        }
    });
    let window = composite(&renderer, &background);
    assert_eq!(*window.get_pixel(0, 0), Rgba([0, 255, 0, 255]));
    assert_eq!(*window.get_pixel(2, 0), Rgba([128, 0, 0, 255]));
    assert_eq!(*window.get_pixel(2, 3), Rgba([128, 0, 127, 255]));
    assert_eq!(*window.get_pixel(3, 7), Rgba([0, 0, 0, 255]));
    assert_eq!(renderer.read_game_rgba().unwrap(), game_before);

    let capture = "<capture:window-overlay-excluded>";
    frame
        .operations
        .push(PreparedOperation::Capture(capture.to_owned()));
    renderer.render_before_clear(&assets, &frame).unwrap();
    let captured = read_texture(&renderer, &renderer.textures[capture].texture);
    assert!(
        captured
            .pixels()
            .all(|pixel| *pixel == Rgba([0, 0, 255, 255]))
    );
    assert_eq!(renderer.read_game_rgba().unwrap(), game_before);
}

#[test]
fn window_overlay_dirty_updates_resize_remove_and_reject_empty_without_losing_layer() {
    let mut renderer = GpuRenderer::headless(GameResolution {
        width: 4,
        height: 4,
    })
    .unwrap();
    let background = RgbaImage::from_pixel(4, 8, Rgba([0, 0, 255, 255]));
    let first = RgbaImage::from_pixel(4, 8, Rgba([255, 0, 0, 255]));
    renderer.set_window_overlay(Some(&first)).unwrap();
    let original_allocation = renderer.window_overlay.as_ref().unwrap().texture.clone();
    assert_eq!(composite(&renderer, &background), first);

    let changed = RgbaImage::from_pixel(4, 8, Rgba([0, 255, 0, 255]));
    renderer.set_window_overlay(Some(&changed)).unwrap();
    assert_eq!(
        renderer.window_overlay.as_ref().unwrap().texture,
        original_allocation
    );
    assert_eq!(composite(&renderer, &background), changed);
    assert!(
        renderer
            .set_window_overlay(Some(&RgbaImage::new(0, 4)))
            .is_err()
    );
    assert_eq!(composite(&renderer, &background), changed);

    let resized = RgbaImage::from_pixel(2, 2, Rgba([128, 0, 0, 128]));
    renderer.set_window_overlay(Some(&resized)).unwrap();
    assert_ne!(
        renderer.window_overlay.as_ref().unwrap().texture,
        original_allocation
    );
    assert_eq!(
        composite(&renderer, &background),
        RgbaImage::from_pixel(4, 8, Rgba([128, 0, 127, 255]))
    );
    assert_eq!(
        composite(
            &renderer,
            &RgbaImage::from_pixel(4, 8, Rgba([0, 64, 0, 64]))
        ),
        RgbaImage::from_pixel(4, 8, Rgba([128, 32, 0, 160]))
    );
    renderer.set_window_overlay(None).unwrap();
    assert!(renderer.window_overlay.is_none());
    assert_eq!(composite(&renderer, &background), background);
}
