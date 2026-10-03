use super::*;

fn catalog(root: PathBuf) -> AssetCatalog {
    AssetCatalog {
        root,
        font_root: Default::default(),
        regions: HashMap::new(),
        composites: HashMap::new(),
        masked_textures: HashMap::new(),
        fonts: HashMap::new(),
        textures: HashMap::new(),
        system_labels: SystemLabelPool::default(),
        captures: Default::default(),
        file_images: Default::default(),
    }
}

fn command(image: stella_assets::native_image::DecodedNativeImage, identity: u64) -> RenderCommand {
    RenderCommand {
        projection_3d: None,
        order: 0,
        sprite: "NATIVE_IMAGE".into(),
        texture: None,
        bound_region: Some(Arc::new(SpriteCatalogRegion {
            native_sheet_id: identity,
            texture_source: stella_assets::image_source::sheet_image_source(
                identity,
                0,
                "/missing/native-image",
            ),
            sprite: SpriteRegion {
                name: "NATIVE_IMAGE".to_owned(),
                x: 0,
                y: 0,
                width: image.width as i16,
                height: image.height as i16,
                pivot_x: 0,
                pivot_y: 0,
                atlas_rotation: 0,
            },
            decoded_image: Some(Arc::new(image)),
        })),
        bound_composite: None,
        geometry: None,
        shader: None,
        dirt: None,
        x: 0.0,
        y: 0.0,
        state: stella_script::RenderState::default().into(),
        world_space: true,
    }
}

#[test]
fn png_native_expanded_alpha_reaches_file_and_retained_gpu_pixels() {
    let root = std::env::temp_dir().join(format!(
        "stella-native-png-pixels-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir(&root).unwrap();
    let green = [0, 255, 0, 255];
    let white = [255; 4];
    let gray_expected = [
        green, white, green, white, white, white, green, white, green, green, green, green, white,
        white, green,
    ]
    .concat();
    let a = [203, 31, 7, 255];
    let b = [29, 17, 83, 255];
    let c = [61, 211, 19, 255];
    let mut identity = 0;
    for (name, bytes, size, program, expected) in [
        (
            "gray1.png",
            include_bytes!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/../stella-assets/src/native_image/png/fixtures/gray1-trns.png"
            ))
            .as_slice(),
            GameResolution {
                width: 5,
                height: 3,
            },
            NativeProgram::SpriteAlpha,
            gray_expected.clone(),
        ),
        (
            "gray1-adam7.png",
            include_bytes!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/../stella-assets/src/native_image/png/fixtures/gray1-trns-adam7.png"
            ))
            .as_slice(),
            GameResolution {
                width: 5,
                height: 3,
            },
            NativeProgram::SpriteAlpha,
            gray_expected,
        ),
        (
            "palette4.png",
            include_bytes!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/../stella-assets/src/native_image/png/fixtures/palette4-opaque.png"
            ))
            .as_slice(),
            GameResolution {
                width: 3,
                height: 2,
            },
            NativeProgram::Sprite,
            [c, b, a, a, c, b].concat(),
        ),
    ] {
        let mut renderer = GpuRenderer::headless(size).unwrap();
        for file_backed in [false, true] {
            identity += 1;
            let decoded =
                stella_assets::native_image::decode_native_texture(bytes, Some("png")).unwrap();
            let mut draw = command(decoded, identity);
            let mut assets = if file_backed {
                std::fs::write(root.join(name), bytes).unwrap();
                let region = Arc::make_mut(draw.bound_region.as_mut().unwrap());
                region.decoded_image = None;
                region.texture_source =
                    stella_assets::image_source::sheet_image_source(identity, 0, name);
                catalog(root.clone())
            } else {
                catalog(Default::default())
            };
            let frame = assets
                .prepare_gpu_frame_at_resolution(size, &[draw], &[], &[], &[])
                .unwrap();
            assert_eq!(
                frame.draws[0].program, program,
                "{name}, file={file_backed}"
            );
            let actual = renderer
                .render_to_rgba(&assets, &frame, [0, 255, 0])
                .unwrap();
            assert_eq!(actual, expected, "{name}, file={file_backed}");
        }
    }
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn bmp_native_os2_p4_stride_and_palette_reach_actual_gpu_pixels() {
    let mut bytes = vec![0; 90];
    bytes[..2].copy_from_slice(b"BM");
    bytes[10..14].copy_from_slice(&90u32.to_le_bytes());
    bytes[14..18].copy_from_slice(&64u32.to_le_bytes());
    bytes[18..22].copy_from_slice(&0x10009u32.to_le_bytes());
    bytes[22..26].copy_from_slice(&0x10002u32.to_le_bytes());
    bytes[26..28].copy_from_slice(&1u16.to_le_bytes());
    bytes[28..30].copy_from_slice(&4u16.to_le_bytes());
    bytes[46..50].copy_from_slice(&3u32.to_le_bytes());
    bytes[78..90].copy_from_slice(&[7, 31, 203, 0, 83, 17, 29, 173, 19, 211, 61, 0]);
    bytes.extend_from_slice(&[
        0x01, 0x20, 0x12, 0x01, 0x21, 0x02, 0x10, 0x20, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff,
        0xff,
    ]);
    let decoded = stella_assets::native_image::decode_native_texture(&bytes, Some("png")).unwrap();
    assert_eq!((decoded.width, decoded.height), (9, 2));
    assert_eq!(decoded.layout.pixels, SurfaceFormat::P4);
    let size = GameResolution {
        width: 9,
        height: 2,
    };
    let mut assets = catalog(Default::default());
    let frame = assets
        .prepare_gpu_frame_at_resolution(size, &[command(decoded, 1)], &[], &[], &[])
        .unwrap();
    assert_eq!(frame.draws[0].program, NativeProgram::SpriteAlpha);
    let mut renderer = GpuRenderer::headless(size).unwrap();
    let actual = renderer
        .render_to_rgba(&assets, &frame, [0, 255, 0])
        .unwrap();
    let a = [203, 31, 7, 255];
    let b = [29, 17, 83, 255];
    let c = [61, 211, 19, 255];
    assert_eq!(
        actual,
        [c, b, a, c, b, a, c, a, a, a, b, c, a, b, c, a, b, a].concat()
    );
}

#[test]
fn truecolor_tga_native_scanlines_and_alpha_reach_actual_gpu_pixels() {
    let size = GameResolution {
        width: 2,
        height: 2,
    };
    let mut renderer = GpuRenderer::headless(size).unwrap();
    let mut identity = 0;
    for depth in [24, 32] {
        for rle in [false, true] {
            for descriptor in [0x10, 0x30] {
                let mut bytes = vec![0; 18];
                bytes[2] = if rle { 10 } else { 2 };
                bytes[12..14].copy_from_slice(&2u16.to_le_bytes());
                bytes[14..16].copy_from_slice(&2u16.to_le_bytes());
                bytes[16] = depth;
                bytes[17] = descriptor; // Horizontal bit ignored, zero attribute bits.
                let pixel_size = usize::from(depth / 8);
                if rle {
                    bytes.push(2);
                }
                bytes.extend_from_slice(&[0, 0, 255, 255][..pixel_size]);
                bytes.extend_from_slice(&[255, 0, 0, 128][..pixel_size]);
                if rle {
                    bytes.extend_from_slice(&[255; 4][..pixel_size]); // Discarded overflow.
                    bytes.push(1);
                }
                bytes.extend_from_slice(&[255, 0, 0, 255][..pixel_size]);
                bytes.extend_from_slice(&[0, 0, 255, 0][..pixel_size]);
                let decoded =
                    stella_assets::native_image::decode_native_texture(&bytes, Some("tga"))
                        .unwrap();
                identity += 1;
                let mut assets = catalog(Default::default());
                let frame = assets
                    .prepare_gpu_frame_at_resolution(
                        size,
                        &[command(decoded, identity)],
                        &[],
                        &[],
                        &[],
                    )
                    .unwrap();
                assert_eq!(
                    frame.draws[0].program,
                    if depth == 32 {
                        NativeProgram::SpriteAlpha
                    } else {
                        NativeProgram::Sprite
                    }
                );
                let pixels = renderer
                    .render_to_rgba(&assets, &frame, [0, 255, 0])
                    .unwrap();
                assert_eq!(pixels.len(), 16);
                // Purple's pp.ps leaves stored RGB intact and pp.fx uses ONE,
                // ONE_MINUS_SRC_ALPHA, including straight-alpha TGA pixels.
                let first_row = [
                    [255, 0, 0, 255],
                    if depth == 32 {
                        [0, 127, 255, 255]
                    } else {
                        [0, 0, 255, 255]
                    },
                ];
                let second_row = [
                    [0, 0, 255, 255],
                    if depth == 32 {
                        [255, 255, 0, 255]
                    } else {
                        [255, 0, 0, 255]
                    },
                ];
                let expected = if descriptor & 0x20 == 0 {
                    [second_row, first_row]
                } else {
                    [first_row, second_row]
                };
                for (actual, expected) in pixels
                    .as_chunks::<4>()
                    .0
                    .iter()
                    .zip(expected.into_iter().flatten())
                {
                    for (&actual, expected) in actual.iter().zip(expected) {
                        assert!(
                            actual.abs_diff(expected) <= 1,
                            "depth={depth}, rle={rle}, descriptor={descriptor:#x}: {pixels:?}"
                        );
                    }
                }
            }
        }
    }
}

#[test]
fn readable_unsupported_images_fail_file_and_retained_texture_publication() {
    let root = std::env::temp_dir().join(format!(
        "stella-native-texture-rejection-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&root).unwrap();
    let mut tga = vec![0; 18];
    tga[2] = 2;
    tga[12..14].copy_from_slice(&1u16.to_le_bytes());
    tga[14..16].copy_from_slice(&1u16.to_le_bytes());
    tga[16] = 16;
    tga[17] = 0x20;
    tga.extend_from_slice(&[0, 0x7c]);
    let mut bmp = vec![0; 58];
    bmp[..2].copy_from_slice(b"BM");
    bmp[2..6].copy_from_slice(&58u32.to_le_bytes());
    bmp[10..14].copy_from_slice(&54u32.to_le_bytes());
    bmp[14..18].copy_from_slice(&40u32.to_le_bytes());
    bmp[18..22].copy_from_slice(&1u32.to_le_bytes());
    bmp[22..26].copy_from_slice(&1u32.to_le_bytes());
    bmp[26..28].copy_from_slice(&1u16.to_le_bytes());
    bmp[28..30].copy_from_slice(&32u16.to_le_bytes());
    bmp[54..58].copy_from_slice(&[7, 31, 203, 0]);
    for (name, bytes, format) in [
        ("rejected.tga", tga, "R5G5B5"),
        ("rejected.bmp", bmp, "X8R8G8B8"),
    ] {
        std::fs::write(root.join(name), &bytes).unwrap();
        let extension = PathBuf::from(name);
        let decoded = stella_assets::native_image::decode_native_image(
            &bytes,
            extension.extension().and_then(|value| value.to_str()),
        )
        .unwrap();
        let mut assets = catalog(root.clone());
        let error = assets.texture(name).unwrap_err();
        assert!(format!("{error:#}").contains(&format!("Unsupported texture format: {format}")));
        assert!(assets.textures.is_empty());
        let error = assets
            .prepare_gpu_frame(&[command(decoded, 1)], &[], &[], &[])
            .err()
            .expect("unsupported native texture must fail frame preparation");
        assert!(format!("{error:#}").contains(&format!("Unsupported texture format: {format}")));
        assert!(assets.textures.is_empty());
    }
    std::fs::remove_dir_all(root).unwrap();
}
