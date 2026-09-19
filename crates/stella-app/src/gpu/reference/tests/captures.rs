use super::*;

const SHEET: &str = "<reference-capture-sheet>";

fn capture_assets() -> AssetCatalog {
    let (_, mut assets) = fixture(Vec::new(), NativeProgram::Sprite);
    assets.textures.clear();
    assets.textures.insert(
        SHEET.to_owned(),
        TextureAsset::new(
            RgbaImage::from_pixel(32, 32, image::Rgba([0, 255, 0, 255])),
            SurfaceFormat::A8B8G8R8,
        ),
    );
    for (name, size) in [("FULL", 32), ("PATCH", 16)] {
        assets.regions.insert(
            name.to_owned(),
            AtlasRegion {
                texture: SHEET.to_owned(),
                sprite: SpriteRegion {
                    name: name.to_owned(),
                    x: 0,
                    y: 0,
                    width: size,
                    height: size,
                    pivot_x: 0,
                    pivot_y: 0,
                    atlas_rotation: 0,
                },
            },
        );
    }
    assets
}

fn sprite(order: u64, name: &str) -> RenderCommand {
    RenderCommand {
        projection_3d: None,
        order,
        sprite: name.into(),
        texture: None,
        bound_region: None,
        bound_composite: None,
        geometry: None,
        shader: None,
        dirt: None,
        x: 0.0,
        y: 0.0,
        state: stella_script::RenderState::default().into(),
        world_space: false,
    }
}

fn rect(order: u64, bounds: [f64; 4], rgba: [f64; 4]) -> RectRenderCommand {
    RectRenderCommand {
        projection_3d: None,
        order,
        red: rgba[0],
        green: rgba[1],
        blue: rgba[2],
        alpha: rgba[3],
        left: bounds[0],
        top: bounds[1],
        right: bounds[2],
        bottom: bounds[3],
        color_program: ColorProgram::Plain,
        vertices: None,
        mesh_topology: ColorMeshTopology::TriangleFan,
        clip_rect: None,
    }
}

fn capture(order: u64) -> CaptureRenderCommand {
    CaptureRenderCommand {
        order,
        name: "FULL".to_owned(),
        texture_source: SHEET.to_owned(),
        temporary: false,
    }
}

fn capture_pattern(assets: &mut AssetCatalog) -> PreparedFrame {
    assets
        .prepare_gpu_frame_at_resolution(
            GameResolution {
                width: 32,
                height: 32,
            },
            &[sprite(0, "PATCH"), sprite(5, "FULL")],
            &[],
            &[
                rect(1, [16.0, 0.0, 32.0, 16.0], [255.0, 0.0, 0.0, 0.25]),
                rect(2, [0.0, 16.0, 32.0, 32.0], [0.0, 0.0, 255.0, 0.75]),
                rect(4, [0.0, 0.0, 32.0, 32.0], [0.0, 0.0, 0.0, 1.0]),
            ],
            &[capture(3)],
        )
        .unwrap()
}

#[test]
fn reference_capture_flips_rows_discards_alpha_and_keeps_original_atlas_identity() {
    let mut assets = capture_assets();
    let frame = capture_pattern(&mut assets);
    let physical = assets.captures.bindings[SHEET].source.clone();
    assert!(frame.required_textures.contains(SHEET));
    assert!(frame.required_textures.contains(&physical));
    let mut pixels = vec![0; 32 * 32];
    frame.render_reference(&mut assets, &mut pixels).unwrap();
    // The unmodified original sheet is required before the capture. Resolving
    // it again through the final logical binding would load a future image.
    assert_eq!(
        assets.textures[SHEET].image.get_pixel(31, 31).0,
        [0, 255, 0, 255]
    );
    let saved = &assets.textures[&physical];
    assert_eq!(saved.upload_surface_format(), SurfaceFormat::A8B8G8R8);
    assert_eq!(saved.image.get_pixel(0, 0).0, [0, 0, 255, 255]);
    assert_eq!(saved.image.get_pixel(0, 31).0, [0, 255, 0, 255]);
    assert_eq!(saved.image.get_pixel(31, 31).0, [255, 0, 0, 255]);
    assert_eq!(pixels[0], 0x0000ff);
    assert_eq!(pixels[31 * 32], 0x00ff00);
    assert_eq!(pixels[32 * 32 - 1], 0xff0000);
    assert_eq!(assets.regions.len(), 2);
    assert_eq!(assets.regions["PATCH"].sprite.width, 16);
    assert_eq!(assets.regions["PATCH"].texture, SHEET);
    compare_gpu(&frame, &assets, &pixels);
}

#[test]
fn reference_capture_survives_next_frame_and_resize_without_resizing_its_image() {
    let mut assets = capture_assets();
    let frame = capture_pattern(&mut assets);
    frame
        .render_reference(&mut assets, &mut [0; 32 * 32])
        .unwrap();
    let physical = assets.captures.bindings[SHEET].source.clone();
    let larger = GameResolution {
        width: 64,
        height: 48,
    };
    let next = assets
        .prepare_gpu_frame_at_resolution(larger, &[sprite(0, "FULL")], &[], &[], &[])
        .unwrap();
    assert_eq!(next.required_textures, HashSet::from([physical.clone()]));
    let mut pixels = vec![0; 64 * 48];
    next.render_reference(&mut assets, &mut pixels).unwrap();
    assert_eq!(pixels[0], 0x0000ff);
    assert_eq!(pixels[31 * 64], 0x00ff00);
    assert_eq!(pixels[31 * 64 + 31], 0xff0000);
    assert_eq!(pixels[32], 0);
    assert_eq!(pixels[32 * 64], 0);
    assert_eq!(assets.textures[&physical].image.dimensions(), (32, 32));
    compare_gpu(&next, &assets, &pixels);
    let wrong_size = assets.prepare_gpu_frame_at_resolution(larger, &[], &[], &[], &[capture(0)]);
    assert!(
        wrong_size
            .err()
            .unwrap()
            .to_string()
            .contains("Wrong size capture target image")
    );
    assert_eq!(assets.captures.bindings[SHEET].source, physical);
}

#[test]
fn reference_capture_generations_preserve_earlier_draws_then_retire_unused_images() {
    let mut assets = capture_assets();
    let first = capture_pattern(&mut assets);
    first
        .render_reference(&mut assets, &mut [0; 32 * 32])
        .unwrap();
    let first_name = assets.captures.bindings[SHEET].source.clone();
    let frame = assets
        .prepare_gpu_frame_at_resolution(
            GameResolution {
                width: 32,
                height: 32,
            },
            &[sprite(0, "FULL"), sprite(6, "FULL")],
            &[],
            &[
                rect(1, [0.0, 0.0, 32.0, 32.0], [255.0, 255.0, 0.0, 0.25]),
                rect(3, [0.0, 0.0, 32.0, 32.0], [255.0, 0.0, 255.0, 0.5]),
                rect(5, [0.0, 0.0, 32.0, 32.0], [0.0, 0.0, 0.0, 1.0]),
            ],
            &[capture(2), capture(4)],
        )
        .unwrap();
    let current_name = assets.captures.bindings[SHEET].source.clone();
    assert_ne!(current_name, first_name);
    assert_eq!(frame.capture_formats.len(), 2);
    assert_eq!(frame.retired_textures.len(), 2);
    let mut pixels = vec![0; 32 * 32];
    frame.render_reference(&mut assets, &mut pixels).unwrap();
    assert!(pixels.iter().all(|pixel| *pixel == 0xff00ff));
    assert!(assets.textures.contains_key(&first_name));
    assert!(assets.textures.contains_key(&current_name));
    for name in frame
        .capture_formats
        .keys()
        .filter(|name| **name != current_name)
    {
        assert!(!assets.textures.contains_key(name));
    }
    compare_gpu(&frame, &assets, &pixels);
    let next = assets
        .prepare_gpu_frame_at_resolution(
            GameResolution {
                width: 32,
                height: 32,
            },
            &[sprite(0, "FULL")],
            &[],
            &[],
            &[],
        )
        .unwrap();
    next.render_reference(&mut assets, &mut pixels).unwrap();
    assert!(!assets.textures.contains_key(&first_name));
    assert!(assets.textures.contains_key(&current_name));
    assert!(pixels.iter().all(|pixel| *pixel == 0xff00ff));
}

#[test]
fn reference_capture_temporary_image_does_not_restore_a_released_sheet() {
    let mut assets = capture_assets();
    let frame = assets
        .prepare_gpu_frame_at_resolution(
            GameResolution {
                width: 32,
                height: 32,
            },
            &[],
            &[],
            &[],
            &[CaptureRenderCommand {
                order: 0,
                name: "RELEASED".to_owned(),
                texture_source: "<capture:released-image>".to_owned(),
                temporary: true,
            }],
        )
        .unwrap();
    let mut pixels = vec![0x123456; 32 * 32];
    frame.render_reference(&mut assets, &mut pixels).unwrap();
    assert!(pixels.iter().all(|pixel| *pixel == 0x123456));
    assert!(assets.captures.bindings.is_empty());
    assert!(!assets.regions.contains_key("RELEASED"));
    assert_eq!(frame.capture_formats.len(), 1);
    for physical in frame.capture_formats.keys() {
        assert!(frame.retired_textures.contains(physical));
        assert!(!assets.textures.contains_key(physical));
    }
}

#[test]
fn reference_capture_of_a_captured_draw_flips_each_generation_exactly_once() {
    let mut assets = capture_assets();
    let first = capture_pattern(&mut assets);
    first
        .render_reference(&mut assets, &mut [0; 32 * 32])
        .unwrap();
    let second = assets
        .prepare_gpu_frame_at_resolution(
            GameResolution {
                width: 32,
                height: 32,
            },
            &[sprite(0, "FULL"), sprite(3, "FULL")],
            &[],
            &[rect(2, [0.0, 0.0, 32.0, 32.0], [0.0, 0.0, 0.0, 1.0])],
            &[capture(1)],
        )
        .unwrap();
    let mut pixels = vec![0; 32 * 32];
    second.render_reference(&mut assets, &mut pixels).unwrap();
    assert_eq!(pixels[0], 0x00ff00);
    assert_eq!(pixels[31], 0xff0000);
    assert_eq!(pixels[31 * 32], 0x0000ff);
    assert_eq!(pixels[32 * 32 - 1], 0x0000ff);
    compare_gpu(&second, &assets, &pixels);
}
