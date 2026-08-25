use super::*;

#[test]
fn shipped_gamelogic_owns_initial_screen_physics_scale_and_script_clocks() {
    let data_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../runtime/data");
    if !data_root.join("scripts_common/gamelogic.lua").is_file() {
        return;
    }
    let runtime = StellaLua::new(data_root).unwrap();
    let globals = runtime.lua().globals();
    let environment = game_environment(runtime.lua()).unwrap();

    assert!(matches!(
        globals.raw_get::<Value>("screen").unwrap(),
        Value::Nil
    ));
    assert!(matches!(
        environment.raw_get::<Value>("screen").unwrap(),
        Value::Nil
    ));

    let tracked_names = [
        "physicsScale",
        "worldScale",
        "time",
        "g_time",
        "deltaTime",
        "currentTimeStep",
        "playtimeCounter",
    ];
    for name in tracked_names {
        assert!(matches!(
            globals.raw_get::<Value>(name).unwrap(),
            Value::Nil
        ));
        assert!(matches!(
            environment.raw_get::<Value>(name).unwrap(),
            Value::Nil
        ));
    }

    let assignments = Rc::new(RefCell::new(BTreeMap::<String, f64>::new()));
    let captured = Rc::clone(&assignments);
    let metatable = environment.metatable().unwrap();
    metatable
        .set(
            "__newindex",
            runtime
                .lua()
                .create_function(move |_, (table, key, value): (mlua::Table, Value, Value)| {
                    if let Value::String(name) = &key {
                        let name = name.to_string_lossy();
                        if matches!(
                            name.as_str(),
                            "physicsScale"
                                | "worldScale"
                                | "time"
                                | "g_time"
                                | "deltaTime"
                                | "currentTimeStep"
                                | "playtimeCounter"
                        ) {
                            captured
                                .borrow_mut()
                                .insert(name, value_number(&value).unwrap());
                        }
                    }
                    table.raw_set(key, value)
                })
                .unwrap(),
        )
        .unwrap();

    runtime.execute("scripts_common/gamelogic.lua").unwrap();
    let expected = BTreeMap::from([("physicsScale".to_owned(), 0.05)]);
    assert_eq!(&*assignments.borrow(), &expected);
    assert!(matches!(
        globals.raw_get::<Value>("screen").unwrap(),
        Value::Nil
    ));
    let screen = environment.raw_get::<mlua::Table>("screen").unwrap();
    assert_eq!(screen.get::<f64>("left").unwrap(), 0.0);
    assert_eq!(screen.get::<f64>("top").unwrap(), 0.0);
    assert_eq!(screen.get::<f64>("right").unwrap(), 1024.0);
    assert_eq!(screen.get::<f64>("bottom").unwrap(), 768.0);
    assert!(matches!(
        screen.raw_get::<Value>("width").unwrap(),
        Value::Nil
    ));
    assert!(matches!(
        screen.raw_get::<Value>("height").unwrap(),
        Value::Nil
    ));
    assert_eq!(environment.raw_get::<f64>("physicsScale").unwrap(), 0.05);
    assert!(matches!(
        environment.raw_get::<Value>("worldScale").unwrap(),
        Value::Nil
    ));

    runtime.finish_gamelogic_load().unwrap();
    assert_eq!(environment.raw_get::<f64>("physicsScale").unwrap(), 0.05);
    for name in &tracked_names[1..] {
        assert!(matches!(
            environment.raw_get::<Value>(*name).unwrap(),
            Value::Nil
        ));
    }

    let booted = StellaLua::new(runtime.data_root()).unwrap();
    booted.boot("scripts/game.lua").unwrap();
    let booted_environment = game_environment(booted.lua()).unwrap();
    assert_eq!(
        booted_environment.raw_get::<f64>("physicsScale").unwrap(),
        0.05
    );
    for name in &tracked_names[1..] {
        assert!(matches!(
            booted_environment.raw_get::<Value>(*name).unwrap(),
            Value::Nil
        ));
    }

    let clock_assignments = Rc::new(RefCell::new(Vec::<(String, f64)>::new()));
    let captured = Rc::clone(&clock_assignments);
    booted_environment
        .metatable()
        .unwrap()
        .set(
            "__newindex",
            booted
                .lua()
                .create_function(move |_, (table, key, value): (mlua::Table, Value, Value)| {
                    if let Value::String(name) = &key {
                        let name = name.to_string_lossy();
                        if matches!(name.as_str(), "time" | "playtimeCounter") {
                            captured
                                .borrow_mut()
                                .push((name, value_number(&value).unwrap()));
                        }
                    }
                    table.raw_set(key, value)
                })
                .unwrap(),
        )
        .unwrap();
    let update = booted_environment.get::<Function>("update").unwrap();
    let frame_delta = f64::from(1.0_f32 / 60.0_f32);
    update.call::<()>((frame_delta, frame_delta)).unwrap();
    assert_eq!(
        &*clock_assignments.borrow(),
        &[
            ("time".to_owned(), 0.0),
            ("playtimeCounter".to_owned(), 0.0)
        ]
    );
    update.call::<()>((frame_delta, frame_delta)).unwrap();
    assert_eq!(
        booted_environment.raw_get::<f64>("time").unwrap(),
        frame_delta
    );
    assert_eq!(
        booted_environment
            .raw_get::<f64>("playtimeCounter")
            .unwrap(),
        frame_delta
    );
    for name in ["g_time", "deltaTime", "currentTimeStep"] {
        assert!(matches!(
            booted_environment.raw_get::<Value>(name).unwrap(),
            Value::Nil
        ));
    }
}

#[test]
fn host_draw_invokes_the_shipped_outer_callback_without_a_second_drawcalls_pass() {
    let runtime = StellaLua::new("/tmp").unwrap();
    runtime
        .execute_source(
            r#"
                draw_order = {}
                DrawCalls = {
                    initDraw = function()
                        table.insert(draw_order, "host-init-drawcalls")
                    end,
                    draw = function()
                        table.insert(draw_order, "host-drain-drawcalls")
                    end,
                }
                draw = function()
                    table.insert(draw_order, "menu")
                    table.insert(draw_order, "notifications")
                    table.insert(draw_order, "notification-particles")
                    table.insert(draw_order, "loading-screen")
                    table.insert(draw_order, "subsystems")
                    table.insert(draw_order, "menu-particles")
                end
            "#,
        )
        .unwrap();

    assert!(runtime.draw().unwrap());
    let environment = game_environment(runtime.lua()).unwrap();
    let order = environment.get::<mlua::Table>("draw_order").unwrap();
    let order = order
        .sequence_values::<String>()
        .collect::<mlua::Result<Vec<_>>>()
        .unwrap();
    assert_eq!(
        order,
        [
            "menu",
            "notifications",
            "notification-particles",
            "loading-screen",
            "subsystems",
            "menu-particles",
        ]
    );
}

#[test]
fn bitmap_font_metrics_spacing_and_baseline_match_native_font_records() {
    let data_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../runtime/data");
    let runtime = StellaLua::new(data_root).unwrap();
    runtime
        .execute_source(
            r#"
                width_without_font_ok = pcall(function() return res.getStringWidth("ABC") end)
                res.createBitmapFont("fonts/1024x768/FONT_CRIMSON_BASIC.dat")
                res.useFont("FONT_CRIMSON_BASIC")
                font_width = res.getStringWidth("ABC")
                font_ascending = res.getFontMaxAscending()
                font_descending = res.getFontMaxDescending()
                font_leading = res.getFontLeading()
                font_tracking = res.getFontTracking()
                font_height = res.getFontHeight()
                res.useFont("DOES_NOT_EXIST")
                preserved_font_width = res.getStringWidth("ABC")
                res.drawString("TEXTS_BASIC", "MISSING_KEY", 12, 34, "RIGHT", "BOTTOM")
                "#,
        )
        .unwrap();
    let environment = game_environment(runtime.lua()).unwrap();
    assert!(!environment.get::<bool>("width_without_font_ok").unwrap());
    assert_eq!(environment.get::<f64>("font_width").unwrap(), 110.0);
    assert_eq!(environment.get::<f64>("font_ascending").unwrap(), 59.0);
    assert_eq!(environment.get::<f64>("font_descending").unwrap(), 15.0);
    assert_eq!(environment.get::<f64>("font_leading").unwrap(), 67.0);
    assert_eq!(environment.get::<f64>("font_tracking").unwrap(), -3.0);
    assert_eq!(environment.get::<f64>("font_height").unwrap(), 74.0);
    assert_eq!(
        environment.get::<f64>("preserved_font_width").unwrap(),
        110.0
    );
    let commands = runtime.take_text_commands();
    assert_eq!(commands.len(), 1);
    assert_eq!(commands[0].text, "MISSING_KEY");
    assert_eq!(commands[0].font, "FONT_CRIMSON_BASIC");
    assert_eq!(commands[0].horizontal_anchor, "RIGHT");
    assert_eq!(commands[0].vertical_anchor, "BOTTOM");
}

#[test]
fn locale_and_width_dispatchers_require_exact_strings_and_ignore_extras() {
    let data_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../runtime/data");
    let runtime = StellaLua::new(data_root).unwrap();
    runtime
        .execute_source(
            r#"
                res.createBitmapFont("fonts/1024x768/FONT_CRIMSON_BASIC.dat")
                res.useFont("FONT_CRIMSON_BASIC")
                load_numeric_group_rejected = not pcall(
                    res.loadLocale, 1, "en_EN")
                load_numeric_locale_rejected = not pcall(
                    res.loadLocale, "ABSENT_GROUP", 1)
                load_trailing_accepted = pcall(
                    res.loadLocale, "ABSENT_GROUP", "en_EN", "ignored")
                use_numeric_rejected = not pcall(res.useLocale, 1)
                use_trailing_accepted = pcall(
                    res.useLocale, "en_EN", "ignored")
                width_numeric_rejected = not pcall(res.getStringWidth, 123)
                width_trailing_accepted, width_trailing = pcall(
                    res.getStringWidth, "ABC", "ignored")
                "#,
        )
        .unwrap();

    let environment = game_environment(runtime.lua()).unwrap();
    for name in [
        "load_numeric_group_rejected",
        "load_numeric_locale_rejected",
        "load_trailing_accepted",
        "use_numeric_rejected",
        "use_trailing_accepted",
        "width_numeric_rejected",
        "width_trailing_accepted",
    ] {
        assert!(environment.get::<bool>(name).unwrap(), "{name}");
    }
    assert_eq!(environment.get::<f64>("width_trailing").unwrap(), 110.0);
}

#[test]
fn clip_text_uses_localized_font_width_break_set_and_forced_split_contract() {
    let (fixed_lines, fixed_widest) = native_clip_text_lines("one two-three\nfour", 7.0, |line| {
        line.chars().count() as i32
    });
    assert_eq!(fixed_lines, ["one", "two-", "three", "four"]);
    assert_eq!(fixed_widest, 5);
    let (forced_lines, forced_widest) =
        native_clip_text_lines("abcdefgh", 3.0, |line| line.chars().count() as i32);
    assert_eq!(forced_lines, ["abc", "def", "gh"]);
    assert_eq!(forced_widest, 3);

    let runtime = StellaLua::new("/tmp").unwrap();
    runtime
        .execute_source(
            r#"
                res.createSystemFont("CLIP_FONT", "Arial", 2, 255, 255, 255, 255)
                res.useFont("CLIP_FONT")
                wrapped_expected_width = res.getStringWidth("three")
                clippedText.identityMarker = "preserved"
                retainedClippedText = clippedText
                clippedText = { shadowMarker = true }
                clipText("TEXTS_BASIC", "one two-three\nfour", 7)
                wrapped = retainedClippedText
                wrapped_lines = wrapped.lines
                wrapped_widest = wrapped.widestLine
                clipText("TEXTS_BASIC", "abcdefgh", 3)
                forced = retainedClippedText
                clipped_text_identity_preserved = rawequal(wrapped, forced)
                clipped_text_shadow_untouched = clippedText.lines == nil
                wrapped_lines_replaced = not rawequal(wrapped_lines, forced.lines)
                forced_lines = forced.lines
                forced_widest = forced.widestLine
                forced_recombined = table.concat(forced_lines)
                forced_measured_widest = 0
                for _, line in ipairs(forced_lines) do
                    forced_measured_widest = math.max(
                        forced_measured_widest, res.getStringWidth(line)
                    )
                end
                clip_trailing_accepted = pcall(
                    clipText, "TEXTS_BASIC", "text", 7, false
                )
                clipText("TEXTS_BASIC", "", 7)
                empty = retainedClippedText
                bad_clip_arity_fails = not pcall(
                    clipText, "TEXTS_BASIC", "text"
                )
                bad_clip_group_fails = not pcall(
                    clipText, 1, "text", 7
                )
                bad_clip_key_fails = not pcall(
                    clipText, "TEXTS_BASIC", 2, 7
                )
                bad_clip_width_fails = not pcall(
                    clipText, "TEXTS_BASIC", "text", "7"
                )
                "#,
        )
        .unwrap();
    let environment = game_environment(runtime.lua()).unwrap();
    let wrapped_lines: mlua::Table = environment.get("wrapped_lines").unwrap();
    assert_eq!(wrapped_lines.raw_len(), 4);
    for (index, expected) in ["one", "two-", "three", "four"].into_iter().enumerate() {
        assert_eq!(
            wrapped_lines.raw_get::<String>(index + 1).unwrap(),
            expected
        );
    }
    assert_eq!(
        environment.get::<f64>("wrapped_widest").unwrap(),
        environment.get::<f64>("wrapped_expected_width").unwrap()
    );
    assert!(
        environment
            .get::<bool>("clipped_text_identity_preserved")
            .unwrap()
    );
    assert!(
        environment
            .get::<bool>("clipped_text_shadow_untouched")
            .unwrap()
    );
    assert!(environment.get::<bool>("wrapped_lines_replaced").unwrap());

    let forced_lines: mlua::Table = environment.get("forced_lines").unwrap();
    assert!(forced_lines.raw_len() >= 2);
    assert_eq!(
        environment.get::<String>("forced_recombined").unwrap(),
        "abcdefgh"
    );
    assert_eq!(
        environment.get::<f64>("forced_widest").unwrap(),
        environment.get::<f64>("forced_measured_widest").unwrap()
    );

    let empty: mlua::Table = environment.get("empty").unwrap();
    assert_eq!(empty.get::<mlua::Table>("lines").unwrap().raw_len(), 0);
    assert_eq!(empty.get::<f64>("widestLine").unwrap(), 0.0);
    assert_eq!(empty.get::<String>("identityMarker").unwrap(), "preserved");
    assert!(environment.get::<bool>("bad_clip_arity_fails").unwrap());
    for name in [
        "bad_clip_group_fails",
        "bad_clip_key_fails",
        "bad_clip_width_fails",
        "clip_trailing_accepted",
    ] {
        assert!(environment.get::<bool>(name).unwrap(), "{name}");
    }
}

#[test]
fn game_lua_constructor_publishes_native_gesture_and_clip_tables() {
    let runtime = StellaLua::new("/tmp").unwrap();
    let environment = game_environment(runtime.lua()).unwrap();

    let cursor = environment.get::<mlua::Table>("cursor").unwrap();
    assert!(cursor.pairs::<Value, Value>().next().is_none());
    assert!(matches!(
        environment.get::<Value>("multitouchSweep").unwrap(),
        Value::Table(_)
    ));
    let zoom = environment.get::<mlua::Table>("multitouchZoom").unwrap();
    assert_eq!(zoom.get::<f64>("zoomCoolingTime").unwrap(), -1.0);
    let clipped_text = environment.get::<mlua::Table>("clippedText").unwrap();
    assert_eq!(clipped_text.raw_len(), 0);
    assert!(clipped_text.pairs::<Value, Value>().next().is_none());

    runtime
        .execute_source(
            r#"
                nativeGestureWeak = setmetatable(
                    { multitouchSweep, multitouchZoom },
                    { __mode = "v" }
                )
                multitouchSweep = { shadow = true }
                multitouchZoom = { shadow = true }
                collectgarbage("collect")
                collectgarbage("collect")
                nativeSweepRetained = nativeGestureWeak[1] ~= nil
                nativeZoomRetained = nativeGestureWeak[2] ~= nil
            "#,
        )
        .unwrap();
    assert!(environment.get::<bool>("nativeSweepRetained").unwrap());
    assert!(environment.get::<bool>("nativeZoomRetained").unwrap());
}

#[test]
fn native_constructor_does_not_publish_script_pointer_event_literals() {
    let sandbox = ShippedDataSandbox::new("script-pointer-event-literals");
    let runtime = StellaLua::new(&sandbox.data_root).unwrap();
    let globals = runtime.lua().globals();
    let names = [
        "LBUTTON", "RBUTTON", "LPRESS", "LHOLD", "LRELEASE", "RPRESS", "RHOLD", "RRELEASE",
        "HOVER", "PRESS", "RELEASE", "WHEEL",
    ];
    for name in names {
        assert!(matches!(
            globals.raw_get::<Value>(name).unwrap(),
            Value::Nil
        ));
    }

    runtime.boot("scripts/game.lua").unwrap();
    let environment = game_environment(runtime.lua()).unwrap();
    for name in names {
        assert!(matches!(
            environment.raw_get::<Value>(name).unwrap(),
            Value::Nil
        ));
    }

    // The native touch bridge owns the LBUTTON key-table field directly and
    // does not depend on a same-named Lua global.
    runtime.set_cursor(12.0, 34.0, true).unwrap();
    let key_pressed = environment.get::<mlua::Table>("keyPressed").unwrap();
    let key_hold = environment.get::<mlua::Table>("keyHold").unwrap();
    assert!(key_pressed.get::<bool>("LBUTTON").unwrap());
    assert!(key_hold.get::<bool>("LBUTTON").unwrap());
    let cursor = environment.get::<mlua::Table>("cursor").unwrap();
    assert_eq!(cursor.get::<f64>("x").unwrap(), 12.0);
    assert_eq!(cursor.get::<f64>("y").unwrap(), 34.0);
    assert!(matches!(
        cursor.raw_get::<Value>("down").unwrap(),
        Value::Nil
    ));
}

#[test]
fn constructor_input_tables_keep_native_identity_after_shadowing() {
    let runtime = StellaLua::new("/tmp").unwrap();
    runtime.execute_source("function update() end").unwrap();
    let environment = game_environment(runtime.lua()).unwrap();
    let native_pressed = environment.get::<mlua::Table>("keyPressed").unwrap();
    let native_released = environment.get::<mlua::Table>("keyReleased").unwrap();
    let native_hold = environment.get::<mlua::Table>("keyHold").unwrap();
    let native_cursor = environment.get::<mlua::Table>("cursor").unwrap();

    runtime
        .execute_source(
            r#"
                keyPressed = { shadow = true }
                keyReleased = { shadow = true }
                keyHold = { shadow = true }
                cursor = { shadow = true }
            "#,
        )
        .unwrap();

    runtime.set_key("KEY_BACK", true).unwrap();
    runtime.set_cursor(12.0, 34.0, true).unwrap();
    runtime.mouse_wheel(1, false, false).unwrap();
    assert!(native_pressed.get::<bool>("KEY_BACK").unwrap());
    assert!(native_hold.get::<bool>("KEY_BACK").unwrap());
    assert!(native_pressed.get::<bool>("LBUTTON").unwrap());
    assert!(native_hold.get::<bool>("LBUTTON").unwrap());
    assert_eq!(native_cursor.get::<f64>("x").unwrap(), 12.0);
    assert_eq!(native_cursor.get::<f64>("y").unwrap(), 34.0);
    assert_eq!(native_cursor.get::<f64>("wheel").unwrap(), 1.0);
    assert!(native_cursor.get::<bool>("wheelTriggered").unwrap());

    for name in ["keyPressed", "keyReleased", "keyHold", "cursor"] {
        let shadow = environment.get::<mlua::Table>(name).unwrap();
        assert!(shadow.get::<bool>("shadow").unwrap());
        assert!(shadow.get::<Option<Value>>("KEY_BACK").unwrap().is_none());
        assert!(shadow.get::<Option<Value>>("LBUTTON").unwrap().is_none());
        assert!(shadow.get::<Option<Value>>("wheel").unwrap().is_none());
    }

    assert!(runtime.update(0.0).unwrap());
    assert!(!native_pressed.get::<bool>("KEY_BACK").unwrap());
    assert!(!native_pressed.get::<bool>("LBUTTON").unwrap());
    assert!(!native_released.get::<bool>("KEY_BACK").unwrap());
    assert!(!native_cursor.get::<bool>("wheelTriggered").unwrap());
}

#[test]
fn game_lua_constructor_loads_the_three_native_persistent_tables() {
    let sandbox = ShippedDataSandbox::new("constructor-persistence");
    let appdata = sandbox.root.join("appdata");
    fs::write(
        appdata.join("highscores.lua"),
        "episode = 4\nscore = 12345\n",
    )
    .unwrap();
    fs::write(
        appdata.join("settings.lua"),
        "musicVolume = 0.375\nnested = { enabled = true }\n",
    )
    .unwrap();
    fs::write(appdata.join("bi_data.lua"), "launchCount = 9\n").unwrap();

    let runtime = StellaLua::new(&sandbox.data_root).unwrap();
    let environment = game_environment(runtime.lua()).unwrap();
    let highscores = environment.get::<mlua::Table>("highscores").unwrap();
    let settings = environment.get::<mlua::Table>("settings").unwrap();
    let bi_data = environment.get::<mlua::Table>("bi_data").unwrap();
    assert_eq!(highscores.get::<i64>("episode").unwrap(), 4);
    assert_eq!(highscores.get::<i64>("score").unwrap(), 12_345);
    assert_eq!(settings.get::<f64>("musicVolume").unwrap(), 0.375);
    assert!(
        settings
            .get::<mlua::Table>("nested")
            .unwrap()
            .get::<bool>("enabled")
            .unwrap()
    );
    assert_eq!(bi_data.get::<i64>("launchCount").unwrap(), 9);
}

#[test]
fn native_touch_publication_replaces_the_table_caps_at_two_and_formats_ids() {
    let runtime = StellaLua::new("/tmp").unwrap();
    let environment = game_environment(runtime.lua()).unwrap();
    assert!(matches!(
        environment.raw_get::<Value>("touches").unwrap(),
        Value::Nil
    ));
    assert!(matches!(
        environment.raw_get::<Value>("touchcount").unwrap(),
        Value::Nil
    ));
    runtime
        .set_touches(&[(7, 12, 34), (u64::from(u32::MAX), -5, 81), (9, 1, 2)])
        .unwrap();
    assert!(!runtime.update(0.0).unwrap());
    let first_published = environment.get::<mlua::Table>("touches").unwrap();
    environment.set("initialTouches", first_published).unwrap();
    assert!(!runtime.update(0.0).unwrap());
    runtime
        .execute_source(
            r#"
                touchesTableWasReplaced = not rawequal(initialTouches, touches)
                thirdTouchWasCapped = touches["9"] == nil
            "#,
        )
        .unwrap();

    assert!(environment.get::<bool>("touchesTableWasReplaced").unwrap());
    assert!(environment.get::<bool>("thirdTouchWasCapped").unwrap());
    assert_eq!(environment.get::<f64>("touchcount").unwrap(), 2.0);
    let touches = environment.get::<mlua::Table>("touches").unwrap();
    let first = touches.get::<mlua::Table>("7").unwrap();
    let signed_low_id = touches.get::<mlua::Table>("-1").unwrap();
    assert_eq!(first.get::<f64>("x").unwrap(), 12.0);
    assert_eq!(first.get::<f64>("y").unwrap(), 34.0);
    assert_eq!(signed_low_id.get::<f64>("x").unwrap(), -5.0);
    assert_eq!(signed_low_id.get::<f64>("y").unwrap(), 81.0);

    runtime.set_touches(&[]).unwrap();
    assert!(!runtime.update(0.0).unwrap());
    let next = environment.get::<mlua::Table>("touches").unwrap();
    assert_eq!(next.raw_len(), 0);
    assert_eq!(environment.get::<f64>("touchcount").unwrap(), 0.0);
}

#[test]
fn native_key_injection_separates_hold_press_release_and_repeat() {
    let runtime = StellaLua::new("/tmp").unwrap();
    runtime.execute_source("function update() end").unwrap();
    let environment = game_environment(runtime.lua()).unwrap();

    runtime.set_key("KEY_BACK", true).unwrap();
    let key_hold = environment.get::<mlua::Table>("keyHold").unwrap();
    let key_pressed = environment.get::<mlua::Table>("keyPressed").unwrap();
    let global_pressed = environment.get::<mlua::Table>("g_keyPressed").unwrap();
    assert!(key_hold.get::<bool>("KEY_BACK").unwrap());
    assert!(key_pressed.get::<bool>("KEY_BACK").unwrap());
    assert_eq!(global_pressed.raw_get::<String>(1).unwrap(), "KEY_BACK");

    assert!(runtime.update(0.0).unwrap());
    assert!(key_hold.get::<bool>("KEY_BACK").unwrap());
    assert!(!key_pressed.get::<bool>("KEY_BACK").unwrap());
    assert!(
        global_pressed
            .raw_get::<Option<String>>(1)
            .unwrap()
            .is_none()
    );
    for key in NATIVE_FRAME_KEYS {
        assert!(key_hold.get::<Option<bool>>(key).unwrap().is_some());
    }

    // Auto-repeat down events leave the native held byte set but do not set
    // the edge byte a second time.
    runtime.set_key("KEY_BACK", true).unwrap();
    assert!(!key_pressed.get::<bool>("KEY_BACK").unwrap());

    runtime.set_key("KEY_BACK", false).unwrap();
    let key_released = environment.get::<mlua::Table>("keyReleased").unwrap();
    assert!(!key_hold.get::<bool>("KEY_BACK").unwrap());
    assert!(key_released.get::<bool>("KEY_BACK").unwrap());
    assert!(runtime.update(0.0).unwrap());
    assert!(!key_released.get::<bool>("KEY_BACK").unwrap());
}

#[test]
fn view_disappearance_clears_touches_and_releases_only_the_primary_button() {
    let runtime = StellaLua::new("/tmp").unwrap();
    runtime.execute_source("function update() end").unwrap();
    runtime.set_cursor(12.0, 34.0, true).unwrap();
    runtime.set_key("KEY_BACK", true).unwrap();
    runtime.set_touches(&[(7, 12, 34), (8, 56, 78)]).unwrap();

    runtime.view_did_disappear(true).unwrap();
    let environment = game_environment(runtime.lua()).unwrap();
    let key_hold = environment.get::<mlua::Table>("keyHold").unwrap();
    let key_released = environment.get::<mlua::Table>("keyReleased").unwrap();
    assert!(!key_hold.get::<bool>("LBUTTON").unwrap());
    assert!(key_hold.get::<bool>("KEY_BACK").unwrap());
    assert!(key_released.get::<bool>("LBUTTON").unwrap());

    assert!(runtime.update(0.0).unwrap());
    let touches = environment.get::<mlua::Table>("touches").unwrap();
    assert_eq!(environment.get::<f64>("touchcount").unwrap(), 0.0);
    assert_eq!(touches.raw_len(), 0);
    assert!(!key_released.get::<bool>("LBUTTON").unwrap());

    // An already empty controller must not manufacture a release edge.
    runtime.view_did_disappear(false).unwrap();
    let key_released = environment.get::<mlua::Table>("keyReleased").unwrap();
    assert!(!key_released.get::<bool>("LBUTTON").unwrap());
}

#[test]
fn application_activation_gates_callbacks_until_loaded_and_clears_input_first() {
    let runtime = StellaLua::new("/tmp").unwrap();
    runtime
        .execute_source(
            r#"
                lifecycle = {}
                function gamePaused()
                    table.insert(lifecycle, {
                        name = "paused",
                        back = keyHold.KEY_BACK,
                        button = keyHold.LBUTTON,
                    })
                end
                function gameResumed()
                    table.insert(lifecycle, {
                        name = "resumed",
                        back = keyHold.KEY_BACK,
                        button = keyHold.LBUTTON,
                    })
                end
            "#,
        )
        .unwrap();
    runtime.set_key("KEY_BACK", true).unwrap();
    runtime.set_cursor(12.0, 34.0, true).unwrap();
    runtime.set_touches(&[(7, 12, 34)]).unwrap();

    runtime.set_application_active(false).unwrap();
    let environment = game_environment(runtime.lua()).unwrap();
    let lifecycle = environment.get::<mlua::Table>("lifecycle").unwrap();
    assert_eq!(lifecycle.raw_len(), 0);
    let key_hold = environment.get::<mlua::Table>("keyHold").unwrap();
    assert!(!key_hold.get::<bool>("KEY_BACK").unwrap());
    assert!(!key_hold.get::<bool>("LBUTTON").unwrap());

    runtime.gamelogic_loaded.set(true);
    runtime.set_key("KEY_BACK", true).unwrap();
    runtime.set_cursor(12.0, 34.0, true).unwrap();
    runtime.set_touches(&[(7, 12, 34)]).unwrap();
    runtime.set_application_active(false).unwrap();
    let paused = lifecycle.raw_get::<mlua::Table>(1).unwrap();
    assert_eq!(paused.get::<String>("name").unwrap(), "paused");
    assert!(!paused.get::<bool>("back").unwrap());
    assert!(!paused.get::<bool>("button").unwrap());

    // sub_100401678 clears only the hold bytes; an edge that arrived before
    // activation changed remains pending for the first resumed frame.
    let key_pressed = environment.get::<mlua::Table>("keyPressed").unwrap();
    assert!(key_pressed.get::<bool>("KEY_BACK").unwrap());
    runtime.set_application_active(true).unwrap();
    let resumed = lifecycle.raw_get::<mlua::Table>(2).unwrap();
    assert_eq!(resumed.get::<String>("name").unwrap(), "resumed");
    assert!(!resumed.get::<bool>("back").unwrap());
    assert!(!resumed.get::<bool>("button").unwrap());

    // AppController does not deduplicate its native stopUpdate dispatch.
    // applicationWillTerminate therefore delivers one final gamePaused even
    // when applicationWillResignActive already stopped the display link.
    runtime.set_application_active(false).unwrap();
    runtime.set_application_active(false).unwrap();
    assert_eq!(lifecycle.raw_len(), 4);
    for index in [3, 4] {
        let paused = lifecycle.raw_get::<mlua::Table>(index).unwrap();
        assert_eq!(paused.get::<String>("name").unwrap(), "paused");
        assert!(!paused.get::<bool>("back").unwrap());
        assert!(!paused.get::<bool>("button").unwrap());
    }

    assert!(!runtime.update(0.0).unwrap());
    assert_eq!(environment.get::<f64>("touchcount").unwrap(), 0.0);
}

#[test]
fn application_audio_activation_honours_nested_setting_and_device_lifetime() {
    let runtime = StellaLua::new("/tmp").unwrap();
    {
        let mut resources = runtime
            .resource_runtime
            .lock()
            .expect("resource runtime lock poisoned");
        resources.audio_output_created = true;
        resources.audio_output_started = true;
        resources.audio_input_created = true;
        resources.audio_input_started = true;
        resources.master_volume = -1.0;
    }

    assert!(runtime.set_application_audio_active(false).unwrap());
    {
        let resources = runtime
            .resource_runtime
            .lock()
            .expect("resource runtime lock poisoned");
        assert!(!resources.audio_output_started);
        assert!(!resources.audio_input_started);
    }

    runtime
        .execute_source("settings.root = { audioEnabled = false }")
        .unwrap();
    assert!(runtime.set_application_audio_active(true).unwrap());
    {
        let resources = runtime
            .resource_runtime
            .lock()
            .expect("resource runtime lock poisoned");
        assert!(!resources.audio_output_started);
        assert!(resources.audio_input_started);
        assert_eq!(resources.master_volume, -1.0);
    }

    runtime
        .execute_source("settings.root.audioEnabled = true")
        .unwrap();
    assert!(runtime.set_application_audio_active(true).unwrap());
    let resources = runtime
        .resource_runtime
        .lock()
        .expect("resource runtime lock poisoned");
    assert!(resources.audio_output_started);
    assert!(resources.audio_input_started);
    assert_eq!(resources.master_volume, 1.0);
}

#[test]
fn active_frame_recovers_stopped_output_only_for_exact_true_setting() {
    let runtime = StellaLua::new("/tmp").unwrap();
    runtime
        .execute_source(
            r#"
                settings.root = { audioEnabled = true }
                update = function() end
            "#,
        )
        .unwrap();
    {
        let mut resources = runtime
            .resource_runtime
            .lock()
            .expect("resource runtime lock poisoned");
        resources.audio_output_created = true;
        resources.audio_output_started = false;
        resources.master_volume = -1.0;
    }

    // Activation starts the output immediately. Stop it behind GameApp's
    // back, then prove the next native frame repairs that actual device byte.
    runtime.set_application_audio_active(true).unwrap();
    {
        let mut resources = runtime.resource_runtime.lock().unwrap();
        resources.audio_output_started = false;
        resources.master_volume = -1.0;
    }
    runtime.update(0.0).unwrap();
    {
        let resources = runtime.resource_runtime.lock().unwrap();
        assert!(resources.audio_output_started);
        assert_eq!(resources.master_volume, 1.0);
    }

    for setting in ["false", "'not-a-boolean'", "nil"] {
        runtime
            .execute_source(&format!("settings.root.audioEnabled = {setting}"))
            .unwrap();
        runtime
            .resource_runtime
            .lock()
            .unwrap()
            .audio_output_started = false;
        runtime.update(0.0).unwrap();
        assert!(
            !runtime
                .resource_runtime
                .lock()
                .unwrap()
                .audio_output_started
        );
    }

    runtime
        .execute_source("settings.root.audioEnabled = true")
        .unwrap();
    runtime.set_application_audio_active(false).unwrap();
    runtime.update(0.0).unwrap();
    assert!(
        !runtime
            .resource_runtime
            .lock()
            .unwrap()
            .audio_output_started
    );
}

#[test]
fn native_two_touch_distance_drives_half_delta_user_zoom() {
    let _pinch_guard = lock_native_pinch_for_test();
    let runtime = StellaLua::new("/tmp").unwrap();
    runtime
        .execute_source(
            r#"
                zoomDeltas = {}
                function applyUserZoom(delta)
                    table.insert(zoomDeltas, delta)
                end
                setWorldScale(2)
            "#,
        )
        .unwrap();

    runtime.set_touches(&[(1, 0, 0), (2, 3, 4)]).unwrap();
    assert!(!runtime.update(0.0).unwrap());
    assert!(!runtime.update(0.0).unwrap());
    runtime.set_touches(&[(1, 0, 0), (2, 6, 8)]).unwrap();
    assert!(!runtime.update(0.0).unwrap());
    runtime.set_touches(&[(1, 0, 0), (2, 9, 12)]).unwrap();
    assert!(!runtime.update(0.0).unwrap());

    let environment = game_environment(runtime.lua()).unwrap();
    let deltas = environment.get::<mlua::Table>("zoomDeltas").unwrap();
    assert_eq!(deltas.raw_len(), 2);
    assert_eq!(deltas.raw_get::<f64>(1).unwrap(), 1.0);
    assert_eq!(deltas.raw_get::<f64>(2).unwrap(), 1.0);

    // Leaving the exact-two-touch state ends the gesture. Re-entering with
    // two touches establishes a new baseline and emits no stale delta.
    runtime
        .set_touches(&[(1, 0, 0), (2, 9, 12), (3, 1, 1)])
        .unwrap();
    assert!(!runtime.update(0.0).unwrap());
    runtime.set_touches(&[(1, 0, 0), (2, 18, 24)]).unwrap();
    assert!(!runtime.update(0.0).unwrap());
    assert_eq!(deltas.raw_len(), 2);

    // Purple keeps the active flag and both gesture baselines in three
    // process globals (`byte_100C0FF20`, `dword_100C0FF24/+28`), not in
    // GameApp. A second runtime therefore inherits an unfinished gesture.
    let second_runtime = StellaLua::new("/tmp").unwrap();
    second_runtime
        .execute_source(
            r#"
                inheritedZoomDeltas = {}
                function applyUserZoom(delta)
                    table.insert(inheritedZoomDeltas, delta)
                end
            "#,
        )
        .unwrap();
    second_runtime
        .set_touches(&[(11, 0, 0), (12, 36, 48)])
        .unwrap();
    assert!(!second_runtime.update(0.0).unwrap());
    let inherited = game_environment(second_runtime.lua())
        .unwrap()
        .get::<mlua::Table>("inheritedZoomDeltas")
        .unwrap();
    assert_eq!(inherited.raw_len(), 1);
    assert_eq!(inherited.raw_get::<f64>(1).unwrap(), 1.5);

    second_runtime.set_touches(&[]).unwrap();
    assert!(!second_runtime.update(0.0).unwrap());
}

#[test]
fn native_smooth_mouse_wheel_uses_cubic_ease_and_retargets_in_flight() {
    let runtime = StellaLua::new("/tmp").unwrap();
    runtime
        .execute_source(
            r#"
                zoomDeltas = {}
                function applyUserZoom(delta)
                    table.insert(zoomDeltas, delta)
                end
                setMaxWorldScale(4)
                resetMouseWheelScale(1)
            "#,
        )
        .unwrap();

    runtime.mouse_wheel(1, false, false).unwrap();
    let cursor = runtime
        .lua()
        .globals()
        .get::<mlua::Table>("cursor")
        .unwrap();
    assert_eq!(cursor.get::<f64>("wheel").unwrap(), 1.0);
    assert!(cursor.get::<bool>("wheelTriggered").unwrap());
    {
        let bridge = runtime.render.lock().unwrap();
        assert!(bridge.smooth_zooming);
        assert_eq!(bridge.input_zoom.smooth_start, 1.0);
        assert_eq!(bridge.input_zoom.smooth_target, 1.2_f32);
        assert_eq!(bridge.input_zoom.smooth_elapsed, 0.0);
        assert_eq!(bridge.input_zoom.smooth_duration, 0.5);
    }

    assert!(!runtime.update(0.25).unwrap());
    let shifted = 0.1_f32 / 0.5_f32 - 1.0_f32;
    let eased = shifted.mul_add(shifted * shifted, 1.0_f32);
    let expected_current = (1.2_f32 - 1.0_f32).mul_add(eased, 1.0_f32);
    let environment = game_environment(runtime.lua()).unwrap();
    let deltas = environment.get::<mlua::Table>("zoomDeltas").unwrap();
    assert_eq!(deltas.raw_len(), 1);
    assert_eq!(
        deltas.raw_get::<f64>(1).unwrap(),
        f64::from((expected_current - 1.0_f32) * 0.5_f32)
    );
    assert!(!cursor.get::<bool>("wheelTriggered").unwrap());

    runtime.mouse_wheel(1, false, false).unwrap();
    let bridge = runtime.render.lock().unwrap();
    let expected_target = 1.0_f32.mul_add(0.2_f32 * 0.5_f32, 1.2_f32);
    assert_eq!(bridge.input_zoom.smooth_start, expected_current);
    assert_eq!(bridge.input_zoom.smooth_target, expected_target);
    assert_eq!(bridge.input_zoom.smooth_elapsed, 0.0);
    assert_eq!(bridge.input_zoom.smooth_duration, 0.9_f32);
}

#[test]
fn native_direct_mouse_wheel_honours_shift_and_control() {
    let runtime = StellaLua::new("/tmp").unwrap();
    runtime
        .execute_source(
            r#"
                zoomDeltas = {}
                function applyUserZoom(delta)
                    table.insert(zoomDeltas, delta)
                end
                setGameParameters({ gameWorldScale = 2 })
                enableSmoothZooming(false)
                resetMouseWheelScale(0.5)
            "#,
        )
        .unwrap();

    runtime.mouse_wheel(2, true, false).unwrap();
    let expected_current = 0.5_f32 + 2.0_f32 * ((0.1_f64 / 2.0_f64) as f32 * 0.05_f32);
    assert_eq!(
        runtime.render.lock().unwrap().input_zoom.current,
        expected_current
    );
    assert!(!runtime.update(0.0).unwrap());
    let environment = game_environment(runtime.lua()).unwrap();
    let deltas = environment.get::<mlua::Table>("zoomDeltas").unwrap();
    assert_eq!(deltas.raw_len(), 1);
    assert_eq!(
        deltas.raw_get::<f64>(1).unwrap(),
        f64::from((expected_current - 0.5_f32) * 0.5_f32)
    );

    runtime.mouse_wheel(-3, false, true).unwrap();
    assert_eq!(
        runtime.render.lock().unwrap().input_zoom.current,
        expected_current
    );
    assert!(!runtime.update(0.0).unwrap());
    assert_eq!(deltas.raw_len(), 1);
}

#[test]
fn resource_draw_string_is_strict_and_uses_combined_anchor_and_context_matrix() {
    let data_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../runtime/data");
    let runtime = StellaLua::new(data_root).unwrap();
    runtime
        .execute_source(
            r#"
                res.createBitmapFont("fonts/1024x768/FONT_CRIMSON_BASIC.dat")
                res.useFont("FONT_CRIMSON_BASIC")
                setRenderState(10, 20, 2, 3, 1.5707963267948966, 4, 5, 0.6)
                res.drawString("TEXTS_BASIC", "RAW_TEXT", 7, 8, "BOTTOM", "RIGHT")
                missing_coordinates_fail = not pcall(
                    res.drawString, "TEXTS_BASIC", "RAW_TEXT", 7
                )
                bad_optional_anchor_fails = not pcall(
                    res.drawString, "TEXTS_BASIC", "RAW_TEXT", 7, 8, 123
                )
                unknown_anchor_fails = not pcall(
                    res.drawString, "TEXTS_BASIC", "RAW_TEXT", 7, 8, "NOT_AN_ANCHOR"
                )
                "#,
        )
        .unwrap();

    let environment = game_environment(runtime.lua()).unwrap();
    assert!(environment.get::<bool>("missing_coordinates_fail").unwrap());
    assert!(
        environment
            .get::<bool>("bad_optional_anchor_fails")
            .unwrap()
    );
    assert!(environment.get::<bool>("unknown_anchor_fails").unwrap());
    let bridge = runtime.render.lock().unwrap();
    assert_eq!(bridge.text_commands.len(), 1);
    let command = &bridge.text_commands[0];
    assert_eq!(command.text, "RAW_TEXT");
    assert_eq!(command.horizontal_anchor, "RIGHT");
    assert_eq!(command.vertical_anchor, "BOTTOM");
    let state = RenderState {
        translate_x: 10.0,
        translate_y: 20.0,
        scale_x: 2.0,
        scale_y: 3.0,
        angle: f64::from(std::f64::consts::FRAC_PI_2 as f32),
        pivot_x: 4.0,
        pivot_y: 5.0,
        alpha: f64::from(0.6_f32),
        ..RenderState::default()
    };
    let expected = native_state_screen_point(state, 7.0, 8.0);
    assert!((command.x - expected[0]).abs() < 1e-12);
    assert!((command.y - expected[1]).abs() < 1e-12);
    let cosine = state.angle.cos();
    let sine = state.angle.sin();
    let expected_matrix = [2.0 * cosine, -2.0 * sine, 3.0 * sine, 3.0 * cosine];
    for (actual, expected) in command.matrix.unwrap().into_iter().zip(expected_matrix) {
        assert!((actual - expected).abs() < 1e-12);
    }
    assert_eq!(command.alpha, f64::from(0.6_f32));
    // ResourceManager.drawString only borrows the live context; unlike
    // drawUITextNative, it does not replace any state field.
    assert_eq!(bridge.state.scale_x, 2.0);
    assert_eq!(bridge.state.scale_y, 3.0);
    assert_eq!(bridge.state.pivot_x, 4.0);
    assert_eq!(bridge.state.pivot_y, 5.0);
}

#[test]
fn composite_resource_tables_and_partial_entry_updates_match_native_shape() {
    let data_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../runtime/data");
    let runtime = StellaLua::new(data_root).unwrap();
    runtime
        .execute_source(
            r##"
                ResourceManager.native_createSpriteSheet(
                    "images/1024x768/BUTTONS_SHEET_1.dat"
                )
                res.createCompoSpriteSet(
                    "images/1024x768/BUTTONS_COMPOSPRITES.dat"
                )
                compo_data = res.getCompoSpriteData("BTN_OPTIONS_SMALL")
                compo_first = res.getCompoSpriteEntry("BTN_OPTIONS_SMALL", 0)
                compo_named = res.getCompoSpriteEntry("BTN_OPTIONS_SMALL", compo_first.name)
                compo_bound_count = select("#", res.getCompoSpriteBounds("BTN_OPTIONS_SMALL"))
                missing_bound_count = select("#", res.getCompoSpriteBounds("DOES_NOT_EXIST"))
                colon_bound_count = select("#", res:getCompoSpriteBounds("BTN_OPTIONS_SMALL"))
                sprite_colon_width, sprite_colon_height = res:getSpriteBounds("BTN_OPTIONS_SMALL")
                bounds_wrong_slot_fails = not pcall(
                    res.getSpriteBounds, "BTN_OPTIONS_SMALL", 123, "BTN_OPTIONS_SMALL"
                )
                pivot_wrong_slot_fails = not pcall(
                    res.getSpritePivot, "BTN_OPTIONS_SMALL", 123, "BTN_OPTIONS_SMALL"
                )
                compo_wrong_slot_fails = not pcall(
                    res.getCompoSpriteBounds, "BTN_OPTIONS_SMALL", 123, "BTN_OPTIONS_SMALL"
                )
                res.setCompoSpriteEntry("BTN_OPTIONS_SMALL", 0, {
                    x = compo_first.x + 10,
                    flipX = not compo_first.flipX,
                    visible = false
                })
                compo_updated = res.getCompoSpriteEntry("BTN_OPTIONS_SMALL", 0)
                res.setCompoSpriteEntry("BTN_OPTIONS_SMALL", 0, {
                    x = "12.5",
                    y = {},
                    scaleX = true,
                    flipX = 0,
                    flipY = false,
                    visible = ""
                })
                compo_coerced = res.getCompoSpriteEntry("BTN_OPTIONS_SMALL", 0)
                "##,
        )
        .unwrap();
    let environment = game_environment(runtime.lua()).unwrap();
    let data: mlua::Table = environment.get("compo_data").unwrap();
    assert!(data.raw_len() > 0);
    let first: mlua::Table = environment.get("compo_first").unwrap();
    let named: mlua::Table = environment.get("compo_named").unwrap();
    let updated: mlua::Table = environment.get("compo_updated").unwrap();
    let coerced: mlua::Table = environment.get("compo_coerced").unwrap();
    assert_eq!(
        first.get::<String>("name").unwrap(),
        named.get::<String>("name").unwrap()
    );
    assert_eq!(
        updated.get::<f64>("x").unwrap(),
        first.get::<f64>("x").unwrap() + 10.0
    );
    assert_ne!(
        updated.get::<bool>("flipX").unwrap(),
        first.get::<bool>("flipX").unwrap()
    );
    assert!(!updated.get::<bool>("visible").unwrap());
    assert_eq!(coerced.get::<f64>("x").unwrap(), 12.5);
    assert_eq!(coerced.get::<f64>("y").unwrap(), 0.0);
    assert_eq!(coerced.get::<f64>("scaleX").unwrap(), 0.0);
    // Lua 5.1 boolean conversion treats numeric zero and empty strings as
    // true. Only nil and the boolean false are false.
    assert!(coerced.get::<bool>("flipX").unwrap());
    assert!(!coerced.get::<bool>("flipY").unwrap());
    assert!(coerced.get::<bool>("visible").unwrap());
    assert_eq!(environment.get::<i64>("compo_bound_count").unwrap(), 4);
    assert_eq!(environment.get::<i64>("missing_bound_count").unwrap(), 0);
    assert_eq!(environment.get::<i64>("colon_bound_count").unwrap(), 4);
    assert!(environment.get::<f64>("sprite_colon_width").unwrap() > 0.0);
    assert!(environment.get::<f64>("sprite_colon_height").unwrap() > 0.0);
    for name in [
        "bounds_wrong_slot_fails",
        "pivot_wrong_slot_fails",
        "compo_wrong_slot_fails",
    ] {
        assert!(environment.get::<bool>(name).unwrap(), "{name}");
    }
    let updates = runtime.take_composite_updates();
    assert_eq!(
        updates.get("BTN_OPTIONS_SMALL").unwrap().len(),
        data.raw_len()
    );
}

#[test]
fn composite_resource_handwritten_stack_abi_matches_native_dispatch_order() {
    let data_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../runtime/data");
    let runtime = StellaLua::new(data_root).unwrap();
    runtime
        .execute_source(
            r##"
                ResourceManager.native_createSpriteSheet(
                    "images/1024x768/BUTTONS_SHEET_1.dat"
                )
                res.createCompoSpriteSet(
                    "images/1024x768/BUTTONS_COMPOSPRITES.dat"
                )
                local composite = "BTN_OPTIONS_SMALL"
                local first = res.getCompoSpriteEntry(composite, 0)

                negative_fraction = res.getCompoSpriteEntry(composite, -0.75)
                missing_selector_count = select("#", res.getCompoSpriteEntry(composite))
                invalid_selector_count = select("#", res.getCompoSpriteEntry(composite, {}))
                missing_resource_count = select("#",
                    res.getCompoSpriteEntry("DOES_NOT_EXIST", "0"))
                invalid_set_count = select("#",
                    res.setCompoSpriteEntry(composite, {}, false))
                missing_set_count = select("#",
                    res.setCompoSpriteEntry("DOES_NOT_EXIST", "0", false))

                numeric_string_get_fails = not pcall(
                    res.getCompoSpriteEntry, composite, "0")
                numeric_string_set_fails = not pcall(
                    res.setCompoSpriteEntry, composite, "0", {})
                valid_set_requires_table = not pcall(
                    res.setCompoSpriteEntry, composite, 0, false)
                get_requires_string_name = not pcall(
                    res.getCompoSpriteEntry, 123, 0)
                data_requires_string_name = not pcall(
                    res.getCompoSpriteData, 123)
                data_colon_call_fails = not pcall(function()
                    return res:getCompoSpriteData(composite)
                end)
                entry_colon_call_fails = not pcall(function()
                    return res:getCompoSpriteEntry(composite, 0)
                end)
                set_colon_call_fails = not pcall(function()
                    return res:setCompoSpriteEntry(composite, 0, {})
                end)

                -- Purple's raw part access would dereference an invalid
                -- pointer for these selectors. The Rust host preserves the
                -- failure boundary as a recoverable Lua error.
                out_of_range_get_fails = not pcall(
                    res.getCompoSpriteEntry, composite, -1)
                unknown_name_get_fails = not pcall(
                    res.getCompoSpriteEntry, composite, "DOES_NOT_EXIST")
                nan_index_get_fails = not pcall(
                    res.getCompoSpriteEntry, composite, 0 / 0)
                missing_data_fails = not pcall(
                    res.getCompoSpriteData, "DOES_NOT_EXIST")

                local renamed = first.name .. "#RUNTIME_INSTANCE"
                res.setCompoSpriteEntry(composite, 0, {
                    name = renamed,
                    x = " 0x10 ",
                    y = "not-a-number"
                })
                renamed_entry = res.getCompoSpriteEntry(composite, renamed)
                old_name_get_fails = not pcall(
                    res.getCompoSpriteEntry, composite, first.name)
            "##,
        )
        .unwrap();

    let environment = game_environment(runtime.lua()).unwrap();
    let negative_fraction: mlua::Table = environment.get("negative_fraction").unwrap();
    let renamed: mlua::Table = environment.get("renamed_entry").unwrap();
    assert_eq!(
        negative_fraction.get::<String>("name").unwrap(),
        renamed
            .get::<String>("name")
            .unwrap()
            .trim_end_matches("#RUNTIME_INSTANCE")
    );
    assert_eq!(renamed.get::<f64>("x").unwrap(), 16.0);
    assert_eq!(renamed.get::<f64>("y").unwrap(), 0.0);
    for name in [
        "missing_selector_count",
        "invalid_selector_count",
        "missing_resource_count",
        "invalid_set_count",
        "missing_set_count",
    ] {
        assert_eq!(environment.get::<i64>(name).unwrap(), 0, "{name}");
    }
    for name in [
        "numeric_string_get_fails",
        "numeric_string_set_fails",
        "valid_set_requires_table",
        "get_requires_string_name",
        "data_requires_string_name",
        "data_colon_call_fails",
        "entry_colon_call_fails",
        "set_colon_call_fails",
        "out_of_range_get_fails",
        "unknown_name_get_fails",
        "nan_index_get_fails",
        "missing_data_fails",
        "old_name_get_fails",
    ] {
        assert!(environment.get::<bool>(name).unwrap(), "{name}");
    }
    let updates = runtime.take_composite_updates();
    assert_eq!(
        updates["BTN_OPTIONS_SMALL"][0].sprite,
        renamed.get::<String>("name").unwrap()
    );
}
