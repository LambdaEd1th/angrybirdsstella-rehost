use super::super::*;

#[test]
fn static_sprite_tables_share_ios_rand_with_lua_but_animated_tables_do_not() {
    let runtime = unlocked_test_runtime();
    runtime
        .execute_source(
            r#"
                blockTable = {
                    themes = {
                        random_static = {
                            bgLayers = {
                                { sprite = { "STATIC_A", "STATIC_B", "STATIC_C" } },
                                {
                                    sprite = { "ANIM_A", "ANIM_B" },
                                    animationSpeed = "0.25"
                                }
                            }
                        }
                    }
                }
                math.randomseed(1)
                setTheme("random_static")
                random_after_theme_selection = math.random()
            "#,
        )
        .unwrap();

    let bridge = runtime.render.lock().unwrap();
    assert_eq!(bridge.theme_background_layers[0].sprite, "STATIC_B");
    assert_eq!(bridge.theme_background_layers[1].sprite, "ANIM_A");
    drop(bridge);

    let environment = game_environment(runtime.lua()).unwrap();
    assert_eq!(
        environment
            .get::<f64>("random_after_theme_selection")
            .unwrap(),
        f64::from(f32::from_bits(0x3e06_b1d8))
    );
}

#[test]
fn animated_theme_layer_preserves_frames_scale_alpha_and_parallax_velocity() {
    let runtime = unlocked_test_runtime();
    runtime
        .execute_source(
            r#"
                blockTable = {
                    themes = {
                        animated = {
                            bgLayers = {{
                                sprite = { "WAVE_1", "WAVE_2", "WAVE_3" },
                                offsetX = 0,
                                offsetY = 10,
                                scale = 0.6,
                                zDistance = 0.25,
                                velX = 8,
                                velY = 4,
                                animationSpeed = 0.2,
                                minAlpha = 0.2,
                                maxAlpha = 0.8
                            }}
                        }
                    }
                }
                setTheme("animated")
                "#,
        )
        .unwrap();

    let mut bridge = runtime.render.lock().unwrap();
    assert_eq!(bridge.theme_background_layers.len(), 1);
    let layer = &bridge.theme_background_layers[0];
    assert_eq!(layer.animation_frames, ["WAVE_1", "WAVE_2", "WAVE_3"]);
    assert_eq!(
        (layer.scale_x, layer.scale_y),
        (f64::from(0.6_f32), f64::from(0.6_f32))
    );
    assert_eq!(layer.alpha, 1.0);

    bridge.advance_native_theme_frame(0.1);
    let layer = &bridge.theme_background_layers[0];
    assert_eq!(layer.sprite, "WAVE_1");
    assert_eq!(layer.animation_frame, 0);
    assert_eq!(layer.alpha, f64::from(0.2_f32));
    assert_eq!(
        layer.offset_x,
        f64::from((8.0_f32 * 0.1_f32).mul_add(0.75_f32, 0.0_f32))
    );
    assert!(matches!(
        layer.offset_y,
        ThemeVerticalOffset::Pixels(value)
            if value == f64::from((4.0_f32 * 0.1_f32).mul_add(0.75_f32, 10.0_f32))
    ));

    bridge.advance_native_theme_frame(0.11);
    let layer = &bridge.theme_background_layers[0];
    assert_eq!(layer.sprite, "WAVE_2");
    assert_eq!(layer.animation_frame, 1);
    let timer = (0.1_f32 + 0.11_f32) - 0.2_f32;
    let alpha_mix = ((timer / 0.2_f32) - 0.5_f32).abs() * 2.0_f32;
    let expected_alpha = (1.0_f32 - alpha_mix).mul_add(0.2_f32, alpha_mix * 0.8_f32);
    assert_eq!(layer.alpha, f64::from(expected_alpha));
}

#[test]
fn missing_theme_sprite_geometry_matches_native_zero_resource_queries() {
    let runtime = unlocked_test_runtime();
    runtime
        .execute_source(
            r#"
                blockTable = {
                    themes = {
                        moving = {
                            bgLayers = {
                                {
                                    sprite = "RIGHT_DOWN",
                                    offsetX = 700,
                                    offsetY = 600,
                                    scale = 1,
                                    zDistance = 0,
                                    velX = 1,
                                    velY = 1
                                },
                                {
                                    sprite = "LEFT_UP",
                                    offsetX = -700,
                                    offsetY = -600,
                                    scale = 1,
                                    zDistance = 0,
                                    velX = -1,
                                    velY = -1
                                }
                            }
                        }
                    }
                }
                setWorldScale(20)
                setMaxWorldScale(20)
                setTheme("moving")
                "#,
        )
        .unwrap();

    let mut bridge = runtime.render.lock().unwrap();
    bridge.advance_native_theme_frame(0.0);
    let right_down = &bridge.theme_background_layers[0];
    // sub_10045CD14/60/AC/F8 return zero for an unknown resource. With no
    // width or height, sub_10009B8B4 cannot apply its tile wrap.
    assert_eq!(right_down.geometry.width(), 0.0);
    assert_eq!(right_down.geometry.height(), 0.0);
    assert_eq!(right_down.offset_x, 700.0);
    assert!(matches!(
        right_down.offset_y,
        ThemeVerticalOffset::Pixels(value) if value == 600.0
    ));
    let left_up = &bridge.theme_background_layers[1];
    assert_eq!(left_up.offset_x, -700.0);
    assert!(matches!(
        left_up.offset_y,
        ThemeVerticalOffset::Pixels(value) if value == -600.0
    ));
}

#[test]
fn theme_frame_keeps_native_position_and_parallax_offset_pairs_distinct() {
    let runtime = unlocked_test_runtime();
    runtime
        .execute_source(
            r#"
                blockTable = {
                    themes = {
                        split_motion = {
                            bgLayers = {{
                                sprite = "BG",
                                posX = 10,
                                posY = 20,
                                offsetX = 30,
                                offsetY = 40,
                                zDistance = 0.25,
                                velX = 2,
                                velY = 4,
                                xSpeedAdd = 3,
                                ySpeedAdd = -1
                            }},
                            fgLayers = {{
                                sprite = "FG",
                                posX = -10,
                                posY = -20,
                                offsetX = -30,
                                offsetY = -40,
                                zDistance = 0.5,
                                velX = -2,
                                velY = -4,
                                xSpeedAdd = -3,
                                ySpeedAdd = 1
                            }}
                        }
                    }
                }
                setTheme("split_motion")
                "#,
        )
        .unwrap();

    let mut bridge = runtime.render.lock().unwrap();
    bridge.advance_native_theme_frame(0.5);

    let background = &bridge.theme_background_layers[0];
    assert_eq!((background.velocity_x, background.velocity_y), (5.0, 3.0));
    // sub_1000607E8 integrates the full velocity into +0x34/+0x38.
    assert_eq!((background.position_x, background.position_y), (12.5, 21.5));
    // sub_10009B8B4 independently integrates `(1-zDistance)` into
    // +0x3c/+0x40 during the preceding background pass.
    assert_eq!(background.offset_x, 31.875);
    assert!(matches!(
        background.offset_y,
        ThemeVerticalOffset::Pixels(value) if value == 41.125
    ));

    let foreground = &bridge.theme_foreground_layers[0];
    assert_eq!((foreground.velocity_x, foreground.velocity_y), (-5.0, -3.0));
    assert_eq!(
        (foreground.position_x, foreground.position_y),
        (-12.5, -21.5)
    );
    assert_eq!(foreground.offset_x, -31.25);
    assert!(matches!(
        foreground.offset_y,
        ThemeVerticalOffset::Pixels(value) if value == -40.75
    ));
}

#[test]
fn native_physics_lock_gates_the_complete_theme_frame_chain() {
    let runtime = unlocked_test_runtime();
    runtime
        .execute_source(
            r#"
                blockTable = {
                    themes = {
                        locked = {
                            bgLayers = {{
                                sprite = "BG",
                                posX = 10,
                                offsetX = 30,
                                zDistance = 0,
                                velX = 4
                            }}
                        }
                    }
                }
                setTheme("locked")
                setPhysicsEnabled(false, "transition")
                update = function() end
                "#,
        )
        .unwrap();

    runtime.update(0.5).unwrap();
    {
        let bridge = runtime.render.lock().unwrap();
        let layer = &bridge.theme_background_layers[0];
        assert_eq!((layer.position_x, layer.offset_x), (10.0, 30.0));
    }

    runtime
        .execute_source("setPhysicsEnabled(true, 'transition')")
        .unwrap();
    runtime.update(0.5).unwrap();
    let bridge = runtime.render.lock().unwrap();
    let layer = &bridge.theme_background_layers[0];
    assert_eq!((layer.position_x, layer.offset_x), (12.0, 32.0));
}

#[test]
fn theme_frame_precedes_update_physics_lock_mutation() {
    let runtime = unlocked_test_runtime();
    runtime
        .execute_source(
            r#"
                blockTable = {
                    themes = {
                        ordered = {
                            bgLayers = {{
                                sprite = "BG",
                                posX = 10,
                                offsetX = 30,
                                zDistance = 0,
                                velX = 4
                            }}
                        }
                    }
                }
                setTheme("ordered")
                createCircle("body", "", 0, 0, 1, 1, 0, 0, true, false, 1)
                setWorldGravity(0, 0)
                updatePhysics = function()
                    setPhysicsEnabled(false, "during-step")
                end
                update = function() end
            "#,
        )
        .unwrap();

    let delta = f32::from_bits(0x3D08_8889);
    runtime.update(f64::from(delta)).unwrap();

    let bridge = runtime.render.lock().unwrap();
    assert!(!bridge.physics_enabled);
    assert_eq!(
        bridge.theme_background_layers[0].position_x,
        f64::from(4.0_f32.mul_add(delta, 10.0_f32))
    );
    assert_eq!(
        bridge.theme_background_layers[0].offset_x,
        f64::from(4.0_f32.mul_add(delta, 30.0_f32))
    );
}
