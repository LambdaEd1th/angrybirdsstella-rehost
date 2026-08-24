use super::super::*;

#[test]
fn theme_contract_builds_both_native_render_passes() {
    let runtime = StellaLua::new("/tmp").unwrap();
    register_test_sprite_sheet(
        &runtime,
        &["TEST_BACKGROUND", "TEST_BACKGROUND_2", "TEST_FOREGROUND"],
    );
    runtime
            .execute_source(
                r#"
                blockTable = {
                    themes = {
                        test_theme = {
                            skyColor = { r = 12, g = 34, b = 56 },
                            bgLayers = {
                                {
                                    sprite = "TEST_BACKGROUND",
                                    offsetX = 0,
                                    offsetY = 10,
                                    scaleX = 1,
                                    scaleY = 1,
                                    zDistance = 0.8,
                                    flags = { "H_REPEAT" }
                                },
                                {
                                    sprite = "TEST_BACKGROUND_2",
                                    offsetX = 0,
                                    offsetY = -2,
                                    scaleX = 1,
                                    scaleY = 1,
                                    zDistance = 1
                                }
                            },
                            fgLayers = {
                                {
                                    sprite = "TEST_FOREGROUND",
                                    offsetX = 5,
                                    offsetY = "bottom",
                                    scaleX = 1.2,
                                    scaleY = 1.2,
                                    zDistance = 0.25
                                }
                            }
                        }
                    }
                }
                setWorldScale(20)
                setMaxWorldScale(20)
                setGameParameters({ gameWorldScale = 0.25 })
                setTheme("test_theme")
                createThemeSprite("preserved", "TEST_THEME_SPRITE", 0, 0, 1, 1, 0, 0, 0, false, 0, 0)
                assert(not pcall(setThemeOffsetY, 7, 7))
                assert(not pcall(setThemeOffsetY, "test_theme", "7"))
                assert(not pcall(native_setThemeFgLayerOffsetY, 7, 1, 2))
                assert(not pcall(native_setThemeFgLayerOffsetY, "test_theme", "1", 2))
                assert(not pcall(native_setThemeFgLayerOffsetY, "test_theme", 1, "2"))
                setThemeOffsetY("test_theme", 768)
                native_setThemeFgLayerOffsetY("test_theme", 1, 12.75)
                native_resetThemeSystem()
                drawBackgroundNative(-1)
                drawForegroundNative()
                "#,
            )
            .unwrap();

    let bridge = runtime.render.lock().unwrap();
    assert_eq!(bridge.background_color, [12, 34, 56]);
    assert_eq!(bridge.theme_background_layers.len(), 2);
    assert_eq!(bridge.theme_foreground_layers.len(), 1);
    assert_eq!(bridge.theme_offset_y, 768.0);
    assert!(matches!(
        bridge.theme_background_layers[0].offset_y,
        ThemeVerticalOffset::Pixels(value) if value == 778.0
    ));
    assert!(matches!(
        bridge.theme_background_layers[1].offset_y,
        ThemeVerticalOffset::Pixels(value) if value == -1.75
    ));
    assert!(matches!(
        bridge.theme_foreground_layers[0].offset_y,
        ThemeVerticalOffset::Pixels(value) if value == 12.75
    ));
    assert!(
        bridge
            .theme_sprites
            .contains_key(&(0, "preserved".to_owned()))
    );
    assert!(
        bridge
            .commands
            .iter()
            .any(|command| command.sprite == "TEST_BACKGROUND_2")
    );
    assert!(
        bridge
            .commands
            .iter()
            .any(|command| command.sprite == "TEST_FOREGROUND")
    );
    // Purple 1.1.6 stores and updates the layer-owned 0x88-byte
    // ThemeSpriteData records, but no renderer traverses that vector.
    assert!(
        bridge
            .commands
            .iter()
            .all(|command| command.sprite != "TEST_THEME_SPRITE")
    );
}

#[test]
fn set_theme_destroys_owned_sprite_vectors_while_native_reset_preserves_them() {
    let runtime = StellaLua::new("/tmp").unwrap();
    runtime
        .execute_source(
            r#"
                blockTable = {
                    themes = {
                        old = { bgLayers = {{ sprite = "OLD_LAYER" }} },
                        new = { bgLayers = {{ sprite = "NEW_LAYER" }} }
                    }
                }
                setTheme("old")
                createThemeSprite("stale", "STALE", 0, 0, 1, 1, 0, 0, 0, false, 0, 0)
                setTheme("new")
                createThemeSprite("fresh", "FRESH", 0, 0, 1, 1, 0, 0, 0, false, 0, 0)
                native_resetThemeSystem()
                "#,
        )
        .unwrap();

    let bridge = runtime.render.lock().unwrap();
    assert!(!bridge.theme_sprites.contains_key(&(0, "stale".to_owned())));
    assert!(bridge.theme_sprites.contains_key(&(0, "fresh".to_owned())));
}

#[test]
fn native_theme_reset_clears_only_camera_reference_and_effect_pair() {
    let runtime = StellaLua::new("/tmp").unwrap();
    {
        let mut bridge = runtime.render.lock().unwrap();
        bridge.theme_camera.valid = true;
        bridge.theme_camera.x = 12.0;
        bridge.theme_camera.y = 34.0;
        bridge.theme_camera.scale = 56.0;
        bridge.theme_camera.effect_x = 7.0;
        bridge.theme_camera.effect_y = 8.0;
        bridge.theme_offset_y = 9.0;
    }
    runtime.execute_source("native_resetThemeSystem()").unwrap();
    let bridge = runtime.render.lock().unwrap();
    assert!(!bridge.theme_camera.valid);
    assert_eq!(bridge.theme_camera.effect_x, 0.0);
    assert_eq!(bridge.theme_camera.effect_y, 0.0);
    assert_eq!(bridge.theme_camera.x, 12.0);
    assert_eq!(bridge.theme_camera.y, 34.0);
    assert_eq!(bridge.theme_camera.scale, 56.0);
    assert_eq!(bridge.theme_offset_y, 9.0);
}

#[test]
fn accelerometer_filter_feeds_both_theme_passes_and_activation_resets_it() {
    let runtime = StellaLua::new("/tmp").unwrap();
    {
        let mut bridge = runtime.render.lock().unwrap();
        bridge.accelerometer_sample = [1.234_567, -2.345_678];
        bridge.accelerometer_filtered = [7.0, 8.0];
        bridge.theme_camera.effect_x = 9.0;
        bridge.theme_camera.effect_y = 10.0;
    }

    runtime
        .execute_source("setAccelerometerActive(true)")
        .unwrap();
    {
        let bridge = runtime.render.lock().unwrap();
        assert!(bridge.accelerometer_active);
        assert_eq!(bridge.accelerometer_filtered, [0.0; 2]);
        // The ThemeManager pair is not touched until the next native frame.
        assert_eq!(
            [bridge.theme_camera.effect_x, bridge.theme_camera.effect_y],
            [9.0, 10.0]
        );
    }

    let first = [
        (f64::from(1.234_567_f32) * 0.2_f64) as f32,
        (f64::from(-2.345_678_f32) * 0.2_f64) as f32,
    ];
    {
        let mut bridge = runtime.render.lock().unwrap();
        bridge.advance_native_theme_frame(1.0 / 60.0);
        assert_eq!(bridge.accelerometer_filtered, first);
        assert_eq!(
            [bridge.theme_camera.effect_x, bridge.theme_camera.effect_y],
            first
        );
        bridge.advance_native_theme_frame(1.0 / 60.0);
        let second = [
            first[0].mul_add(0.8_f32, first[0]),
            first[1].mul_add(0.8_f32, first[1]),
        ];
        assert_eq!(bridge.accelerometer_filtered, second);
        assert_eq!(
            [bridge.theme_camera.effect_x, bridge.theme_camera.effect_y],
            second
        );
    }

    runtime
        .execute_source("setAccelerometerActive(false)")
        .unwrap();
    {
        let mut bridge = runtime.render.lock().unwrap();
        assert!(!bridge.accelerometer_active);
        assert_eq!(bridge.accelerometer_filtered, [0.0; 2]);
        bridge.advance_native_theme_frame(1.0 / 60.0);
        assert_eq!(
            [bridge.theme_camera.effect_x, bridge.theme_camera.effect_y],
            [0.0; 2]
        );
    }
}

#[test]
fn theme_sky_color_is_stored_at_selection_and_applied_only_by_background_draw() {
    let runtime = StellaLua::new("/tmp").unwrap();
    runtime
        .execute_source(
            r#"
                blockTable = { themes = { colors = {
                    skyColor = { r = -4.5, g = 12.9, b = 999 },
                    groundColor = { r = "1.5", g = -2, b = 260.25 },
                    bgLayers = {}, fgLayers = {}
                } } }
                setBGColor(3, 4, 5)
                setTheme("colors")
            "#,
        )
        .unwrap();
    {
        let bridge = runtime.render.lock().unwrap();
        assert_eq!(bridge.background_color, [3, 4, 5]);
        assert_eq!(bridge.theme_sky_color, [-4.5, 12.9_f32, 999.0]);
        assert_eq!(bridge.theme_ground_color, [1.5, -2.0, 260.25]);
    }

    runtime.execute_source("drawForegroundNative()").unwrap();
    assert_eq!(runtime.render.lock().unwrap().background_color, [3, 4, 5]);

    runtime.execute_source("drawBackgroundNative(-1)").unwrap();
    assert_eq!(
        runtime.render.lock().unwrap().background_color,
        [0, 12, 255]
    );
}
