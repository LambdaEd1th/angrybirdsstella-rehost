//! Native screenshot readback timing, row origin and retained drawable alpha.

use super::*;
use image::ImageEncoder;

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
        file_images: Default::default(),
    }
}

#[test]
fn screenshot_shares_capture_each_call_before_later_draws_with_original_alpha() {
    let size = GameResolution::new(7, 3).unwrap(); // Non-aligned readback rows.
    let unique = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let root = std::env::temp_dir().join(format!("stella-share-gpu-{unique}/data"));
    std::fs::create_dir_all(&root).unwrap();
    let runtime = StellaLua::new_with_resolution(&root, size.width, size.height).unwrap();
    runtime
        .execute_source(
            r#"
        drawRect(1, 0, 0, 0.25, 0, 0, 7, 1, true)
        drawRect(0, 1, 0, 0, 0, 1, 7, 3, true)
        res.setClipRect(1, 1, 1, 1)
        native_shareScreenShot("first 🐦")
        res.setClipRect(0, 0, 7, 3)
        drawRect(0, 0, 1, 0.5, 0, 0, 7, 3, true)
        native_shareScreenShot("second")
        drawRect(1, 1, 1, 1, 0, 0, 7, 3, true)
    "#,
        )
        .unwrap();
    let requests = runtime.take_screenshot_share_requests();
    assert_eq!(requests.iter().map(|r| r.order).collect::<Vec<_>>(), [2, 4]);
    assert!(runtime.take_render_commands().is_empty());
    assert!(runtime.take_text_commands().is_empty());
    assert!(runtime.take_capture_commands().is_empty());
    let rectangles = runtime.take_rect_commands();
    let mut assets = catalog();
    let mut renderer = GpuRenderer::headless(size).unwrap();
    // Render each native call's draw prefix without the share implementation,
    // then independently copy the raw attachment. Capture equality is exact,
    // including alpha; it must not inherit another GPU's UNORM blend rounding.
    let references: Vec<_> = requests
        .iter()
        .map(|request| {
            let prefix: Vec<_> = rectangles
                .iter()
                .filter(|rectangle| rectangle.order < request.order)
                .cloned()
                .collect();
            let frame = assets
                .prepare_gpu_frame_at_resolution(size, &[], &[], &prefix, &[])
                .unwrap();
            renderer.render_offscreen(&assets, &frame, [0; 3]).unwrap();
            read_texture(&renderer, &renderer.game_texture).into_raw()
        })
        .collect();
    let frame = assets
        .prepare_gpu_frame_with_shares_at_resolution(size, &[], &[], &rectangles, &[], &requests)
        .unwrap();
    let final_frame = renderer.render_to_rgba(&assets, &frame, [0; 3]).unwrap();
    assert!(
        final_frame
            .as_chunks::<4>()
            .0
            .iter()
            .all(|pixel| *pixel == [255; 4])
    );
    // Resize and draw before consumption. Each readback owns its old extent
    // and samples; neither the new framebuffer nor scissor can alter them.
    let resized = GameResolution::new(14, 6).unwrap();
    renderer.resize_game_target(resized).unwrap();
    let empty = assets
        .prepare_gpu_frame_at_resolution(resized, &[], &[], &[], &[])
        .unwrap();
    renderer.render_offscreen(&assets, &empty, [0; 3]).unwrap();
    let shares = renderer.take_screenshot_shares();
    assert_eq!(shares.len(), 2);
    for (index, share) in shares.iter().enumerate() {
        assert_eq!(share.request, requests[index]);
        assert_eq!(share.resolution, size);
        assert_eq!(
            share.rgba, references[index],
            "share {index} call-prefix RGBA"
        );
        assert_ne!(share.rgba, final_frame);
        let mut png = Vec::new();
        image::codecs::png::PngEncoder::new(&mut png)
            .write_image(
                &share.rgba,
                size.width,
                size.height,
                image::ExtendedColorType::Rgba8,
            )
            .unwrap();
        assert_eq!(
            image::load_from_memory(&png)
                .unwrap()
                .into_rgba8()
                .into_raw(),
            references[index],
            "share {index} PNG must preserve every RGBA sample"
        );
        for (pixel_index, pixel) in share.rgba.as_chunks::<4>().0.iter().enumerate() {
            // drawRect packs alpha with FCVTZS before native glBlendFunc.
            // PlainAlpha uses SRC_ALPHA for alpha as well as RGB.
            let (source, destination): ([u8; 4], [u8; 4]) = if index == 1 {
                let start = pixel_index * 4;
                let destination: [u8; 4] = references[0][start..start + 4].try_into().unwrap();
                ([0, 0, 255, 127], destination)
            } else if pixel_index < 7 {
                ([255, 0, 0, 63], [0, 0, 0, 255])
            } else {
                ([0, 255, 0, 0], [0, 0, 0, 255])
            };
            for channel in 0..4 {
                let alpha = u32::from(source[3]);
                let numerator = u32::from(source[channel]) * alpha
                    + u32::from(destination[channel]) * (255 - alpha);
                // Only the analytic blend permits neighboring UNORM integers.
                // Zero/one factors stay exact; readback and PNG stay byte-exact.
                assert!(
                    (numerator / 255..=numerator.div_ceil(255))
                        .contains(&u32::from(pixel[channel])),
                    "share {index}, pixel {pixel_index}, channel {channel}: {} versus {numerator}/255",
                    pixel[channel]
                );
            }
        }
    }
    assert!(renderer.take_screenshot_shares().is_empty());
    drop(runtime);
    std::fs::remove_dir_all(root.parent().unwrap()).unwrap();
}

#[test]
fn share_readbacks_split_draw_batches_without_registering_sprite_images() {
    let size = GameResolution::new(7, 3).unwrap();
    let mut assets = catalog();
    let share = ScreenshotShareRequest {
        order: 0,
        sequence: -1,
        filename: "Stella_Screenshot-1.png".into(),
        title: String::new(),
    };
    let frame = assets
        .prepare_gpu_frame_with_shares_at_resolution(
            size,
            &[],
            &[],
            &[],
            &[],
            std::slice::from_ref(&share),
        )
        .unwrap();
    assert_eq!(
        frame.operations,
        [PreparedOperation::ScreenshotShare(share.clone())]
    );
    assert!(assets.captures.bindings.is_empty());
    let mut renderer = GpuRenderer::headless(size).unwrap();
    renderer
        .render_offscreen(&assets, &frame, [17, 33, 65])
        .unwrap();
    let capture = renderer.take_screenshot_shares().pop().unwrap();
    assert_eq!(capture.request, share);
    assert!(
        capture
            .rgba
            .as_chunks::<4>()
            .0
            .iter()
            .all(|pixel| *pixel == [17, 33, 65, 255])
    );
    assert!(
        renderer
            .textures
            .keys()
            .all(|name| !name.starts_with("<capture-generation:"))
    );
}
