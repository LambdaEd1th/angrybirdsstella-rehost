use super::*;

mod captures;

fn vertex(position: [f32; 4], uv: [f32; 2]) -> GpuVertex {
    GpuVertex {
        position: [0.0; 2],
        uv,
        source: uv,
        clip_position: position,
        draw_index: 0,
        padding: 0,
    }
}

fn fixture(vertices: Vec<GpuVertex>, program: NativeProgram) -> (PreparedFrame, AssetCatalog) {
    let texture_name = "<reference-test>".to_owned();
    let frame = PreparedFrame {
        resolution: GameResolution {
            width: 32,
            height: 32,
        },
        draws: vec![PreparedDraw {
            vertices: 0..vertices.len() as u32,
            texture_pair: 0,
            program,
            scissor: None,
        }],
        vertices,
        uniforms: vec![DrawUniform {
            header: [1.0, 1.0, 0.0, 0.0],
            diffuse: [1.0; 4],
            params: [0.0, 1.0, 0.0, 0.0],
            fill: [1.0, 1.0, 0.0, 0.0],
        }],
        texture_pairs: vec![(texture_name.clone(), WHITE_TEXTURE.to_owned())],
        required_textures: HashSet::from([texture_name.clone()]),
        operations: vec![PreparedOperation::Draw(0)],
        ..PreparedFrame::default()
    };
    let assets = AssetCatalog {
        root: std::path::PathBuf::new(),
        font_root: std::path::PathBuf::new(),
        regions: HashMap::new(),
        composites: HashMap::new(),
        masked_textures: HashMap::new(),
        fonts: HashMap::new(),
        textures: HashMap::from([(
            texture_name,
            TextureAsset::new(
                RgbaImage::from_pixel(1, 1, image::Rgba([255, 0, 0, 255])),
                SurfaceFormat::A8B8G8R8,
            ),
        )]),
        system_labels: SystemLabelPool::default(),
        captures: Default::default(),
    };
    (frame, assets)
}

fn compare_gpu(frame: &PreparedFrame, assets: &AssetCatalog, expected: &[u32]) {
    let mut gpu = GpuRenderer::headless(frame.resolution).unwrap();
    let actual = gpu.render_to_rgba(assets, frame, [0; 3]).unwrap();
    for (index, (pixel, expected)) in actual.as_chunks::<4>().0.iter().zip(expected).enumerate() {
        let expected = [
            (*expected >> 16) as u8,
            (*expected >> 8) as u8,
            *expected as u8,
        ];
        for channel in 0..3 {
            assert!(
                pixel[channel].abs_diff(expected[channel]) <= 1,
                "pixel {index}, channel {channel}: GPU {} vs reference {}",
                pixel[channel],
                expected[channel]
            );
        }
    }
}

#[test]
fn homogeneous_clipper_intersects_each_wgpu_plane_before_dividing() {
    let outside = [
        [-2.0, 0.0, 0.5, 1.0],
        [2.0, 0.0, 0.5, 1.0],
        [0.0, -2.0, 0.5, 1.0],
        [0.0, 2.0, 0.5, 1.0],
        [0.0, 0.0, -0.5, 1.0],
        [0.0, 0.0, 1.5, 1.0],
    ];
    for position in outside {
        let clipped = clip_triangle([
            vertex(position, [0.0; 2]).into(),
            vertex([-0.25, -0.25, 0.5, 1.0], [1.0, 0.0]).into(),
            vertex([0.25, 0.25, 0.5, 1.0], [0.0, 1.0]).into(),
        ]);
        assert_eq!(clipped.len(), 4);
        for vertex in clipped {
            assert!((0..6).all(|plane| vertex.plane_distance(plane) >= -1.0e-12));
        }
    }
    assert!(
        clip_triangle([
            vertex([-0.5, -0.5, -2.0, 1.0], [0.0; 2]).into(),
            vertex([0.5, -0.5, -2.0, 1.0], [0.0; 2]).into(),
            vertex([0.0, 0.5, -2.0, 1.0], [0.0; 2]).into(),
        ])
        .is_empty()
    );
}

#[test]
fn reference_near_plane_keeps_the_visible_piece_and_matches_gpu_coverage() {
    let (frame, mut assets) = fixture(
        vec![
            vertex([-0.75, -0.75, -0.5, 1.0], [0.0, 0.0]),
            vertex([0.75, -0.75, 0.5, 1.0], [1.0, 0.0]),
            vertex([0.0, 0.75, 0.5, 1.0], [0.5, 1.0]),
        ],
        NativeProgram::Sprite,
    );
    let mut pixels = vec![0; 32 * 32];
    frame.render_reference(&mut assets, &mut pixels).unwrap();
    assert_eq!(pixels[16 * 32 + 16], 0xff0000);
    assert_eq!(pixels[26 * 32 + 5], 0);
    assert!(pixels.iter().filter(|pixel| **pixel != 0).count() > 100);
    compare_gpu(&frame, &assets, &pixels);
}

#[test]
fn reference_texture_interpolation_divides_attributes_by_w() {
    let (frame, mut assets) = fixture(
        vec![
            vertex([-1.0, -1.0, 0.5, 1.0], [0.0, 0.0]),
            vertex([4.0, -4.0, 2.0, 4.0], [1.0, 0.0]),
            vertex([-1.0, 1.0, 0.5, 1.0], [0.0, 1.0]),
        ],
        NativeProgram::Sprite,
    );
    assets.textures.get_mut("<reference-test>").unwrap().image =
        RgbaImage::from_fn(256, 1, |x, _| image::Rgba([x as u8, x as u8, x as u8, 255]));
    let mut pixels = vec![0; 32 * 32];
    frame.render_reference(&mut assets, &mut pixels).unwrap();
    // At (8.5,24.5), the second barycentric weight is 8.5/32. Its w=4
    // changes u to (8.5/128)/(1-3*8.5/128), sampling gray 21. Affine UV
    // interpolation instead produces gray 68 and is observably incorrect.
    assert_eq!(pixels[24 * 32 + 8], 0x151515);
    compare_gpu(&frame, &assets, &pixels);
}

#[test]
fn reference_top_left_rule_blends_a_shared_quad_edge_once_and_applies_scissor() {
    let corners = [
        vertex([-1.0, 1.0, 0.5, 1.0], [0.0, 0.0]),
        vertex([1.0, 1.0, 0.5, 1.0], [1.0, 0.0]),
        vertex([-1.0, -1.0, 0.5, 1.0], [0.0, 1.0]),
        vertex([1.0, -1.0, 0.5, 1.0], [1.0, 1.0]),
    ];
    let (mut frame, mut assets) = fixture(
        [0, 1, 2, 2, 1, 3].map(|i| corners[i]).to_vec(),
        NativeProgram::SpriteAlpha,
    );
    frame.uniforms[0].header[0] = 0.5;
    frame.draws[0].scissor = Some([8, 8, 16, 16]);
    let mut pixels = vec![0; 32 * 32];
    frame.render_reference(&mut assets, &mut pixels).unwrap();
    assert_eq!(pixels.iter().filter(|pixel| **pixel != 0).count(), 256);
    assert!(pixels.iter().all(|pixel| *pixel == 0 || *pixel == 0x800000));
    compare_gpu(&frame, &assets, &pixels);
}
