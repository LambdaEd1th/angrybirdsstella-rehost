use super::*;

#[test]
fn parses_animation_skin_sprite_transform() {
    let document = serde_json::json!({
        "default": {
            "PIG_NORMAL_BODY": {
                "PIG_NORMAL_BODY": {
                    "x": -8.16,
                    "y": -43.63,
                    "scaleX": 0.75,
                    "scaleY": 1.25,
                    "rotation": 0.5
                }
            }
        }
    });
    let skins = parse_animation_skins(&document);
    let transform = &skins["default"]["PIG_NORMAL_BODY"]["PIG_NORMAL_BODY"];
    assert_eq!(transform.sprite, "PIG_NORMAL_BODY");
    assert_eq!(transform.x, f64::from(-8.16_f32));
    assert_eq!(transform.y, f64::from(-43.63_f32));
    assert_eq!(transform.scale_x, 0.75);
    assert_eq!(transform.scale_y, 1.25);
    assert_eq!(transform.angle, f64::from(0.5_f32));
}

#[test]
fn animation_skin_rotation_uses_native_inverse_attachment_basis() {
    let mut definition = AnimationDefinition {
        parents: BTreeMap::from([("SLOT_TEST".to_owned(), "root".to_owned())]),
        slots: vec!["SLOT_TEST".to_owned()],
        ..AnimationDefinition::default()
    };
    let mut action = AnimationAction::default();
    let slot = action.targets.entry("SLOT_TEST".to_owned()).or_default();
    slot.sprite.push((0.0, "TEST_SPRITE".to_owned()));
    definition.actions.insert("idle".to_owned(), action);
    definition.skins.insert(
        "default".to_owned(),
        BTreeMap::from([(
            "TEST".to_owned(),
            BTreeMap::from([(
                "TEST_SPRITE".to_owned(),
                AnimationSkinTransform {
                    sprite: "TEST_SPRITE".to_owned(),
                    x: 0.0,
                    y: 0.0,
                    scale_x: 2.0,
                    scale_y: 3.0,
                    angle: std::f64::consts::FRAC_PI_2,
                },
            )]),
        )]),
    );
    let mut animation = AnimationRuntime::default();
    animation.definitions.insert("test".to_owned(), definition);
    animation
        .skins
        .insert("test".to_owned(), "default".to_owned());
    animation.playback.insert(
        "test".to_owned(),
        AnimationPlayback::active("idle".to_owned(), "repeat".to_owned(), 0.0, 1.0, 1.0),
    );
    bind_test_animation_sprites(&mut animation, "test", &["TEST_SPRITE"]);

    let matrix = animation_render_commands(&animation, "test")[0]
        .state
        .matrix
        .unwrap();
    // sub_100011BC4 calls sinf(-rotation): a positive skin angle therefore
    // produces the opposite basis from an ordinary positive rotation track.
    assert!(matrix[0].abs() < 2.0e-7);
    assert!((matrix[1] - 3.0).abs() < 2.0e-7);
    assert!((matrix[2] + 2.0).abs() < 2.0e-7);
    assert!(matrix[3].abs() < 3.0e-7);
}

#[test]
fn animation_skin_resolves_namespaced_track_alias_before_basename() {
    let document = serde_json::json!({
        "default": {
            "BORDER_DOWN": {
                "borders_chapter_2_end/CHAPTER2_PAGE1_PANEL1_DOWN": {
                    "name": "CHAPTER2_PAGE1_PANEL1_DOWN",
                    "x": -12.76,
                    "y": 526.61
                }
            }
        }
    });
    let mut definition = AnimationDefinition {
        slots: vec!["SLOT_BORDER_DOWN".to_owned()],
        skins: parse_animation_skins(&document),
        ..AnimationDefinition::default()
    };
    let mut action = AnimationAction::default();
    action
        .targets
        .entry("SLOT_BORDER_DOWN".to_owned())
        .or_default()
        .sprite
        .push((
            0.0,
            "borders_chapter_2_end/CHAPTER2_PAGE1_PANEL1_DOWN".to_owned(),
        ));
    definition
        .actions
        .insert("Animation".to_owned(), action.clone());
    let playback =
        AnimationPlayback::active("Animation".to_owned(), "repeat".to_owned(), 0.0, 1.0, 1.0);

    let (sprite, transform) =
        animation_slot_attachment(&definition, &playback, Some("default"), "SLOT_BORDER_DOWN")
            .expect("namespaced attachment");
    assert_eq!(sprite, "CHAPTER2_PAGE1_PANEL1_DOWN");
    let transform = transform.expect("skin transform");
    assert_eq!(transform.x, f64::from(-12.76_f32));
    assert_eq!(transform.y, f64::from(526.61_f32));
}

#[test]
fn shipped_leaves_use_inverse_skin_rotation_and_native_layer_order() {
    let data_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../runtime/data");
    let asset = animation_asset(&data_root, "animations/LEAVES.anim.json");
    let sheet = stella_assets::ka3d::SpriteSheet::parse(
        &fs::read(data_root.join("images/1024x768/MENU_ELEMENTS_1.dat")).unwrap(),
    )
    .unwrap();
    let duration = asset.actions["Transition_Animation"];
    let native_duration = f64::from(duration as f32);
    assert_eq!(native_duration, f64::from(0.8_f32));

    let mut animation = AnimationRuntime::default();
    animation.actions.insert("leaves".to_owned(), asset.actions);
    animation
        .definitions
        .insert("leaves".to_owned(), asset.definition);
    animation
        .skins
        .insert("leaves".to_owned(), "default".to_owned());
    animation.sprite_regions.insert(
        "leaves".to_owned(),
        sheet
            .sprites
            .iter()
            .filter(|sprite| sprite.name.starts_with("TRANSITION_LEAF_"))
            .map(|sprite| {
                (
                    sprite.name.clone(),
                    SpriteCatalogRegion {
                        native_sheet_id: 1,
                        texture_source: sheet.texture_for(sprite).unwrap_or_default().to_owned(),
                        sprite: sprite.clone(),
                    },
                )
            })
            .collect(),
    );
    animation.playback.insert(
        "leaves".to_owned(),
        AnimationPlayback::active(
            "Transition_Animation".to_owned(),
            "once".to_owned(),
            native_duration,
            native_duration,
            1.5,
        ),
    );

    let leaves = animation_render_commands(&animation, "leaves")
        .into_iter()
        .filter(|command| command.sprite.starts_with("TRANSITION_LEAF_"))
        .collect::<Vec<_>>();
    assert_eq!(leaves.len(), 16);
    assert!(
        leaves
            .iter()
            .all(|command| command.state.sprite_pivot.is_none()),
        "SpriteComponentCustom must retain its native SPRT-pivot anchor"
    );
    for command in &leaves {
        let region = &command
            .bound_region
            .as_ref()
            .expect("LEAVES command retains its atlas region")
            .sprite;
        let expected = match command.sprite.as_str() {
            "TRANSITION_LEAF_1" => (439, 272, 220, 136),
            "TRANSITION_LEAF_2" => (371, 327, 186, 163),
            "TRANSITION_LEAF_3" => (315, 254, 158, 126),
            sprite => panic!("unexpected LEAVES sprite {sprite}"),
        };
        assert_eq!(
            (region.width, region.height, region.pivot_x, region.pivot_y),
            expected
        );
    }
    assert_eq!(
        leaves
            .iter()
            .map(|command| command.sprite.as_str())
            .collect::<Vec<_>>(),
        [
            "TRANSITION_LEAF_2",
            "TRANSITION_LEAF_1",
            "TRANSITION_LEAF_1",
            "TRANSITION_LEAF_2",
            "TRANSITION_LEAF_2",
            "TRANSITION_LEAF_2",
            "TRANSITION_LEAF_2",
            "TRANSITION_LEAF_3",
            "TRANSITION_LEAF_1",
            "TRANSITION_LEAF_1",
            "TRANSITION_LEAF_1",
            "TRANSITION_LEAF_3",
            "TRANSITION_LEAF_2",
            "TRANSITION_LEAF_2",
            "TRANSITION_LEAF_3",
            "TRANSITION_LEAF_3",
        ]
    );

    // Settled LEAF_1 is the z=6 entry at this authored location.
    // Its default-skin scaleX is negative and its attachment rotation is
    // -0.6332055. Purple applies sinf(-rotation), making both first-column
    // entries negative. Using the ordinary bone rotation basis makes m10
    // positive and leaves a visible hole in the covered transition frame.
    let leaf_1 = &leaves[8];
    assert_eq!(leaf_1.sprite, "TRANSITION_LEAF_1");
    assert_eq!((leaf_1.x as f32).to_bits(), 0x4411_6c0d);
    assert_eq!((leaf_1.y as f32).to_bits(), 0x437d_b065);
    let matrix = leaf_1.state.matrix.expect("leaf affine matrix");
    assert!(matrix[0] < 0.0);
    assert!(matrix[1] < 0.0);
    assert!(matrix[2] < 0.0);
    assert!(matrix[3] > 0.0);
}

#[test]
fn shipped_leaves_follow_native_recursive_world_matrices_during_both_phases() {
    let data_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../runtime/data");
    let asset = animation_asset(&data_root, "animations/LEAVES.anim.json");
    let sheet = stella_assets::ka3d::SpriteSheet::parse(
        &fs::read(data_root.join("images/1024x768/MENU_ELEMENTS_1.dat")).unwrap(),
    )
    .unwrap();
    let mut animation = AnimationRuntime::default();
    animation.actions.insert("leaves".to_owned(), asset.actions);
    animation
        .definitions
        .insert("leaves".to_owned(), asset.definition);
    animation
        .skins
        .insert("leaves".to_owned(), "default".to_owned());
    animation.sprite_regions.insert(
        "leaves".to_owned(),
        sheet
            .sprites
            .iter()
            .filter(|sprite| sprite.name.starts_with("TRANSITION_LEAF_"))
            .map(|sprite| {
                (
                    sprite.name.clone(),
                    SpriteCatalogRegion {
                        native_sheet_id: 1,
                        texture_source: sheet.texture_for(sprite).unwrap_or_default().to_owned(),
                        sprite: sprite.clone(),
                    },
                )
            })
            .collect(),
    );
    animation.matrices.insert(
        "leaves".to_owned(),
        AnimationAffine {
            m00: 1.0,
            m01: 0.0,
            m10: 0.0,
            m11: 1.0,
            x: 512.0,
            y: 384.0,
        },
    );

    for (action, expected) in [
        (
            "Transition_Animation",
            [
                (
                    0_usize,
                    0x4482_56c9,
                    0x4459_e519,
                    [0x3fbb_fd76, 0xbec6_893d, 0x3ec6_893e, 0x3fbb_fd76],
                ),
                (
                    8,
                    0x4491_cf0b,
                    0x441a_f482,
                    [0xbf5c_6669, 0xbf5d_a627, 0xbf5d_4949, 0x3f5c_c2e8],
                ),
                (
                    15,
                    0x445b_240f,
                    0x4437_e806,
                    [0xbfb8_4539, 0x3b7d_86b5, 0xbb06_f5a9, 0xbfc9_e655],
                ),
            ],
        ),
        (
            "Transition_Animation_Backwards",
            [
                (
                    0_usize,
                    0x4495_5a8b,
                    0x4482_f01c,
                    [0x3fbb_fd76, 0xbec6_893d, 0x3ec6_893e, 0x3fbb_fd76],
                ),
                (
                    8,
                    0x4498_8b9a,
                    0x441a_e177,
                    [0xbf5b_d84e, 0xbf5e_3390, 0xbf5d_d677, 0x3f5c_3491],
                ),
                (
                    15,
                    0x4471_76e3,
                    0x445a_ad95,
                    [0xbfbe_6f99, 0x3c76_adeb, 0xbc62_accb, 0xbfc5_fcfe],
                ),
            ],
        ),
    ] {
        animation.playback.insert(
            "leaves".to_owned(),
            AnimationPlayback::active(action.to_owned(), "once".to_owned(), 0.4, 0.8, 1.5),
        );
        let leaves = animation_render_commands(&animation, "leaves")
            .into_iter()
            .filter(|command| command.sprite.starts_with("TRANSITION_LEAF_"))
            .collect::<Vec<_>>();
        assert_eq!(leaves.len(), 16);
        for (index, expected_x, expected_y, expected_matrix) in expected {
            let command = &leaves[index];
            assert_eq!((command.x as f32).to_bits(), expected_x, "{action} x");
            assert_eq!((command.y as f32).to_bits(), expected_y, "{action} y");
            assert_eq!(
                command
                    .state
                    .matrix
                    .expect("LEAVES command matrix")
                    .map(|value| (value as f32).to_bits()),
                expected_matrix,
                "{action} matrix"
            );
        }
    }
}

#[test]
fn shipped_leaves_bind_skin_at_apply_and_retain_pointer_until_the_next_apply() {
    let data_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../runtime/data");
    let runtime = StellaLua::new(data_root).unwrap();
    runtime
        .execute_source(
            r#"
                AnimationWrapperNative.loadFromBundle(
                    "leaves", "animations/LEAVES.anim.json"
                )
                AnimationWrapperNative.start(
                    "leaves", "Transition_Animation", "once"
                )
                AnimationWrapperNative.draw("leaves")
            "#,
        )
        .unwrap();
    assert!(
        runtime
            .take_render_commands()
            .iter()
            .all(|command| !command.sprite.starts_with("TRANSITION_LEAF_")),
        "animation JSON loading must not resolve DiscreteString skin aliases"
    );

    runtime
        .execute_source(
            r#"
                res.createSpriteSheet("images/1024x768/MENU_ELEMENTS_1.dat")
                AnimationWrapperNative.seek("leaves", 0.4)
                AnimationWrapperNative.draw("leaves")
            "#,
        )
        .unwrap();
    assert_eq!(
        runtime
            .take_render_commands()
            .iter()
            .filter(|command| command.sprite.starts_with("TRANSITION_LEAF_"))
            .count(),
        16,
        "forced target application resolves the current skin through live resources"
    );

    runtime
        .execute_source(
            r#"
                res.releaseSpriteSheet(
                    "images/1024x768/MENU_ELEMENTS_1.dat", false
                )
                AnimationWrapperNative.draw("leaves")
            "#,
        )
        .unwrap();
    assert_eq!(
        runtime
            .take_render_commands()
            .iter()
            .filter(|command| command.sprite.starts_with("TRANSITION_LEAF_"))
            .count(),
        16,
        "the SpriteComponent retains its concrete pointer until a setter runs"
    );

    runtime
        .execute_source(
            r#"
                AnimationWrapperNative.seek("leaves", 0.4)
                AnimationWrapperNative.draw("leaves")
            "#,
        )
        .unwrap();
    assert!(
        runtime
            .take_render_commands()
            .iter()
            .all(|command| !command.sprite.starts_with("TRANSITION_LEAF_")),
        "the next forced ApplyHandler pass stores null after resource release"
    );
}

#[test]
fn missing_skin_clears_selection_but_rebinds_default_only_on_next_apply() {
    let unique = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let root = std::env::temp_dir().join(format!(
        "stella-animation-skin-lifecycle-{}-{unique}",
        std::process::id()
    ));
    fs::create_dir_all(root.join("animations")).unwrap();
    fs::create_dir_all(root.join("images")).unwrap();
    fs::write(
        root.join("images/SHEET.dat"),
        test_textured_sprite_sheet_with_names(
            "skin-lifecycle.pvr",
            &[("DEFAULT_SPRITE", 10, 20), ("COSTUME_SPRITE", 30, 40)],
        ),
    )
    .unwrap();
    fs::write(root.join("images/skin-lifecycle.pvr"), []).unwrap();

    let targets = serde_json::json!({
        "SLOT_TEST": {
            "sprite": {
                "type": "DiscreteString",
                "keyframes": [[0, "ATTACHMENT"]]
            }
        }
    });
    let document = serde_json::json!({
        "children": [{
            "name": "SLOT_TEST",
            "comps": [{"type": "game::SpriteComponentCustom"}]
        }],
        "comps": [{
            "type": "game::Animation",
            "data": {"actions": {"idle": {"clips": {"": {"targets": targets}}}}}
        }]
    });
    fs::write(
        root.join("animations/test.anim.json"),
        serde_json::to_vec(&document).unwrap(),
    )
    .unwrap();
    let skins = serde_json::json!({
        "default": {
            "TEST": {
                "ATTACHMENT": {"name": "DEFAULT_SPRITE"}
            }
        },
        "Costume": {
            "TEST": {
                "ATTACHMENT": {"name": "COSTUME_SPRITE"}
            }
        }
    });
    fs::write(
        root.join("animations/test.skins.json"),
        serde_json::to_vec(&skins).unwrap(),
    )
    .unwrap();

    let runtime = StellaLua::new(&root).unwrap();
    let draw_sprite = |runtime: &StellaLua| {
        runtime
            .execute_source("AnimationWrapperNative.draw('scene')")
            .unwrap();
        runtime
            .take_render_commands()
            .into_iter()
            .next()
            .map(|command| command.sprite)
    };
    runtime
        .execute_source(
            r#"
                res.createSpriteSheet("images/SHEET.dat")
                AnimationWrapperNative.loadFromBundle(
                    "scene", "animations/test.anim.json"
                )
                AnimationWrapperNative.start("scene", "idle", "repeat")
            "#,
        )
        .unwrap();
    assert_eq!(draw_sprite(&runtime).as_deref(), Some("DEFAULT_SPRITE"));

    runtime
        .execute_source("AnimationWrapperNative.setSkin('scene', 'Costume')")
        .unwrap();
    assert_eq!(
        draw_sprite(&runtime).as_deref(),
        Some("DEFAULT_SPRITE"),
        "setSkin changes only the selected pointer, not an existing component binding"
    );
    runtime
        .execute_source("AnimationWrapperNative.seek('scene', 0)")
        .unwrap();
    assert_eq!(draw_sprite(&runtime).as_deref(), Some("COSTUME_SPRITE"));

    runtime
        .execute_source("AnimationWrapperNative.setSkin('scene', 'missing')")
        .unwrap();
    assert_eq!(
        draw_sprite(&runtime).as_deref(),
        Some("COSTUME_SPRITE"),
        "clearing the current skin pointer does not mutate the bound SpriteComponent"
    );
    runtime
        .execute_source("AnimationWrapperNative.seek('scene', 0)")
        .unwrap();
    assert_eq!(
        draw_sprite(&runtime).as_deref(),
        Some("DEFAULT_SPRITE"),
        "the next ApplyHandler lookup falls back through the default-skin pointer"
    );
}

#[test]
fn shipped_telepod_composite_attachment_leaves_sprite_component_null() {
    let data_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../runtime/data");
    let runtime = StellaLua::new(data_root).unwrap();
    runtime
        .execute_source(
            r#"
                ResourceManager.native_createSpriteSheet(
                    "images/1024x768/MENU_ELEMENTS_1.dat"
                )
                ResourceManager.native_createSpriteSheet(
                    "images/1024x768/MENU_ELEMENTS_2.dat"
                )
                ResourceManager.native_createSpriteSheet(
                    "images/1024x768/MENU_ELEMENTS_3.dat"
                )
                ResourceManager.native_createSpriteSheet(
                    "images/1024x768/MENU_GATE_LEVELS.dat"
                )
                res.createCompoSpriteSet(
                    "images/1024x768/MENU_COMPOSPRITES.dat"
                )
                AnimationWrapperNative.loadFromBundle(
                    "telepod-leaves", "animations/TELEPOD_PAGE_LEAVES.anim.json"
                )
                -- Releasing the set must not matter: SpriteComponentCustom
                -- never accepts its CompoSprite pointer in the first place.
                res.releaseCompoSpriteSet(
                    "images/1024x768/MENU_COMPOSPRITES.dat"
                )
                AnimationWrapperNative.start(
                    "telepod-leaves", "animation", "repeat"
                )
                AnimationWrapperNative.draw("telepod-leaves")
                "#,
        )
        .unwrap();

    let animation = runtime._animation_runtime.lock().unwrap();
    assert!(!animation.sprite_regions["telepod-leaves"].contains_key("GENERAL_UI_BG"));
    assert!(!animation.sprite_metrics["telepod-leaves"].contains_key("GENERAL_UI_BG"));
    drop(animation);
    let commands = runtime.take_render_commands();
    assert!(
        commands
            .iter()
            .all(|command| command.sprite != "GENERAL_UI_BG"),
        "a null SpriteComponent pointer must submit no draw command"
    );
    assert!(
        commands
            .iter()
            .any(|command| command.sprite == "UI_BG_LEAF_1"),
        "ordinary AtlasSprite attachments still draw"
    );
}

#[test]
fn animation_skin_inherits_default_and_hides_unselected_attachments() {
    let mut definition = AnimationDefinition {
        slots: vec!["SLOT_EYES".to_owned(), "SLOT_ATTACHMENT".to_owned()],
        ..AnimationDefinition::default()
    };
    let mut action = AnimationAction::default();
    action
        .targets
        .entry("SLOT_EYES".to_owned())
        .or_default()
        .sprite
        .push((0.0, "EYES_ALIAS".to_owned()));
    action
        .targets
        .entry("SLOT_ATTACHMENT".to_owned())
        .or_default()
        .sprite
        .push((0.0, "COSTUME_ALIAS".to_owned()));
    definition.actions.insert("idle".to_owned(), action);

    let skin_transform = |sprite: &str| AnimationSkinTransform {
        sprite: sprite.to_owned(),
        x: 0.0,
        y: 0.0,
        scale_x: 1.0,
        scale_y: 1.0,
        angle: 0.0,
    };
    let mut default_skin = AnimationSkin::new();
    default_skin
        .entry("EYES".to_owned())
        .or_default()
        .insert("EYES_ALIAS".to_owned(), skin_transform("Eyes_Real"));
    definition.skins.insert("default".to_owned(), default_skin);
    definition
        .skins
        .insert("Normal".to_owned(), AnimationSkin::new());
    let mut costume_skin = AnimationSkin::new();
    costume_skin
        .entry("ATTACHMENT".to_owned())
        .or_default()
        .insert("COSTUME_ALIAS".to_owned(), skin_transform("COSTUME_REAL"));
    definition.skins.insert("Costume".to_owned(), costume_skin);

    let mut animation = AnimationRuntime::default();
    animation.definitions.insert("bird".to_owned(), definition);
    animation
        .skins
        .insert("bird".to_owned(), "Normal".to_owned());
    animation.playback.insert(
        "bird".to_owned(),
        AnimationPlayback::active("idle".to_owned(), "repeat".to_owned(), 0.0, 1.0, 1.0),
    );
    bind_test_animation_sprites(&mut animation, "bird", &["Eyes_Real"]);

    let commands = animation_render_commands(&animation, "bird");
    assert_eq!(commands.len(), 1);
    assert_eq!(commands[0].sprite, "Eyes_Real");
}

#[test]
fn custom_animation_sprite_component_preserves_case_for_native_lookup() {
    let mut definition = AnimationDefinition {
        slots: vec!["SLOT_DIRECT".to_owned()],
        ..AnimationDefinition::default()
    };
    let mut action = AnimationAction::default();
    action
        .targets
        .entry("SLOT_DIRECT".to_owned())
        .or_default()
        .sprite
        .push((0.0, "Mixed_Case_Sprite".to_owned()));
    definition.actions.insert("idle".to_owned(), action);
    let playback = AnimationPlayback::active("idle".to_owned(), "repeat".to_owned(), 0.0, 1.0, 1.0);

    assert_eq!(
        animation_slot_attachment(&definition, &playback, None, "SLOT_DIRECT")
            .map(|(sprite, _)| sprite),
        Some("Mixed_Case_Sprite".to_owned())
    );
}

#[test]
fn selected_skin_preserves_mixed_case_editor_guide_attachment() {
    let mut definition = AnimationDefinition {
        slots: vec!["SLOT_Poppy_idle".to_owned()],
        ..AnimationDefinition::default()
    };
    let mut action = AnimationAction::default();
    action
        .targets
        .entry("SLOT_Poppy_idle".to_owned())
        .or_default()
        .sprite
        .push((0.0, "Poppy_idle".to_owned()));
    definition.actions.insert("idle".to_owned(), action);
    let mut default_skin = AnimationSkin::new();
    default_skin
        .entry("Poppy_idle".to_owned())
        .or_default()
        .insert(
            "Poppy_idle".to_owned(),
            AnimationSkinTransform {
                sprite: "Poppy_idle".to_owned(),
                x: 6.62,
                y: -63.18,
                scale_x: 1.0,
                scale_y: 1.0,
                angle: 0.0,
            },
        );
    definition.skins.insert("default".to_owned(), default_skin);
    definition
        .skins
        .insert("Normal".to_owned(), AnimationSkin::new());
    let playback = AnimationPlayback::active("idle".to_owned(), "repeat".to_owned(), 0.0, 1.0, 1.0);

    let (sprite, _) =
        animation_slot_attachment(&definition, &playback, Some("Normal"), "SLOT_Poppy_idle")
            .expect("the guide remains an unresolved native attachment request");
    assert_eq!(sprite, "Poppy_idle");
    assert_ne!(sprite, "POPPY_IDLE");
}

#[test]
fn animation_z_order_draws_opaque_background_before_foreground_slots() {
    let mut definition = AnimationDefinition {
        slots: vec!["BACKGROUND".to_owned(), "FOREGROUND".to_owned()],
        ..AnimationDefinition::default()
    };
    let mut action = AnimationAction::default();
    let background = action.targets.entry("BACKGROUND".to_owned()).or_default();
    background
        .sprite
        .push((0.0, "OPAQUE_BACKGROUND".to_owned()));
    background.z_order.push((0.0, 14));
    let foreground = action.targets.entry("FOREGROUND".to_owned()).or_default();
    foreground
        .sprite
        .push((0.0, "VISIBLE_FOREGROUND".to_owned()));
    foreground.z_order.push((0.0, 5));
    definition.actions.insert("draw".to_owned(), action);

    let mut animation = AnimationRuntime::default();
    animation.definitions.insert("panel".to_owned(), definition);
    animation.playback.insert(
        "panel".to_owned(),
        AnimationPlayback::active("draw".to_owned(), "repeat".to_owned(), 0.0, 1.0, 1.0),
    );
    bind_test_animation_sprites(
        &mut animation,
        "panel",
        &["OPAQUE_BACKGROUND", "VISIBLE_FOREGROUND"],
    );

    let commands = animation_render_commands(&animation, "panel");
    assert_eq!(commands[0].sprite, "OPAQUE_BACKGROUND");
    assert_eq!(commands[1].sprite, "VISIBLE_FOREGROUND");
}

#[test]
fn animation_sprite_components_append_native_centering_after_atlas_pivot_vertices() {
    let definition = AnimationDefinition {
        slots: vec!["SLOT_PANEL".to_owned()],
        actions: BTreeMap::from([(
            "idle".to_owned(),
            AnimationAction {
                targets: BTreeMap::from([(
                    "SLOT_PANEL".to_owned(),
                    AnimationTarget {
                        sprite: vec![(0.0, "PANEL".to_owned())],
                        ..AnimationTarget::default()
                    },
                )]),
                ..AnimationAction::default()
            },
        )]),
        ..AnimationDefinition::default()
    };
    let mut animation = AnimationRuntime::default();
    animation.definitions.insert("panel".to_owned(), definition);
    animation.playback.insert(
        "panel".to_owned(),
        AnimationPlayback::active("idle".to_owned(), "repeat".to_owned(), 0.0, 1.0, 1.0),
    );
    let geometry = BTreeMap::from([(
        "PANEL".to_owned(),
        SpriteGeometry {
            min_x: -3.0,
            min_y: -7.0,
            max_x: 8.0,
            max_y: 14.0,
        },
    )]);
    animation
        .sprite_geometry
        .insert("panel".to_owned(), geometry);
    animation.sprite_regions.insert(
        "panel".to_owned(),
        BTreeMap::from([(
            "PANEL".to_owned(),
            SpriteCatalogRegion {
                native_sheet_id: 1,
                texture_source: "panel.pvr".to_owned(),
                sprite: stella_assets::ka3d::SpriteRegion {
                    name: "PANEL".to_owned(),
                    x: 0,
                    y: 0,
                    width: 11,
                    height: 21,
                    pivot_x: 3,
                    pivot_y: 7,
                    atlas_rotation: 0,
                },
            },
        )]),
    );

    let command = &animation_render_commands(&animation, "panel")[0];
    // sub_100095A4C appends (pivot - size/2) = (-2.5,-3.5). The atlas
    // pivot remains live in the base SpriteComponent vertices, yielding a
    // final raw rectangle centred on the slot without replacing that pivot.
    assert_eq!((command.x, command.y), (-2.5, -3.5));
    assert_eq!(command.state.sprite_pivot, None);
}

#[test]
fn animation_render_preserves_full_parent_child_affine_matrix() {
    let mut definition = AnimationDefinition {
        parents: BTreeMap::from([("SLOT_TEST".to_owned(), "root".to_owned())]),
        slots: vec!["SLOT_TEST".to_owned()],
        ..AnimationDefinition::default()
    };
    let mut action = AnimationAction::default();
    action
        .targets
        .entry("root".to_owned())
        .or_default()
        .scale
        .push((0.0, [2.0, 3.0]));
    let slot = action.targets.entry("SLOT_TEST".to_owned()).or_default();
    slot.rotation.push((0.0, std::f64::consts::FRAC_PI_4));
    slot.sprite.push((0.0, "TEST_SPRITE".to_owned()));
    definition.actions.insert("idle".to_owned(), action);

    let mut animation = AnimationRuntime::default();
    animation.definitions.insert("test".to_owned(), definition);
    animation.playback.insert(
        "test".to_owned(),
        AnimationPlayback::active("idle".to_owned(), "repeat".to_owned(), 0.0, 1.0, 1.0),
    );
    bind_test_animation_sprites(&mut animation, "test", &["TEST_SPRITE"]);

    let matrix = animation_render_commands(&animation, "test")[0]
        .state
        .matrix
        .unwrap();
    let cosine = (std::f64::consts::FRAC_PI_4 as f32).cos();
    let expected = [
        f64::from(2.0_f32 * cosine),
        f64::from(-2.0_f32 * cosine),
        f64::from(3.0_f32 * cosine),
        f64::from(3.0_f32 * cosine),
    ];
    for (actual, expected) in matrix.into_iter().zip(expected) {
        assert_eq!(actual.to_bits(), expected.to_bits());
    }
    // The two basis columns are not orthogonal, so scale/rotation
    // decomposition would necessarily lose information here.
    assert!((matrix[0] * matrix[1] + matrix[2] * matrix[3]).abs() > 1.0);
}

#[test]
fn animation_entity_queries_match_scene_relative_native_matrices_and_bounds() {
    let mut definition = AnimationDefinition {
        entities: ["root", "JOINT", "SLOT_TEST"]
            .into_iter()
            .map(str::to_owned)
            .collect(),
        parents: BTreeMap::from([
            ("JOINT".to_owned(), "root".to_owned()),
            ("SLOT_TEST".to_owned(), "JOINT".to_owned()),
        ]),
        slots: vec!["SLOT_TEST".to_owned()],
        ..AnimationDefinition::default()
    };
    let mut action = AnimationAction::default();
    let root = action.targets.entry("root".to_owned()).or_default();
    root.translation.push((0.0, [10.0, 20.0]));
    root.scale.push((0.0, [2.0, 3.0]));
    let joint = action.targets.entry("JOINT".to_owned()).or_default();
    joint.translation.push((0.0, [5.0, 7.0]));
    joint.scale.push((0.0, [0.5, 2.0]));
    action
        .targets
        .entry("SLOT_TEST".to_owned())
        .or_default()
        .sprite
        .push((0.0, "TEST_SPRITE".to_owned()));
    definition.actions.insert("idle".to_owned(), action);

    let mut animation = AnimationRuntime::default();
    animation.definitions.insert("test".to_owned(), definition);
    animation.transforms.insert(
        "test".to_owned(),
        AnimationTransform {
            x: 500.0,
            y: 600.0,
            scale_x: 10.0,
            scale_y: 20.0,
            angle: 0.25,
        },
    );
    animation.matrices.insert(
        "test".to_owned(),
        AnimationAffine::from_transform(animation.transforms["test"]),
    );
    animation.playback.insert(
        "test".to_owned(),
        AnimationPlayback::active("idle".to_owned(), "repeat".to_owned(), 0.0, 1.0, 1.0),
    );
    let local = animation_entity_local_transform(&animation, "test", "JOINT").unwrap();
    assert_eq!((local.x, local.y), (5.0, 7.0));
    assert_eq!((local.scale_x, local.scale_y), (0.5, 2.0));

    let world = animation_entity_world_affine(&animation, "test", "SLOT_TEST").unwrap();
    // Wrapper-level transforms are deliberately absent: the native query
    // multiplies inverse(scene) by entity, yielding scene-relative values.
    assert_eq!(
        (world.x, world.y),
        (f64::from(f32::from_bits(0x419f_fffe)), 41.0)
    );
    assert_eq!(
        (world.scale_x(), world.scale_y()),
        (
            f64::from(f32::from_bits(0x3f7f_ffff)),
            f64::from(f32::from_bits(0x40bf_ffff)),
        )
    );
    assert_eq!(world.angle(), f64::from(f32::from_bits(0x31aa_b55d)));
    assert_eq!(
        animation_entity_has_sprite(&animation, "test", "SLOT_TEST"),
        Some(true)
    );

    let geometry = BTreeMap::from([(
        "TEST_SPRITE".to_owned(),
        SpriteGeometry {
            min_x: -20.0,
            min_y: -5.0,
            max_x: 20.0,
            max_y: 5.0,
        },
    )]);
    animation
        .sprite_geometry
        .insert("test".to_owned(), geometry);
    assert_eq!(
        animation_entity_world_bounds(&animation, "test", "SLOT_TEST"),
        [
            f64::from(f32::from_bits(0xb600_0000)),
            f64::from(f32::from_bits(0x4130_0002)),
            f64::from(f32::from_bits(0x421f_fffe)),
            71.0,
        ]
    );
    assert_eq!(
        animation_entity_world_bounds(&animation, "test", "missing"),
        [0.0; 4]
    );
}

#[test]
fn animation_world_bounds_include_selected_skin_attachment_transform() {
    let mut definition = AnimationDefinition {
        entities: ["root", "SLOT_TEST"]
            .into_iter()
            .map(str::to_owned)
            .collect(),
        parents: BTreeMap::from([("SLOT_TEST".to_owned(), "root".to_owned())]),
        slots: vec!["SLOT_TEST".to_owned()],
        ..AnimationDefinition::default()
    };
    let mut action = AnimationAction::default();
    let root = action.targets.entry("root".to_owned()).or_default();
    root.translation.push((0.0, [10.0, 20.0]));
    root.scale.push((0.0, [1.0, 6.0]));
    action
        .targets
        .entry("SLOT_TEST".to_owned())
        .or_default()
        .sprite
        .push((0.0, "TEST_SPRITE".to_owned()));
    definition.actions.insert("idle".to_owned(), action);
    definition.skins.insert(
        "default".to_owned(),
        BTreeMap::from([(
            "TEST".to_owned(),
            BTreeMap::from([(
                "TEST_SPRITE".to_owned(),
                AnimationSkinTransform {
                    sprite: "TEST_SPRITE".to_owned(),
                    x: 100.0,
                    y: -10.0,
                    scale_x: 2.0,
                    scale_y: 0.5,
                    angle: 0.0,
                },
            )]),
        )]),
    );

    let mut animation = AnimationRuntime::default();
    animation.definitions.insert("test".to_owned(), definition);
    animation
        .skins
        .insert("test".to_owned(), "default".to_owned());
    animation.playback.insert(
        "test".to_owned(),
        AnimationPlayback::active("idle".to_owned(), "repeat".to_owned(), 0.0, 1.0, 1.0),
    );
    let geometry = BTreeMap::from([(
        "TEST_SPRITE".to_owned(),
        SpriteGeometry {
            min_x: -20.0,
            min_y: -5.0,
            max_x: 20.0,
            max_y: 5.0,
        },
    )]);
    animation
        .sprite_geometry
        .insert("test".to_owned(), geometry);

    assert_eq!(
        animation_entity_world_bounds(&animation, "test", "SLOT_TEST"),
        [70.0, -55.0, 150.0, -25.0]
    );
}

#[test]
fn animation_native_lifecycle_preserves_void_abi_cache_and_active_scene() {
    let unique = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let root = std::env::temp_dir().join(format!(
        "stella-animation-native-{}-{unique}",
        std::process::id()
    ));
    fs::create_dir_all(root.join("animations")).unwrap();
    fs::create_dir_all(root.join("images")).unwrap();
    fs::write(
        root.join("images/SHEET.dat"),
        test_textured_sprite_sheet("TEST_SPRITE", "sheet.pvr", 40, 10),
    )
    .unwrap();
    fs::write(root.join("images/sheet.pvr"), []).unwrap();
    let targets = serde_json::json!({
        "root": {
            "translation": {"keyframes": [[0, [10, 20]]]},
            "scale": {"keyframes": [[0, [2, 3]]]},
            "rotation": {"keyframes": [[0, 0]]},
            "spineEvent": {"keyframes": [
                [0, "instantIn:::"],
                [0.5, "cameraShake:2::"],
                [0.75, "playAudio:::clip"],
                [1, ""]
            ]}
        },
        "JOINT": {
            "translation": {"keyframes": [[0, [5, 7]]]},
            "scale": {"keyframes": [[0, [0.5, 2]]]},
            "rotation": {"keyframes": [[0, 0]]}
        },
        "SLOT_TEST": {
            "alpha": {"keyframes": [[0, 1]]},
            "sprite": {"keyframes": [[0, "TEST_SPRITE"]]},
            "zOrder": {"keyframes": [[0, 1]]}
        }
    });
    let document = serde_json::json!({
        "children": [{
            "name": "root",
            "children": [{
                "name": "JOINT",
                "children": [{
                    "name": "SLOT_TEST",
                    "comps": [{"type": "game::SpriteComponentCustom"}]
                }]
            }]
        }],
        "comps": [{
            "type": "game::Animation",
            "data": {"actions": {"idle": {"clips": {"": {"targets": targets}}}}}
        }]
    });
    fs::write(
        root.join("animations/test.anim.json"),
        serde_json::to_vec(&document).unwrap(),
    )
    .unwrap();

    let runtime = StellaLua::new(&root).unwrap();
    runtime
        .execute_source(
            r##"
                res.createSpriteSheet("images/SHEET.dat")
                preload_count = select("#", AnimationWrapperNative.preloadFromBundle(
                    "animations/test.anim.json"
                ))
                load_count = select("#", AnimationWrapperNative.loadFromBundle(
                    "scene", "animations/test.anim.json"
                ))
                prestart_local_x, prestart_local_y =
                    AnimationWrapperNative.getEntityPosition("scene", "JOINT")
                prestart_transform_count = select("#",
                    AnimationWrapperNative.getEntityWorldTransform("scene", "SLOT_TEST")
                )
                _, _, _, _, _, prestart_has_sprite =
                    AnimationWrapperNative.getEntityWorldTransform("scene", "SLOT_TEST")
                prestart_is_playing = AnimationWrapperNative.isPlaying("scene")
                AnimationWrapperNative.setSpeed("scene", 0.25)
                AnimationWrapperNative.seek("scene", 0.75)
                AnimationWrapperNative.pause("scene")
                AnimationWrapperNative.resume("scene")
                post_prestart_local_x, post_prestart_local_y =
                    AnimationWrapperNative.getEntityPosition("scene", "JOINT")
                timeline_events = {}
                AnimationWrapperNative.setPlaybackEvent("scene", function(
                    tag, action, event, integer, number, text
                )
                    table.insert(timeline_events, {
                        tag = tag,
                        action = action,
                        event = event,
                        integer = integer,
                        number = number,
                        text = text
                    })
                end)
                start_count = select("#", AnimationWrapperNative.start(
                    "scene", "idle", "once"
                ))
                AnimationWrapperNative.update(0.6)
                AnimationWrapperNative.update(0.4)
                timeline_event_count = #timeline_events
                AnimationWrapperNative.start("scene", "idle", "repeat")
                speed_count = select("#", AnimationWrapperNative.setSpeed(
                    "scene", 0.123456789
                ))
                seek_count = select("#", AnimationWrapperNative.seek(
                    "scene", 0.987654321
                ))
                bad_speed_type = pcall(
                    AnimationWrapperNative.setSpeed, "scene", "0.5"
                )
                bad_seek_slot = pcall(
                    AnimationWrapperNative.seek, "scene", "bad", 0.5
                )
                bad_seek_tag = pcall(
                    AnimationWrapperNative.seek, {}, 0.5
                )
                skin_count = select("#", AnimationWrapperNative.setSkin(
                    "scene", "missing"
                ))
                AnimationWrapperNative.setTranslation(
                    "scene", 500.123456789, 600.987654321
                )
                AnimationWrapperNative.setScale("scene", 10, 20)
                AnimationWrapperNative.setRotation("scene", 0.123456789)
                AnimationWrapperNative.setScale(
                    "scene", 10.123456789, 20.987654321
                )
                AnimationWrapperNative.setTranslation("missing_scene", 1, 2)
                AnimationWrapperNative.setRotation("missing_scene", 3)
                AnimationWrapperNative.setScale("missing_scene", 4, 5)

                local_x, local_y = AnimationWrapperNative.getEntityPosition(
                    "scene", "JOINT"
                )
                world_x, world_y = AnimationWrapperNative.getEntityWorldPosition(
                    "scene", "SLOT_TEST"
                )
                transform_count = select("#",
                    AnimationWrapperNative.getEntityWorldTransform("scene", "SLOT_TEST")
                )
                _, _, _, _, _, has_sprite =
                    AnimationWrapperNative.getEntityWorldTransform("scene", "SLOT_TEST")
                missing_transform_count = select("#",
                    AnimationWrapperNative.getEntityWorldTransform("scene", "missing")
                )
                bounds_count = select("#",
                    AnimationWrapperNative.getEntityWorldBounds("scene", "SLOT_TEST")
                )
                bounds_left, bounds_top, bounds_right, bounds_bottom =
                    AnimationWrapperNative.getEntityWorldBounds("scene", "SLOT_TEST")
                res.releaseSpriteSheet("images/SHEET.dat", false)
                released_left, released_top, released_right, released_bottom =
                    AnimationWrapperNative.getEntityWorldBounds("scene", "SLOT_TEST")
                AnimationWrapperNative.draw("scene")

                AnimationWrapperNative.pause("scene")
                paused_is_playing = AnimationWrapperNative.isPlaying("scene")
                AnimationWrapperNative.resume("scene")
                resumed_is_playing = AnimationWrapperNative.isPlaying("scene")
                AnimationWrapperNative.clearCache()
                survives_clear_cache = AnimationWrapperNative.containsEntity(
                    "scene", "SLOT_TEST"
                )
                stop_count = select("#", AnimationWrapperNative.stop("scene", "idle"))
                stopped_is_playing = AnimationWrapperNative.isPlaying("scene")
                unknown_start_count = select("#", AnimationWrapperNative.start(
                    "scene", "missing", "repeat"
                ))
                unknown_is_playing = AnimationWrapperNative.isPlaying("scene")
                -- Leave the final state at a known, f32-quantized seek value
                -- so the host-side assertion can inspect the native member.
                AnimationWrapperNative.seek("scene", 0.987654321)
                "##,
        )
        .unwrap();

    let environment = game_environment(runtime.lua()).unwrap();
    for name in [
        "preload_count",
        "load_count",
        "start_count",
        "speed_count",
        "seek_count",
        "skin_count",
        "stop_count",
        "unknown_start_count",
    ] {
        assert_eq!(environment.get::<i64>(name).unwrap(), 0, "{name}");
    }
    assert_eq!(
        (
            environment.get::<f64>("prestart_local_x").unwrap(),
            environment.get::<f64>("prestart_local_y").unwrap(),
            environment.get::<f64>("post_prestart_local_x").unwrap(),
            environment.get::<f64>("post_prestart_local_y").unwrap(),
        ),
        (0.0, 0.0, 0.0, 0.0)
    );
    assert_eq!(
        environment.get::<i64>("prestart_transform_count").unwrap(),
        6
    );
    assert!(!environment.get::<bool>("prestart_has_sprite").unwrap());
    assert!(!environment.get::<bool>("prestart_is_playing").unwrap());
    assert_eq!(
        (
            environment.get::<f64>("local_x").unwrap(),
            environment.get::<f64>("local_y").unwrap()
        ),
        (5.0, 7.0)
    );
    assert_eq!(
        (
            environment.get::<f64>("world_x").unwrap(),
            environment.get::<f64>("world_y").unwrap()
        ),
        (f64::from(f32::from_bits(0x41a0_0002)), 41.0)
    );
    assert_eq!(environment.get::<i64>("transform_count").unwrap(), 6);
    assert!(environment.get::<bool>("has_sprite").unwrap());
    assert_eq!(
        environment.get::<i64>("missing_transform_count").unwrap(),
        0
    );
    assert_eq!(environment.get::<i64>("bounds_count").unwrap(), 4);
    for (name, expected) in [
        ("bounds_left", f64::from(f32::from_bits(0x3680_0000))),
        ("bounds_top", 11.0),
        ("bounds_right", f64::from(f32::from_bits(0x4220_0001))),
        ("bounds_bottom", 71.0),
        ("released_left", f64::from(f32::from_bits(0x3680_0000))),
        ("released_top", 11.0),
        ("released_right", f64::from(f32::from_bits(0x4220_0001))),
        ("released_bottom", 71.0),
    ] {
        assert_eq!(environment.get::<f64>(name).unwrap(), expected, "{name}");
    }
    let retained = runtime
        .take_render_commands()
        .into_iter()
        .find(|command| command.sprite == "TEST_SPRITE")
        .expect("released animation component retained its sprite pointer");
    let retained = retained.bound_region.expect("retained atlas region");
    assert_eq!((retained.sprite.width, retained.sprite.height), (40, 10));
    assert!(retained.texture_source.ends_with("images/sheet.pvr"));
    let timeline_events = environment.get::<mlua::Table>("timeline_events").unwrap();
    assert_eq!(environment.get::<i64>("timeline_event_count").unwrap(), 3);
    let first = timeline_events.raw_get::<mlua::Table>(1).unwrap();
    assert_eq!(first.get::<String>("event").unwrap(), "instantIn");
    assert_eq!(first.get::<i32>("integer").unwrap(), 0);
    assert_eq!(first.get::<f64>("number").unwrap(), 0.0);
    assert_eq!(first.get::<String>("text").unwrap(), "");
    let second = timeline_events.raw_get::<mlua::Table>(2).unwrap();
    assert_eq!(second.get::<String>("event").unwrap(), "cameraShake");
    assert_eq!(second.get::<i32>("integer").unwrap(), 2);
    let third = timeline_events.raw_get::<mlua::Table>(3).unwrap();
    assert_eq!(third.get::<String>("event").unwrap(), "PLAYBACK_END");
    assert_eq!(third.get::<f64>("number").unwrap(), 0.0);
    assert_eq!(third.get::<String>("text").unwrap(), "");
    assert!(!environment.get::<bool>("paused_is_playing").unwrap());
    assert!(environment.get::<bool>("resumed_is_playing").unwrap());
    assert!(environment.get::<bool>("survives_clear_cache").unwrap());
    assert!(!environment.get::<bool>("stopped_is_playing").unwrap());
    assert!(!environment.get::<bool>("unknown_is_playing").unwrap());
    assert!(!environment.get::<bool>("bad_speed_type").unwrap());
    assert!(!environment.get::<bool>("bad_seek_slot").unwrap());
    assert!(!environment.get::<bool>("bad_seek_tag").unwrap());
    {
        let animation = runtime
            ._animation_runtime
            .lock()
            .expect("animation runtime lock poisoned");
        assert!(animation.bundle_cache.is_empty());
        assert!(animation.app_data_cache.is_empty());
        assert!(animation.definitions.contains_key("scene"));
        assert!(!animation.transforms.contains_key("missing_scene"));
        let transform = animation.transforms["scene"];
        assert_eq!(transform.x, f64::from(500.123_44_f32));
        assert_eq!(transform.y, f64::from(600.987_7_f32));
        assert_eq!(transform.scale_x, f64::from(10.123_457_f32));
        assert_eq!(transform.scale_y, f64::from(20.987_654_f32));
        assert_eq!(transform.angle, f64::from(0.123_456_79_f32));
        let matrix = animation.matrices["scene"];
        assert_eq!(matrix.x, transform.x);
        assert_eq!(matrix.y, transform.y);
        assert_eq!(matrix.scale_x(), transform.scale_x);
        assert_eq!(matrix.scale_y(), f64::from(f32::from_bits(0x41a7_e6b8)));
        assert_eq!(matrix.angle(), transform.angle);
        let playback = &animation.playback["scene"];
        let control = playback.current_control().unwrap();
        assert_eq!(control.speed, f64::from(0.123_456_79_f32));
        assert_eq!(control.elapsed, f64::from(0.987_654_3_f32));
    }
    drop(runtime);
    fs::remove_dir_all(root).unwrap();
}
