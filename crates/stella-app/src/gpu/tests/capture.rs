//! GL_RGB capture content, native Image identity, and deferred GPU lifetimes.

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

fn register_capture(assets: &mut AssetCatalog, source: &str, size: GameResolution) {
    assets.regions.insert(
        "CAP".to_owned(),
        AtlasRegion {
            texture: source.to_owned(),
            sprite: SpriteRegion {
                name: "CAP".to_owned(),
                x: 0,
                y: 0,
                width: size.width as i16,
                height: size.height as i16,
                pivot_x: 0,
                pivot_y: 0,
                atlas_rotation: 3,
            },
        },
    );
}

fn capture(order: u64, source: &str) -> CaptureRenderCommand {
    CaptureRenderCommand {
        order,
        name: "CAP".to_owned(),
        texture_source: source.to_owned(),
        temporary: false,
    }
}

fn draw(order: u64, name: &str, width: f32, height: f32) -> RenderCommand {
    RenderCommand {
        order,
        sprite: name.into(),
        x: 0.0,
        y: 0.0,
        world_space: true,
        projection_3d: None,
        texture: None,
        bound_region: None,
        bound_composite: None,
        geometry: None,
        shader: None,
        dirt: None,
        state: stella_script::RenderState {
            draw_size: Some([f64::from(width), f64::from(height)]),
            ..Default::default()
        }
        .into(),
    }
}

fn rect(order: u64, rgb: [f64; 3], alpha: f64, bounds: [f64; 4]) -> RectRenderCommand {
    RectRenderCommand {
        order,
        red: rgb[0],
        green: rgb[1],
        blue: rgb[2],
        alpha,
        left: bounds[0],
        top: bounds[1],
        right: bounds[2],
        bottom: bounds[3],
        color_program: ColorProgram::Plain,
        vertices: None,
        mesh_topology: ColorMeshTopology::TriangleFan,
        clip_rect: None,
        projection_3d: None,
    }
}

fn pixel(rgba: &[u8], size: GameResolution, x: u32, y: u32) -> [u8; 4] {
    let offset = ((y * size.width + x) * 4) as usize;
    rgba[offset..offset + 4].try_into().unwrap()
}

#[test]
fn lua_capture_registers_geometry_and_draws_the_upright_framebuffer() {
    let size = GameResolution {
        width: 16,
        height: 8,
    };
    let runtime = StellaLua::new_with_resolution("/tmp", size.width, size.height).unwrap();
    runtime
        .execute_source(
            r#"
        drawRect(1, 0, 0, 1, 0, 0, 16, 4, true)
        drawRect(0, 0, 1, 1, 0, 4, 16, 8, true)
        res.setClipRect(4, 2, 3, 2)
        res.captureSprite("CAP")
        clearScreen()
        res.drawSprite("CAP", 0, 0)
    "#,
        )
        .unwrap();
    let mut assets = catalog();
    assets
        .apply_sprite_catalog_snapshot(runtime.sprite_catalog_snapshot_since(0).unwrap())
        .unwrap();
    let region_before = assets.regions["CAP"].sprite.clone();
    let captures = runtime.take_capture_commands();
    let frame = assets
        .prepare_gpu_frame_at_resolution(
            size,
            &runtime.take_render_commands(),
            &runtime.take_text_commands(),
            &runtime.take_rect_commands(),
            &captures,
        )
        .unwrap();
    assert_eq!(assets.regions["CAP"].sprite, region_before);
    assert_eq!(region_before.atlas_rotation, 3);
    let binding = &assets.captures.bindings[&captures[0].texture_source];
    assert_eq!(binding.surface_format, SurfaceFormat::B8G8R8);
    let mut renderer = GpuRenderer::headless(size).unwrap();
    let rgba = renderer
        .render_to_rgba(&assets, &frame, [0, 255, 0])
        .unwrap();
    assert_eq!(pixel(&rgba, size, 1, 1), [255, 0, 0, 255]);
    assert_eq!(pixel(&rgba, size, 14, 6), [0, 0, 255, 255]);
}

#[test]
fn lua_capture_keeps_same_file_images_and_reloaded_sheet_owners_independent() {
    // Native each SPRT load constructs a distinct Image/GL texture even when
    // the path matches. Use actual PNG I/O here, not pre-keyed synthetic images.
    fn sheet(name: &str) -> Vec<u8> {
        fn string(bytes: &mut Vec<u8>, text: &str) {
            bytes.extend_from_slice(&(text.len() as u16).to_be_bytes());
            bytes.extend_from_slice(text.as_bytes());
        }
        let mut payload = 1_u16.to_be_bytes().to_vec();
        string(&mut payload, "same.png");
        payload.extend_from_slice(&1_u16.to_be_bytes());
        string(&mut payload, name);
        for value in [0_u16, 0, 8, 8, 0, 0] {
            payload.extend_from_slice(&value.to_be_bytes());
        }
        let mut bytes = b"KA3D".to_vec();
        bytes.extend_from_slice(&(payload.len() as u32 + 8).to_be_bytes());
        bytes.extend_from_slice(b"SPRT");
        bytes.extend_from_slice(&(payload.len() as u32).to_be_bytes());
        bytes.extend_from_slice(&payload);
        bytes
    }
    let unique = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let root = std::env::temp_dir().join(format!(
        "stella-capture-image-{}-{unique}",
        std::process::id()
    ));
    std::fs::create_dir(&root).unwrap();
    std::fs::write(root.join("A.dat"), sheet("A_SPRITE")).unwrap();
    std::fs::write(root.join("B.dat"), sheet("B_SPRITE")).unwrap();
    RgbaImage::from_pixel(24, 8, image::Rgba([0, 255, 0, 255]))
        .save(root.join("same.png"))
        .unwrap();
    let size = GameResolution {
        width: 24,
        height: 8,
    };
    let runtime = StellaLua::new_with_resolution(&root, size.width, size.height).unwrap();
    runtime
        .execute_source(
            r#"
        res.createSpriteSheet("A.dat")
        res.createSpriteSheet("B.dat")
        drawRect(1, 0, 0, 1, 0, 0, 24, 8, true)
        res.captureSprite("A")
        clearScreen()
        res.drawSprite("A_SPRITE", 0, 0)
        res.createSpriteSheet("A.dat", true)
        res.drawSprite("A_SPRITE", 8, 0)
        res.drawSprite("B_SPRITE", 16, 0)
    "#,
        )
        .unwrap();
    let captures = runtime.take_capture_commands();
    let draws = runtime.take_render_commands();
    assert_eq!(captures.len(), 1);
    assert_eq!(draws.len(), 3);
    let sources = draws
        .iter()
        .map(|draw| &draw.bound_region.as_ref().unwrap().texture_source)
        .collect::<Vec<_>>();
    assert_eq!(sources[0], &captures[0].texture_source);
    assert_ne!(sources[0], sources[1]);
    assert_ne!(sources[0], sources[2]);
    assert_ne!(sources[1], sources[2]);
    let image_path = std::fs::canonicalize(root.join("same.png")).unwrap();
    for source in &sources {
        assert_eq!(
            std::path::Path::new(stella_assets::image_source::image_source_path(source)),
            image_path
        );
    }
    let mut assets = catalog();
    assets.root = root.clone();
    assets.font_root = root.clone();
    assets
        .apply_sprite_catalog_snapshot(runtime.sprite_catalog_snapshot_since(0).unwrap())
        .unwrap();
    assert_eq!(&assets.regions["A_SPRITE"].texture, sources[1]);
    let frame = assets
        .prepare_gpu_frame_at_resolution(
            size,
            &draws,
            &[],
            &runtime.take_rect_commands(),
            &captures,
        )
        .unwrap();
    assert_eq!(
        assets.textures.len(),
        1,
        "unmodified owners share immutable PNG bytes"
    );
    assert!(assets.textures.contains_key(image_path.to_str().unwrap()));
    let mut renderer = GpuRenderer::headless(size).unwrap();
    let rgba = renderer
        .render_to_rgba(&assets, &frame, [0, 0, 255])
        .unwrap();
    assert_eq!(pixel(&rgba, size, 4, 4), [255, 0, 0, 255]);
    assert_eq!(pixel(&rgba, size, 12, 4), [0, 255, 0, 255]);
    assert_eq!(pixel(&rgba, size, 20, 4), [0, 255, 0, 255]);
    assert_eq!(
        renderer.textures.len(),
        3,
        "white, original PNG, captured Image"
    );
    // The old native Image is still retained by a previously bound draw after
    // the public sheet has been replaced, including on later host frames.
    let retained_frame = assets
        .prepare_gpu_frame_at_resolution(size, &draws, &[], &[], &[])
        .unwrap();
    assert_eq!(
        renderer
            .render_to_rgba(&assets, &retained_frame, [0, 0, 255])
            .unwrap(),
        rgba
    );
    for name in ["A.dat", "B.dat", "same.png"] {
        std::fs::remove_file(root.join(name)).unwrap();
    }
    std::fs::remove_dir(root).unwrap();
}

#[test]
fn rgb_capture_forces_alpha_one_instead_of_copying_framebuffer_alpha() {
    let size = GameResolution {
        width: 8,
        height: 8,
    };
    let source = "<capture:rgb-alpha>";
    let mut assets = catalog();
    register_capture(&mut assets, source, size);
    let frame = assets
        .prepare_gpu_frame_at_resolution(
            size,
            &[draw(3, "CAP", 8.0, 8.0)],
            &[],
            &[
                rect(0, [230.0, 40.0, 90.0], 0.25, [0.0, 0.0, 8.0, 8.0]),
                rect(2, [0.0, 255.0, 0.0], 1.0, [0.0, 0.0, 8.0, 8.0]),
            ],
            &[capture(1, source)],
        )
        .unwrap();
    let mut renderer = GpuRenderer::headless(size).unwrap();
    let rgba = renderer.render_to_rgba(&assets, &frame, [0, 0, 0]).unwrap();
    assert_eq!(pixel(&rgba, size, 4, 4), [230, 40, 90, 255]);
}

#[test]
fn repeated_captures_bind_per_draw_generations_and_retire_only_after_last_use() {
    let size = GameResolution {
        width: 16,
        height: 8,
    };
    let source = "<capture:generations>";
    let mut assets = catalog();
    register_capture(&mut assets, source, size);
    let frame = assets
        .prepare_gpu_frame_at_resolution(
            size,
            &[draw(3, "CAP", 8.0, 8.0), draw(6, "CAP", 16.0, 8.0)],
            &[],
            &[
                rect(0, [255.0, 0.0, 0.0], 1.0, [0.0, 0.0, 16.0, 8.0]),
                rect(2, [0.0, 0.0, 255.0], 1.0, [0.0, 0.0, 16.0, 8.0]),
                rect(5, [0.0, 255.0, 0.0], 1.0, [0.0, 0.0, 16.0, 8.0]),
            ],
            &[capture(1, source), capture(4, source)],
        )
        .unwrap();
    let generations = frame
        .operations
        .iter()
        .filter_map(|operation| match operation {
            PreparedOperation::Capture(name) => Some(name.clone()),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(generations.len(), 2);
    assert_ne!(generations[0], generations[1]);
    assert!(frame.retired_textures.contains(&generations[0]));
    assert!(frame.required_textures.contains(&generations[0]));
    let mut renderer = GpuRenderer::headless(size).unwrap();
    let rgba = renderer.render_to_rgba(&assets, &frame, [0, 0, 0]).unwrap();
    assert_eq!(pixel(&rgba, size, 2, 3), [255, 0, 0, 255]);
    assert_eq!(pixel(&rgba, size, 13, 3), [0, 0, 255, 255]);
    assert!(renderer.textures.contains_key(&generations[0]));
    let next = assets
        .prepare_gpu_frame_at_resolution(size, &[draw(0, "CAP", 16.0, 8.0)], &[], &[], &[])
        .unwrap();
    let next_rgba = renderer
        .render_to_rgba(&assets, &next, [0, 255, 0])
        .unwrap();
    assert_eq!(next_rgba, rgba);
    assert!(!renderer.textures.contains_key(&generations[0]));
    assert!(renderer.textures.contains_key(&generations[1]));
}

#[test]
fn capture_existing_image_preserves_shadowed_geometry_and_retained_aliases() {
    let size = GameResolution {
        width: 16,
        height: 8,
    };
    let source = "original-atlas.png";
    let mut assets = catalog();
    assets
        .textures
        .insert(source.to_owned(), alpha_texture(size.width, size.height));
    assets
        .textures
        .insert("shadow.png".to_owned(), alpha_texture(4, 4));
    let retained = SpriteCatalogRegion {
        decoded_image: None,
        native_sheet_id: 42,
        texture_source: source.to_owned(),
        sprite: SpriteRegion {
            name: "CAP".to_owned(),
            x: 2,
            y: 1,
            width: 4,
            height: 2,
            pivot_x: 0,
            pivot_y: 0,
            atlas_rotation: 0,
        },
    };
    assets.regions.insert(
        "CAP".to_owned(),
        AtlasRegion {
            texture: "shadow.png".to_owned(),
            sprite: SpriteRegion {
                width: 3,
                height: 3,
                ..retained.sprite.clone()
            },
        },
    );
    assets.composites.insert("CAP".to_owned(), Vec::new());
    let mut retained_draw = draw(4, "CAP", 4.0, 2.0);
    retained_draw.bound_region = Some(Arc::new(retained));
    let frame = assets
        .prepare_gpu_frame_at_resolution(
            size,
            &[retained_draw],
            &[],
            &[
                rect(0, [255.0, 0.0, 0.0], 1.0, [0.0, 0.0, 16.0, 4.0]),
                rect(1, [0.0, 0.0, 255.0], 1.0, [0.0, 4.0, 16.0, 8.0]),
                rect(3, [0.0, 255.0, 0.0], 1.0, [0.0, 0.0, 16.0, 8.0]),
            ],
            &[capture(2, source)],
        )
        .unwrap();
    assert_eq!(assets.regions["CAP"].texture, "shadow.png");
    assert_eq!(assets.regions["CAP"].sprite.width, 3);
    assert!(assets.composites.contains_key("CAP"));
    let binding = &assets.captures.bindings[source];
    assert_eq!((binding.width, binding.height), (16, 8));
    assert_eq!(binding.surface_format, SurfaceFormat::A8B8G8R8);
    assert_eq!(
        frame.draws.last().unwrap().program,
        NativeProgram::SpriteAlpha
    );
    let mut renderer = GpuRenderer::headless(size).unwrap();
    let rgba = renderer.render_to_rgba(&assets, &frame, [0, 0, 0]).unwrap();
    // An existing region retains flags 0, unlike a new captured fullSprite.
    // Its atlas row 1 now contains the framebuffer's lower blue half.
    assert_eq!(pixel(&rgba, size, 1, 0), [0, 0, 255, 255]);
    assert_eq!(pixel(&rgba, size, 8, 0), [0, 255, 0, 255]);
}

#[test]
fn captured_images_keep_pixels_and_native_dimensions_across_drawable_resize() {
    let initial = GameResolution {
        width: 8,
        height: 4,
    };
    let resized = GameResolution {
        width: 16,
        height: 8,
    };
    let source = "<capture:resize-content>";
    let mut assets = catalog();
    register_capture(&mut assets, source, initial);
    let first = assets
        .prepare_gpu_frame_at_resolution(
            initial,
            &[],
            &[],
            &[
                rect(0, [255.0, 0.0, 0.0], 1.0, [0.0, 0.0, 8.0, 2.0]),
                rect(1, [0.0, 0.0, 255.0], 1.0, [0.0, 2.0, 8.0, 4.0]),
            ],
            &[capture(2, source)],
        )
        .unwrap();
    let mut renderer = GpuRenderer::headless(initial).unwrap();
    renderer
        .render_offscreen(&assets, &first, [0, 0, 0])
        .unwrap();
    let generation = assets.captures.bindings[source].source.clone();
    renderer.resize_game_target(resized);
    let next = assets
        .prepare_gpu_frame_at_resolution(resized, &[draw(0, "CAP", 8.0, 4.0)], &[], &[], &[])
        .unwrap();
    let rgba = renderer
        .render_to_rgba(&assets, &next, [0, 255, 0])
        .unwrap();
    assert_eq!(pixel(&rgba, resized, 1, 0), [255, 0, 0, 255]);
    assert_eq!(pixel(&rgba, resized, 1, 3), [0, 0, 255, 255]);
    assert_eq!(pixel(&rgba, resized, 9, 2), [0, 255, 0, 255]);
    assert_eq!(assets.regions["CAP"].sprite.width, 8);
    assert_eq!(renderer.textures[&generation].texture.width(), 8);
    let error = assets
        .prepare_gpu_frame_at_resolution(resized, &[], &[], &[], &[capture(0, source)])
        .err()
        .unwrap();
    assert!(
        error
            .to_string()
            .contains("Wrong size capture target image")
    );
    assert_eq!(assets.captures.bindings[source].source, generation);
}

#[test]
fn captures_resolve_as_fill_and_both_explicit_and_native_quad_sources() {
    let size = GameResolution {
        width: 8,
        height: 8,
    };
    let source = "<capture:draw-paths>";
    let mut assets = catalog();
    register_capture(&mut assets, source, size);
    let mut masked = draw(1, "CAP", 8.0, 8.0);
    masked.texture = Some(Arc::new(stella_script::SpriteTextureSubmission {
        name: "CAP".into(),
        scale: 1.0,
        binding: MaskedTextureBinding::Source(source.to_owned()),
    }));
    let positions = [[0.0, 0.0], [8.0, 0.0], [0.0, 8.0], [8.0, 8.0]];
    let mut native = draw(2, "CAP", 8.0, 8.0);
    native.geometry = Some(SpriteGeometrySubmission::NativeAtlasQuad(Arc::new(
        positions,
    )));
    let mut explicit = draw(3, "CAP", 8.0, 8.0);
    explicit.geometry = Some(SpriteGeometrySubmission::ExplicitQuad(Arc::new(
        RenderQuad {
            positions,
            uv: [[0.0, 1.0], [1.0, 1.0], [0.0, 0.0], [1.0, 0.0]],
        },
    )));
    let frame = assets
        .prepare_gpu_frame_at_resolution(
            size,
            &[masked, native, explicit],
            &[],
            &[],
            &[capture(0, source)],
        )
        .unwrap();
    let physical = &assets.captures.bindings[source].source;
    assert_eq!(
        frame.draw_texture_pair(0),
        (physical.as_str(), physical.as_str())
    );
    assert_eq!(frame.draw_texture_pair(1).0, physical);
    let mut renderer = GpuRenderer::headless(size).unwrap();
    let rgba = renderer
        .render_to_rgba(&assets, &frame, [30, 90, 150])
        .unwrap();
    assert_eq!(pixel(&rgba, size, 3, 3), [30, 90, 150, 255]);
}

#[test]
fn temporary_capture_images_do_not_accumulate_bindings_or_gpu_allocations() {
    let size = GameResolution {
        width: 4,
        height: 4,
    };
    let mut assets = catalog();
    let mut renderer = GpuRenderer::headless(size).unwrap();
    for index in 0..4 {
        let command = CaptureRenderCommand {
            temporary: true,
            ..capture(0, &format!("<capture:temporary:{index}>"))
        };
        let frame = assets
            .prepare_gpu_frame_at_resolution(size, &[], &[], &[], &[command])
            .unwrap();
        assert!(assets.captures.bindings.is_empty());
        renderer
            .render_offscreen(&assets, &frame, [10, 20, 30])
            .unwrap();
        assert_eq!(
            renderer.textures.len(),
            1,
            "only the white fallback image remains"
        );
        assert!(renderer.retired_textures.is_empty());
    }
}
