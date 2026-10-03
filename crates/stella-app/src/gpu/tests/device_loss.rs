use super::*;

#[test]
fn device_loss_rejects_render_readback_overlay_and_presentation() {
    let size = GameResolution::new(8, 4).unwrap();
    let assets = AssetCatalog {
        root: PathBuf::new(),
        font_root: PathBuf::new(),
        regions: HashMap::new(),
        composites: HashMap::new(),
        masked_textures: HashMap::new(),
        fonts: HashMap::new(),
        textures: HashMap::new(),
        system_labels: SystemLabelPool::default(),
        captures: CapturedTextureCatalog::default(),
        file_images: Default::default(),
    };
    let frame = PreparedFrame {
        resolution: size,
        ..Default::default()
    };
    let mut renderer = GpuRenderer::headless(size).unwrap();
    renderer
        .render_offscreen(&assets, &frame, [1, 2, 3])
        .unwrap();
    assert_eq!(&renderer.read_game_rgba().unwrap()[..4], &[1, 2, 3, 255]);
    renderer.destroy_device_for_test();

    let expected = "wgpu device lost (Destroyed)";
    assert_eq!(
        renderer
            .render_offscreen(&assets, &frame, [4, 5, 6])
            .unwrap_err()
            .to_string(),
        expected
    );
    assert_eq!(renderer.read_game_rgba().unwrap_err().to_string(), expected);
    assert_eq!(
        renderer
            .set_window_overlay(Some(&RgbaImage::new(8, 4)))
            .unwrap_err()
            .to_string(),
        expected
    );
    // Even a minimized window must surface terminal device loss.
    assert_eq!(
        renderer.present_to_window(0, 0).unwrap_err().to_string(),
        expected
    );
    assert_eq!(
        renderer.resize_surface(0, 0).unwrap_err().to_string(),
        expected
    );
    assert_eq!(
        renderer.resize_game_target(size).unwrap_err().to_string(),
        expected
    );
    assert_eq!(
        renderer
            .resize_game_target(GameResolution::new(16, 8).unwrap())
            .unwrap_err()
            .to_string(),
        expected
    );
    assert_eq!(renderer.resolution, size);
}
