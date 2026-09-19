use super::super::*;
use super::configure_theme_camera_fixture;

fn set_background_geometry(runtime: &StellaLua, geometry: SpriteGeometry) {
    let mut bridge = runtime.render.lock().unwrap();
    for layer in &mut bridge.theme_background_layers {
        layer.geometry = geometry;
        layer.animation_geometries.fill(geometry);
    }
}

#[test]
fn theme_repeat_flags_follow_native_column_and_row_submission_order() {
    let runtime = StellaLua::new("/tmp").unwrap();
    configure_theme_camera_fixture(&runtime);
    register_test_sprite_sheet(&runtime, &["TILE"]);
    runtime
        .execute_source(
            r#"
                blockTable = {
                    themes = {
                        repeat_left = {
                            bgLayers = {{
                                sprite = "TILE",
                                offsetX = 0,
                                offsetY = 0,
                                scaleX = 1,
                                scaleY = 1,
                                zDistance = 1,
                                flags = { "V_REPEAT", "REPEAT_LEFT_ONLY" }
                            }}
                        },
                        repeat_right = {
                            bgLayers = {{
                                sprite = "TILE",
                                offsetX = 0,
                                offsetY = 0,
                                scaleX = 1,
                                scaleY = 1,
                                zDistance = 1,
                                flags = { "V_REPEAT", "REPEAT_RIGHT_ONLY" }
                            }}
                        }
                    }
                }
                setWorldScale(20)
                setMaxWorldScale(20)
                setTheme("repeat_left")
                "#,
        )
        .unwrap();

    set_background_geometry(
        &runtime,
        SpriteGeometry {
            min_x: -128.0,
            min_y: -128.0,
            max_x: 128.0,
            max_y: 128.0,
        },
    );
    runtime.execute_source("drawBackgroundNative(-1)").unwrap();

    let positions = {
        let mut bridge = runtime.render.lock().unwrap();
        let positions = bridge
            .commands
            .iter()
            .filter(|command| command.sprite == "TILE")
            .map(|command| (command.x, command.y))
            .collect::<Vec<_>>();
        bridge.commands.clear();
        positions
    };
    assert_eq!(
        positions,
        vec![
            (512.0, 384.0),
            (256.0, 384.0),
            (256.0, 128.00002),
            (256.0, -127.99999),
            (256.0, 640.0),
            (256.0, 896.0),
            (0.0, 384.0),
            (0.0, 128.00002),
            (0.0, -127.99999),
            (0.0, 640.0),
            (0.0, 896.0),
            (512.0, 128.00002),
            (512.0, -127.99999),
            (512.0, 640.0),
            (512.0, 896.0),
        ]
    );

    runtime
        .execute_source(
            r#"
                setTheme("repeat_right")
                "#,
        )
        .unwrap();
    set_background_geometry(
        &runtime,
        SpriteGeometry {
            min_x: -128.0,
            min_y: -128.0,
            max_x: 128.0,
            max_y: 128.0,
        },
    );
    runtime.execute_source("drawBackgroundNative(-1)").unwrap();
    let bridge = runtime.render.lock().unwrap();
    let positions = bridge
        .commands
        .iter()
        .filter(|command| command.sprite == "TILE")
        .map(|command| (command.x, command.y))
        .collect::<Vec<_>>();
    assert_eq!(
        positions,
        vec![
            (512.0, 384.0),
            (768.0, 384.0),
            (768.0, 128.00002),
            (768.0, -127.99999),
            (768.0, 640.0),
            (768.0, 896.0),
            (1024.0, 384.0),
            (1024.0, 128.00002),
            (1024.0, -127.99999),
            (1024.0, 640.0),
            (1024.0, 896.0),
            (512.0, 128.00002),
            (512.0, -127.99999),
            (512.0, 640.0),
            (512.0, 896.0),
        ]
    );
}

#[test]
fn theme_repeat_culls_an_offscreen_reference_but_keeps_visible_columns() {
    let runtime = StellaLua::new("/tmp").unwrap();
    configure_theme_camera_fixture(&runtime);
    register_test_sprite_sheet(&runtime, &["TILE"]);
    runtime
        .execute_source(
            r#"
                blockTable = {
                    themes = {
                        offscreen = {
                            bgLayers = {{
                                sprite = "TILE",
                                offsetX = -1024,
                                offsetY = 0,
                                scale = 1,
                                zDistance = 0,
                                flags = { "H_REPEAT" }
                            }}
                        }
                    }
                }
                setWorldScale(20)
                setMaxWorldScale(20)
                setTheme("offscreen")
                "#,
        )
        .unwrap();

    set_background_geometry(
        &runtime,
        SpriteGeometry {
            min_x: -128.0,
            min_y: -128.0,
            max_x: 128.0,
            max_y: 128.0,
        },
    );
    runtime.execute_source("drawBackgroundNative(-1)").unwrap();

    let bridge = runtime.render.lock().unwrap();
    let positions = bridge
        .commands
        .iter()
        .filter(|command| command.sprite == "TILE")
        .map(|command| command.x)
        .collect::<Vec<_>>();
    assert_eq!(positions, [0.0, 256.0, 512.0, 768.0, 1024.0]);
}

#[test]
fn theme_repeat_density_uses_refreshed_camera_scale_instead_of_fixed_twenty() {
    let runtime = StellaLua::new("/tmp").unwrap();
    configure_theme_camera_fixture(&runtime);
    register_test_sprite_sheet(&runtime, &["TILE"]);
    runtime
        .execute_source(
            r#"
                blockTable = {
                    themes = {
                        camera_scaled = {
                            bgLayers = {{
                                sprite = "TILE",
                                offsetX = 0,
                                offsetY = 0,
                                scale = 1,
                                zDistance = 0,
                                flags = { "H_REPEAT" }
                            }}
                        }
                    }
                }
                setWorldScale(4)
                setMaxWorldScale(4)
                setTheme("camera_scaled")
                "#,
        )
        .unwrap();
    set_background_geometry(
        &runtime,
        SpriteGeometry {
            min_x: -8.0,
            min_y: -8.0,
            max_x: 8.0,
            max_y: 8.0,
        },
    );
    {
        let mut bridge = runtime.render.lock().unwrap();
        bridge.resolution_camera_scale = 4.0;
        bridge.theme_camera = ThemeCameraReference {
            valid: true,
            x: 128.0,
            y: 96.0,
            saved_y: 96.0,
            scale: 4.0,
            ..ThemeCameraReference::default()
        };
    }

    runtime.execute_source("drawBackgroundNative(-1)").unwrap();
    let bridge = runtime.render.lock().unwrap();
    let tiles = bridge
        .commands
        .iter()
        .filter(|command| command.sprite == "TILE")
        .collect::<Vec<_>>();
    assert_eq!(tiles.len(), 65);
    assert!(tiles.iter().all(|command| command.state.scale_x == 1.0));
}

#[test]
fn theme_draw_uses_live_renderer_extent_instead_of_authored_asset_size() {
    let runtime = StellaLua::new_with_resolution("/tmp", 1280, 720).unwrap();
    configure_theme_camera_fixture(&runtime);
    register_test_sprite_sheet(&runtime, &["CENTER"]);
    runtime
        .execute_source(
            r#"
                -- Native resolution changes always invoke the shipped hook.
                resolutionChanged = function() end
                blockTable = {
                    themes = {
                        live_extent = {
                            bgLayers = {{
                                sprite = "CENTER",
                                offsetX = 0,
                                offsetY = 0,
                                scale = 1,
                                zDistance = 0
                            }}
                        }
                    }
                }
                setWorldScale(20)
                setMaxWorldScale(20)
                setTheme("live_extent")
                drawBackgroundNative(-1)
            "#,
        )
        .unwrap();
    {
        let mut bridge = runtime.render.lock().unwrap();
        let command = bridge
            .commands
            .iter()
            .find(|command| command.sprite == "CENTER")
            .unwrap();
        assert_eq!((command.x, command.y), (640.5, 360.5));
        bridge.commands.clear();
    }

    assert!(runtime.set_screen_resolution(1400, 900).unwrap());
    runtime.execute_source("drawBackgroundNative(-1)").unwrap();
    let bridge = runtime.render.lock().unwrap();
    let command = bridge
        .commands
        .iter()
        .find(|command| command.sprite == "CENTER")
        .unwrap();
    // The no-op script hook does not move screen/ref camera on resize.
    // Native zDistance=0 remains world-anchored instead of recentering.
    assert_eq!((command.x, command.y), (640.5, 360.5));
}

#[test]
fn native_theme_culling_uses_center_bounds_instead_of_atlas_pivot_bounds() {
    let runtime = StellaLua::new("/tmp").unwrap();
    configure_theme_camera_fixture(&runtime);
    register_test_sprite_sheet(&runtime, &["ASYMMETRIC"]);
    runtime
        .execute_source(
            r#"
                blockTable = {
                    themes = {
                        asymmetric = {
                            bgLayers = {{
                                sprite = "ASYMMETRIC",
                                offsetX = 1024,
                                offsetY = 0,
                                scale = 1,
                                zDistance = 0
                            }}
                        }
                    }
                }
                setWorldScale(1)
                setMaxWorldScale(1)
                setTheme("asymmetric")
                "#,
        )
        .unwrap();
    set_background_geometry(
        &runtime,
        SpriteGeometry {
            min_x: 0.0,
            min_y: -50.0,
            max_x: 100.0,
            max_y: 50.0,
        },
    );
    {
        let mut bridge = runtime.render.lock().unwrap();
        bridge.resolution_camera_scale = 1.0;
        bridge.theme_camera = ThemeCameraReference {
            valid: true,
            x: 0.0,
            y: 384.0,
            saved_y: 384.0,
            scale: 1.0,
            ..ThemeCameraReference::default()
        };
    }

    runtime.execute_source("drawBackgroundNative(-1)").unwrap();
    let bridge = runtime.render.lock().unwrap();
    let command = bridge
        .commands
        .iter()
        .find(|command| command.sprite == "ASYMMETRIC")
        .expect("native center bounds still intersect the right edge");
    assert_eq!((command.x, command.y), (1074.0, 384.0));
    assert_eq!(
        (command.state.translate_x, command.state.translate_y),
        (-50.0, -50.0)
    );
    assert_eq!(command.state.sprite_pivot, Some([0.0, 0.0]));
}

#[test]
fn native_theme_centers_an_asymmetric_composite_around_the_culling_origin() {
    let runtime = StellaLua::new("/tmp").unwrap();
    configure_theme_camera_fixture(&runtime);
    register_test_sprite_sheet_with_sizes(&runtime, &[("COMPOSITE_PART", 100, 40)]);
    let unique = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let file_name = format!("stella-theme-composite-{}-{unique}.dat", std::process::id());
    let path = runtime.data_root().join(&file_name);
    fs::write(
        &path,
        test_composite_set_with_part("ASYMMETRIC_COMPOSITE", "COMPOSITE_PART"),
    )
    .unwrap();
    runtime
        .execute_source(&format!(
            r#"
                res.createCompoSpriteSet("{file_name}")
                res.setCompoSpriteEntry("ASYMMETRIC_COMPOSITE", 0, {{
                    x = 30,
                    y = -10
                }})
                blockTable = {{
                    themes = {{
                        asymmetric_composite = {{
                            bgLayers = {{{{
                                sprite = "ASYMMETRIC_COMPOSITE",
                                offsetX = 0,
                                offsetY = 0,
                                scale = 1,
                                zDistance = 0
                            }}}}
                        }}
                    }}
                }}
                setWorldScale(1)
                setMaxWorldScale(1)
                setTheme("asymmetric_composite")
                "#
        ))
        .unwrap();
    fs::remove_file(path).unwrap();
    {
        let mut bridge = runtime.render.lock().unwrap();
        let geometry = bridge.theme_background_layers[0].geometry;
        assert_eq!(
            (
                geometry.min_x,
                geometry.min_y,
                geometry.max_x,
                geometry.max_y,
            ),
            (-20.0, -30.0, 80.0, 10.0)
        );
        bridge.resolution_camera_scale = 1.0;
        bridge.theme_camera = ThemeCameraReference {
            valid: true,
            x: 0.0,
            y: 384.0,
            saved_y: 384.0,
            scale: 1.0,
            ..ThemeCameraReference::default()
        };
    }

    runtime.execute_source("drawBackgroundNative(-1)").unwrap();
    let bridge = runtime.render.lock().unwrap();
    let command = bridge
        .commands
        .iter()
        .find(|command| command.sprite == "ASYMMETRIC_COMPOSITE")
        .expect("the asymmetric composite intersects the viewport");
    assert_eq!((command.x, command.y), (30.0, 374.0));
    assert_eq!(
        (command.state.translate_x, command.state.translate_y),
        (-30.0, 10.0)
    );
    assert_eq!(command.state.sprite_pivot, None);
}

#[test]
fn native_theme_repeats_accumulate_float32_world_coordinates() {
    let runtime = StellaLua::new("/tmp").unwrap();
    configure_theme_camera_fixture(&runtime);
    register_test_sprite_sheet(&runtime, &["FLOAT_REPEAT"]);
    runtime
        .execute_source(
            r#"
                blockTable = {
                    themes = {
                        float_repeat = {
                            bgLayers = {{
                                sprite = "FLOAT_REPEAT",
                                offsetX = -1000,
                                offsetY = 0,
                                scale = 1,
                                zDistance = 0,
                                flags = { "H_REPEAT" }
                            }}
                        }
                    }
                }
                setWorldScale(3)
                setMaxWorldScale(3)
                setTheme("float_repeat")
                "#,
        )
        .unwrap();
    set_background_geometry(
        &runtime,
        SpriteGeometry {
            min_x: -3.0,
            min_y: -3.0,
            max_x: 4.0,
            max_y: 4.0,
        },
    );
    {
        let mut bridge = runtime.render.lock().unwrap();
        bridge.resolution_camera_scale = 3.0;
        bridge.theme_camera = ThemeCameraReference {
            valid: true,
            x: 0.0,
            y: 128.0,
            scale: 3.0,
            ..ThemeCameraReference::default()
        };
    }

    runtime.execute_source("drawBackgroundNative(-1)").unwrap();
    let bridge = runtime.render.lock().unwrap();
    let first_visible = bridge
        .commands
        .iter()
        .find(|command| command.sprite == "FLOAT_REPEAT")
        .expect("a repeated column must enter the viewport");

    // Literal register sequence: the centered authored offset is divided by
    // the reference scale, then every FADD updates that world-space S value.
    let mut expected_world = -999.5_f32 / 3.0_f32;
    let step_world = 7.0_f32 / 3.0_f32;
    for _ in 0..143 {
        expected_world += step_world;
    }
    let expected_screen = expected_world * 3.0_f32;
    assert_eq!(first_visible.x, expected_screen);
    assert_ne!(first_visible.x, 0.5);
}
