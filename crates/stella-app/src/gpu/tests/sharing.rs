//! Native screenshot readback timing, row origin and retained drawable alpha.

use super::*;

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
    let mut assets = catalog();
    let frame = assets
        .prepare_gpu_frame_with_shares_at_resolution(
            size,
            &runtime.take_render_commands(),
            &runtime.take_text_commands(),
            &runtime.take_rect_commands(),
            &runtime.take_capture_commands(),
            &requests,
        )
        .unwrap();
    let mut renderer = GpuRenderer::headless(size).unwrap();
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
        for (pixel_index, pixel) in share.rgba.as_chunks::<4>().0.iter().enumerate() {
            // drawRect packs alpha with FCVTZS before native glBlendFunc.
            // PlainAlpha uses SRC_ALPHA for alpha as well as RGB.
            let expected = if index == 1 && pixel_index < 7 {
                [32, 0, 127, 168]
            } else if index == 1 {
                [0, 0, 127, 191]
            } else if pixel_index < 7 {
                [63, 0, 0, 208]
            } else {
                [0, 0, 0, 255]
            };
            assert_eq!(*pixel, expected, "share {index}, pixel {pixel_index}");
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
