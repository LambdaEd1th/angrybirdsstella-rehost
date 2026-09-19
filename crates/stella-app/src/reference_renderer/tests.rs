use super::*;
use super::{shader::apply_sprite_shader, texture::sample_bilinear_clamped};
use crate::reference_renderer::commands::render_game;
use std::path::Path;
use stella_assets::surface_format::SurfaceFormat;
use stella_script::RenderState;

#[test]
fn reference_game_preserves_capture_commands_and_cross_frame_painter_order() {
    let mut assets = AssetCatalog {
        root: std::path::PathBuf::new(),
        font_root: std::path::PathBuf::new(),
        regions: HashMap::new(),
        composites: HashMap::new(),
        masked_textures: HashMap::new(),
        fonts: HashMap::new(),
        textures: HashMap::new(),
        system_labels: SystemLabelPool::default(),
        captures: Default::default(),
    };
    let mut pixels = vec![0; (GAME_WIDTH * GAME_HEIGHT) as usize];
    commands::render_game_with_captures(
        &mut assets,
        &[],
        &[],
        &[],
        &[CaptureRenderCommand {
            order: 0,
            name: "NEW_CAPTURE".to_owned(),
            texture_source: "<capture:NEW_CAPTURE>".to_owned(),
            temporary: false,
        }],
        [23, 47, 89],
        &mut pixels,
    )
    .unwrap();
    let binding = &assets.captures.bindings["<capture:NEW_CAPTURE>"];
    assert_eq!(binding.surface_format, SurfaceFormat::B8G8R8);
    let captured = &assets.textures[&binding.source];
    assert_eq!(captured.image.dimensions(), (GAME_WIDTH, GAME_HEIGHT));
    assert_eq!(captured.upload_surface_format(), SurfaceFormat::B8G8R8);
    assert_eq!(captured.image.get_pixel(0, 0).0, [23, 47, 89, 255]);
    assets.regions.insert(
        "PATCH".to_owned(),
        AtlasRegion {
            texture: "<capture:NEW_CAPTURE>".to_owned(),
            sprite: SpriteRegion {
                name: "PATCH".to_owned(),
                x: 0,
                y: 0,
                width: 32,
                height: 32,
                pivot_x: 0,
                pivot_y: 0,
                atlas_rotation: 0,
            },
        },
    );
    let sprite = RenderCommand {
        projection_3d: None,
        order: 0,
        sprite: "PATCH".into(),
        texture: None,
        bound_region: None,
        bound_composite: None,
        geometry: None,
        shader: None,
        dirt: None,
        x: 0.0,
        y: 0.0,
        state: RenderState::default().into(),
        world_space: false,
    };
    let overlay = RectRenderCommand {
        projection_3d: None,
        order: 1,
        red: 0.0,
        green: 255.0,
        blue: 0.0,
        alpha: 1.0,
        left: 0.0,
        top: 0.0,
        right: 32.0,
        bottom: 32.0,
        color_program: ColorProgram::Plain,
        vertices: None,
        mesh_topology: ColorMeshTopology::TriangleFan,
        clip_rect: None,
    };
    render_game(&mut assets, &[sprite], &[], &[overlay], [0; 3], &mut pixels).unwrap();
    // The legacy per-type path would draw this later rectangle first, then
    // wrongly cover it with the previous frame's captured blue-gray image.
    assert_eq!(pixels[0], 0x00ff00);
    assert_eq!(pixels[31 * GAME_WIDTH as usize + 31], 0x00ff00);
    assert_eq!(pixels[32], 0);
}

#[test]
fn maps_letterboxed_input_without_clamping_active_drags() {
    assert_eq!(
        map_window_to_game(400.0, 300.0, 800, 600, GameResolution::default()),
        (512.0, 384.0)
    );
    assert_eq!(
        map_window_to_game(0.0, 0.0, 1200, 600, GameResolution::default()),
        (-256.0, 0.0)
    );
}

#[test]
fn wide_input_uses_the_native_drawable_extent_without_four_by_three_letterboxing() {
    let resolution = GameResolution {
        width: 2009,
        height: 1080,
    };
    assert_eq!(
        map_window_to_game(1004.5, 540.0, 2009, 1080, resolution),
        (1004.5, 540.0)
    );
    assert_eq!(
        map_window_to_game(2009.0, 1080.0, 2009, 1080, resolution),
        (2009.0, 1080.0)
    );
}

#[test]
fn colorize_shader_matches_bundled_pixel_program_order() {
    let shader = SpriteShader {
        name: "2d-sprite-colorize4".to_owned(),
        diffuse: [0.5, 1.0, 0.25, 0.5],
        lightness: 0.0,
        saturation: 0.0,
        highlight: 0.0,
    };
    assert_eq!(
        apply_sprite_shader([90, 150, 210, 200], &shader),
        [75, 150, 38, 100]
    );
}

#[test]
fn native_scalar_render_state_uses_scale_after_pivoted_rotation() {
    let command = RenderCommand {
        projection_3d: None,
        order: 0,
        sprite: "TEST".into(),
        texture: None,
        bound_region: None,
        bound_composite: None,
        geometry: None,
        shader: None,
        dirt: None,
        x: 30.0,
        y: 40.0,
        state: stella_script::RenderState {
            translate_x: 10.0,
            translate_y: 20.0,
            scale_x: 2.0,
            scale_y: 3.0,
            angle: std::f64::consts::FRAC_PI_2,
            pivot_x: 4.0,
            pivot_y: 5.0,
            ..stella_script::RenderState::default()
        }
        .into(),
        world_space: false,
    };

    let transform = render_command_transform(&command);
    assert!((transform.x - 98.0).abs() < 1.0e-6);
    assert!((transform.y - 183.0).abs() < 1.0e-6);
    assert!(transform.m00.abs() < 1.0e-6);
    assert!((transform.m01 + 2.0).abs() < 1.0e-6);
    assert!((transform.m10 - 3.0).abs() < 1.0e-6);
    assert!(transform.m11.abs() < 1.0e-6);
}

#[test]
fn tutorial_target_uses_the_same_native_matrix_path_as_every_atlas_sprite() {
    let tutorial = RenderCommand {
        projection_3d: None,
        order: 0,
        sprite: "TUTORIAL_TARGET".into(),
        texture: None,
        bound_region: None,
        bound_composite: None,
        geometry: None,
        shader: None,
        dirt: None,
        x: 100.0,
        y: 200.0,
        state: RenderState {
            scale_x: 2.0,
            scale_y: 3.0,
            ..RenderState::default()
        }
        .into(),
        world_space: false,
    };
    let ordinary = RenderCommand {
        sprite: "ORDINARY_ATLAS_REGION".into(),
        ..tutorial.clone()
    };

    let tutorial = render_command_transform(&tutorial);
    let ordinary = render_command_transform(&ordinary);
    assert_eq!(
        [
            tutorial.x,
            tutorial.y,
            tutorial.m00,
            tutorial.m01,
            tutorial.m10,
            tutorial.m11,
            tutorial.alpha,
        ],
        [
            ordinary.x,
            ordinary.y,
            ordinary.m00,
            ordinary.m01,
            ordinary.m10,
            ordinary.m11,
            ordinary.alpha,
        ]
    );
}

#[test]
fn native_render_boundary_quantizes_to_f32_and_uses_mixed_fmul_fmadd_vertex_math() {
    let command = RenderCommand {
        projection_3d: None,
        order: 0,
        sprite: "TEST".into(),
        texture: None,
        bound_region: None,
        bound_composite: None,
        geometry: None,
        shader: None,
        dirt: None,
        x: 0.25,
        y: -0.25,
        state: stella_script::RenderState {
            translate_x: 16_777_217.0,
            translate_y: -16_777_217.0,
            ..stella_script::RenderState::default()
        }
        .into(),
        world_space: true,
    };
    let boundary = render_command_transform(&command);
    assert_eq!(boundary.x, 16_777_216.0);
    assert_eq!(boundary.y, -16_777_216.0);

    // sub_100467BE8 emits one rounded FMUL addend followed by FMADD, then adds
    // translation. These operands leave exactly 2^-46 on Purple's path but
    // round to zero if both products are rounded before their sum.
    let one_plus_ulp = f32::from_bits(0x3f80_0001);
    let transform = SpriteTransform {
        x: 0.0,
        y: 0.0,
        m00: one_plus_ulp,
        m01: -f32::from_bits(0x3f80_0002),
        m10: 0.0,
        m11: 1.0,
        alpha: 1.0,
    };
    let point = transform.transform_point(one_plus_ulp, 1.0);
    assert_eq!(point[0].to_bits(), 0x2880_0000);
    assert_eq!(one_plus_ulp * one_plus_ulp + transform.m01, 0.0);
}

#[test]
fn bitmap_glyphs_use_native_scale_once_and_preserve_exact_ui_matrix() {
    let mut command = TextRenderCommand {
        order: 0,
        text: "A".to_owned(),
        font: "FONT".to_owned(),
        font_binding: None,
        x: 10.0,
        y: 20.0,
        native_system_origin: None,
        scale_x: 2.0,
        scale_y: 3.0,
        angle: 0.0,
        matrix: None,
        position_matrix: None,
        alpha: 0.75,
        horizontal_anchor: "LEFT".to_owned(),
        vertical_anchor: "TOP".to_owned(),
        projection_3d: None,
        clip_rect: None,
    };
    let scalar = text_glyph_transform(&command, 7.0, 11.0);
    assert_eq!((scalar.x, scalar.y), (24.0, 53.0));
    assert_eq!(
        (scalar.m00, scalar.m01, scalar.m10, scalar.m11),
        (2.0, 0.0, 0.0, 3.0)
    );

    command.matrix = Some([2.0, -3.0, 4.0, 5.0]);
    let affine = text_glyph_transform(&command, 7.0, 11.0);
    assert_eq!((affine.x, affine.y), (-9.0, 103.0));
    assert_eq!(
        (affine.m00, affine.m01, affine.m10, affine.m11),
        (2.0, -3.0, 4.0, 5.0)
    );
    assert_eq!(affine.alpha, 0.75);

    // SystemFont anchors its x/y before GL_Image rotates the label quad. The
    // position-only basis is therefore axis scale even though the quad keeps
    // the complete rotated matrix.
    command.position_matrix = Some([2.0, 0.0, 0.0, 5.0]);
    let system = text_glyph_transform(&command, 7.0, 11.0);
    assert_eq!((system.x, system.y), (24.0, 75.0));
    assert_eq!(
        (system.m00, system.m01, system.m10, system.m11),
        (2.0, -3.0, 4.0, 5.0)
    );
}

#[test]
fn atlas_sampling_matches_native_full_texture_linear_filter() {
    let mut texture = RgbaImage::from_pixel(4, 1, image::Rgba([0, 255, 0, 255]));
    texture.put_pixel(1, 0, image::Rgba([255, 0, 0, 255]));
    texture.put_pixel(2, 0, image::Rgba([0, 0, 255, 255]));

    assert_eq!(
        sample_bilinear_clamped(&texture, 1.5, 0.0),
        [128, 0, 128, 255]
    );
    assert_eq!(
        sample_bilinear_clamped(&texture, 0.5, 0.0),
        [128, 128, 0, 255]
    );
    assert_eq!(
        sample_bilinear_clamped(&texture, -0.5, 0.0),
        [0, 255, 0, 255]
    );
}

#[test]
fn recovered_3d_text_projection_uses_x_rotation_and_perspective_divide() {
    let centered = native_project_clip(
        TextProjection3D {
            x: 0.0,
            y: 0.0,
            z: 100.0,
            rotation_x: 0.0,
            custom_model: true,
        },
        [0.0, 0.0, 0.0],
    );
    assert_eq!([centered[0], centered[1], centered[3]], [0.0, 0.0, 100.0]);
    assert!((centered[2] - 99.99905).abs() < 0.00002);

    let tilted = native_project_clip(
        TextProjection3D {
            x: 0.0,
            y: 0.0,
            z: 100.0,
            rotation_x: std::f32::consts::FRAC_PI_2,
            custom_model: true,
        },
        [11.0, 10.0, 0.0],
    );
    // Independent fixed-point check: X rotation moves local y=10 into z,
    // and cot(0.75) scales x. The clip-space w must survive until rasterization.
    assert!((tilted[0] - 11.807688).abs() < 0.00002);
    assert!(tilted[1].abs() < 0.00002);
    assert!((tilted[2] - 109.999054).abs() < 0.00003);
    assert!((tilted[3] - 110.0).abs() < 0.00002);
}

#[test]
fn composite_native_flip_multipliers_and_radian_angle_are_applied() {
    let parent = SpriteTransform::from_scale_rotation(100.0, 200.0, 2.0, 3.0, 0.0, 0.75);
    let child = composite_child_transform(
        parent,
        &CompositePart {
            sprite: "PART".to_owned(),
            x: 5.0,
            y: -4.0,
            scale_x: 0.5,
            scale_y: 0.25,
            flip_x: -1.0,
            flip_y: -1.0,
            angle: std::f32::consts::FRAC_PI_2,
            visible: true,
        },
    );

    assert_eq!((child.x, child.y), (110.0, 188.0));
    assert!(child.m00.abs() < 1e-6);
    assert!((child.m01 - 0.5).abs() < 1e-6);
    assert!((child.m10 + 1.5).abs() < 1e-6);
    assert!(child.m11.abs() < 1e-6);
    assert_eq!(child.alpha, 0.75);
}

#[test]
fn shipped_challenge_level_end_background_occludes_the_complete_native_framebuffer() {
    let data_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../runtime/data");
    let image_root = data_root.join("images/1024x768");
    let font_root = data_root.join("fonts/1024x768");
    if !image_root.is_dir() || !font_root.is_dir() {
        return;
    }

    let command = |sprite: &str,
                   translate_x: f64,
                   translate_y: f64,
                   scale_x: f64,
                   scale_y: f64,
                   pivot_x: f64,
                   pivot_y: f64| RenderCommand {
        projection_3d: None,
        order: 0,
        sprite: sprite.into(),
        texture: None,
        bound_region: None,
        bound_composite: None,
        geometry: None,
        shader: None,
        dirt: None,
        x: 0.0,
        y: 0.0,
        state: RenderState {
            translate_x,
            translate_y,
            scale_x,
            scale_y,
            pivot_x,
            pivot_y,
            ..RenderState::default()
        }
        .into(),
        world_space: false,
    };
    let commands = [
        command("ENDSCREEN_BG_STRIP", 0.0, 384.0, 1_000.0, 1.0, 0.0, 386.0),
        command("ENDSCREEN_BG_STRIP", 0.959, 384.0, 1_000.0, 1.0, 0.0, 386.0),
        command("ENDSCREEN_WIN", 513.0, 384.0, 1.0, 1.0, 504.0, 387.0),
        command("ENDSCREEN_BG_FG", 506.0, 650.0, 1.0, 1.0, 480.0, 120.0),
    ];
    let sentinel = 0x00ff_00ff;
    let mut target = vec![sentinel; (GAME_WIDTH * GAME_HEIGHT) as usize];
    let mut assets = AssetCatalog::load(image_root, font_root).unwrap();
    render_game(&mut assets, &commands, &[], &[], [255, 0, 255], &mut target).unwrap();

    let uncovered = target
        .iter()
        .enumerate()
        .filter(|(_, pixel)| **pixel == sentinel)
        .map(|(index, _)| (index as u32 % GAME_WIDTH, index as u32 / GAME_WIDTH))
        .collect::<Vec<_>>();
    assert!(
        uncovered.is_empty(),
        "the level-end background left {} framebuffer pixels uncovered: {uncovered:?}",
        uncovered.len()
    );
}
