//! Window compositor pixel checks without a native window or surface.

use super::*;
use crate::gpu::tests::read_texture;
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

fn substitute_surface(renderer: &GpuRenderer, width: u32, height: u32) -> wgpu::Texture {
    let config = renderer.surface_config.as_ref();
    renderer.device.create_texture(&wgpu::TextureDescriptor {
        label: Some("window overlay test substitute surface"),
        size: wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: config.map_or(GAME_FORMAT, |config| config.format),
        usage: wgpu::TextureUsages::COPY_DST
            | wgpu::TextureUsages::COPY_SRC
            | wgpu::TextureUsages::RENDER_ATTACHMENT,
        view_formats: config.map_or(&[], |config| &config.view_formats),
    })
}

pub(super) fn composite(renderer: &GpuRenderer, background: &RgbaImage) -> RgbaImage {
    let (width, height) = background.dimensions();
    let target = substitute_surface(renderer, width, height);
    let mut bytes = background.as_raw().clone();
    if matches!(
        target.format(),
        wgpu::TextureFormat::Bgra8Unorm | wgpu::TextureFormat::Bgra8UnormSrgb
    ) {
        for pixel in bytes.as_chunks_mut::<4>().0 {
            pixel.swap(0, 2);
        }
    }
    renderer.queue.write_texture(
        target.as_image_copy(),
        &bytes,
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
        &super::super::window_target_view(&target),
        width,
        height,
    );
    renderer.queue.submit([encoder.finish()]);
    read_texture(renderer, &target)
}

fn present(renderer: &GpuRenderer, width: u32, height: u32) -> RgbaImage {
    let target = substitute_surface(renderer, width, height);
    let view = super::super::window_target_view(&target);
    let mut encoder = renderer.device.create_command_encoder(&Default::default());
    renderer
        .encode_window_game(&mut encoder, &view, width, height)
        .unwrap();
    renderer.encode_window_overlay(&mut encoder, &view, width, height);
    renderer.queue.submit([encoder.finish()]);
    read_texture(renderer, &target)
}

fn render_game_pixels(renderer: &mut GpuRenderer, pixels: &RgbaImage) {
    let size = GameResolution {
        width: pixels.width(),
        height: pixels.height(),
    };
    let rectangles: Vec<_> = pixels
        .enumerate_pixels()
        .map(|(x, y, pixel)| RectRenderCommand {
            order: u64::from(y * size.width + x),
            red: f64::from(pixel[0]) / 255.0,
            green: f64::from(pixel[1]) / 255.0,
            blue: f64::from(pixel[2]) / 255.0,
            alpha: 1.0,
            left: f64::from(x),
            top: f64::from(y),
            right: f64::from(x + 1),
            bottom: f64::from(y + 1),
            color_program: ColorProgram::Plain,
            vertices: None,
            mesh_topology: ColorMeshTopology::TriangleFan,
            clip_rect: None,
            projection_3d: None,
        })
        .collect();
    let mut assets = catalog();
    let frame = assets
        .prepare_gpu_frame_at_resolution(size, &[], &[], &rectangles, &[])
        .unwrap();
    renderer.resize_game_target(size).unwrap();
    renderer
        .render_offscreen(&assets, &frame, [0, 0, 0])
        .unwrap();
    assert_eq!(renderer.read_game_rgba().unwrap(), *pixels.as_raw());
}

const WINDOW_FORMATS: [wgpu::TextureFormat; 4] = [
    wgpu::TextureFormat::Rgba8Unorm,
    wgpu::TextureFormat::Rgba8UnormSrgb,
    wgpu::TextureFormat::Bgra8Unorm,
    wgpu::TextureFormat::Bgra8UnormSrgb,
];

#[test]
fn window_color_formats_preserve_all_game_bytes_scaling_and_resized_bindings() {
    let mut renderer = GpuRenderer::headless(GameResolution {
        width: 256,
        height: 1,
    })
    .unwrap();
    let gradient = RgbaImage::from_fn(256, 1, |x, _| {
        Rgba([x as u8, (255 - x) as u8, (x * 37) as u8, 255])
    });
    let black_white = RgbaImage::from_fn(2, 1, |x, _| {
        let gray = x as u8 * 255;
        Rgba([gray, gray, gray, 255])
    });
    let mut results = Vec::new();
    for format in WINDOW_FORMATS {
        render_game_pixels(&mut renderer, &gradient);
        renderer
            .configure_window_target_for_test(&[format], 256, 3)
            .unwrap();
        let unscaled = present(&renderer, 256, 3);
        assert_eq!(renderer.read_game_rgba().unwrap(), *gradient.as_raw());
        // resize_game_target must rebind the retained window pipeline to the
        // replacement game texture. Do not recreate it for this second blit.
        render_game_pixels(&mut renderer, &black_white);
        let scaled = present(&renderer, 4, 2);
        assert_eq!(renderer.read_game_rgba().unwrap(), *black_white.as_raw());
        eprintln!(
            "{format:?}: game128={:?}; scaled={:?}",
            unscaled.get_pixel(128, 1).0,
            scaled.pixels().map(|pixel| pixel.0).collect::<Vec<_>>()
        );
        results.push((format, unscaled, scaled));
    }
    let baseline_scaled = results[0].2.clone();
    for (format, unscaled, scaled) in results {
        assert_eq!(
            scaled, baseline_scaled,
            "{format:?} changed scaling relative to the ordinary UNORM format"
        );
        for x in 0..256 {
            assert_eq!(
                unscaled.get_pixel(x, 1),
                gradient.get_pixel(x, 0),
                "{format:?} changed the game color at {x}"
            );
            for y in [0, 2] {
                assert_eq!(*unscaled.get_pixel(x, y), Rgba([0, 0, 0, 255]));
            }
        }
        for y in 0..2 {
            for (x, value) in [0, 64, 191, 255].into_iter().enumerate() {
                let pixel = scaled.get_pixel(x as u32, y).0;
                assert_eq!(pixel, [pixel[0], pixel[0], pixel[0], 255]);
                if x == 0 || x == 3 {
                    assert_eq!(pixel[0], value, "{format:?} changed a clamped endpoint");
                } else {
                    // Vulkan float-to-UNORM conversion may choose either
                    // neighboring integer. Metal gives64/191 here; lavapipe
                    // gives63/191. Keep one-byte filtered precision separate
                    // from the exact unscaled and cross-format checks above.
                    assert!(
                        pixel[0].abs_diff(value) <= 1,
                        "{format:?} changed the encoded-channel interpolation: {pixel:?} vs {value}"
                    );
                }
            }
        }
    }
}

#[test]
fn window_color_formats_preserve_premultiplied_ui_blending_and_capture_boundary() {
    let size = GameResolution {
        width: 8,
        height: 2,
    };
    let mut renderer = GpuRenderer::headless(size).unwrap();
    let game = RgbaImage::from_pixel(8, 2, Rgba([64, 128, 192, 255]));
    render_game_pixels(&mut renderer, &game);
    let alphas = [0_u8, 1, 16, 64, 128, 192, 254, 255];
    let layer = RgbaImage::from_fn(8, 4, |x, _| {
        let alpha = alphas[x as usize];
        Rgba([alpha, alpha / 2, alpha / 4, alpha])
    });
    let mut results = Vec::new();
    for format in WINDOW_FORMATS {
        renderer.set_window_overlay(None).unwrap();
        renderer
            .configure_window_target_for_test(&[format], 8, 4)
            .unwrap();
        renderer.set_window_overlay(Some(&layer)).unwrap();
        let window = present(&renderer, 8, 4);
        assert_eq!(renderer.read_game_rgba().unwrap(), *game.as_raw());
        eprintln!(
            "{format:?}: ui128/game={:?}; ui128/letterbox={:?}",
            window.get_pixel(4, 1).0,
            window.get_pixel(4, 0).0
        );
        results.push((format, window));
    }
    for (format, window) in results {
        for y in 0..4 {
            for x in 0..8 {
                let background = if (1..3).contains(&y) {
                    [64_u16, 128, 192]
                } else {
                    [0, 0, 0]
                };
                let source = layer.get_pixel(x, y);
                let actual = window.get_pixel(x, y);
                for channel in 0..3 {
                    let expected = u16::from(source[channel])
                        + (background[channel] * u16::from(255 - source[3]) + 127) / 255;
                    assert!(
                        (u16::from(actual[channel])).abs_diff(expected) <= 1,
                        "{format:?} changed premultiplied UI at {x},{y}, channel {channel}: \
                         actual={actual:?}, expected channel={expected}"
                    );
                }
                assert_eq!(actual[3], 255);
            }
        }
    }
    let source = "<capture:window-color-excluded>";
    let frame = PreparedFrame {
        resolution: size,
        operations: vec![PreparedOperation::Capture(source.to_owned())],
        ..Default::default()
    };
    renderer.render_before_clear(&catalog(), &frame).unwrap();
    assert_eq!(
        read_texture(&renderer, &renderer.textures[source].texture),
        game
    );
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

#[test]
fn replacing_window_presentation_preserves_game_capture_and_uploaded_overlay_pixels() {
    let size = GameResolution {
        width: 4,
        height: 4,
    };
    let mut renderer = GpuRenderer::headless(size).unwrap();
    let assets = catalog();
    renderer
        .render_offscreen(
            &assets,
            &PreparedFrame {
                resolution: size,
                ..Default::default()
            },
            [0, 0, 255],
        )
        .unwrap();
    let game_before = renderer.read_game_rgba().unwrap();
    let game_allocation = renderer.game_texture.clone();
    let capture = "<capture:surface-recovery>";
    renderer
        .render_before_clear(
            &assets,
            &PreparedFrame {
                resolution: size,
                operations: vec![PreparedOperation::Capture(capture.to_owned())],
                ..Default::default()
            },
        )
        .unwrap();
    let capture_allocation = renderer.textures[capture].texture.clone();
    let capture_before = read_texture(&renderer, &capture_allocation);
    renderer
        .configure_window_target_for_test(&[wgpu::TextureFormat::Rgba8Unorm], 4, 8)
        .unwrap();
    let mut layer = RgbaImage::from_pixel(4, 8, Rgba([128, 0, 0, 128]));
    layer.put_pixel(0, 0, Rgba([0, 255, 0, 255]));
    renderer.set_window_overlay(Some(&layer)).unwrap();
    let overlay_allocation = renderer.window_overlay.as_ref().unwrap().texture.clone();
    let expected = present(&renderer, 4, 8);
    assert_eq!(*expected.get_pixel(0, 0), Rgba([0, 255, 0, 255]));
    assert_eq!(*expected.get_pixel(2, 0), Rgba([128, 0, 0, 255]));
    assert_eq!(*expected.get_pixel(2, 3), Rgba([128, 0, 127, 255]));
    for format in [
        wgpu::TextureFormat::Bgra8UnormSrgb,
        wgpu::TextureFormat::Bgra8Unorm,
        wgpu::TextureFormat::Rgba8UnormSrgb,
        wgpu::TextureFormat::Rgba8Unorm,
    ] {
        renderer
            .configure_window_target_for_test(&[format], 4, 8)
            .unwrap();
        assert_eq!(renderer.game_texture, game_allocation);
        assert_eq!(renderer.textures[capture].texture, capture_allocation);
        assert_eq!(
            renderer.window_overlay.as_ref().unwrap().texture,
            overlay_allocation
        );
        assert_eq!(present(&renderer, 4, 8), expected, "replacement {format:?}");
        assert_eq!(renderer.read_game_rgba().unwrap(), game_before);
        assert_eq!(read_texture(&renderer, &capture_allocation), capture_before);
    }
}
