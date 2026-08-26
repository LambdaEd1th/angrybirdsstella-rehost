//! Viewport conversion, immediate batching, ordering and capture boundaries.

use super::*;

#[test]
fn native_viewport_projection_rounds_scale_before_fused_translation() {
    assert_eq!(
        frame::screen_to_clip([0.0, 0.0], GameResolution::default()),
        [-1.0, 1.0]
    );
    let center = frame::screen_to_clip([512.0, 384.0], GameResolution::default());
    assert_eq!(center[0], 0.0);
    // -2/768 is rounded to f32 before FMADD. At y=384 that leaves
    // exactly -2^-25 rather than the algebraically simplified zero.
    assert_eq!(center[1].to_bits(), 0xb300_0000);
}

#[test]
fn wide_native_viewport_projects_its_own_center_and_clamps_scissors() {
    let resolution = GameResolution {
        width: 2009,
        height: 1080,
    };
    let center = frame::screen_to_clip([1004.5, 540.0], resolution);
    // The original GL path rounds 2/width before FMADD, so an odd-width
    // framebuffer leaves a tiny residual at its mathematical center.
    assert!(center[0].abs() < f32::EPSILON);
    assert!(center[1].abs() < f32::EPSILON);

    let mut prepared = PreparedFrame {
        resolution,
        current_clip: Some([-20, -10, 2500, 1400]),
        ..PreparedFrame::default()
    };
    prepared.push_quad(
        [[0.0, 0.0], [1.0, 0.0], [0.0, 1.0], [1.0, 1.0]],
        [[0.0, 0.0]; 4],
        [[0.0, 0.0]; 4],
        [[0.0, 0.0]; 4],
        DrawUniform::default(),
        WHITE_TEXTURE.to_owned(),
        WHITE_TEXTURE.to_owned(),
        NativeProgram::SpriteAlpha,
    );
    assert_eq!(prepared.draws[0].scissor, Some([0, 0, 2009, 1080]));
}

#[test]
fn wide_headless_readback_removes_wgpu_row_padding() {
    let resolution = GameResolution {
        width: 1001,
        height: 3,
    };
    let assets = AssetCatalog {
        root: std::path::PathBuf::new(),
        font_root: std::path::PathBuf::new(),
        regions: HashMap::new(),
        composites: HashMap::new(),
        masked_textures: HashMap::new(),
        fonts: HashMap::new(),
        textures: HashMap::new(),
        system_labels: SystemLabelPool::default(),
    };
    let frame = PreparedFrame {
        resolution,
        ..PreparedFrame::default()
    };
    let mut renderer = GpuRenderer::headless(resolution).unwrap();
    let rgba = renderer
        .render_to_rgba(&assets, &frame, [17, 34, 51])
        .unwrap();
    assert_eq!(
        rgba.len(),
        (resolution.width * resolution.height * 4) as usize
    );
    assert!(rgba.chunks_exact(4).all(|pixel| pixel == [17, 34, 51, 255]));
}

#[test]
fn renderer_recreates_game_and_capture_targets_on_resolution_change() {
    let initial = GameResolution::default();
    let wide = GameResolution {
        width: 1429,
        height: 768,
    };
    let mut renderer = GpuRenderer::headless(initial).unwrap();
    renderer.textures.insert(
        "<capture:resize-test>".to_owned(),
        resources::create_capture_texture(&renderer.device, "<capture:resize-test>", initial),
    );

    renderer.resize_game_target(wide);

    assert_eq!(renderer.resolution, wide);
    assert_eq!(
        renderer::initialization::target::game_texture_size(&renderer.game_texture),
        wide
    );
    let capture = &renderer.textures["<capture:resize-test>"].texture;
    assert_eq!(capture.width(), wide.width);
    assert_eq!(capture.height(), wide.height);
}

#[test]
fn quad_stream_uses_native_triangle_order_and_draw_index() {
    let mut frame = PreparedFrame::default();
    frame.push_quad(
        [[0.0, 0.0], [1.0, 0.0], [0.0, 1.0], [1.0, 1.0]],
        [[0.0, 0.0]; 4],
        [[0.0, 0.0]; 4],
        [[0.0, 0.0]; 4],
        DrawUniform::default(),
        WHITE_TEXTURE.to_owned(),
        WHITE_TEXTURE.to_owned(),
        NativeProgram::SpriteAlpha,
    );
    assert_eq!(
        frame
            .vertices
            .iter()
            .map(|vertex| vertex.position)
            .collect::<Vec<_>>(),
        vec![
            [0.0, 0.0],
            [1.0, 0.0],
            [0.0, 1.0],
            [0.0, 1.0],
            [1.0, 0.0],
            [1.0, 1.0]
        ]
    );
    assert!(frame.vertices.iter().all(|vertex| vertex.draw_index == 0));
}

#[test]
fn adjacent_compatible_quads_share_one_wgpu_draw_without_sharing_uniforms() {
    let mut frame = PreparedFrame::default();
    for offset in [0.0, 1.0] {
        let mut uniform = DrawUniform::default();
        uniform.header[0] = offset;
        frame.push_quad(
            [
                [offset, 0.0],
                [offset + 1.0, 0.0],
                [offset, 1.0],
                [offset + 1.0, 1.0],
            ],
            [[0.0, 0.0]; 4],
            [[0.0, 0.0]; 4],
            [[0.0, 0.0]; 4],
            uniform,
            "theme.pvr".to_owned(),
            WHITE_TEXTURE.to_owned(),
            NativeProgram::SpriteAlpha,
        );
    }

    assert_eq!(frame.uniforms.len(), 2);
    assert_eq!(frame.draws.len(), 1);
    assert_eq!(frame.operations, [PreparedOperation::Draw(0)]);
    assert_eq!(frame.draws[0].vertices, 0..12);
    assert!(
        frame.vertices[..6]
            .iter()
            .all(|vertex| vertex.draw_index == 0)
    );
    assert!(
        frame.vertices[6..]
            .iter()
            .all(|vertex| vertex.draw_index == 1)
    );
}

#[test]
fn native_clip_edges_become_clamped_wgpu_scissors_and_empty_clips_skip_draws() {
    let mut frame = PreparedFrame {
        current_clip: Some([-5, 10, 1050, 100]),
        ..PreparedFrame::default()
    };
    frame.push_quad(
        [[0.0, 0.0], [1.0, 0.0], [0.0, 1.0], [1.0, 1.0]],
        [[0.0, 0.0]; 4],
        [[0.0, 0.0]; 4],
        [[0.0, 0.0]; 4],
        DrawUniform::default(),
        WHITE_TEXTURE.to_owned(),
        WHITE_TEXTURE.to_owned(),
        NativeProgram::SpriteAlpha,
    );
    assert_eq!(frame.draws[0].scissor, Some([0, 10, 1024, 90]));

    frame.current_clip = Some([20, 30, 10, 40]);
    frame.push_quad(
        [[0.0, 0.0], [1.0, 0.0], [0.0, 1.0], [1.0, 1.0]],
        [[0.0, 0.0]; 4],
        [[0.0, 0.0]; 4],
        [[0.0, 0.0]; 4],
        DrawUniform::default(),
        WHITE_TEXTURE.to_owned(),
        WHITE_TEXTURE.to_owned(),
        NativeProgram::SpriteAlpha,
    );
    assert_eq!(frame.draws.len(), 1);
    assert_eq!(frame.uniforms.len(), 1);
}

#[test]
fn mixed_command_classes_keep_native_immediate_submission_order() {
    let texture_name = "<order-test>".to_owned();
    let mut assets = AssetCatalog {
        root: std::path::PathBuf::new(),
        font_root: std::path::PathBuf::new(),
        regions: HashMap::from([(
            "ORDER_SPRITE".to_owned(),
            AtlasRegion {
                texture: texture_name.clone(),
                sprite: SpriteRegion {
                    name: "ORDER_SPRITE".to_owned(),
                    x: 0,
                    y: 0,
                    width: 1,
                    height: 1,
                    pivot_x: 0,
                    pivot_y: 0,
                    atlas_rotation: 0,
                },
            },
        )]),
        composites: HashMap::new(),
        masked_textures: HashMap::new(),
        fonts: HashMap::new(),
        textures: HashMap::from([(texture_name, alpha_texture(1, 1))]),
        system_labels: SystemLabelPool::default(),
    };
    let rect = |order| RectRenderCommand {
        order,
        red: 1.0,
        green: 1.0,
        blue: 1.0,
        alpha: 1.0,
        left: 0.0,
        top: 0.0,
        right: 1.0,
        bottom: 1.0,
        color_program: ColorProgram::PlainAlpha,
        vertices: None,
        mesh_topology: ColorMeshTopology::TriangleFan,
        clip_rect: None,
    };
    let sprite = RenderCommand {
        order: 1,
        sprite: "ORDER_SPRITE".into(),
        texture: None,
        bound_region: None,
        bound_composite: None,
        geometry: None,
        shader: None,
        clip_holes: Vec::new(),
        dirt: None,
        x: 0.0,
        y: 0.0,
        state: stella_script::RenderState::default(),
        world_space: true,
    };
    let frame = assets
        .prepare_gpu_frame(&[sprite], &[], &[rect(0), rect(2)], &[])
        .unwrap();
    assert_eq!(
        frame
            .draws
            .iter()
            .map(|draw| draw.program)
            .collect::<Vec<_>>(),
        vec![
            NativeProgram::PlainAlpha,
            NativeProgram::SpriteAlpha,
            NativeProgram::PlainAlpha
        ]
    );
}

#[test]
fn capture_sprite_copies_the_immediate_framebuffer_for_later_draws() {
    let mut assets = AssetCatalog {
        root: std::path::PathBuf::new(),
        font_root: std::path::PathBuf::new(),
        regions: HashMap::new(),
        composites: HashMap::new(),
        masked_textures: HashMap::new(),
        fonts: HashMap::new(),
        textures: HashMap::new(),
        system_labels: SystemLabelPool::default(),
    };
    let rect = |order, red, blue| RectRenderCommand {
        order,
        red,
        green: 0.0,
        blue,
        alpha: 1.0,
        left: 0.0,
        top: 0.0,
        right: GAME_WIDTH as f64,
        bottom: GAME_HEIGHT as f64,
        color_program: ColorProgram::Plain,
        vertices: None,
        mesh_topology: ColorMeshTopology::TriangleFan,
        clip_rect: None,
    };
    let native_clear = RectRenderCommand {
        order: 2,
        red: 0.0,
        green: 0.0,
        blue: 255.0,
        alpha: 1.0,
        left: -32000.0,
        top: -32000.0,
        right: 32000.0,
        bottom: 32000.0,
        color_program: ColorProgram::Plain,
        vertices: Some(vec![
            [-32000.0, -32000.0],
            [32000.0, -32000.0],
            [-32000.0, 32000.0],
            [32000.0, 32000.0],
        ]),
        mesh_topology: ColorMeshTopology::TriangleStrip,
        clip_rect: None,
    };
    let captured = RenderCommand {
        order: 3,
        sprite: "CAPTURED_FRAME".into(),
        texture: None,
        bound_region: None,
        bound_composite: None,
        geometry: None,
        shader: None,
        clip_holes: Vec::new(),
        dirt: None,
        x: 0.0,
        y: 0.0,
        state: stella_script::RenderState::default(),
        world_space: true,
    };
    let frame = assets
        .prepare_gpu_frame(
            &[captured],
            &[],
            &[rect(0, 255.0, 0.0), native_clear],
            &[CaptureRenderCommand {
                order: 1,
                name: "CAPTURED_FRAME".to_owned(),
            }],
        )
        .unwrap();
    assert_eq!(
        frame.operations,
        [
            PreparedOperation::Draw(0),
            PreparedOperation::Capture("<capture:CAPTURED_FRAME>".to_owned()),
            PreparedOperation::Draw(1),
            PreparedOperation::Draw(2),
        ]
    );
    let mut renderer = GpuRenderer::headless(GameResolution::default()).unwrap();
    let rgba = renderer.render_to_rgba(&assets, &frame, [0, 0, 0]).unwrap();
    assert_eq!(&rgba[..4], &[255, 0, 0, 255]);
    let center = ((GAME_HEIGHT / 2 * GAME_WIDTH + GAME_WIDTH / 2) * 4) as usize;
    assert_eq!(&rgba[center..center + 4], &[255, 0, 0, 255]);
}

#[test]
fn shipped_challenge_level_end_background_occludes_the_complete_gpu_framebuffer() {
    let data_root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../runtime/data");
    let image_root = data_root.join("images/1024x768");
    let font_root = data_root.join("fonts/1024x768");
    if !image_root.is_dir() || !font_root.is_dir() {
        return;
    }

    let command = |order,
                   sprite: &str,
                   translate_x: f64,
                   translate_y: f64,
                   scale_x: f64,
                   scale_y: f64,
                   pivot_x: f64,
                   pivot_y: f64| RenderCommand {
        order,
        sprite: sprite.into(),
        texture: None,
        bound_region: None,
        bound_composite: None,
        geometry: None,
        shader: None,
        clip_holes: Vec::new(),
        dirt: None,
        x: 0.0,
        y: 0.0,
        state: stella_script::RenderState {
            translate_x,
            translate_y,
            scale_x,
            scale_y,
            pivot_x,
            pivot_y,
            ..stella_script::RenderState::default()
        },
        world_space: false,
    };
    let commands = [
        command(
            0,
            "ENDSCREEN_BG_STRIP",
            0.0,
            384.0,
            1_000.0,
            1.0,
            0.0,
            386.0,
        ),
        command(
            1,
            "ENDSCREEN_BG_STRIP",
            0.959,
            384.0,
            1_000.0,
            1.0,
            0.0,
            386.0,
        ),
        command(2, "ENDSCREEN_WIN", 513.0, 384.0, 1.0, 1.0, 504.0, 387.0),
        command(3, "ENDSCREEN_BG_FG", 506.0, 650.0, 1.0, 1.0, 480.0, 120.0),
    ];
    let mut assets = AssetCatalog::load(image_root, font_root).unwrap();
    let frame = assets.prepare_gpu_frame(&commands, &[], &[], &[]).unwrap();
    let mut renderer = GpuRenderer::headless(GameResolution::default()).unwrap();
    let rgba = renderer
        .render_to_rgba(&assets, &frame, [255, 0, 255])
        .unwrap();
    let uncovered = rgba
        .chunks_exact(4)
        .enumerate()
        .filter(|(_, pixel)| *pixel == [255, 0, 255, 255])
        .map(|(index, _)| (index as u32 % GAME_WIDTH, index as u32 / GAME_WIDTH))
        .collect::<Vec<_>>();

    assert!(
        uncovered.is_empty(),
        "the challenge result background left {} GPU pixels uncovered: {uncovered:?}",
        uncovered.len()
    );
}
