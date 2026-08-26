use super::super::*;

#[test]
fn theme_refresh_recovers_reference_camera_and_native_layer_transform() {
    let runtime = StellaLua::new("/tmp").unwrap();
    register_test_sprite_sheet(&runtime, &["CAMERA_TILE"]);
    runtime
        .execute_source(
            r#"
                objects = {
                    castleCameraData = {
                        ipad = { sx = 4, sy = 4 },
                        ios = { px = 10, py = -5 }
                    },
                    birdCameraData = {
                        ios = { px = 20, py = -10 }
                    }
                }
                gameCamera = {
                    resolutionCorrectedCameras = {
                        [2] = { sx = 6 }
                    },
                    endCameraIndex = 2
                }
                g_useLowerCameraAsThemeReferencePoint = true
                blockTable = {
                    themes = {
                        camera = {
                            bgLayers = {{
                                sprite = "CAMERA_TILE",
                                offsetX = 8,
                                offsetY = 4,
                                scale = 2,
                                zDistance = 0.25
                            }}
                        }
                    }
                }
                setWorldScale(5)
                setMaxWorldScale(99)
                setTopLeft(2, -3)
                setTheme("camera")
                native_refreshThemeSystem()
            "#,
        )
        .unwrap();

    {
        let mut bridge = runtime.render.lock().unwrap();
        assert!(bridge.theme_camera.valid);
        // 0x10009B040/0x10009B198 select the component-wise maximum when the
        // historical "lower camera" switch is enabled.
        assert_eq!(
            (
                bridge.theme_camera.x,
                bridge.theme_camera.y,
                bridge.theme_camera.scale,
                bridge.resolution_camera_scale,
            ),
            (20.0, -5.0, 4.0, 6.0)
        );
        let geometry = SpriteGeometry {
            min_x: -3.0,
            min_y: -2.0,
            max_x: 13.0,
            max_y: 6.0,
        };
        bridge.theme_background_layers[0].geometry = geometry;
        bridge.theme_background_layers[0].animation_geometries[0] = geometry;
    }

    runtime.execute_source("drawBackgroundNative(-1)").unwrap();
    let bridge = runtime.render.lock().unwrap();
    let command = bridge
        .commands
        .iter()
        .find(|command| command.sprite == "CAMERA_TILE")
        .unwrap();
    // Literal results of sub_10009CEB0 followed by sub_100067A04, using the
    // recovered float32 instruction order (not a screenshot-derived target).
    assert_eq!((command.x, command.y), (212.5625, 96.375_007_629_394_53));
    assert_eq!(
        (command.state.scale_x, command.state.scale_y),
        (2.625, 2.625)
    );
    assert_eq!(
        (command.state.translate_x, command.state.translate_y),
        (-21.0, -10.5)
    );
    assert_eq!(command.state.sprite_pivot, Some([0.0, 0.0]));
}

#[test]
fn theme_refresh_uses_reference_camera_fallback_and_zero_overrides() {
    let runtime = StellaLua::new("/tmp").unwrap();
    runtime
        .execute_source(
            r#"
                objects = {
                    castleCameraData = {
                        referenceCamera = { sx = 7.5 },
                        ios = { px = 123, py = -456 }
                    }
                }
                g_useZeroAsThemeReferencePointX = true
                g_useZeroAsThemeReferencePointY = true
                native_refreshThemeSystem()
            "#,
        )
        .unwrap();
    let bridge = runtime.render.lock().unwrap();
    assert!(bridge.theme_camera.valid);
    assert_eq!(
        (
            bridge.theme_camera.x,
            bridge.theme_camera.y,
            bridge.theme_camera.scale,
        ),
        (0.0, 0.0, 7.5)
    );
}

#[test]
fn native_theme_scale_uses_reference_camera_instead_of_physics_scale() {
    // 0x10009C0DC..0x10009C108:
    // (current/end) * ((end/reference) * (1-z)) + (end/reference) * z.
    assert_eq!(native_theme_parallax_scale(5.0, 6.0, 4.0, 0.25), 1.3125);
}

#[test]
fn theme_layer_preserves_remaining_scalar_record_abi_and_native_defaults() {
    let runtime = StellaLua::new("/tmp").unwrap();
    register_test_sprite_sheet(&runtime, &["SCALAR_TILE"]);
    runtime
        .execute_source(
            r#"
                blockTable = {
                    themes = {
                        scalars = {
                            bgLayers = {
                                {
                                    sprite = "SCALAR_TILE",
                                    parallaxSpeed = 0.123456789,
                                    zDistance = 0.234567891,
                                    scaleSpeed = 0.345678912,
                                    angleMult = 0.456789123,
                                    xMult = 0.567891234,
                                    yMult = 0.678912345
                                },
                                { sprite = "SCALAR_TILE" }
                            }
                        }
                    }
                }
                setTheme("scalars")
            "#,
        )
        .unwrap();

    let bridge = runtime.render.lock().unwrap();
    let authored = &bridge.theme_background_layers[0];
    assert_eq!(
        (
            authored.parallax_speed,
            authored.z_distance,
            authored.scale_speed,
            authored.angle_multiplier,
            authored.x_multiplier,
            authored.y_multiplier,
        ),
        (
            f64::from(0.123456789_f64 as f32),
            f64::from(0.234567891_f64 as f32),
            f64::from(0.345678912_f64 as f32),
            f64::from(0.456789123_f64 as f32),
            f64::from(0.567891234_f64 as f32),
            f64::from(0.678912345_f64 as f32),
        )
    );

    let defaults = &bridge.theme_background_layers[1];
    assert_eq!(
        (
            defaults.parallax_speed,
            defaults.z_distance,
            defaults.scale_speed,
            defaults.angle_multiplier,
            defaults.x_multiplier,
            defaults.y_multiplier,
        ),
        (1.0, 0.0, 1.0, 0.0, 0.0, 1.0)
    );
}

#[test]
fn theme_layer_narrows_active_draw_scalars_and_keeps_uniform_scale_fallbacks_independent() {
    let runtime = StellaLua::new("/tmp").unwrap();
    register_test_sprite_sheet(&runtime, &["DRAW_SCALAR_TILE"]);
    runtime
        .execute_source(
            r#"
                blockTable = {
                    themes = {
                        draw_scalars = {
                            bgLayers = {{
                                sprite = "DRAW_SCALAR_TILE",
                                offsetY = 0.123456789,
                                scale = 0.234567891,
                                scaleX = 0.345678912,
                                animationSpeed = 0.456789123,
                                alpha = 0.567891234,
                                minAlpha = 0.678912345,
                                maxAlpha = 0.789123456
                            }}
                        }
                    }
                }
                setTheme("draw_scalars")
            "#,
        )
        .unwrap();

    let bridge = runtime.render.lock().unwrap();
    let layer = &bridge.theme_background_layers[0];
    assert!(matches!(
        layer.offset_y,
        ThemeVerticalOffset::Pixels(value)
            if value == f64::from(0.123456789_f64 as f32)
    ));
    assert_eq!(layer.scale_x, f64::from(0.345678912_f64 as f32));
    // scaleY falls back to the independent uniform `scale`, not scaleX.
    assert_eq!(layer.scale_y, f64::from(0.234567891_f64 as f32));
    assert_eq!(
        (
            layer.animation_delay,
            layer.alpha,
            layer.min_alpha,
            layer.max_alpha,
        ),
        (
            f64::from(0.456789123_f64 as f32),
            f64::from(0.567891234_f64 as f32),
            f64::from(0.678912345_f64 as f32),
            f64::from(0.789123456_f64 as f32),
        )
    );
}

#[test]
fn theme_layer_preserves_xmult_and_relative_record_fields() {
    let runtime = StellaLua::new("/tmp").unwrap();
    register_test_sprite_sheet(&runtime, &["RELATIVE_TILE"]);
    runtime
        .execute_source(
            r#"
                originalCameras = {
                    [2] = { sx = 8, sy = 9 }
                }
                objects = {
                    castleCameraData = {
                        ipad = { sx = 4, sy = 4 },
                        ios = { px = 10, py = 20 }
                    }
                }
                gameCamera = {
                    originalCameras = {
                        [2] = { sx = 12, sy = 13 }
                    },
                    resolutionCorrectedCameras = {
                        [2] = { sx = 6 }
                    },
                    endCameraIndex = 2
                }
                blockTable = {
                    themes = {
                        relative = {
                            bgLayers = {{
                                sprite = "RELATIVE_TILE",
                                offsetX = 0,
                                offsetY = 999,
                                zDistance = 0.25,
                                xMult = 0.5,
                                relativeX = 0.125,
                                relativeY = 0.75
                            }}
                        }
                    }
                }
                setWorldScale(5)
                setTopLeft(2, -3)
                setTheme("relative")
                native_refreshThemeSystem()
            "#,
        )
        .unwrap();

    {
        let bridge = runtime.render.lock().unwrap();
        assert_eq!(bridge.theme_camera.original_scale_ratio, 2.0);
        let layer = &bridge.theme_background_layers[0];
        assert_eq!(layer.x_multiplier, 0.5);
        assert_eq!(layer.relative_x, Some(0.125));
        assert_eq!(layer.relative_y, Some(0.75));
        assert_eq!(
            native_theme_relative_y_offset(
                layer.relative_y.unwrap() as f32,
                bridge.screen_height as f32,
                bridge.theme_camera.original_scale_ratio,
            ),
            96.0
        );
    }

    let geometry = SpriteGeometry {
        min_x: -3.0,
        min_y: -2.0,
        max_x: 13.0,
        max_y: 6.0,
    };
    {
        let mut bridge = runtime.render.lock().unwrap();
        bridge.theme_background_layers[0].geometry = geometry;
        bridge.theme_background_layers[0].animation_geometries[0] = geometry;
    }
    runtime.execute_source("drawBackgroundNative(-1)").unwrap();
    let bridge = runtime.render.lock().unwrap();
    let command = bridge
        .commands
        .iter()
        .find(|command| command.sprite == "RELATIVE_TILE")
        .unwrap();

    // Literal float32 sequence from sub_10009CEB0. In particular, X uses
    // `(zDistance + xMult) * cameraDeltaX`, while the relativeY-derived 96
    // replaces the authored offsetY=999 before the normal Y projection.
    let current_scale = 5.0_f32;
    let end_scale = 6.0_f32;
    let reference_scale = 4.0_f32;
    let z_distance = 0.25_f32;
    let ratio = current_scale / end_scale;
    let one_minus_z = 1.0_f32 - z_distance;
    let centered_x = 16.0_f32.mul_add(0.5_f32, -3.0_f32);
    let local_x = centered_x / reference_scale;
    let base_x = z_distance.mul_add(local_x / ratio, local_x * one_minus_z) + 10.0_f32;
    let camera_x = 2.0_f32 + (1024.0_f32 * 0.5_f32) / current_scale;
    let x_world = base_x + (z_distance + 0.5_f32) * (camera_x - 10.0_f32);
    let expected_x = (x_world - 2.0_f32) * current_scale;

    let centered_y = 8.0_f32.mul_add(0.5_f32, -2.0_f32);
    let local_y = (96.0_f32 + centered_y) / reference_scale;
    let base_y = z_distance.mul_add(local_y / ratio, local_y * one_minus_z) + 20.0_f32;
    let camera_y = -3.0_f32 + (768.0_f32 * 0.5_f32) / current_scale;
    let y_world = base_y + z_distance * (camera_y - 20.0_f32);
    let expected_y = (y_world - -3.0_f32) * current_scale;
    assert_eq!(
        (command.x, command.y),
        (expected_x.into(), expected_y.into())
    );
}

#[test]
fn theme_world_fields_refresh_live_offsets_and_share_native_cmwc() {
    let runtime = StellaLua::new("/tmp").unwrap();
    register_test_sprite_sheet(&runtime, &["WORLD_ANCHOR", "WORLD_RANDOM"]);
    runtime
        .execute_source(
            r#"
                leftLimitWorld = -100
                rightLimitWorld = 200
                topLimitWorld = -50
                bottomLimitWorld = 150
                objects = {
                    castleCameraData = {
                        ipad = { sx = 4, sy = 4 },
                        ios = { px = 10, py = 20 }
                    }
                }
                gameCamera = {
                    resolutionCorrectedCameras = {
                        [2] = { sx = 6 }
                    },
                    endCameraIndex = 2
                }
                blockTable = {
                    themes = {
                        world = {
                            bgLayers = {
                                {
                                    sprite = "WORLD_ANCHOR",
                                    offsetX = 999,
                                    offsetY = 999,
                                    worldX = 0.25,
                                    worldY = 0.75
                                },
                                {
                                    sprite = "WORLD_RANDOM",
                                    offsetX = 1,
                                    offsetY = 2,
                                    worldW = 0.5,
                                    worldH = 0.25
                                }
                            }
                        }
                    }
                }
                setWorldScale(5)
                setTopLeft(2, -3)
                setTheme("world")
                native_refreshThemeSystem()
                drawBackgroundNative(-1)
            "#,
        )
        .unwrap();

    let screen_left = 2.0_f32;
    let screen_right = screen_left + 1024.0_f32 / 5.0_f32;
    let screen_top = -3.0_f32;
    let screen_bottom = screen_top + 768.0_f32 / 5.0_f32;
    let left = (-100.0_f32).min(screen_left);
    let right = 200.0_f32.max(screen_right);
    let top = (-50.0_f32).min(screen_top);
    let bottom = 150.0_f32.max(screen_bottom);
    let width = right - left;
    let height = bottom - top;

    let mut expected_random = NativeParticleRandom::default();
    let random_x = expected_random.next() as f32;
    let random_y = expected_random.next() as f32;
    let bridge = runtime.render.lock().unwrap();
    let anchored = &bridge.theme_background_layers[0];
    assert_eq!(
        anchored.offset_x,
        f64::from(width.mul_add(0.25_f32, left) - 10.0_f32)
    );
    let ThemeVerticalOffset::Pixels(anchored_y) = anchored.offset_y else {
        panic!("worldY must replace the native numeric offset")
    };
    assert_eq!(
        anchored_y,
        f64::from(height.mul_add(0.75_f32, top) - 20.0_f32)
    );

    let randomized = &bridge.theme_background_layers[1];
    assert_eq!(
        randomized.offset_x,
        f64::from(width.mul_add(0.5_f32 * (0.5_f32 - random_x), 1.0_f32))
    );
    let ThemeVerticalOffset::Pixels(randomized_y) = randomized.offset_y else {
        panic!("worldH must update the native numeric offset")
    };
    assert_eq!(
        randomized_y,
        f64::from(height.mul_add(0.25_f32 * (0.5_f32 - random_y), 2.0_f32))
    );
    assert_eq!(bridge.particle_random.index, 1);
}

#[test]
fn theme_refresh_resolves_symbolic_foreground_offsets_from_all_corrected_cameras() {
    let runtime = StellaLua::new("/tmp").unwrap();
    register_test_sprite_sheet(&runtime, &["TOP", "BOTTOM"]);
    runtime
        .execute_source(
            r#"
                objects = {
                    castleCameraData = {
                        ipad = { sx = 4 },
                        ios = { px = 10, py = 20 }
                    }
                }
                gameCamera = {
                    resolutionCorrectedCameras = {
                        { sx = 4, px = 2, py = 6, left = -1, top = 0 },
                        { sx = 8, px = 3, py = 10, left = -2, top = 0 }
                    },
                    endCameraIndex = 2
                }
                blockTable = {
                    themes = {
                        anchored = {
                            fgLayers = {
                                {
                                    sprite = "TOP",
                                    offsetY = "top",
                                    scaleY = 1.5,
                                    zDistance = 0
                                },
                                {
                                    sprite = "BOTTOM",
                                    offsetY = "bottom",
                                    scaleY = 1.5,
                                    zDistance = 0
                                }
                            }
                        }
                    }
                }
                setWorldScale(8)
                setTheme("anchored")
            "#,
        )
        .unwrap();

    let geometry = SpriteGeometry {
        min_x: -4.0,
        min_y: -2.0,
        max_x: 6.0,
        max_y: 8.0,
    };
    {
        let mut bridge = runtime.render.lock().unwrap();
        for layer in &mut bridge.theme_foreground_layers {
            layer.geometry = geometry;
            layer.animation_geometries.fill(geometry);
        }
    }
    runtime
        .execute_source("native_refreshThemeSystem()")
        .unwrap();

    let bridge = runtime.render.lock().unwrap();
    assert!(matches!(
        bridge.theme_foreground_layers[0].offset_y,
        ThemeVerticalOffset::Top
    ));
    assert!(matches!(
        bridge.theme_foreground_layers[1].offset_y,
        ThemeVerticalOffset::Bottom
    ));
    // sub_100099828 returns vertical bounds 83 and 166. The top branch uses
    // max=166; the bottom branch uses min=83. end/reference is 8/4=2,
    // and the signed integer half-height term is (10/2)*1.5 = 7.5.
    assert_eq!(
        bridge.theme_foreground_layers[0].resolved_offset_y,
        Some(-90.5)
    );
    assert_eq!(
        bridge.theme_foreground_layers[1].resolved_offset_y,
        Some(350.0)
    );
}
