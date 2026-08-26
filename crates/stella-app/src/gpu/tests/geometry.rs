//! Native colored meshes and DrawablePolygon/Dirt submission.

use super::*;
use crate::gpu::frame::append_gpu_dirt_triangles;

#[test]
fn native_color_mesh_reaches_gpu_as_one_triangle_fan_draw() {
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
    let vertices = vec![[10.0, 20.0], [30.0, 20.0], [40.0, 40.0], [5.0, 50.0]];
    let command = RectRenderCommand {
        order: 0,
        red: 0.25,
        green: 0.5,
        blue: 0.75,
        alpha: 0.6,
        left: 5.0,
        top: 20.0,
        right: 40.0,
        bottom: 50.0,
        color_program: ColorProgram::PlainAlpha,
        vertices: Some(vertices.clone()),
        mesh_topology: ColorMeshTopology::TriangleFan,
        clip_rect: None,
    };
    let frame = assets.prepare_gpu_frame(&[], &[], &[command], &[]).unwrap();
    let indices = [0usize, 1, 2, 0, 2, 3];
    assert_eq!(frame.draws.len(), 1);
    assert_eq!(frame.vertices.len(), 6);
    for (vertex, index) in frame.vertices.iter().zip(indices) {
        assert_eq!(vertex.position, vertices[index].map(|value| value as f32));
    }
    assert_eq!(frame.uniforms[0].diffuse, [0.25, 0.5, 0.75, 1.0]);
    assert_eq!(frame.uniforms[0].header[0], 0.6);
    assert_eq!(frame.draws[0].program, NativeProgram::PlainAlpha);
}

#[test]
fn native_triangle_list_reaches_gpu_without_fan_reindexing() {
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
    let vertices = vec![
        [10.0, 10.0],
        [30.0, 10.0],
        [20.0, 20.0],
        [20.0, 20.0],
        [30.0, 30.0],
        [10.0, 30.0],
    ];
    let command = RectRenderCommand {
        order: 0,
        red: 1.0,
        green: 1.0,
        blue: 1.0,
        alpha: 1.0,
        left: 10.0,
        top: 10.0,
        right: 30.0,
        bottom: 30.0,
        color_program: ColorProgram::Plain,
        vertices: Some(vertices.clone()),
        mesh_topology: ColorMeshTopology::TriangleList,
        clip_rect: None,
    };
    let frame = assets.prepare_gpu_frame(&[], &[], &[command], &[]).unwrap();
    assert_eq!(frame.draws.len(), 1);
    assert_eq!(frame.vertices.len(), vertices.len());
    for (actual, expected) in frame.vertices.iter().zip(vertices) {
        assert_eq!(actual.position, expected.map(|value| value as f32));
    }
}

#[test]
fn dirt_mesh_uses_native_physics_uv_scale_and_opaque_pipeline() {
    let mut frame = PreparedFrame::default();
    append_gpu_dirt_triangles(
        &mut frame,
        &[RenderTriangle {
            vertices: [[-1.0, -2.0], [3.0, -2.0], [-1.0, 4.0]],
        }],
        SpriteTransform::from_scale_rotation(100.0, 200.0, 0.5, 2.0, 0.0, 0.25),
        "DIRT_TEXTURE.png".to_owned(),
    );
    assert_eq!(frame.draws.len(), 1);
    assert_eq!(frame.draws[0].program, NativeProgram::Sprite);
    assert_eq!(frame.draw_texture_pair(0).1, "DIRT_TEXTURE.png");
    assert_eq!(frame.uniforms[0].header[0], 1.0);
    assert_eq!(frame.uniforms[0].header[2], 3.0);
    assert_eq!(
        frame
            .vertices
            .iter()
            .map(|vertex| vertex.position)
            .collect::<Vec<_>>(),
        vec![[90.0, 120.0], [130.0, 120.0], [90.0, 360.0]]
    );
    assert_eq!(
        frame
            .vertices
            .iter()
            .map(|vertex| vertex.uv)
            .collect::<Vec<_>>(),
        vec![[-1.0, -2.0], [3.0, -2.0], [-1.0, 4.0]]
    );
}

#[test]
fn dirt_mesh_uses_constructor_time_texture_pointers_after_catalog_shadowing() {
    let mut assets = AssetCatalog {
        root: std::path::PathBuf::new(),
        font_root: std::path::PathBuf::new(),
        regions: HashMap::new(),
        composites: HashMap::new(),
        masked_textures: HashMap::from([
            ("DIRT_BG".to_owned(), "second-bg.pvr".to_owned()),
            ("DIRT_FG".to_owned(), "second-fg.pvr".to_owned()),
        ]),
        fonts: HashMap::new(),
        textures: HashMap::new(),
        system_labels: SystemLabelPool::default(),
    };
    let triangle = vec![RenderTriangle {
        vertices: [[0.0, 0.0], [1.0, 0.0], [0.0, 1.0]],
    }];
    let dirt = DirtRenderCommand {
        background_texture: "DIRT_BG".to_owned(),
        foreground_texture: "DIRT_FG".to_owned(),
        background_texture_binding: MaskedTextureBinding::Source("first-bg.pvr".to_owned()),
        foreground_texture_binding: MaskedTextureBinding::Source("first-fg.pvr".to_owned()),
        background_triangles: vec![triangle.clone()],
        foreground_triangles: vec![triangle],
    };
    let command = RenderCommand {
        order: 0,
        sprite: "DIRT".to_owned(),
        texture: None,
        texture_scale: 1.0,
        masked_texture_binding: None,
        bound_region: None,
        bound_composite: None,
        shader: None,
        clip_holes: Vec::new(),
        dirt: Some(dirt),
        x: 0.0,
        y: 0.0,
        state: stella_script::RenderState::default(),
        world_space: true,
    };
    let frame = assets.prepare_gpu_frame(&[command], &[], &[], &[]).unwrap();
    assert_eq!(frame.draws.len(), 2);
    assert_eq!(frame.draw_texture_pair(0).1, "first-bg.pvr");
    assert_eq!(frame.draw_texture_pair(1).1, "first-fg.pvr");
}
