use super::*;

#[test]
fn global_draw_compo_sprite_matches_native_legacy_part_scaling() {
    let data_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../build/extracted/data");
    let runtime = StellaLua::new(data_root).unwrap();
    runtime
        .execute_source(
            r#"
                ResourceManager.native_createSpriteSheet(
                    "images/1024x768/BUTTONS_SHEET_1.dat"
                )
                res.createCompoSpriteSet(
                    "images/1024x768/BUTTONS_COMPOSPRITES.dat"
                )
                compo_parts = res.getCompoSpriteData("BTN_OPTIONS_SMALL")
                legacy_first = res.getCompoSpriteEntry("BTN_OPTIONS_SMALL", 0)
                legacy_pivot_x, legacy_pivot_y = res.getSpritePivot(legacy_first.name)
                res.setCompoSpriteEntry("BTN_OPTIONS_SMALL", 0, {
                    scaleX = 9,
                    scaleY = 8,
                    angle = 1.25,
                    flipX = true,
                    flipY = true,
                    visible = false
                })
                setRenderState(10, 20, 2, 3, 0.25, 4, 5, 0.6)
                drawCompoSprite("BTN_OPTIONS_SMALL", 100, 200, 0.5, 0.25)
                "#,
        )
        .unwrap();
    let environment = game_environment(runtime.lua()).unwrap();
    let parts: mlua::Table = environment.get("compo_parts").unwrap();
    let first: mlua::Table = environment.get("legacy_first").unwrap();
    let first_x = first.get::<f64>("x").unwrap();
    let first_y = first.get::<f64>("y").unwrap();
    let pivot_x = environment.get::<f64>("legacy_pivot_x").unwrap();
    let pivot_y = environment.get::<f64>("legacy_pivot_y").unwrap();
    let commands = runtime.take_render_commands();
    assert_eq!(commands.len(), parts.raw_len());
    let first_command = &commands[0];
    assert_eq!(first_command.sprite, first.get::<String>("name").unwrap());
    assert_eq!(first_command.state.scale_x, 1.0);
    assert_eq!(first_command.state.scale_y, 0.75);
    let angle = f64::from(0.25_f32);
    assert_eq!(first_command.state.angle, angle);
    assert_eq!(first_command.state.alpha, f64::from(0.6_f32));
    assert_eq!(first_command.state.pivot_x, (pivot_x - first_x) * 0.5);
    assert_eq!(first_command.state.pivot_y, (pivot_y - first_y) * 0.25);
    let cosine = angle.cos();
    let sine = angle.sin();
    let expected_matrix = [cosine, -0.5 * sine, 1.5 * sine, 0.75 * cosine];
    assert_eq!(first_command.state.matrix, Some(expected_matrix));
    let expected_x = 2.0 * (110.0 + cosine * 0.5 * first_x - sine * 0.25 * first_y);
    let expected_y = 3.0 * (220.0 + sine * 0.5 * first_x + cosine * 0.25 * first_y);
    assert!((first_command.x - expected_x).abs() < 1.0e-9);
    assert!((first_command.y - expected_y).abs() < 1.0e-9);
    // sub_10004DDA0 ignores the entry's scale/angle/flip/visible fields.
    assert_ne!(first_command.state.scale_x, 9.0);
    assert_ne!(first_command.state.angle, 1.25);
}

#[test]
fn sprite_draw_anchors_match_native_pivot_relative_offsets_and_overloads() {
    let data_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../build/extracted/data");
    let runtime = StellaLua::new(data_root).unwrap();
    runtime
        .execute_source(
            r#"
                ResourceManager.native_createSpriteSheet(
                    "images/1024x768/BUTTONS_SHEET_1.dat"
                )
                res.createCompoSpriteSet(
                    "images/1024x768/BUTTONS_COMPOSPRITES.dat"
                )
                res.drawSprite("BTN_OPTIONS_SMALL", 100, 200, "LEFT", "TOP")
                res:drawSprite("BTN_OPTIONS_SMALL", 100, 200, "RIGHT", "BOTTOM")
                res.drawSprite(
                    "BTN_OPTIONS_SMALL", 100, 200, "HPIVOT", "VPIVOT", 123, 234
                )
                anchor_error = not pcall(function()
                    res.drawSprite("BTN_OPTIONS_SMALL", 0, 0, "NOT_AN_ANCHOR")
                end)
                missing_coordinates_fail = not pcall(
                    res.drawSprite, "BTN_OPTIONS_SMALL"
                )
                non_string_anchor_fails = not pcall(
                    res.drawSprite, "BTN_OPTIONS_SMALL", 0, 0, 123
                )
                lone_size_argument_is_ignored = pcall(
                    res.drawSprite, "", 0, 0, "HPIVOT", "VPIVOT", "ignored"
                )
                bad_complete_size_pair_fails = not pcall(
                    res.drawSprite, "", 0, 0, "HPIVOT", "VPIVOT", "bad", 1
                )
                "#,
        )
        .unwrap();
    let environment = game_environment(runtime.lua()).unwrap();
    for name in [
        "anchor_error",
        "missing_coordinates_fail",
        "non_string_anchor_fails",
        "lone_size_argument_is_ignored",
        "bad_complete_size_pair_fails",
    ] {
        assert!(environment.get::<bool>(name).unwrap(), "{name}");
    }
    let commands = runtime.take_render_commands();
    assert_eq!(commands.len(), 3);
    assert_eq!((commands[0].x, commands[0].y), (139.0, 237.0));
    assert_eq!((commands[1].x, commands[1].y), (60.0, 162.0));
    assert_eq!(commands[2].state.draw_size, Some([123.0, 234.0]));
}

#[test]
fn rotated_atlas_draw_keeps_atlas_and_render_state_pivots_independent() {
    let data_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../build/extracted/data");
    let runtime = StellaLua::new(data_root).unwrap();
    runtime
        .execute_source(
            r#"
                ResourceManager.native_createSpriteSheet(
                    "images/1024x768/BUTTONS_SHEET_1.dat"
                )
                pivot_x, pivot_y = res.getSpritePivot("ICON_OFF")
                setRenderState(512, 384, 1, 1, math.pi / 2, pivot_x, pivot_y)
                res.drawSprite("ICON_OFF", 0, 0, "HPIVOT", "VPIVOT")
                "#,
        )
        .unwrap();

    let environment = game_environment(runtime.lua()).unwrap();
    assert_eq!(environment.get::<f64>("pivot_x").unwrap(), 36.0);
    assert_eq!(environment.get::<f64>("pivot_y").unwrap(), 36.0);

    let commands = runtime.take_render_commands();
    assert_eq!(commands.len(), 1);
    let command = &commands[0];
    // AtlasSprite supplies the raw rectangle (-atlasPivot, size) to
    // GL_Context.  The context's independent pivot remains live and must not
    // be folded into the atlas vertices a second time.
    assert_eq!((command.x, command.y), (-36.0, -36.0));
    assert_eq!(command.state.sprite_pivot, Some([0.0, 0.0]));
    assert_eq!((command.state.pivot_x, command.state.pivot_y), (36.0, 36.0));
}

#[test]
fn resource_clip_rect_is_truncated_and_captured_by_draw_submission() {
    let runtime = StellaLua::new("/tmp").unwrap();
    runtime
        .execute_source(
            r#"
                res.setClipRect(10.9, 20.9, 30.5, 40.5)
                clip_x, clip_y, clip_width, clip_height = res.getClipRect()
                setRenderState(1, 2, 3, 4, 5, 6, 7, 0.5)
                drawRect(255, 128, 64, 0.75, 0, 0, 100, 100, true)
                res.setClipRect(16777216, 0, 1, 1)
                wide_clip_x, wide_clip_y, wide_clip_width, wide_clip_height =
                    res.getClipRect()
                "#,
        )
        .unwrap();
    let environment = game_environment(runtime.lua()).unwrap();
    assert_eq!(environment.get::<f64>("clip_x").unwrap(), 10.0);
    assert_eq!(environment.get::<f64>("clip_y").unwrap(), 20.0);
    assert_eq!(environment.get::<f64>("clip_width").unwrap(), 31.0);
    assert_eq!(environment.get::<f64>("clip_height").unwrap(), 41.0);
    // The member adds the already narrowed float32 arguments; +1 is lost at
    // 2^24 instead of being summed in Lua-double precision.
    assert_eq!(environment.get::<f64>("wide_clip_x").unwrap(), 16_777_216.0);
    assert_eq!(environment.get::<f64>("wide_clip_y").unwrap(), 0.0);
    assert_eq!(environment.get::<f64>("wide_clip_width").unwrap(), 0.0);
    assert_eq!(environment.get::<f64>("wide_clip_height").unwrap(), 1.0);
    let commands = runtime.take_rect_commands();
    assert_eq!(commands.len(), 1);
    assert_eq!(commands[0].clip_rect, Some([10, 20, 41, 61]));
}

#[test]
fn mixed_draw_command_classes_share_one_native_submission_sequence() {
    let runtime = StellaLua::new("/tmp").unwrap();
    register_test_sprite_sheet(&runtime, &["ORDER_SPRITE"]);
    runtime
        .execute_source(
            r#"
                res.drawSprite("MISSING_RESOURCE_DOES_NOT_CONSUME_ORDER", 0, 0)
                drawRect(255, 255, 255, 1, 0, 0, 10, 10, true)
                res.drawSprite("ORDER_SPRITE", 0, 0)
                res.captureSprite("ORDER_CAPTURE")
                drawRect(255, 255, 255, 1, 10, 10, 20, 20, true)
                "#,
        )
        .unwrap();
    let sprites = runtime.take_render_commands();
    let rectangles = runtime.take_rect_commands();
    let captures = runtime.take_capture_commands();
    assert_eq!(sprites.len(), 1);
    assert_eq!(rectangles.len(), 2);
    assert_eq!(rectangles[0].order, 0);
    assert_eq!(sprites[0].order, 1);
    assert_eq!(
        captures,
        [CaptureRenderCommand {
            order: 2,
            name: "ORDER_CAPTURE".to_owned(),
        }]
    );
    assert_eq!(rectangles[1].order, 3);
}

#[test]
fn clear_screen_draws_at_the_call_point_and_preserves_prior_capture() {
    let runtime = StellaLua::new("/tmp").unwrap();
    runtime
        .execute_source(
            r#"
                setBGColor(12, 34, 56)
                drawRect(255, 0, 0, 1, 0, 0, 1024, 768, true)
                res.captureSprite("BEFORE_CLEAR")
                res.setClipRect(10, 20, 30, 40)
                setRenderState(100, 200, 2, 3, 0.5, 6, 7, 0.25)
                clearScreen()
                drawRect(0, 255, 0, 1, 1, 2, 3, 4, true)
                "#,
        )
        .unwrap();

    let bridge = runtime.render.lock().unwrap();
    assert_eq!(bridge.capture_commands.len(), 1);
    assert_eq!(bridge.capture_commands[0].order, 1);
    assert_eq!(bridge.capture_commands[0].name, "BEFORE_CLEAR");
    assert_eq!(bridge.rect_commands.len(), 3);

    let clear = &bridge.rect_commands[1];
    assert_eq!(clear.order, 2);
    assert_eq!(
        (clear.left, clear.top, clear.right, clear.bottom),
        (-32000.0, -32000.0, 32000.0, 32000.0)
    );
    assert_eq!(
        clear.vertices.as_deref(),
        Some(
            [
                [-32000.0, -32000.0],
                [32000.0, -32000.0],
                [-32000.0, 32000.0],
                [32000.0, 32000.0],
            ]
            .as_slice()
        )
    );
    assert_eq!(clear.mesh_topology, ColorMeshTopology::TriangleStrip);
    assert_eq!(clear.clip_rect, None);
    assert_eq!(
        (clear.red, clear.green, clear.blue, clear.alpha),
        (12.0 / 255.0, 34.0 / 255.0, 56.0 / 255.0, 1.0)
    );

    let after = &bridge.rect_commands[2];
    assert_eq!(after.order, 3);
    assert_eq!(after.clip_rect, None);
    assert_eq!(bridge.state.translate_x, 0.0);
    assert_eq!(bridge.state.translate_y, 0.0);
    assert_eq!(bridge.state.scale_x, 1.0);
    assert_eq!(bridge.state.scale_y, 1.0);
    assert_eq!(bridge.state.angle, 0.0);
    assert_eq!(bridge.state.alpha, 1.0);
}

#[test]
fn background_color_quantizes_to_f32_before_native_channel_packing() {
    let runtime = StellaLua::new("/tmp").unwrap();
    runtime
        .execute_source(
            r#"
                setBGColor(0.99999999, 254.9999999, -0.25)
                red, green, blue = getBGColor()
                missing_fails = not pcall(setBGColor, 1, 2)
                string_fails = not pcall(setBGColor, 1, "2", 3)
                boolean_fails = not pcall(setBGColor, true, 2, 3)
                red_after, green_after, blue_after = getBGColor()
                clearScreen()
                "#,
        )
        .unwrap();

    let environment = game_environment(runtime.lua()).unwrap();
    assert_eq!(environment.get::<f64>("red").unwrap(), 1.0);
    assert_eq!(environment.get::<f64>("green").unwrap(), 255.0);
    assert_eq!(environment.get::<f64>("blue").unwrap(), 0.0);
    assert!(environment.get::<bool>("missing_fails").unwrap());
    assert!(environment.get::<bool>("string_fails").unwrap());
    assert!(environment.get::<bool>("boolean_fails").unwrap());
    assert_eq!(environment.get::<f64>("red_after").unwrap(), 1.0);
    assert_eq!(environment.get::<f64>("green_after").unwrap(), 255.0);
    assert_eq!(environment.get::<f64>("blue_after").unwrap(), 0.0);

    let bridge = runtime.render.lock().unwrap();
    assert_eq!(bridge.background_color, [1, 255, 0]);
    let clear = bridge.rect_commands.last().unwrap();
    assert_eq!(
        (clear.red, clear.green, clear.blue),
        (1.0 / 255.0, 1.0, 0.0)
    );
}
