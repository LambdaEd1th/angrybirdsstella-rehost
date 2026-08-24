use super::super::*;

#[test]
fn theme_spawn_parameters_expand_layers_in_shared_cmwc_order() {
    let runtime = StellaLua::new("/tmp").unwrap();
    runtime
        .execute_source(
            r#"
                blockTable = {
                    themes = {
                        spawned = {
                            bgLayers = {
                                {
                                    sprite = { "A", "B" },
                                    velX = 1,
                                    velY = -2,
                                    xSpeedAdd = 0.5,
                                    ySpeedAdd = -0.25,
                                    animationSpeed = 1,
                                    animationTimeline = { { 0.5, 0.25 }, 0.75 },
                                    spawnParameters = {
                                        amount = 2,
                                        xSpeedVariance = 4,
                                        ySpeedVariance = 8,
                                        area = {
                                            screenX = 100,
                                            screenY = 200,
                                            screenW = 20,
                                            screenH = 40,
                                            worldX = 0.25,
                                            worldY = 0.5,
                                            worldW = 0.2,
                                            worldH = 0.4
                                        }
                                    }
                                },
                                {
                                    sprite = "OMITTED",
                                    spawnParameters = { amount = 0 }
                                },
                                { sprite = "PLAIN" }
                            }
                        }
                    }
                }
                setTheme("spawned")
                "#,
        )
        .unwrap();

    let bridge = runtime.render.lock().unwrap();
    assert_eq!(bridge.theme_background_layers.len(), 3);
    let mut control = NativeParticleRandom::default();
    for layer in &bridge.theme_background_layers[..2] {
        let expected_velocity_x = 1.0_f32 + (control.next() * 4.0_f64) as f32 + 0.5_f32;
        let expected_velocity_y = -2.0_f32 + (control.next() * 8.0_f64) as f32 - 0.25_f32;
        let expected_x = control.next().mul_add(20.0, 90.0) as f32;
        let expected_y = control.next().mul_add(40.0, 180.0) as f32;
        let expected_timeline = (control.next() as f32).mul_add(0.25_f32, 0.5_f32);
        assert_eq!(
            (layer.velocity_x, layer.velocity_y),
            (
                f64::from(expected_velocity_x),
                f64::from(expected_velocity_y)
            )
        );
        assert_eq!(layer.offset_x, f64::from(expected_x));
        assert!(matches!(
            layer.offset_y,
            ThemeVerticalOffset::Pixels(value) if value == f64::from(expected_y)
        ));
        assert_eq!(layer.animation_timeline, [expected_timeline, 0.75_f32]);
        assert_eq!(layer.definition_index, 1);
        assert_eq!(
            (
                layer.world_x,
                layer.world_y,
                layer.world_width,
                layer.world_height
            ),
            (
                Some(0.25),
                Some(0.5),
                Some(f64::from(0.2_f32)),
                Some(f64::from(0.4_f32))
            )
        );
    }
    assert_eq!(bridge.theme_background_layers[2].sprite, "PLAIN");
    assert_eq!(bridge.theme_background_layers[2].definition_index, 3);
    assert_eq!(bridge.particle_random.index, control.index);
}

#[test]
fn animation_wrap_resamples_timeline_and_spawn_coordinates_with_native_indexing() {
    let runtime = StellaLua::new("/tmp").unwrap();
    runtime
        .execute_source(
            r#"
                blockTable = {
                    themes = {
                        refresh = {
                            bgLayers = {{
                                sprite = { "A", "B" },
                                animationTimeline = { { 0.1, 0.2 }, { 0.3, 0.4 } },
                                flags = {
                                    "REFRESH_ANIMATION_TIMELINE",
                                    "REFRESH_ANIMATION_COORDINATES"
                                },
                                spawnParameters = {
                                    amount = 1,
                                    area = {
                                        screenX = 30,
                                        screenY = 40,
                                        screenW = 10,
                                        screenH = 20,
                                        worldX = 0.25,
                                        worldY = 0.5,
                                        worldW = 0.2,
                                        worldH = 0.4
                                    }
                                }
                            }}
                        }
                    }
                }
                setTheme("refresh")
                "#,
        )
        .unwrap();

    let mut bridge = runtime.render.lock().unwrap();
    bridge.world_scale = 10.0;
    bridge.screen_width = 100;
    bridge.screen_height = 100;
    bridge.theme_background_layers[0].animation_frame = 1;
    bridge.theme_background_layers[0].animation_timer = 1.0;
    let original_first_delay = bridge.theme_background_layers[0].animation_timeline[0];
    let mut control = bridge.particle_random.clone();
    let replacement_second_delay = (control.next() as f32).mul_add(0.2_f32, 0.1_f32);
    let _discarded_one_past_end_sample = control.next();
    let _screen_x_sample = control.next();
    let _screen_y_sample = control.next();
    let world_width_sample = control.next() as f32;
    let world_height_sample = control.next() as f32;
    let expected_x = 10.0_f32.mul_add(0.2_f32 * (0.5_f32 - world_width_sample), 2.5_f32);
    let expected_y = 10.0_f32.mul_add(0.4_f32 * (0.5_f32 - world_height_sample), 5.0_f32);

    bridge.advance_native_theme_frame_with_limits(0.0, ThemeWorldLimits::default());
    let layer = &bridge.theme_background_layers[0];
    assert_eq!(layer.animation_frame, 0);
    assert_eq!(layer.animation_timeline[0], original_first_delay);
    assert_eq!(layer.animation_timeline[1], replacement_second_delay);
    assert_eq!(layer.offset_x, f64::from(expected_x));
    assert!(matches!(
        layer.offset_y,
        ThemeVerticalOffset::Pixels(value) if value == f64::from(expected_y)
    ));
    assert_eq!(bridge.particle_random.index, control.index);
}
