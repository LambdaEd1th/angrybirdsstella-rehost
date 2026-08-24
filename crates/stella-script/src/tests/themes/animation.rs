use super::super::*;

#[test]
fn theme_animation_binding_uses_recovered_table_fields_and_lua51_coercions() {
    let runtime = StellaLua::new("/tmp").unwrap();
    runtime
        .execute_source(
            r#"
                createThemeAnimation({
                    layer = 2,
                    name = "animated",
                    spriteName = "START",
                    x = 7,
                    y = 9,
                    scaleX = 1.25,
                    scaleY = 1.5,
                    angle = 0.4,
                    velX = 2,
                    velY = 3,
                    scaleSpeed = 0.5,
                    startAnimTimer = 4,
                    animDelay = 0.2,
                    startingDelay = 0.7,
                    isAnimation = true,
                    bLoop = true,
                    animation = { "FRAME_1", "FRAME_2" }
                })
                createThemeAnimation({ name = "not_top", spriteName = "IGNORED" }, 3)
                createThemeAnimation({
                    layer = 3,
                    name = "stops_at_wrong_type",
                    spriteName = "START",
                    x = 0,
                    y = 0,
                    scaleX = 1,
                    scaleY = 1,
                    angle = 0,
                    velX = 0,
                    velY = 0,
                    scaleSpeed = 0,
                    startAnimTimer = 0,
                    animDelay = 0.1,
                    startingDelay = 0.1,
                    isAnimation = true,
                    bLoop = true,
                    animation = { "ONLY", false, "SKIPPED" }
                })
                createThemeAnimation({
                    layer = "0x4",
                    name = 123,
                    spriteName = 456,
                    x = "7.5",
                    y = "-2.25",
                    scaleX = "1.125",
                    scaleY = "0.75",
                    angle = "0.5",
                    velX = "2.5",
                    velY = "-3.5",
                    scaleSpeed = "0.25",
                    startAnimTimer = "1.5",
                    animDelay = "0.125",
                    isAnimation = 1,
                    bLoop = "true",
                    animation = { "FRAME_A", 7, "FRAME_C", false, "SKIPPED" }
                })
                "#,
        )
        .unwrap();

    let bridge = runtime.render.lock().unwrap();
    assert!(
        !bridge
            .theme_sprites
            .keys()
            .any(|(_, name)| name == "not_top")
    );
    let sprite = &bridge.theme_sprites[&(2, "animated".to_owned())];
    assert_eq!(sprite.sprite, "START");
    assert_eq!((sprite.original_x, sprite.original_y), (7.0, 9.0));
    assert_eq!(sprite.animation_frames, ["FRAME_1", "FRAME_2"]);
    assert_eq!(sprite.animation_start_timer, 4.0);
    assert_eq!(sprite.animation_frame_time, f64::from(0.2_f32));
    assert_eq!(sprite.animation_timer, f64::from(0.7_f32));
    assert!(sprite.is_animation);
    assert!(sprite.animation_looping);
    assert_eq!(
        bridge.theme_sprites[&(3, "stops_at_wrong_type".to_owned())].animation_frames,
        ["ONLY"]
    );
    let coerced = &bridge.theme_sprites[&(4, "123".to_owned())];
    assert_eq!(coerced.sprite, "456");
    assert_eq!((coerced.x, coerced.y), (7.5, -2.25));
    assert_eq!((coerced.scale_x, coerced.scale_y), (1.125, 0.75));
    assert_eq!((coerced.velocity_x, coerced.velocity_y), (2.5, -3.5));
    assert_eq!(coerced.animation_start_timer, 1.5);
    assert_eq!(coerced.animation_frame_time, 0.125);
    assert_eq!(coerced.animation_timer, 0.125);
    assert_eq!(coerced.animation_frames, ["FRAME_A", "7", "FRAME_C"]);
    assert!(!coerced.is_animation);
    assert!(!coerced.animation_looping);
}
