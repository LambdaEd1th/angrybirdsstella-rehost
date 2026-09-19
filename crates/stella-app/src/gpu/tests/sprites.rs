//! Native/explicit atlas quads and recovered sprite transform behavior.

use super::*;

#[test]
fn near_degenerate_atlas_matrix_is_submitted_without_a_host_epsilon_cull() {
    let texture_name = "<near-degenerate-atlas-test>".to_owned();
    let mut assets = AssetCatalog {
        root: std::path::PathBuf::new(),
        font_root: std::path::PathBuf::new(),
        regions: HashMap::from([(
            "TINY".to_owned(),
            AtlasRegion {
                texture: texture_name.clone(),
                sprite: SpriteRegion {
                    name: "TINY".to_owned(),
                    x: 0,
                    y: 0,
                    width: 1000,
                    height: 1000,
                    pivot_x: 500,
                    pivot_y: 500,
                    atlas_rotation: 0,
                },
            },
        )]),
        composites: HashMap::new(),
        masked_textures: HashMap::new(),
        fonts: HashMap::new(),
        textures: HashMap::from([(texture_name, alpha_texture(1000, 1000))]),
        system_labels: SystemLabelPool::default(),
        captures: Default::default(),
    };
    let command = RenderCommand {
        projection_3d: None,
        order: 0,
        sprite: "TINY".into(),
        texture: None,
        bound_region: None,
        bound_composite: None,
        geometry: None,
        shader: None,
        dirt: None,
        x: 100.0,
        y: 200.0,
        state: stella_script::RenderState {
            // The determinant is 1e-8, below f32::EPSILON, but Purple has no
            // determinant threshold in either caller or Sprite::Draw.
            scale_x: 0.0001,
            scale_y: 0.0001,
            ..stella_script::RenderState::default()
        }
        .into(),
        world_space: true,
    };

    let frame = assets.prepare_gpu_frame(&[command], &[], &[], &[]).unwrap();
    assert_eq!(frame.vertices.len(), 6);
    assert_eq!(frame.draws.len(), 1);
    assert_eq!(frame.vertices[0].position, [99.95, 199.95]);
    assert_eq!(frame.vertices[5].position, [100.05, 200.05]);
}

#[test]
fn native_explicit_quad_reaches_gpu_in_recovered_triangle_and_uv_order() {
    let texture_name = "<explicit-quad-test>".to_owned();
    let active_texture = "<explicit-quad-shadow>".to_owned();
    let mut assets = AssetCatalog {
        root: std::path::PathBuf::new(),
        font_root: std::path::PathBuf::new(),
        regions: HashMap::from([(
            "MASK".to_owned(),
            AtlasRegion {
                texture: active_texture.clone(),
                sprite: SpriteRegion {
                    name: "MASK".to_owned(),
                    x: 7,
                    y: 9,
                    width: 10,
                    height: 20,
                    pivot_x: 4,
                    pivot_y: 5,
                    atlas_rotation: 0,
                },
            },
        )]),
        composites: HashMap::new(),
        masked_textures: HashMap::new(),
        fonts: HashMap::new(),
        textures: HashMap::from([
            (texture_name.clone(), alpha_texture(16, 32)),
            (active_texture, alpha_texture(4, 4)),
        ]),
        system_labels: SystemLabelPool::default(),
        captures: Default::default(),
    };
    let positions = [[90.0, 80.0], [10.0, 70.0], [100.0, 20.0], [20.0, 10.0]];
    let uv = [[0.9, 0.8], [0.1, 0.7], [1.0, 0.2], [0.2, 0.1]];
    let command = RenderCommand {
        projection_3d: None,
        order: 0,
        sprite: "MASK".into(),
        texture: None,
        bound_region: Some(
            SpriteCatalogRegion {
                decoded_image: None,
                native_sheet_id: 1,
                texture_source: texture_name.clone(),
                sprite: SpriteRegion {
                    name: "MASK".to_owned(),
                    x: 7,
                    y: 9,
                    width: 10,
                    height: 20,
                    pivot_x: 4,
                    pivot_y: 5,
                    atlas_rotation: 0,
                },
            }
            .into(),
        ),
        bound_composite: None,
        geometry: Some(SpriteGeometrySubmission::ExplicitQuad(Arc::new(
            RenderQuad { positions, uv },
        ))),
        shader: None,
        dirt: None,
        x: 0.0,
        y: 0.0,
        state: stella_script::RenderState {
            alpha: 0.25,
            ..stella_script::RenderState::default()
        }
        .into(),
        world_space: true,
    };
    let frame = assets.prepare_gpu_frame(&[command], &[], &[], &[]).unwrap();
    let indices = [0usize, 1, 2, 2, 1, 3];
    assert_eq!(frame.vertices.len(), 6);
    for (vertex, index) in frame.vertices.iter().zip(indices) {
        assert_eq!(vertex.position, positions[index].map(|value| value as f32));
        assert_eq!(vertex.uv, uv[index].map(|value| value as f32));
    }
    assert_eq!(frame.uniforms[0].header[0], 0.25);
    assert_eq!(frame.draw_texture_pair(0).0, texture_name);
    assert_eq!(frame.draws[0].program, NativeProgram::SpriteAlpha);
}

#[test]
fn native_atlas_quad_keeps_positions_and_signed_rotated_region_uvs() {
    let texture_name = "<native-atlas-quad-test>".to_owned();
    let active_texture = "<native-atlas-quad-shadow>".to_owned();
    let mut assets = AssetCatalog {
        root: std::path::PathBuf::new(),
        font_root: std::path::PathBuf::new(),
        regions: HashMap::from([(
            "RUBBER".to_owned(),
            AtlasRegion {
                texture: active_texture.clone(),
                sprite: SpriteRegion {
                    name: "RUBBER".to_owned(),
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
        textures: HashMap::from([
            (texture_name.clone(), alpha_texture(20, 40)),
            (active_texture, alpha_texture(4, 4)),
        ]),
        system_labels: SystemLabelPool::default(),
        captures: Default::default(),
    };
    let positions = [[10.0, 16.0], [10.0, 24.0], [30.0, 16.0], [30.0, 24.0]];
    let command = RenderCommand {
        projection_3d: None,
        order: 0,
        sprite: "RUBBER".into(),
        texture: None,
        bound_region: Some(
            SpriteCatalogRegion {
                decoded_image: None,
                native_sheet_id: 1,
                texture_source: texture_name.clone(),
                sprite: SpriteRegion {
                    name: "RUBBER".to_owned(),
                    x: -4,
                    y: -8,
                    width: 10,
                    height: 20,
                    pivot_x: 3,
                    pivot_y: 7,
                    atlas_rotation: 1,
                },
            }
            .into(),
        ),
        bound_composite: None,
        geometry: Some(SpriteGeometrySubmission::NativeAtlasQuad(Arc::new(
            positions,
        ))),
        shader: None,
        dirt: None,
        x: 999.0,
        y: 999.0,
        state: stella_script::RenderState {
            alpha: 0.375,
            ..stella_script::RenderState::default()
        }
        .into(),
        world_space: true,
    };
    let frame = assets.prepare_gpu_frame(&[command], &[], &[], &[]).unwrap();
    let indices = [0usize, 1, 2, 2, 1, 3];
    let uv = [[0.8, -0.2], [0.8, 0.05], [-0.2, -0.2], [-0.2, 0.05]];
    assert_eq!(frame.vertices.len(), 6);
    for (vertex, index) in frame.vertices.iter().zip(indices) {
        assert_eq!(vertex.position, positions[index].map(|value| value as f32));
        assert_eq!(vertex.uv, uv[index]);
    }
    assert_eq!(frame.uniforms[0].header[0], 0.375);
    assert_eq!(frame.draw_texture_pair(0).0, texture_name);
    assert_eq!(frame.draws[0].program, NativeProgram::SpriteAlpha);
}

#[test]
fn raw_atlas_quad_preserves_custom_model_space_and_native_uvs() {
    let texture_name = "<raw-atlas-quad-test>".to_owned();
    let mut assets = AssetCatalog {
        root: std::path::PathBuf::new(),
        font_root: std::path::PathBuf::new(),
        regions: HashMap::new(),
        composites: HashMap::new(),
        masked_textures: HashMap::new(),
        fonts: HashMap::new(),
        textures: HashMap::from([(texture_name.clone(), alpha_texture(20, 40))]),
        system_labels: SystemLabelPool::default(),
        captures: Default::default(),
    };
    let positions = [[-2.0, -3.0], [2.0, -3.0], [-2.0, 3.0], [2.0, 3.0]];
    let projection = TextProjection3D {
        x: 0.25,
        y: -0.5,
        z: 0.001,
        rotation_x: 1.1,
        custom_model: true,
    };
    let sprite = SpriteRegion {
        name: "RAW".to_owned(),
        x: -4,
        y: -8,
        width: 10,
        height: 20,
        pivot_x: 3,
        pivot_y: 7,
        atlas_rotation: 1,
    };
    let uv = sprite.native_uvs(20.0, 40.0);
    let command = RenderCommand {
        projection_3d: Some(Arc::new(projection)),
        order: 0,
        sprite: "RAW".into(),
        texture: None,
        bound_region: Some(Arc::new(SpriteCatalogRegion {
            decoded_image: None,
            native_sheet_id: 1,
            texture_source: texture_name,
            sprite,
        })),
        bound_composite: None,
        geometry: Some(SpriteGeometrySubmission::RawAtlasQuad(Arc::new(positions))),
        shader: None,
        dirt: None,
        x: 9999.0,
        y: -8888.0,
        state: stella_script::RenderState {
            translate_x: 20.0,
            translate_y: 30.0,
            scale_x: 2.0,
            scale_y: -3.0,
            angle: 0.75,
            pivot_x: 4.0,
            pivot_y: 5.0,
            alpha: 0.375,
            ..stella_script::RenderState::default()
        }
        .into(),
        world_space: true,
    };
    let mut normalized_command = command.clone();
    normalized_command.geometry = Some(SpriteGeometrySubmission::NativeAtlasQuad(Arc::new(
        positions,
    )));
    let frame = assets.prepare_gpu_frame(&[command], &[], &[], &[]).unwrap();
    let normalized = assets
        .prepare_gpu_frame(&[normalized_command], &[], &[], &[])
        .unwrap();
    assert_eq!(frame.vertices.len(), 6);
    assert!(
        frame
            .vertices
            .iter()
            .any(|vertex| vertex.clip_position[3] < 0.0)
    );
    assert!(
        frame
            .vertices
            .iter()
            .any(|vertex| vertex.clip_position[3] > 0.0)
    );
    for ((vertex, normalized), index) in frame
        .vertices
        .iter()
        .zip(&normalized.vertices)
        .zip([0, 1, 2, 2, 1, 3])
    {
        let [px, py] = positions[index].map(|value| value as f32);
        let [x, y, z, w] = native_project_clip(projection, [px, py, 0.0]);
        assert_eq!(vertex.position, [px, py]);
        assert_eq!(vertex.clip_position, [x, y, (z + w) * 0.5, w]);
        assert_eq!(vertex.uv, uv[index]);
        assert_ne!(vertex.clip_position, normalized.clip_position);
    }
    assert_eq!(frame.uniforms[0].header[0], 0.375);
    assert_eq!(frame.draws[0].program, NativeProgram::SpriteAlpha);
}

#[test]
fn render_state_pivot_is_not_applied_twice_after_native_sprite_anchoring() {
    let texture_name = "<pivot-test>".to_owned();
    let mut assets = AssetCatalog {
        root: std::path::PathBuf::new(),
        font_root: std::path::PathBuf::new(),
        regions: HashMap::from([(
            "PIVOT_SPRITE".to_owned(),
            AtlasRegion {
                texture: texture_name.clone(),
                sprite: SpriteRegion {
                    name: "PIVOT_SPRITE".to_owned(),
                    x: 0,
                    y: 0,
                    width: 10,
                    height: 20,
                    pivot_x: 4,
                    pivot_y: 5,
                    atlas_rotation: 0,
                },
            },
        )]),
        composites: HashMap::new(),
        masked_textures: HashMap::new(),
        fonts: HashMap::new(),
        textures: HashMap::from([(texture_name, alpha_texture(10, 20))]),
        system_labels: SystemLabelPool::default(),
        captures: Default::default(),
    };
    let command = RenderCommand {
        projection_3d: None,
        order: 0,
        sprite: "PIVOT_SPRITE".into(),
        texture: None,
        bound_region: None,
        bound_composite: None,
        geometry: None,
        shader: None,
        dirt: None,
        // ResourceManager's AtlasSprite path has already converted HPIVOT /
        // VPIVOT to the native raw rectangle origin.
        x: -4.0,
        y: -5.0,
        state: stella_script::RenderState {
            translate_x: 100.0,
            translate_y: 200.0,
            scale_x: 2.0,
            scale_y: 2.0,
            angle: std::f64::consts::FRAC_PI_2,
            sprite_pivot: Some([0.0, 0.0]),
            pivot_x: 4.0,
            pivot_y: 5.0,
            draw_size: Some([30.0, 40.0]),
            ..stella_script::RenderState::default()
        }
        .into(),
        world_space: false,
    };
    let frame = assets.prepare_gpu_frame(&[command], &[], &[], &[]).unwrap();
    assert_eq!(frame.vertices[0].position, [210.0, 392.0]);
    assert_eq!(frame.vertices[5].position, [130.0, 452.0]);
}

#[test]
fn explicit_sprite_pivot_override_replaces_an_atlas_pivot() {
    let texture_name = "<sprite-pivot-override-test>".to_owned();
    let mut assets = AssetCatalog {
        root: std::path::PathBuf::new(),
        font_root: std::path::PathBuf::new(),
        regions: HashMap::from([(
            "PANEL".to_owned(),
            AtlasRegion {
                texture: texture_name.clone(),
                sprite: SpriteRegion {
                    name: "PANEL".to_owned(),
                    x: 0,
                    y: 0,
                    width: 10,
                    height: 20,
                    pivot_x: 0,
                    pivot_y: 0,
                    atlas_rotation: 0,
                },
            },
        )]),
        composites: HashMap::new(),
        masked_textures: HashMap::new(),
        fonts: HashMap::new(),
        textures: HashMap::from([(texture_name, alpha_texture(10, 20))]),
        system_labels: SystemLabelPool::default(),
        captures: Default::default(),
    };
    let command = RenderCommand {
        projection_3d: None,
        order: 0,
        sprite: "PANEL".into(),
        texture: None,
        bound_region: None,
        bound_composite: None,
        geometry: None,
        shader: None,
        dirt: None,
        x: 100.0,
        y: 200.0,
        state: stella_script::RenderState {
            sprite_pivot: Some([5.0, 10.0]),
            ..stella_script::RenderState::default()
        }
        .into(),
        world_space: true,
    };

    let frame = assets.prepare_gpu_frame(&[command], &[], &[], &[]).unwrap();
    assert_eq!(frame.vertices[0].position, [95.0, 190.0]);
    assert_eq!(frame.vertices[5].position, [105.0, 210.0]);
}

#[test]
fn retained_animation_region_draws_after_active_resource_catalog_release() {
    let texture_name = "<retained-animation-region>".to_owned();
    let mut assets = AssetCatalog {
        root: std::path::PathBuf::new(),
        font_root: std::path::PathBuf::new(),
        regions: HashMap::new(),
        composites: HashMap::new(),
        masked_textures: HashMap::new(),
        fonts: HashMap::new(),
        textures: HashMap::from([(texture_name.clone(), alpha_texture(16, 16))]),
        system_labels: SystemLabelPool::default(),
        captures: Default::default(),
    };
    let command = RenderCommand {
        projection_3d: None,
        order: 0,
        sprite: "RETAINED".into(),
        texture: None,
        bound_region: Some(
            SpriteCatalogRegion {
                decoded_image: None,
                native_sheet_id: 1,
                texture_source: texture_name.clone(),
                sprite: SpriteRegion {
                    name: "RETAINED".to_owned(),
                    x: 2,
                    y: 4,
                    width: 8,
                    height: 6,
                    pivot_x: 4,
                    pivot_y: 3,
                    atlas_rotation: 0,
                },
            }
            .into(),
        ),
        bound_composite: None,
        geometry: None,
        shader: None,
        dirt: None,
        x: 20.0,
        y: 30.0,
        state: stella_script::RenderState::default().into(),
        world_space: true,
    };

    let frame = assets.prepare_gpu_frame(&[command], &[], &[], &[]).unwrap();
    assert_eq!(frame.draw_texture_pair(0).0, texture_name);
    assert_eq!(frame.vertices[0].position, [16.0, 27.0]);
    assert_eq!(frame.vertices[5].position, [24.0, 33.0]);
}

#[test]
fn selected_sprite_uses_its_submission_time_mask_texture_pointer() {
    let base_texture = "<selected-base>".to_owned();
    let retained_mask = "<selected-retained-mask>".to_owned();
    let active_mask = "<selected-active-mask>".to_owned();
    let mut assets = AssetCatalog {
        root: std::path::PathBuf::new(),
        font_root: std::path::PathBuf::new(),
        regions: HashMap::new(),
        composites: HashMap::new(),
        masked_textures: HashMap::from([("MASK".to_owned(), active_mask.clone())]),
        fonts: HashMap::new(),
        textures: HashMap::from([
            (base_texture.clone(), alpha_texture(16, 16)),
            (retained_mask.clone(), alpha_texture(8, 8)),
            (active_mask, alpha_texture(4, 4)),
        ]),
        system_labels: SystemLabelPool::default(),
        captures: Default::default(),
    };
    let command = RenderCommand {
        projection_3d: None,
        order: 0,
        sprite: "SELECTED".into(),
        texture: Some(Arc::new(stella_script::SpriteTextureSubmission {
            name: Arc::from("MASK"),
            scale: 0.25,
            binding: MaskedTextureBinding::Source(retained_mask.clone()),
        })),
        bound_region: Some(
            SpriteCatalogRegion {
                decoded_image: None,
                native_sheet_id: 1,
                texture_source: base_texture.clone(),
                sprite: SpriteRegion {
                    name: "SELECTED".to_owned(),
                    x: 0,
                    y: 0,
                    width: 8,
                    height: 6,
                    pivot_x: 4,
                    pivot_y: 3,
                    atlas_rotation: 0,
                },
            }
            .into(),
        ),
        bound_composite: None,
        geometry: None,
        shader: None,
        dirt: None,
        x: 20.0,
        y: 30.0,
        state: stella_script::RenderState {
            masked_texture_matrix: Some([100.0, 200.0, 2.0, 0.0, 0.0, 3.0]),
            ..stella_script::RenderState::default()
        }
        .into(),
        world_space: true,
    };

    let frame = assets.prepare_gpu_frame(&[command], &[], &[], &[]).unwrap();
    assert_eq!(frame.draw_texture_pair(0).0, base_texture);
    assert_eq!(frame.draw_texture_pair(0).1, retained_mask);
    assert_eq!(frame.draws[0].program, NativeProgram::SpriteAlphaMasked);
    assert_eq!(frame.uniforms[0].header[1], 0.25);
    assert_eq!(frame.vertices[0].source, [92.0, 191.0]);
    assert_eq!(frame.vertices[1].source, [108.0, 191.0]);
    assert_eq!(frame.vertices[2].source, [92.0, 209.0]);
    assert_eq!(frame.vertices[5].source, [108.0, 209.0]);
}

#[test]
fn retained_scene_composite_draws_its_frozen_child_after_catalog_release() {
    let texture_name = "<retained-scene-composite>".to_owned();
    let mut assets = AssetCatalog {
        root: std::path::PathBuf::new(),
        font_root: std::path::PathBuf::new(),
        regions: HashMap::new(),
        composites: HashMap::new(),
        masked_textures: HashMap::new(),
        fonts: HashMap::new(),
        textures: HashMap::from([(texture_name.clone(), alpha_texture(16, 16))]),
        system_labels: SystemLabelPool::default(),
        captures: Default::default(),
    };
    let command = RenderCommand {
        projection_3d: None,
        order: 0,
        sprite: "RETAINED_COMPOSITE".into(),
        texture: None,
        bound_region: None,
        bound_composite: Some(
            vec![BoundCompositePart {
                sprite: "RELEASED_CHILD".into(),
                part: CompositePart {
                    sprite: "RELEASED_CHILD".to_owned(),
                    x: 5.0,
                    y: 7.0,
                    scale_x: 1.0,
                    scale_y: 1.0,
                    flip_x: 1.0,
                    flip_y: 1.0,
                    angle: 0.0,
                    visible: true,
                },
                region: Arc::new(SpriteCatalogRegion {
                    decoded_image: None,
                    native_sheet_id: 1,
                    texture_source: texture_name.clone(),
                    sprite: SpriteRegion {
                        name: "RELEASED_CHILD".to_owned(),
                        x: 2,
                        y: 4,
                        width: 8,
                        height: 6,
                        pivot_x: 4,
                        pivot_y: 3,
                        atlas_rotation: 0,
                    },
                }),
            }]
            .into(),
        ),
        geometry: None,
        shader: None,
        dirt: None,
        x: 20.0,
        y: 30.0,
        state: stella_script::RenderState::default().into(),
        world_space: true,
    };

    let frame = assets.prepare_gpu_frame(&[command], &[], &[], &[]).unwrap();
    assert_eq!(frame.draw_texture_pair(0).0, texture_name);
    assert_eq!(frame.vertices[0].position, [21.0, 34.0]);
    assert_eq!(frame.vertices[5].position, [29.0, 40.0]);
}

#[test]
fn rotated_native_pivot_and_non_uniform_scale_reach_gpu_vertices_exactly() {
    let texture_name = "<rotated-pivot-test>".to_owned();
    let mut assets = AssetCatalog {
        root: std::path::PathBuf::new(),
        font_root: std::path::PathBuf::new(),
        regions: HashMap::from([(
            "ROTATED_PIVOT_SPRITE".to_owned(),
            AtlasRegion {
                texture: texture_name.clone(),
                sprite: SpriteRegion {
                    name: "ROTATED_PIVOT_SPRITE".to_owned(),
                    x: 0,
                    y: 0,
                    width: 10,
                    height: 20,
                    pivot_x: 4,
                    pivot_y: 5,
                    atlas_rotation: 0,
                },
            },
        )]),
        composites: HashMap::new(),
        masked_textures: HashMap::new(),
        fonts: HashMap::new(),
        textures: HashMap::from([(texture_name, alpha_texture(10, 20))]),
        system_labels: SystemLabelPool::default(),
        captures: Default::default(),
    };
    let command = RenderCommand {
        projection_3d: None,
        order: 0,
        sprite: "ROTATED_PIVOT_SPRITE".into(),
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
    let frame = assets.prepare_gpu_frame(&[command], &[], &[], &[]).unwrap();
    assert_eq!(frame.vertices[0].position, [108.0, 171.0]);
    assert_eq!(frame.vertices[5].position, [68.0, 201.0]);
}

#[test]
fn downloaded_avatar_retains_distinct_file_generations_through_gpu_submission() {
    let root = std::env::temp_dir().join(format!(
        "stella-avatar-gpu-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&root).unwrap();
    let path = root.join("opaque");
    let mut commands = Vec::new();
    for (index, color) in [[255, 0, 0, 255], [0, 0, 255, 255]].into_iter().enumerate() {
        RgbaImage::from_pixel(17, 13, image::Rgba(color))
            .save_with_format(&path, image::ImageFormat::Png)
            .unwrap();
        let decoded =
            stella_assets::native_image::decode_native_image(&std::fs::read(&path).unwrap(), None)
                .unwrap();
        let source = stella_assets::image_source::sheet_image_source(
            index as u64 + 1,
            0,
            path.to_str().unwrap(),
        );
        commands.push(RenderCommand {
            projection_3d: None,
            order: index as u64,
            sprite: "AVATAR".into(),
            texture: None,
            bound_region: Some(Arc::new(SpriteCatalogRegion {
                decoded_image: Some(Arc::new(decoded)),
                native_sheet_id: index as u64 + 1,
                texture_source: source,
                sprite: SpriteRegion {
                    name: "AVATAR".to_owned(),
                    x: 0,
                    y: 0,
                    width: 17,
                    height: 13,
                    pivot_x: 8,
                    pivot_y: 6,
                    atlas_rotation: 0,
                },
            })),
            bound_composite: None,
            geometry: None,
            shader: None,
            dirt: None,
            x: 8.0 + index as f32 * 20.0,
            y: 6.0,
            state: stella_script::RenderState::default().into(),
            world_space: true,
        });
    }
    std::fs::remove_dir_all(root).unwrap();
    let mut assets = AssetCatalog {
        root: Default::default(),
        font_root: Default::default(),
        regions: HashMap::new(),
        composites: HashMap::new(),
        masked_textures: HashMap::new(),
        fonts: HashMap::new(),
        textures: HashMap::new(),
        system_labels: SystemLabelPool::default(),
        captures: Default::default(),
    };
    let size = GameResolution {
        width: 40,
        height: 16,
    };
    let frame = assets
        .prepare_gpu_frame_at_resolution(size, &commands, &[], &[], &[])
        .unwrap();
    assert_ne!(frame.draw_texture_pair(0).0, frame.draw_texture_pair(1).0);
    let mut renderer = GpuRenderer::headless(size).unwrap();
    let pixels = renderer
        .render_to_rgba(&assets, &frame, [0, 255, 0])
        .unwrap();
    let pixel = |x: usize, y: usize| &pixels[(y * 40 + x) * 4..(y * 40 + x) * 4 + 4];
    assert_eq!(pixel(4, 4), [255, 0, 0, 255]);
    assert_eq!(pixel(24, 4), [0, 0, 255, 255]);
    assert_eq!(pixel(18, 4), [0, 255, 0, 255]);
}
