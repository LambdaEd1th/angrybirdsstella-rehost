use super::super::*;

#[test]
fn theme_sprite_bindings_preserve_native_layered_abi_and_update_fields() {
    let runtime = StellaLua::new("/tmp").unwrap();
    runtime
        .execute_source(
            r#"
                createThemeSprite("shared", "BG_OLD", 10, 20, 2, 3, 0.5, 0, 4, true, 5, -6)
                createThemeSprite("shared", "FG_OLD", 30, 40, 1, 1, 0.25, 1, 0, false, 0, 0)
                modifyThemeSprite("shared", 100, 200, 1.5, 1.6, 0.7, 0)
                setThemeSprite("shared", "BG_NEW", 0)
                removeThemeSprite("shared", 1)
                "#,
        )
        .unwrap();

    {
        let mut bridge = runtime.render.lock().unwrap();
        bridge.theme_background_layers.push(ThemeLayer {
            sprite: "LAYER".to_owned(),
            geometry: SpriteGeometry {
                min_x: 0.0,
                min_y: 0.0,
                max_x: 1.0,
                max_y: 1.0,
            },
            animation_frames: vec!["LAYER".to_owned()],
            animation_geometries: vec![SpriteGeometry {
                min_x: 0.0,
                min_y: 0.0,
                max_x: 1.0,
                max_y: 1.0,
            }],
            animation_delay: 0.0,
            animation_timeline: Vec::new(),
            animation_timeline_definition: Vec::new(),
            animation_timer: 0.0,
            animation_frame: 0,
            definition_index: 1,
            particles: None,
            spawn_interval: -1.0,
            spawner_id: 0,
            spawn_parameters: None,
            position_x: 0.0,
            position_y: 0.0,
            offset_x: 0.0,
            offset_y: ThemeVerticalOffset::Pixels(0.0),
            resolved_offset_y: None,
            scale_x: 1.0,
            scale_y: 1.0,
            parallax_speed: 1.0,
            z_distance: 0.0,
            scale_speed: 1.0,
            angle_multiplier: 0.0,
            x_multiplier: 0.0,
            y_multiplier: 1.0,
            alpha: 1.0,
            min_alpha: 1.0,
            max_alpha: 1.0,
            repeat_x: false,
            repeat_y: false,
            repeat_left_only: false,
            repeat_right_only: false,
            native_flags: 0,
            relative_x: None,
            relative_y: None,
            world_x: None,
            world_y: None,
            world_width: None,
            world_height: None,
            velocity_x: 0.0,
            velocity_y: 0.0,
            motion_y: 0.0,
        });
        assert!(!bridge.theme_sprites.contains_key(&(1, "shared".to_owned())));
        let sprite = bridge
            .theme_sprites
            .get_mut(&(0, "shared".to_owned()))
            .unwrap();
        assert_eq!(sprite.sprite, "BG_NEW");
        assert_eq!((sprite.x, sprite.y), (100.0, 200.0));
        assert_eq!(
            (sprite.scale_x, sprite.scale_y),
            (f64::from(1.5_f32), f64::from(1.6_f32))
        );
        assert_eq!(sprite.angle, f64::from(0.7_f32));
        assert!(sprite.horizontal_flip);
        assert_eq!((sprite.velocity_x, sprite.velocity_y), (5.0, -6.0));
        assert_eq!(sprite.scale_speed, 4.0);
        sprite.angular_velocity = 2.0;

        bridge.advance_native_theme_frame(0.5);
        let sprite = &bridge.theme_sprites[&(0, "shared".to_owned())];
        assert_eq!((sprite.x, sprite.y), (102.5, 197.0));
        assert_eq!(
            (sprite.scale_x, sprite.scale_y),
            (f64::from(3.5_f32), f64::from(3.6_f32))
        );
    }

    runtime.execute_source("rotateThemeSprites(0.5)").unwrap();
    let bridge = runtime.render.lock().unwrap();
    let sprite = &bridge.theme_sprites[&(0, "shared".to_owned())];
    assert_eq!(sprite.angle, f64::from(1.7_f32));
}

#[test]
fn theme_sprite_adapters_enforce_slots_and_float32_rounding() {
    let runtime = StellaLua::new("/tmp").unwrap();
    runtime
        .execute_source(
            r#"
                bad_create_missing = pcall(function()
                    createThemeSprite("bad")
                end)
                bad_create_bool = pcall(function()
                    createThemeSprite("bad", "S", 0, 0, 1, 1, 0, 0, 0, 1, 0, 0)
                end)
                bad_modify = pcall(function()
                    modifyThemeSprite("bad", 0, 0, 1, 1, 0, "0")
                end)
                bad_remove = pcall(function() removeThemeSprite("bad") end)
                bad_rotate = pcall(function() rotateThemeSprites("0.5") end)
                bad_set = pcall(function() setThemeSprite("bad", "S") end)

                createThemeSprite(
                    "rounded", "OLD", 16777217, 0.123456789,
                    1.00000006, 1.00000018, -7.0000001,
                    -0.75, 0.333333333, false, 0.100000001, -0.200000001
                )
                modifyThemeSprite(
                    "rounded", 16777219, 0.987654321,
                    1.00000006, 1.00000018, -7.0000001, -2.5
                )
                setThemeSprite("rounded", "NEW", -9.25)
                "#,
        )
        .unwrap();

    let environment = game_environment(runtime.lua()).unwrap();
    for name in [
        "bad_create_missing",
        "bad_create_bool",
        "bad_modify",
        "bad_remove",
        "bad_rotate",
        "bad_set",
    ] {
        assert!(!environment.get::<bool>(name).unwrap(), "{name}");
    }

    {
        let bridge = runtime.render.lock().unwrap();
        assert!(!bridge.theme_sprites.keys().any(|(_, name)| name == "bad"));
        let sprite = &bridge.theme_sprites[&(0, "rounded".to_owned())];
        assert_eq!(sprite.sprite, "NEW");
        assert_eq!(sprite.x, f64::from(16_777_219_f64 as f32));
        assert_eq!(sprite.y, f64::from(0.987_654_321_f64 as f32));
        assert_eq!(sprite.scale_x, f64::from(1.000_000_06_f64 as f32));
        assert_eq!(sprite.scale_y, f64::from(1.000_000_18_f64 as f32));
        assert_eq!(sprite.angle, f64::from(-7.000_000_1_f64 as f32));
        assert_eq!(sprite.original_x, f64::from(16_777_217_f64 as f32));
        assert_eq!(sprite.velocity_x, f64::from(0.100_000_001_f64 as f32));
        assert_eq!(sprite.velocity_y, f64::from(-0.200_000_001_f64 as f32));
        assert_eq!(sprite.scale_speed, f64::from(0.333_333_333_f64 as f32));
    }

    runtime
        .execute_source("removeThemeSprite('rounded', -1)")
        .unwrap();
    assert!(
        !runtime
            .render
            .lock()
            .unwrap()
            .theme_sprites
            .contains_key(&(0, "rounded".to_owned()))
    );
}

#[test]
fn theme_sprite_vectors_preserve_insertion_order_and_duplicate_first_match() {
    let runtime = StellaLua::new("/tmp").unwrap();
    runtime
        .execute_source(
            r#"
                createThemeSprite("z_first", "FIRST", 0, 0, 1, 1, 0, 0, 0, false, 0, 0)
                createThemeSprite("a_second", "SECOND", 0, 0, 1, 1, 0, 0, 0, false, 0, 0)
                createThemeSprite("duplicate", "DUP_1", 1, 0, 1, 1, 0, 0, 0, false, 0, 0)
                createThemeSprite("duplicate", "DUP_2", 2, 0, 1, 1, 0, 0, 0, false, 0, 0)
                modifyThemeSprite("duplicate", 9, 0, 1, 1, 0, 0)
                setThemeSprite("duplicate", "DUP_1_CHANGED", 0)
                "#,
        )
        .unwrap();

    {
        let bridge = runtime.render.lock().unwrap();
        let entries = bridge
            .theme_sprites
            .iter()
            .map(|((_, name), sprite)| (name.as_str(), sprite.sprite.as_str(), sprite.x))
            .collect::<Vec<_>>();
        assert_eq!(
            entries,
            [
                ("z_first", "FIRST", 0.0),
                ("a_second", "SECOND", 0.0),
                ("duplicate", "DUP_1_CHANGED", 9.0),
                ("duplicate", "DUP_2", 2.0),
            ]
        );
    }

    runtime
        .execute_source("removeThemeSprite('duplicate', 0)")
        .unwrap();
    let bridge = runtime.render.lock().unwrap();
    let entries = bridge
        .theme_sprites
        .iter()
        .map(|((_, name), sprite)| (name.as_str(), sprite.sprite.as_str()))
        .collect::<Vec<_>>();
    assert_eq!(
        entries,
        [
            ("z_first", "FIRST"),
            ("a_second", "SECOND"),
            ("duplicate", "DUP_2"),
        ]
    );
}
