use super::super::*;
use super::configure_theme_camera_fixture;

#[test]
fn theme_contract_builds_both_native_render_passes() {
    let runtime = StellaLua::new("/tmp").unwrap();
    configure_theme_camera_fixture(&runtime);
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

                local themes = blockTable.themes
                blockTable.themes = nil
                missing_themes_bg_fails =
                    not pcall(setThemeOffsetY, "test_theme", 1)
                missing_themes_fg_fails =
                    not pcall(native_setThemeFgLayerOffsetY,
                        "test_theme", 1, 99)
                blockTable.themes = themes

                local theme = themes.test_theme
                themes.test_theme = nil
                missing_theme_bg_fails =
                    not pcall(setThemeOffsetY, "test_theme", 1)
                missing_theme_fg_fails =
                    not pcall(native_setThemeFgLayerOffsetY,
                        "test_theme", 1, 99)
                themes.test_theme = theme

                local bg_layers = theme.bgLayers
                theme.bgLayers = { bg_layers[1] }
                short_bg_layers_fails =
                    not pcall(setThemeOffsetY, "test_theme", 1)
                theme.bgLayers = bg_layers
                setThemeOffsetY("test_theme", 768)

                local fg_layers = theme.fgLayers
                theme.fgLayers = nil
                missing_fg_layers_fails =
                    not pcall(native_setThemeFgLayerOffsetY,
                        "test_theme", 1, 99)
                theme.fgLayers = fg_layers
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
    let environment = game_environment(runtime.lua()).unwrap();
    for flag in [
        "missing_themes_bg_fails",
        "missing_themes_fg_fails",
        "missing_theme_bg_fails",
        "missing_theme_fg_fails",
        "short_bg_layers_fails",
        "missing_fg_layers_fails",
    ] {
        assert!(environment.get::<bool>(flag).unwrap(), "{flag}");
    }
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
        ThemeVerticalOffset::Bottom
    ));
    assert_eq!(
        bridge.theme_foreground_layers[0].resolved_offset_y,
        Some(12.75)
    );
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
fn background_offset_ignores_metatables_and_preserves_writes_before_a_later_error() {
    let runtime = StellaLua::new("/tmp").unwrap();
    configure_theme_camera_fixture(&runtime);
    register_test_sprite_sheet(&runtime, &["PARTIAL_BG_1", "PARTIAL_BG_2"]);
    runtime
        .execute_source(
            r#"
                blockTable = {
                    themes = {
                        partial = {
                            bgLayers = {
                                {
                                    sprite = "PARTIAL_BG_1",
                                    offsetY = 10,
                                    scale = 1
                                },
                                {
                                    sprite = "PARTIAL_BG_2",
                                    offsetY = 20,
                                    scale = 1
                                }
                            },
                            fgLayers = {}
                        }
                    }
                }
                setGameParameters({ gameWorldScale = 768 })
                setTheme("partial")

                local layers = blockTable.themes.partial.bgLayers
                rawset(layers[1], "offsetY", nil)
                offset_lookup_count = 0
                setmetatable(layers[1], {
                    __index = function(_, key)
                        if key == "offsetY" then
                            offset_lookup_count = offset_lookup_count + 1
                            return 20 + offset_lookup_count
                        end
                    end
                })
                setThemeOffsetY("partial", 5)

                setmetatable(layers[1], nil)
                layers[1].offsetY = 10
                blockTable.themes.partial.bgLayers = { layers[1] }
                short_background_fails =
                    not pcall(setThemeOffsetY, "partial", 7)
            "#,
        )
        .unwrap();

    let environment = game_environment(runtime.lua()).unwrap();
    assert_eq!(environment.get::<i64>("offset_lookup_count").unwrap(), 0);
    assert!(environment.get::<bool>("short_background_fails").unwrap());

    let bridge = runtime.render.lock().unwrap();
    assert_eq!(bridge.theme_offset_y, 7.0);
    assert!(matches!(
        bridge.theme_background_layers[0].offset_y,
        ThemeVerticalOffset::Pixels(value) if value == 17.0
    ));
    assert!(matches!(
        bridge.theme_background_layers[1].offset_y,
        ThemeVerticalOffset::Pixels(value) if value == 25.0
    ));
}

#[test]
fn background_offset_uses_drawable_height_without_running_lua_index_hooks() {
    let runtime = StellaLua::new_with_resolution("/tmp", 1024, 512).unwrap();
    configure_theme_camera_fixture(&runtime);
    register_test_sprite_sheet(&runtime, &["LIVE_BG_1", "LIVE_BG_2"]);
    runtime
        .execute_source(
            r#"
                blockTable = {
                    themes = {
                        live = {
                            bgLayers = {
                                { sprite = "LIVE_BG_1", offsetY = 10 },
                                { sprite = "LIVE_BG_2", offsetY = 20 }
                            },
                            fgLayers = {}
                        }
                    }
                }
                setGameParameters({ gameWorldScale = 256 })
                setTheme("live")
                local first = blockTable.themes.live.bgLayers[1]
                first.offsetY = nil
                offset_reads = 0
                setmetatable(first, {
                    __index = function(_, key)
                        if key == "offsetY" then
                            offset_reads = offset_reads + 1
                            setGameParameters({ gameWorldScale = 512 })
                            if offset_reads == 1 then return 10 end
                            return false
                        end
                    end
                })
                -- Native raw reads must not call the hook or change the
                -- scale. Both layers use the live 256/512 ratio.
                setThemeOffsetY("live", 128)
            "#,
        )
        .unwrap();

    let environment = game_environment(runtime.lua()).unwrap();
    assert_eq!(environment.get::<i64>("offset_reads").unwrap(), 0);
    let bridge = runtime.render.lock().unwrap();
    assert!(matches!(
        bridge.theme_background_layers[0].offset_y,
        ThemeVerticalOffset::Pixels(value) if value == 64.0
    ));
    assert!(matches!(
        bridge.theme_background_layers[1].offset_y,
        ThemeVerticalOffset::Pixels(value) if value == 84.0
    ));
}

#[test]
fn set_theme_destroys_owned_sprite_vectors_while_native_reset_preserves_them() {
    let runtime = StellaLua::new("/tmp").unwrap();
    configure_theme_camera_fixture(&runtime);
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
    configure_theme_camera_fixture(&runtime);
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
    configure_theme_camera_fixture(&runtime);
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
    configure_theme_camera_fixture(&runtime);
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
