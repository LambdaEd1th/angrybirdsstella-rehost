use super::*;

#[test]
fn recovered_native_sprite_helpers_emit_every_requested_layer() {
    let data_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../runtime/data");
    let runtime = StellaLua::new(data_root).unwrap();
    runtime
        .execute_source(
            r#"
                ResourceManager.native_createSpriteSheet(
                    "images/1024x768/MENU_ELEMENTS_1.dat"
                )
                res.createBitmapFont("fonts/1024x768/FONT_CRIMSON_BASIC.dat")
                res.useFont("FONT_CRIMSON_BASIC")
                drawTexturedRect("TEXTURED_RECT", 10, 20, 30, 50, true)
                drawSelectedTexturizedObject("MASK", "FILL", 1, 2, 0.5, 0.75)
                renderMaskedImageNative("MASKED_IMAGE", 3, 4, 20, 30, 0, 0, 0.6, 20, 30)
                drawString3D("MISSING_GROUP", "Three dimensional", 7, 8, 9, 0.25, 2, 3, 0.5)
                drawBoxNative({
                    topLeft = "SAVE_FILE_POPUP_LEFT", topMiddle = "SAVE_FILE_POPUP_MID",
                    topRight = "SAVE_FILE_POPUP_RIGHT", left = "SAVE_FILE_POPUP_LEFT",
                    center = "SAVE_FILE_POPUP_MID", right = "SAVE_FILE_POPUP_RIGHT",
                    bottomLeft = "SAVE_FILE_POPUP_LEFT", bottomMiddle = "SAVE_FILE_POPUP_MID",
                    bottomRight = "SAVE_FILE_POPUP_RIGHT"
                }, 0, 0, 100, 50, 1, 1, "", "")
                "#,
        )
        .unwrap();

    let bridge = runtime.render.lock().unwrap();
    assert_eq!(bridge.commands.len(), 11);
    assert!(
        bridge
            .commands
            .iter()
            .all(|command| command.sprite != "TEXTURED_RECT")
    );
    let selected = bridge
        .commands
        .iter()
        .find(|command| command.sprite == "MASK")
        .unwrap();
    assert_eq!(selected.texture_name(), Some("FILL"));
    assert_eq!(selected.x, 40.0);
    assert_eq!(selected.y, (2.0_f32 * 20.0_f32) / 0.75_f32);
    assert_eq!(
        (selected.state.scale_x, selected.state.scale_y),
        (0.5, 0.75)
    );
    assert!(!selected.world_space);
    assert_eq!(
        (
            (selected.state.translate_x + selected.x) * selected.state.scale_x,
            (selected.state.translate_y + selected.y) * selected.state.scale_y,
        ),
        (20.0_f32, 40.0_f32)
    );
    assert_eq!(bridge.text_commands.len(), 1);
    assert_eq!(bridge.text_commands[0].text, "Three dimensional");
    assert_eq!(bridge.text_commands[0].font, "FONT_CRIMSON_BASIC");
    match bridge.text_commands[0].font_binding.as_ref().unwrap() {
        TextFontBinding::Bitmap { texture_source, .. } => {
            assert!(texture_source.ends_with("FONT_CRIMSON_BASIC.pvr"))
        }
        TextFontBinding::System(_) => panic!("shipped font is a bitmap IFont"),
    }
    assert_eq!(bridge.text_commands[0].angle, 0.0);
    assert_eq!(
        bridge.text_commands[0].projection_3d,
        Some(TextProjection3D {
            z: 9.0,
            rotation_x: 0.25,
        })
    );
}

#[test]
fn textured_render_generated_adapters_require_exact_tags_and_ignore_extras() {
    let data_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../runtime/data");
    let runtime = StellaLua::new(data_root).unwrap();
    runtime
        .execute_source(
            r#"
                res.createBitmapFont("fonts/1024x768/FONT_CRIMSON_BASIC.dat")
                res.useFont("FONT_CRIMSON_BASIC")

                textured_rejects_numeric_name = not pcall(
                    drawTexturedRect, 123, 0, 0, 1, 1, true
                )
                textured_rejects_string_number = not pcall(
                    drawTexturedRect, "MISSING", "0", 0, 1, 1, true
                )
                textured_rejects_numeric_boolean = not pcall(
                    drawTexturedRect, "MISSING", 0, 0, 1, 1, 1
                )
                textured_accepts_trailing = pcall(
                    drawTexturedRect, "MISSING", 0, 0, 1, 1, true, "ignored"
                )

                selected_rejects_numeric_sprite = not pcall(
                    drawSelectedTexturizedObject, 123, "MISSING", 0, 0, 1, 1
                )
                selected_rejects_numeric_texture = not pcall(
                    drawSelectedTexturizedObject, "MISSING", 123, 0, 0, 1, 1
                )
                selected_rejects_string_number = not pcall(
                    drawSelectedTexturizedObject, "MISSING", "MISSING", "0", 0, 1, 1
                )
                selected_accepts_trailing = pcall(
                    drawSelectedTexturizedObject,
                    "MISSING", "MISSING", 0, 0, 1, 1, "ignored"
                )

                masked_rejects_numeric_sprite = not pcall(
                    renderMaskedImageNative, 123, 0, 0, 1, 0, 1, 1, 0, 1, 1
                )
                masked_rejects_string_number = not pcall(
                    renderMaskedImageNative,
                    "MISSING", "0", 0, 1, 0, 1, 1, 0, 1, 1
                )
                masked_accepts_trailing = pcall(
                    renderMaskedImageNative,
                    "MISSING", 0, 0, 1, 0, 1, 1, 0, 1, 1, "ignored"
                )

                string3d_rejects_numeric_group = not pcall(
                    drawString3D, 123, "text", 0, 0, 1, 0, 1, 1, 1
                )
                string3d_rejects_numeric_key = not pcall(
                    drawString3D, "MISSING_GROUP", 123, 0, 0, 1, 0, 1, 1, 1
                )
                string3d_rejects_string_number = not pcall(
                    drawString3D, "MISSING_GROUP", "text", "0", 0, 1, 0, 1, 1, 1
                )
                string3d_accepts_trailing = pcall(
                    drawString3D,
                    "MISSING_GROUP", "text", 0, 0, 1, 0, 1, 1, 1, "ignored"
                )
                "#,
        )
        .unwrap();

    let environment = game_environment(runtime.lua()).unwrap();
    for name in [
        "textured_rejects_numeric_name",
        "textured_rejects_string_number",
        "textured_rejects_numeric_boolean",
        "textured_accepts_trailing",
        "selected_rejects_numeric_sprite",
        "selected_rejects_numeric_texture",
        "selected_rejects_string_number",
        "selected_accepts_trailing",
        "masked_rejects_numeric_sprite",
        "masked_rejects_string_number",
        "masked_accepts_trailing",
        "string3d_rejects_numeric_group",
        "string3d_rejects_numeric_key",
        "string3d_rejects_string_number",
        "string3d_accepts_trailing",
    ] {
        assert!(environment.get::<bool>(name).unwrap(), "{name}");
    }

    let bridge = runtime.render.lock().unwrap();
    assert_eq!(bridge.commands.len(), 2);
    assert_eq!(bridge.text_commands.len(), 1);
}

#[test]
fn string_3d_adapter_is_strict_and_does_not_replace_x_rotation_with_z_rotation() {
    let data_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../runtime/data");
    let runtime = StellaLua::new(data_root).unwrap();
    runtime
        .execute_source(
            r#"
                res.createBitmapFont("fonts/1024x768/FONT_CRIMSON_BASIC.dat")
                res.useFont("FONT_CRIMSON_BASIC")
                drawString3D("MISSING_GROUP", "text", 1, 2, 3, 0.75, 4, 5, 0.6)
                missing_3d_values_fail = pcall(drawString3D, "MISSING_GROUP", "text", 1)
            "#,
        )
        .unwrap();
    assert!(
        !game_environment(runtime.lua())
            .unwrap()
            .get::<bool>("missing_3d_values_fail")
            .unwrap()
    );
    let bridge = runtime.render.lock().unwrap();
    let text = &bridge.text_commands[0];
    assert_eq!(
        (text.x, text.y, text.scale_x, text.scale_y),
        (1.0, 2.0, 4.0, 5.0)
    );
    assert_eq!(text.alpha, f64::from(0.6_f32));
    assert_eq!(text.angle, 0.0);
    assert_eq!(
        text.projection_3d,
        Some(TextProjection3D {
            z: 3.0,
            rotation_x: 0.75,
        })
    );
}

#[test]
fn string_3d_uses_text_group_key_and_current_font_like_resource_draw_string() {
    let unique = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let root = std::env::temp_dir().join(format!("stella-text-3d-abi-{unique}"));
    let data_root = root.join("data");
    fs::create_dir_all(&data_root).unwrap();
    fs::create_dir_all(root.join("appdata")).unwrap();
    fs::write(
        data_root.join("FONT.dat"),
        test_bitmap_font_with_glyph("font-atlas.pvr", 6),
    )
    .unwrap();
    fs::write(
        data_root.join("TEXTS.dat"),
        test_localization_table("en_EN", "TITLE", "Localized title"),
    )
    .unwrap();

    let runtime = StellaLua::new(&data_root).unwrap();
    runtime
        .execute_source(
            r#"
                res.createBitmapFont("FONT.dat")
                res.useFont("FONT")
                res.createTextGroupSet("TEXTS.dat")
                res.loadLocale("TEXTS", "en_EN")
                res.useLocale("en_EN")
                drawString3D("TEXTS", "TITLE", 1, 2, 3, 0.25, 4, 5, 0.6)
            "#,
        )
        .unwrap();

    let bridge = runtime.render.lock().unwrap();
    let command = &bridge.text_commands[0];
    assert_eq!(command.text, "Localized title");
    assert_eq!(command.font, "FONT");
    match command.font_binding.as_ref().unwrap() {
        TextFontBinding::Bitmap {
            font,
            texture_source,
        } => {
            assert_eq!(font.glyphs[0].width, 6);
            assert!(texture_source.ends_with("font-atlas.pvr"));
        }
        TextFontBinding::System(_) => panic!("test font is a bitmap IFont"),
    }
    drop(bridge);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn submitted_text_keeps_constructed_font_and_texture_after_replace_and_release() {
    let unique = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let root = std::env::temp_dir().join(format!("stella-text-binding-{unique}"));
    let data_root = root.join("data");
    fs::create_dir_all(data_root.join("first")).unwrap();
    fs::create_dir_all(data_root.join("second")).unwrap();
    fs::create_dir_all(root.join("appdata")).unwrap();
    fs::write(
        data_root.join("first/FONT.dat"),
        test_bitmap_font_with_glyph("first.pvr", 3),
    )
    .unwrap();
    fs::write(data_root.join("first/first.pvr"), b"first texture").unwrap();
    fs::write(
        data_root.join("second/FONT.dat"),
        test_bitmap_font_with_glyph("second.pvr", 11),
    )
    .unwrap();
    fs::write(data_root.join("second/second.pvr"), b"second texture").unwrap();

    let runtime = StellaLua::new(&data_root).unwrap();
    runtime
        .execute_source(
            r#"
                res.createBitmapFont("first/FONT.dat")
                res.useFont("FONT")
            "#,
        )
        .unwrap();
    let (constructed_font, constructed_texture_source) = {
        let resources = runtime.resource_runtime.lock().unwrap();
        (
            Arc::clone(&resources.bitmap_font_values["FONT"]),
            resources.bitmap_font_texture_sources["FONT"].clone(),
        )
    };
    assert!(constructed_texture_source.ends_with("first/first.pvr"));

    // Move the only live texture candidate after construction. A per-draw
    // path resolution would now bind appdata/first.pvr, while Purple's
    // retained BitmapFont texture owner must keep first/first.pvr.
    fs::remove_file(data_root.join("first/first.pvr")).unwrap();
    fs::write(root.join("appdata/first.pvr"), b"late texture").unwrap();
    runtime
        .execute_source(
            r#"
                res.drawString("MISSING_GROUP", "A", 10, 20)
                res.drawString("MISSING_GROUP", "A", 30, 40)
                res.createBitmapFont("second/FONT.dat", true)
                res.releaseFont("FONT")
            "#,
        )
        .unwrap();

    let bridge = runtime.render.lock().unwrap();
    let first_command = &bridge.text_commands[0];
    let second_command = &bridge.text_commands[1];
    let (first_font, first_texture_source) = match first_command.font_binding.as_ref().unwrap() {
        TextFontBinding::Bitmap {
            font,
            texture_source,
        } => {
            assert_eq!(font.glyphs[0].width, 3);
            assert_eq!(font.texture, "first.pvr");
            assert!(texture_source.ends_with("first/first.pvr"));
            (font, texture_source)
        }
        TextFontBinding::System(_) => panic!("test font is a bitmap IFont"),
    };
    let (second_font, second_texture_source) = match second_command.font_binding.as_ref().unwrap() {
        TextFontBinding::Bitmap {
            font,
            texture_source,
        } => (font, texture_source),
        TextFontBinding::System(_) => panic!("test font is a bitmap IFont"),
    };
    assert!(Arc::ptr_eq(first_font, second_font));
    assert!(Arc::ptr_eq(first_font, &constructed_font));
    assert_eq!(first_texture_source, &constructed_texture_source);
    assert_eq!(second_texture_source, &constructed_texture_source);
    {
        let resources = runtime.resource_runtime.lock().unwrap();
        assert!(!resources.bitmap_font_texture_sources.contains_key("FONT"));
    }
    drop(bridge);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn submitted_system_text_retains_native_argb_stroke_metrics_and_face_after_release() {
    let data_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../runtime/data");
    let runtime = StellaLua::new(data_root).unwrap();
    runtime
        .execute_source(
            r#"
                res.createSystemFont(
                    "SYSTEM_PLAIN", "Arial", 12, 64, 128, 32, 255
                )
                res.useFont("SYSTEM_PLAIN")
                res.drawString("MISSING_GROUP", "Plain", 10.75, -20.75, "RIGHT", "BOTTOM")
                res.createSystemFontWithStroke(
                    "SYSTEM_STROKE", "Arial", 20,
                    64, 128, 32, 255, 0, 2, 96, 10, 20, 30
                )
                res.useFont("SYSTEM_STROKE")
                res.drawString("MISSING_GROUP", "Stroke", 30.5, 40.25, "HCENTER", "VCENTER")
                res.createSystemFontWithStroke(
                    "SYSTEM_STROKE", "Arial", 99,
                    255, 255, 255, 255, 0, 0, 255, 0, 0, 0, true
                )
                res.releaseFont("SYSTEM_STROKE")
                res.releaseFont("SYSTEM_PLAIN")
                res.createSystemFont(
                    "SYSTEM_PLAIN", "Arial", 12, 64, 128, 32, 255
                )
                res.useFont("SYSTEM_PLAIN")
                res.drawString(
                    "MISSING_GROUP", "Plain", 50.5, 60.5, "RIGHT", "BOTTOM"
                )
            "#,
        )
        .unwrap();

    let bridge = runtime.render.lock().unwrap();
    assert_eq!(bridge.text_commands.len(), 3);
    let TextFontBinding::System(plain) = bridge.text_commands[0].font_binding.as_ref().unwrap()
    else {
        panic!("plain system font rebound to a bitmap font");
    };
    assert_eq!(plain.family, "Arial");
    assert_eq!(plain.size, 12);
    assert_eq!(plain.fill_rgba, [128, 32, 255, 64]);
    assert_eq!(plain.stroke_width, 0);
    assert_eq!(plain.stroke_rgba, [0, 0, 0, 255]);
    assert_eq!(plain.label_pool_epoch, 0);
    assert_eq!(
        bridge.text_commands[0].native_system_origin,
        Some([10.75, -20.75])
    );
    assert!(!plain.font_data.is_empty());
    assert!(plain.ascending > 0);
    assert!(plain.descending >= 0);

    let TextFontBinding::System(stroked) = bridge.text_commands[1].font_binding.as_ref().unwrap()
    else {
        panic!("stroked system font rebound to a bitmap font");
    };
    assert_eq!(stroked.size, 20);
    assert_eq!(stroked.fill_rgba, [128, 32, 255, 64]);
    assert_eq!(stroked.stroke_width, 2);
    assert_eq!(stroked.stroke_rgba, [10, 20, 30, 96]);
    assert_eq!(stroked.style, 0);
    assert_eq!(stroked.label_pool_epoch, 0);
    assert_eq!(
        bridge.text_commands[1].native_system_origin,
        Some([30.5, 40.25])
    );
    assert_eq!(bridge.text_commands[1].horizontal_anchor, "HCENTER");
    assert_eq!(bridge.text_commands[1].vertical_anchor, "VCENTER");

    let TextFontBinding::System(recreated) = bridge.text_commands[2].font_binding.as_ref().unwrap()
    else {
        panic!("recreated system font lost its native kind");
    };
    assert_eq!(recreated.label_pool_epoch, 1);
    assert_eq!(recreated.family, plain.family);
    assert_eq!(recreated.size, plain.size);
    assert_eq!(recreated.fill_rgba, plain.fill_rgba);
}

#[test]
fn box_native_uses_middle_keys_anchors_background_and_submission_order() {
    let data_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../runtime/data");
    let runtime = StellaLua::new(data_root).unwrap();
    runtime
        .execute_source(
            r#"
                ResourceManager.native_createSpriteSheet(
                    "images/1024x768/MENU_ELEMENTS_1.dat"
                )
                drawBoxNative({
                    topLeft = "SAVE_FILE_POPUP_LEFT", topMiddle = "SAVE_FILE_POPUP_MID",
                    topRight = "SAVE_FILE_POPUP_RIGHT", left = "SAVE_FILE_POPUP_LEFT",
                    center = "SAVE_FILE_POPUP_MID", right = "SAVE_FILE_POPUP_RIGHT",
                    bottomLeft = "SAVE_FILE_POPUP_LEFT", bottomMiddle = "SAVE_FILE_POPUP_MID",
                    bottomRight = "SAVE_FILE_POPUP_RIGHT"
                }, 50, 50, 100, 50, 1, 1, "HCENTER", "VCENTER",
                { red = 0.1, green = 0.2, blue = 0.3, alpha = 0.4 })
                too_few_box_arguments_fail = pcall(drawBoxNative, {}, 0, 0)
                "#,
        )
        .unwrap();

    assert!(
        !game_environment(runtime.lua())
            .unwrap()
            .get::<bool>("too_few_box_arguments_fail")
            .unwrap()
    );
    let bridge = runtime.render.lock().unwrap();
    assert_eq!(bridge.commands.len(), 8);
    assert_eq!(
        bridge
            .commands
            .iter()
            .map(|command| command.sprite.as_str())
            .collect::<Vec<_>>(),
        [
            "SAVE_FILE_POPUP_MID",
            "SAVE_FILE_POPUP_MID",
            "SAVE_FILE_POPUP_LEFT",
            "SAVE_FILE_POPUP_RIGHT",
            "SAVE_FILE_POPUP_LEFT",
            "SAVE_FILE_POPUP_RIGHT",
            "SAVE_FILE_POPUP_LEFT",
            "SAVE_FILE_POPUP_RIGHT"
        ]
    );
    let top = &bridge.commands[0];
    let top_geometry = load_sprite_geometry(runtime.data_root()).0["SAVE_FILE_POPUP_MID"];
    assert_eq!(
        (top.x, top.y),
        (
            -top_geometry.min_x as f32,
            (25.0 - top_geometry.height() - top_geometry.min_y) as f32
        )
    );
    assert_eq!(
        top.state.draw_size,
        Some([100.0, top_geometry.height().floor() as f32])
    );
    assert_eq!((top.state.scale_x, top.state.scale_y), (1.0, 1.0));
    assert_eq!(bridge.rect_commands.len(), 1);
    let background = &bridge.rect_commands[0];
    assert_eq!(background.order, 8);
    assert_eq!(
        (
            background.left,
            background.top,
            background.right,
            background.bottom
        ),
        (0.0, 25.0, 99.0, 74.0)
    );
    assert!((background.red - 25.0 / 255.0).abs() < 1e-12);
    assert!((background.green - 51.0 / 255.0).abs() < 1e-12);
    assert!((background.blue - 76.0 / 255.0).abs() < 1e-12);
    assert_eq!(background.alpha, f64::from(102.0_f32 / 255.0_f32));
    assert_eq!(background.mesh_topology, ColorMeshTopology::TriangleStrip);
    assert_eq!(bridge.state.scale_x, 1.0);
    assert_eq!(bridge.state.alpha, 1.0);
}

#[test]
fn box_native_floors_each_target_rect_and_coerces_lua51_color_strings() {
    let data_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../runtime/data");
    let runtime = StellaLua::new(data_root).unwrap();
    runtime
        .execute_source(
            r#"
                ResourceManager.native_createSpriteSheet(
                    "images/1024x768/BUTTONS_SHEET_1.dat"
                )
                drawBoxNative(
                    { center = "BTN_BG_SMALL" },
                    0.75, 1.75, 10.75, 5.75, 1, 1, "", ""
                )
                drawBoxNative(
                    {},
                    0.75, 1.75, 10.75, 5.75, 1, 1, "", "",
                    { red = "0.99999999", green = false, blue = "0.2", alpha = "bad" }
                )
                "#,
        )
        .unwrap();

    let bridge = runtime.render.lock().unwrap();
    let center = &bridge.commands[0];
    let center_geometry = load_sprite_geometry(runtime.data_root()).0["BTN_BG_SMALL"];
    assert_eq!(
        (center.x, center.y),
        (
            -center_geometry.min_x as f32,
            (1.0 - center_geometry.min_y) as f32
        )
    );
    assert_eq!(center.state.draw_size, Some([10.0, 5.0]));
    let background = &bridge.rect_commands[0];
    assert_eq!(
        (
            background.left,
            background.top,
            background.right,
            background.bottom
        ),
        (0.0, 1.0, 10.0, 6.0)
    );
    assert_eq!(background.red, 1.0);
    assert_eq!(background.green, 1.0);
    assert_eq!(background.blue, 51.0 / 255.0);
    assert_eq!(background.alpha, 1.0);
}

#[test]
fn box_native_uses_native_resource_lookup_culling_and_lua51_stack_rules() {
    let data_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../runtime/data");
    let runtime = StellaLua::new(data_root).unwrap();
    runtime
        .execute_source(
            r#"
                ResourceManager.native_createSpriteSheet(
                    "images/1024x768/BUTTONS_SHEET_1.dat"
                )
                non_table_tenth_ok = pcall(
                    drawBoxNative,
                    { center = "BTN_BG_SMALL" },
                    10, 20, 30, 40, 1, 1, "", "", false
                )
                drawBoxNative(
                    { center = "DOES_NOT_EXIST", topLeft = 123 },
                    10, 20, 30, 40, 1, 1, "", ""
                )
                drawBoxNative(
                    { center = "BTN_BG_SMALL" },
                    10, 900, 30, 40, 1, 1, "", "",
                    { red = 1, green = 0, blue = 0, alpha = 1 }
                )
                drawBoxNative(
                    { topLeft = "BTN_BG_SMALL" },
                    100, 100, 40, 40, 2, 3, "", ""
                )
                ResourceManager.native_releaseSpriteSheet(
                    "images/1024x768/BUTTONS_SHEET_1.dat"
                )
                drawBoxNative(
                    { center = "BTN_BG_SMALL" },
                    10, 20, 30, 40, 1, 1, "", ""
                )
                "#,
        )
        .unwrap();

    assert!(
        game_environment(runtime.lua())
            .unwrap()
            .get::<bool>("non_table_tenth_ok")
            .unwrap()
    );
    let geometry = load_sprite_geometry(runtime.data_root()).0["BTN_BG_SMALL"];
    let bridge = runtime.render.lock().unwrap();
    assert_eq!(bridge.rect_commands.len(), 0);
    assert_eq!(bridge.commands.len(), 2);
    assert_eq!(bridge.commands[0].sprite, "BTN_BG_SMALL");
    let corner = &bridge.commands[1];
    assert_eq!(corner.sprite, "BTN_BG_SMALL");
    assert_eq!(
        corner.state.draw_size,
        Some([
            (geometry.width() * 2.0) as f32,
            (geometry.height() * 3.0) as f32,
        ])
    );
    // sub_100467AF0 subtracts the atlas region's original width/height for
    // RIGHT/BOTTOM anchoring, even when the destination rectangle is scaled.
    assert_eq!(
        (corner.x, corner.y),
        (
            (100.0 - geometry.width() - geometry.min_x) as f32,
            (100.0 - geometry.height() - geometry.min_y) as f32
        )
    );
}

#[test]
fn textured_rect_truncates_destination_and_resets_native_gl_state() {
    let runtime = StellaLua::new("/tmp").unwrap();
    runtime
        .execute_source(
            r#"
                setRenderState(10, 20, 2, 3, 0.5, 6, 7, 0.4)
                drawTexturedRect("TEXTURED_RECT", 10.8, 20.9, 30.2, 50.7, false)
                missing_bool_fails = not pcall(drawTexturedRect, "TEXTURED_RECT", 0, 0, 1, 1)
                "#,
        )
        .unwrap();
    let environment = game_environment(runtime.lua()).unwrap();
    assert!(environment.get::<bool>("missing_bool_fails").unwrap());

    let bridge = runtime.render.lock().unwrap();
    // GL_Context's +152 virtual is nullsub_298 in Purple's only concrete
    // GLES2 context, so neither call submits a draw command.
    assert!(bridge.commands.is_empty());
    assert_eq!(bridge.state.translate_x, 0.0);
    assert_eq!(bridge.state.translate_y, 0.0);
    assert_eq!(bridge.state.scale_x, 1.0);
    assert_eq!(bridge.state.alpha, 1.0);
    assert_eq!(bridge.state.clip_rect, None);
}

#[test]
fn selected_texturized_object_installs_divided_native_gl_state() {
    let runtime = StellaLua::new("/tmp").unwrap();
    runtime
        .execute_source(
            r#"
                setTopLeft(4, 6)
                setWorldScale(2)
                setRenderState(99, 88, 9, 8, 0.5, 7, 11, 0.3)
                drawSelectedTexturizedObject("MASK", "FILL", 1, 2, 0.5, 0.25)
                "#,
        )
        .unwrap();

    let bridge = runtime.render.lock().unwrap();
    assert_eq!(bridge.commands.len(), 1);
    let selected = &bridge.commands[0];
    assert_eq!((selected.x, selected.y), (40.0, 160.0));
    assert_eq!(
        (selected.state.translate_x, selected.state.translate_y),
        (-8.0, -24.0)
    );
    assert_eq!((selected.state.scale_x, selected.state.scale_y), (1.0, 0.5));
    assert_eq!(selected.state.angle, 0.5);
    assert_eq!(
        (selected.state.pivot_x, selected.state.pivot_y),
        (7.0, 11.0)
    );
    assert_eq!(selected.state.alpha, 0.3_f32);
    let (sine, cosine) = 0.5_f32.sin_cos();
    let expected_origin_x = (40.0_f32 + cosine * -7.0_f32 + -sine * -11.0_f32) * 0.5_f32;
    let expected_origin_y = (160.0_f32 + sine * -7.0_f32 + cosine * -11.0_f32) * 0.25_f32;
    assert_eq!(
        selected.state.masked_texture_matrix,
        Some([
            expected_origin_x,
            expected_origin_y,
            0.5_f32 * cosine,
            0.5_f32 * -sine,
            0.25_f32 * sine,
            0.25_f32 * cosine,
        ])
    );
    assert!(!selected.world_space);
    assert_eq!(bridge.state.translate_x, -8.0);
    assert_eq!(bridge.state.scale_y, 0.5);
}

#[test]
fn masked_image_native_emits_recovered_quad_uvs_and_resets_scalar_state() {
    let runtime = StellaLua::new("/tmp").unwrap();
    runtime
        .execute_source(
            r#"
                setRenderState(10, 20, 2, 3, 0.5, 6, 7, 0.25)
                renderMaskedImageNative(
                    "MASK", 1.99999999, 2.9, 101.9, 3.9,
                    2.9, 52.9, 102.9, 53.9, 1.99999999
                )
                missing_coordinates_fail = pcall(renderMaskedImageNative, "MASK", 0, 0)
                "#,
        )
        .unwrap();

    let environment = game_environment(runtime.lua()).unwrap();
    assert!(!environment.get::<bool>("missing_coordinates_fail").unwrap());
    let bridge = runtime.render.lock().unwrap();
    assert_eq!(bridge.commands.len(), 1);
    let command = &bridge.commands[0];
    let Some(SpriteGeometrySubmission::ExplicitQuad(quad)) = command.geometry.as_ref() else {
        panic!("masked image did not retain its native quad");
    };
    assert_eq!(
        quad.positions,
        [[102.0, 53.0], [2.0, 52.0], [101.0, 3.0], [2.0, 2.0]]
    );
    let uv_x = |coordinate: i32| {
        let ndc = ((coordinate as f32) / 1024.0_f32).mul_add(2.0_f32, -1.0_f32);
        let normalized = f64::from(ndc).mul_add(0.5_f64, 0.5_f64);
        f64::from((f64::from(1.99999999_f64 as f32) * normalized) as f32)
    };
    let uv_y = |coordinate: i32| {
        let ndc = ((coordinate as f32) / 768.0_f32).mul_add(2.0_f32, -1.0_f32);
        let normalized = (-f64::from(ndc)).mul_add(0.5_f64, 0.5_f64);
        f64::from((f64::from(1.99999999_f64 as f32) * normalized) as f32)
    };
    let expected_uv = [
        [uv_x(102), uv_y(2)],
        [uv_x(2), uv_y(3)],
        [uv_x(101), uv_y(52)],
        [uv_x(2), uv_y(53)],
    ];
    for (actual, expected) in quad.uv.into_iter().zip(expected_uv) {
        assert_eq!(actual, expected);
    }
    assert_eq!(command.state.alpha, 0.25);
    assert_eq!((command.state.pivot_x, command.state.pivot_y), (6.0, 7.0));
    assert_eq!(command.state.translate_x, 0.0);
    assert_eq!(command.state.translate_y, 0.0);
    assert_eq!(command.state.scale_x, 0.0);
    assert_eq!(command.state.scale_y, 0.0);
    assert_eq!(command.state.angle, 0.0);
}

#[test]
fn native_line_and_polygon_helpers_emit_recovered_meshes_and_outline() {
    let runtime = StellaLua::new("/tmp").unwrap();
    runtime
        .execute_source(
            r#"
                drawLine2D(1.9, 2.9, 11.9, 2.9, 3.9, 255, 128, 64, 32)
                drawRectLines(0, 0, 10, 20, 2, 1, 2, 3, 4)
                setRenderState(10, 20, 2, 3, 0, 0, 0, 0.25)
                drawPolygon({ {x = 1, y = 2}, {x = 3, y = 2}, {x = 1, y = 4} },
                    3, 4, 0.1, 0.2, 0.3, 0.5)
                missing_line_values_fail = pcall(drawLine2D, 0, 0, 1)
                non_table_polygon_point_fails = pcall(drawPolygon,
                    { {x = 1, y = 2}, 7, {x = 4, y = 5} },
                    0, 0, 1, 1, 1, 1)
                "#,
        )
        .unwrap();

    let environment = game_environment(runtime.lua()).unwrap();
    assert!(!environment.get::<bool>("missing_line_values_fail").unwrap());
    assert!(
        !environment
            .get::<bool>("non_table_polygon_point_fails")
            .unwrap()
    );
    let bridge = runtime.render.lock().unwrap();
    assert_eq!(bridge.rect_commands.len(), 9);
    let line = &bridge.rect_commands[0];
    assert_eq!(
        line.vertices.as_deref().unwrap(),
        &[[1.0, 0.5], [1.0, 3.5], [11.0, 0.5], [11.0, 3.5]]
    );
    assert_eq!(line.mesh_topology, ColorMeshTopology::TriangleStrip);
    assert_eq!(line.red, 1.0);
    assert_eq!(line.green, f64::from(128.0_f32 / 255.0_f32));
    assert_eq!(line.blue, f64::from(64.0_f32 / 255.0_f32));
    assert_eq!(line.alpha, f64::from(32.0_f32 / 255.0_f32));
    let polygon = &bridge.rect_commands[5];
    assert_eq!(
        polygon.vertices.as_deref().unwrap(),
        &[[260.0, 420.0], [180.0, 540.0], [180.0, 420.0]]
    );
    assert_eq!(polygon.mesh_topology, ColorMeshTopology::TriangleList);
    assert_eq!(
        (polygon.left, polygon.top, polygon.right, polygon.bottom),
        (180.0, 420.0, 260.0, 540.0)
    );
    assert_eq!(polygon.alpha, 0.125);
    assert_eq!(polygon.order, 5);
    for (index, outline) in bridge.rect_commands[6..].iter().enumerate() {
        assert_eq!((outline.red, outline.green, outline.blue), (0.0, 0.0, 0.0));
        assert_eq!(outline.alpha, 0.25);
        assert_eq!(outline.mesh_topology, ColorMeshTopology::TriangleStrip);
        assert_eq!(outline.order, 6 + index as u64);
    }
}

#[test]
fn rectangle_and_polygon_adapters_match_native_tags_table_count_and_coercion() {
    let runtime = StellaLua::new("/tmp").unwrap();
    runtime
        .execute_source(
            r#"
                rect_rejects_string_number = not pcall(
                    drawRect, "1", 1, 1, 1, 0, 0, 10, 10, true)
                rect_accepts_trailing = pcall(
                    drawRect, 1, 1, 1, 1, 0, 0, 10, 10, true, "ignored")

                polygon_rejects_string_number = not pcall(
                    drawPolygon,
                    { {x = 0, y = 0}, {x = 1, y = 0}, {x = 0, y = 1} },
                    "0", 0, 1, 1, 1, 1)
                polygon_accepts_trailing = pcall(
                    drawPolygon,
                    { {x = 0, y = 0}, {x = 1, y = 0}, {x = 0, y = 1} },
                    0, 0, 1, 1, 1, 1, "ignored")
                polygon_fields_use_tonumber = pcall(
                    drawPolygon,
                    { {x = "1", y = true}, {x = 2}, {x = 1, y = 2} },
                    0, 0, 1, 1, 1, 1)

                local keyed = {
                    {x = 0, y = 0}, {x = 1, y = 0}, {x = 0, y = 1}
                }
                keyed.extra = true
                polygon_counts_hash_keys = not pcall(
                    drawPolygon, keyed, 0, 0, 1, 1, 1, 1)
                "#,
        )
        .unwrap();

    let environment = game_environment(runtime.lua()).unwrap();
    for name in [
        "rect_rejects_string_number",
        "rect_accepts_trailing",
        "polygon_rejects_string_number",
        "polygon_accepts_trailing",
        "polygon_fields_use_tonumber",
        "polygon_counts_hash_keys",
    ] {
        assert!(environment.get::<bool>(name).unwrap(), "{name}");
    }
    // One rectangle plus two triangle-and-three-edge polygons.
    assert_eq!(runtime.render.lock().unwrap().rect_commands.len(), 9);
}

#[test]
fn line_generated_adapters_require_exact_numbers_and_ignore_extras() {
    let runtime = StellaLua::new("/tmp").unwrap();
    runtime
        .execute_source(
            r#"
                line_rejects_string_number = not pcall(
                    drawLine2D, "0", 0, 10, 0, 2, 255, 255, 255, 255)
                rect_rejects_string_number = not pcall(
                    drawRectLines, 0, 0, 10, 10, 2, 255, 255, "255", 255)
                line_accepts_trailing = pcall(
                    drawLine2D, 0, 0, 10, 0, 2, 255, 255, 255, 255, "ignored")
                rect_accepts_trailing = pcall(
                    drawRectLines, 0, 0, 10, 10, 2, 255, 255, 255, 255, "ignored")
                "#,
        )
        .unwrap();

    let environment = game_environment(runtime.lua()).unwrap();
    assert!(
        environment
            .get::<bool>("line_rejects_string_number")
            .unwrap()
    );
    assert!(
        environment
            .get::<bool>("rect_rejects_string_number")
            .unwrap()
    );
    assert!(environment.get::<bool>("line_accepts_trailing").unwrap());
    assert!(environment.get::<bool>("rect_accepts_trailing").unwrap());
    let bridge = runtime.render.lock().unwrap();
    assert_eq!(bridge.rect_commands.len(), 5);
}

#[test]
fn native_draw_polygon_triangulates_a_concave_contour_before_closing_its_outline() {
    let runtime = StellaLua::new("/tmp").unwrap();
    runtime
        .execute_source(
            r#"
                drawPolygon({
                    {x = 0, y = 0}, {x = 4, y = 0}, {x = 4, y = 4},
                    {x = 2, y = 2}, {x = 0, y = 4}
                }, 1, 2, 0.25, 0.5, 0.75, 1)
                "#,
        )
        .unwrap();

    let bridge = runtime.render.lock().unwrap();
    assert_eq!(bridge.rect_commands.len(), 6);
    let fill = &bridge.rect_commands[0];
    assert_eq!(fill.mesh_topology, ColorMeshTopology::TriangleList);
    assert_eq!(fill.vertices.as_ref().unwrap().len(), 9);
    assert_eq!(
        (fill.left, fill.top, fill.right, fill.bottom),
        (20.0, 40.0, 100.0, 120.0)
    );
    assert!(
        bridge.rect_commands[1..]
            .iter()
            .all(|outline| outline.red == 0.0 && outline.alpha == 1.0)
    );
}

#[test]
fn native_draw_rect_honors_strict_abi_fcvtzs_and_live_state() {
    let runtime = StellaLua::new("/tmp").unwrap();
    runtime
        .execute_source(
            r#"
                setRenderState(10, 20, 2, 3, 0, 0, 0, 0.25)
                drawRect(0.5, 0.25, 1, 0.5, 1.9, 2.9, 11.2, 7.8, true)
                drawRect(1, 0, 0, 1, 3.9, 4.9, 8.1, 9.1, false)
                missing_keep_state_fails = pcall(drawRect, 1, 1, 1, 1, 0, 0, 1, 1)
                "#,
        )
        .unwrap();

    let environment = game_environment(runtime.lua()).unwrap();
    assert!(!environment.get::<bool>("missing_keep_state_fails").unwrap());
    let bridge = runtime.render.lock().unwrap();
    assert_eq!(bridge.rect_commands.len(), 2);
    let transformed = &bridge.rect_commands[0];
    assert_eq!(
        transformed.vertices.as_deref().unwrap(),
        &[[22.0, 66.0], [40.0, 66.0], [22.0, 78.0], [40.0, 78.0]]
    );
    assert_eq!(transformed.mesh_topology, ColorMeshTopology::TriangleStrip);
    assert_eq!(transformed.color_program, ColorProgram::PlainAlpha);
    assert!((transformed.red - 127.0 / 255.0).abs() < 1e-12);
    assert!((transformed.green - 63.0 / 255.0).abs() < 1e-12);
    assert_eq!(
        transformed.alpha,
        f64::from((127.0_f32 / 255.0_f32) * 0.25_f32)
    );
    assert_eq!(
        bridge.rect_commands[1].vertices.as_deref().unwrap(),
        &[[3.0, 4.0], [7.0, 4.0], [3.0, 8.0], [7.0, 8.0]]
    );
    assert_eq!(bridge.rect_commands[1].color_program, ColorProgram::Plain);
    assert_eq!(bridge.state.translate_x, 0.0);
    assert_eq!(bridge.state.translate_y, 0.0);
    assert_eq!(bridge.state.scale_x, 1.0);
    assert_eq!(bridge.state.scale_y, 1.0);
    assert_eq!(bridge.state.angle, 0.0);
    assert_eq!(bridge.state.alpha, 1.0);
    assert!(bridge.state.clip_rect.is_none());
}

#[test]
fn native_line_width_uses_direction_weighted_non_uniform_scale() {
    let runtime = StellaLua::new("/tmp").unwrap();
    runtime
        .execute_source(
            r#"
                setRenderState(10, 20, 2, 4, 0, 0, 0, 0.5)
                drawLine2D(0, 0, 10, 0, 3, 255, 255, 255, 255)
                "#,
        )
        .unwrap();

    let bridge = runtime.render.lock().unwrap();
    let line = &bridge.rect_commands[0];
    assert_eq!(
        line.vertices.as_deref().unwrap(),
        &[[20.0, 74.0], [20.0, 86.0], [40.0, 74.0], [40.0, 86.0]]
    );
    assert_eq!(line.mesh_topology, ColorMeshTopology::TriangleStrip);
    assert_eq!(line.color_program, ColorProgram::PlainAlpha);
    assert_eq!(line.alpha, 0.5);
}

#[test]
fn native_line_adapter_rounds_to_float32_before_fcvtzs_and_color_packing() {
    let runtime = StellaLua::new("/tmp").unwrap();
    runtime
        .execute_source(
            r#"
                drawLine2D(
                    3.99999999, 0, 7.99999999, 0, 1.99999999,
                    254.99999999, 0, 0, 255
                )
                "#,
        )
        .unwrap();

    let bridge = runtime.render.lock().unwrap();
    let line = &bridge.rect_commands[0];
    assert_eq!(
        line.vertices.as_deref().unwrap(),
        &[[4.0, -1.0], [4.0, 1.0], [8.0, -1.0], [8.0, 1.0]]
    );
    assert_eq!(line.red, 1.0);
}

#[test]
fn direct_sprite_helpers_match_native_lookup_fallback_and_independent_affine_matrix() {
    let data_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../runtime/data");
    let runtime = StellaLua::new(data_root).unwrap();
    runtime
        .execute_source(
            r#"
                ResourceManager.native_createSpriteSheet(
                    "images/1024x768/MENU_ELEMENTS_1.dat"
                )
                ResourceManager.native_createSpriteSheet(
                    "images/1024x768/MENU_ELEMENTS_2.dat"
                )
                ResourceManager.native_createSpriteSheet(
                    "images/1024x768/MENU_ELEMENTS_3.dat"
                )
                ResourceManager.native_createSpriteSheet(
                    "images/1024x768/MENU_GATE_LEVELS.dat"
                )
                res.createCompoSpriteSet(
                    "images/1024x768/MENU_COMPOSPRITES.dat"
                )
                setRenderState(10, 20, 2, 3, 0.25, 4, 5, 0.5)
                drawSpriteWithoutShader("AB_STELLA_LOGO_MAINMENU", 7, 8, 0.5, 0.25, 0.4)
                drawSpriteWithoutShader("STELLALOGO", 0, 0, 1, 1, 0)
                drawSpriteWithShader("STELLALOGO", {
                        name = "colorize-test",
                        params = {},
                    },
                    100, 200, 2, 3, 0.5)
                missing_transform_fails = not pcall(
                    drawSpriteWithoutShader, "AB_STELLA_LOGO_MAINMENU", 0, 0)
                non_table_shader_fails = not pcall(
                    drawSpriteWithShader, "AB_STELLA_LOGO_MAINMENU", "bad", 0, 0, 1, 1, 0)
                is_compo_loaded = isCompoSprite("STELLALOGO", "ignored")
                res.releaseCompoSpriteSet("images/1024x768/MENU_COMPOSPRITES.dat")
                is_compo_after_release = isCompoSprite("STELLALOGO")
                "#,
        )
        .unwrap();

    let environment = game_environment(runtime.lua()).unwrap();
    assert!(environment.get::<bool>("missing_transform_fails").unwrap());
    assert!(environment.get::<bool>("non_table_shader_fails").unwrap());
    assert!(environment.get::<bool>("is_compo_loaded").unwrap());
    assert!(!environment.get::<bool>("is_compo_after_release").unwrap());
    let bridge = runtime.render.lock().unwrap();
    // WithoutShader skips the composite; WithShader expands STELLALOGO's
    // one direct atlas part after regular sprite lookup fails.
    assert_eq!(bridge.commands.len(), 2);
    assert_eq!(bridge.commands[0].sprite, "AB_STELLA_LOGO_MAINMENU");
    assert_eq!(bridge.commands[1].sprite, "AB_STELLA_LOGO_MAINMENU");
    assert_eq!(
        bridge.commands[1].shader.as_ref().unwrap().name,
        "colorize-test"
    );

    // sub_10006C838 supplies a complete matrix to AtlasSprite::draw.  The
    // current render state's affine transform is deliberately not composed;
    // only its alpha/clip state survives the immediate submission.
    assert_eq!((bridge.commands[0].x, bridge.commands[0].y), (7.0, 8.0));
    let (child_sine, child_cosine) = 0.4_f32.sin_cos();
    let expected_matrix = [
        child_cosine * 0.5_f32,
        -child_sine * 0.25_f32,
        child_sine * 0.5_f32,
        child_cosine * 0.25_f32,
    ];
    for (actual, expected) in bridge.commands[0]
        .state
        .matrix
        .unwrap()
        .into_iter()
        .zip(expected_matrix)
    {
        assert_eq!(actual, expected);
    }
    assert_eq!(bridge.commands[0].state.alpha, 0.5);
}

#[test]
fn direct_sprite_generated_adapters_require_exact_lua_tags_and_ignore_extras() {
    let runtime = StellaLua::new(std::env::temp_dir()).unwrap();
    runtime
        .execute_source(
            r#"
                plain_rejects_numeric_name = not pcall(
                    drawSpriteWithoutShader, 123, 0, 0, 1, 1, 0
                )
                plain_rejects_string_number = not pcall(
                    drawSpriteWithoutShader, "MISSING", "0", 0, 1, 1, 0
                )
                plain_accepts_trailing = pcall(
                    drawSpriteWithoutShader, "MISSING", 0, 0, 1, 1, 0, "ignored"
                )

                shader_rejects_numeric_name = not pcall(
                    drawSpriteWithShader, 123, { name = "", params = {} },
                    0, 0, 1, 1, 0
                )
                shader_rejects_non_table = not pcall(
                    drawSpriteWithShader, "MISSING", true, 0, 0, 1, 1, 0
                )
                shader_rejects_string_number = not pcall(
                    drawSpriteWithShader, "MISSING", { name = "", params = {} },
                    "0", 0, 1, 1, 0
                )
                shader_accepts_trailing = pcall(
                    drawSpriteWithShader, "MISSING", { name = "", params = {} },
                    0, 0, 1, 1, 0, "ignored"
                )

                compo_rejects_numeric_name = not pcall(
                    drawCompoSprite, 123, 0, 0, 1, 1
                )
                compo_rejects_string_number = not pcall(
                    drawCompoSprite, "MISSING", "0", 0, 1, 1
                )
                compo_accepts_trailing = pcall(
                    drawCompoSprite, "MISSING", 0, 0, 1, 1, "ignored"
                )
                lookup_rejects_numeric_name = not pcall(
                    isCompoSprite, 123, "MISSING"
                )
                lookup_accepts_trailing = pcall(
                    isCompoSprite, "MISSING", 123
                )
                "#,
        )
        .unwrap();

    let environment = game_environment(runtime.lua()).unwrap();
    for name in [
        "plain_rejects_numeric_name",
        "plain_rejects_string_number",
        "plain_accepts_trailing",
        "shader_rejects_numeric_name",
        "shader_rejects_non_table",
        "shader_rejects_string_number",
        "shader_accepts_trailing",
        "compo_rejects_numeric_name",
        "compo_rejects_string_number",
        "compo_accepts_trailing",
        "lookup_rejects_numeric_name",
        "lookup_accepts_trailing",
    ] {
        assert!(environment.get::<bool>(name).unwrap(), "{name}");
    }
    assert!(runtime.take_render_commands().is_empty());
}

#[test]
fn immediate_sprite_submission_retains_each_resolved_atlas_across_shadow_and_release() {
    let unique = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let root = std::env::temp_dir().join(format!("stella-retained-draw-{unique}"));
    let data_root = root.join("data");
    for directory in ["first", "second"] {
        fs::create_dir_all(data_root.join(directory)).unwrap();
    }
    fs::write(
        data_root.join("first/FIRST.dat"),
        test_textured_sprite_sheet("SHARED", "first.pvr", 10, 20),
    )
    .unwrap();
    fs::write(data_root.join("first/first.pvr"), []).unwrap();
    fs::write(
        data_root.join("second/SECOND.dat"),
        test_textured_sprite_sheet("SHARED", "second.pvr", 30, 40),
    )
    .unwrap();
    fs::write(data_root.join("second/second.pvr"), []).unwrap();

    let runtime = StellaLua::new(&data_root).unwrap();
    runtime
        .execute_source(
            r#"
                res.createSpriteSheet("first/FIRST.dat")
                drawSpriteWithoutShader("SHARED", 0, 0, 1, 1, 0)
                res.drawSprite("SHARED", 0, 0)
                renderMaskedImageNative("SHARED", 0, 0, 1, 0, 1, 1, 0, 1, 1)

                res.createSpriteSheet("second/SECOND.dat")
                drawSpriteWithoutShader("SHARED", 0, 0, 1, 1, 0)
                res.drawSprite("SHARED", 0, 0)
                drawSelectedTexturizedObject("SHARED", "MISSING_FILL", 0, 0, 1, 1)

                res.releaseSpriteSheet("second/SECOND.dat", false)
                drawBoxNative({ center = "SHARED" }, 0, 0, 10, 20, 1, 1, "", "")
                res.releaseSpriteSheet("first/FIRST.dat", false)
                drawSpriteWithoutShader("SHARED", 0, 0, 1, 1, 0)
            "#,
        )
        .unwrap();

    let bridge = runtime.render.lock().unwrap();
    assert_eq!(bridge.commands.len(), 7);
    let regions = bridge
        .commands
        .iter()
        .map(|command| command.bound_region.as_ref().unwrap())
        .collect::<Vec<_>>();
    assert_eq!(
        regions
            .iter()
            .map(|region| region.sprite.width)
            .collect::<Vec<_>>(),
        [10, 10, 10, 30, 30, 30, 10]
    );
    assert!(regions[0].texture_source.ends_with("first/first.pvr"));
    assert!(regions[1].texture_source.ends_with("first/first.pvr"));
    assert!(regions[2].texture_source.ends_with("first/first.pvr"));
    assert!(regions[3].texture_source.ends_with("second/second.pvr"));
    assert!(regions[4].texture_source.ends_with("second/second.pvr"));
    assert!(regions[5].texture_source.ends_with("second/second.pvr"));
    assert!(regions[6].texture_source.ends_with("first/first.pvr"));
    drop(bridge);

    assert!(
        !runtime
            .sprite_catalog_snapshot_since(0)
            .unwrap()
            .regions
            .contains_key("SHARED")
    );
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn selected_object_submission_retains_both_native_image_pointers() {
    let unique = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let root = std::env::temp_dir().join(format!("stella-selected-pointers-{unique}"));
    let data_root = root.join("data");
    for directory in ["first", "second"] {
        fs::create_dir_all(data_root.join(directory)).unwrap();
    }
    fs::write(
        data_root.join("first/MASK.dat"),
        test_textured_sprite_sheet("SELECTED", "first.pvr", 10, 20),
    )
    .unwrap();
    fs::write(data_root.join("first/first.pvr"), []).unwrap();
    fs::write(
        data_root.join("second/MASK.dat"),
        test_textured_sprite_sheet("SELECTED", "second.pvr", 30, 40),
    )
    .unwrap();
    fs::write(data_root.join("second/second.pvr"), []).unwrap();

    let runtime = StellaLua::new(&data_root).unwrap();
    runtime
        .execute_source(
            r#"
                res.createSpriteSheet("first/MASK.dat")
                drawSelectedTexturizedObject("SELECTED", "MASK", 0, 0, 1, 1)
                res.createSpriteSheet("second/MASK.dat", true)
                res.releaseSpriteSheet("second/MASK.dat", false)
            "#,
        )
        .unwrap();

    assert!(
        !runtime
            .sprite_catalog_snapshot_since(0)
            .unwrap()
            .regions
            .contains_key("SELECTED")
    );
    let bridge = runtime.render.lock().unwrap();
    assert_eq!(bridge.commands.len(), 1);
    let command = &bridge.commands[0];
    assert_eq!(command.bound_region.as_ref().unwrap().sprite.width, 10);
    assert!(
        command
            .bound_region
            .as_ref()
            .unwrap()
            .texture_source
            .ends_with("first/first.pvr")
    );
    match command.masked_texture_binding().unwrap() {
        MaskedTextureBinding::Source(source) => assert!(source.ends_with("first/first.pvr")),
        MaskedTextureBinding::Missing => panic!("mask image was resolved at submission"),
    }
    drop(bridge);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn ui_text_native_matches_strict_abi_localization_floor_pivot_and_live_state() {
    let data_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../runtime/data");
    let runtime = StellaLua::new(&data_root).unwrap();
    runtime
        .execute_source(
            r#"
                res.createBitmapFont("fonts/1024x768/FONT_CRIMSON_BASIC.dat")
                res.createTextGroupSet("localization/TEXTS_BASIC.dat")
                res.loadLocale("TEXTS_BASIC", "en_EN")
                res.useLocale("en_EN")
                setRenderState(3, 4, 9, 8, 0.9, 6, 7, 0.6)
                ui_text = {
                    visible = true,
                    x = 4, y = 5,
                    scaleX = 0.5, scaleY = 0.25,
                    font = "FONT_CRIMSON_BASIC",
                    width = 100,
                    hanchor = "RIGHT", vanchor = "BOTTOM",
                    rotationPivotX = 2, rotationPivotY = 3,
                    floorCoordinates = true,
                    group = "TEXTS_BASIC", text = "TEXT_LEVEL_COMPLETE"
                }
                drawUITextNative(ui_text, 10, 20, 2, 3, 0.4, 0.25)
                exactly_fourth_argument_is_ignored = pcall(
                    drawUITextNative, { visible = false }, 1, 2, "ignored"
                )
                too_few_arguments_fail = not pcall(drawUITextNative, ui_text, 1)
                missing_width_fails = not pcall(drawUITextNative, {
                    visible = true, x = 0, y = 0, scaleX = 1, scaleY = 1,
                    font = "FONT_CRIMSON_BASIC", hanchor = "LEFT", vanchor = "TOP",
                    group = "TEXTS_BASIC", text = "TEXT_LEVEL_COMPLETE"
                }, 0, 0)
                "#,
        )
        .unwrap();

    let environment = game_environment(runtime.lua()).unwrap();
    assert!(
        environment
            .get::<bool>("exactly_fourth_argument_is_ignored")
            .unwrap()
    );
    assert!(environment.get::<bool>("too_few_arguments_fail").unwrap());
    assert!(environment.get::<bool>("missing_width_fails").unwrap());

    let bridge = runtime.render.lock().unwrap();
    assert_eq!(bridge.text_commands.len(), 1);
    let command = &bridge.text_commands[0];
    assert_eq!(command.font, "FONT_CRIMSON_BASIC");
    assert_eq!(
        command.text,
        load_localized_strings(&data_root, "en_EN")["TEXT_LEVEL_COMPLETE"]
    );
    assert_eq!(command.horizontal_anchor, "RIGHT");
    assert_eq!(command.vertical_anchor, "BOTTOM");
    assert_eq!(command.alpha, 0.25);

    let angle = 0.4_f64;
    let cosine = angle.cos();
    let sine = angle.sin();
    let final_x = 10.0 + 2.0 * cosine * 4.0 - 3.0 * sine * 5.0;
    let final_y = 20.0 + 2.0 * sine * 4.0 + 3.0 * cosine * 5.0;
    let final_scale_x = 1.0;
    let final_scale_y = 0.75;
    let pivot_correction_x = 2.0 - cosine * 2.0 + sine * 3.0;
    let pivot_correction_y = 3.0 - sine * 2.0 - cosine * 3.0;
    let expected_x = 3.0 * final_scale_x
        + (final_x / final_scale_x).floor() * final_scale_x
        + final_scale_x * pivot_correction_x;
    let expected_y = 4.0 * final_scale_y
        + (final_y / final_scale_y).floor() * final_scale_y
        + final_scale_y * pivot_correction_y;
    assert!((command.x - expected_x).abs() < 1e-12);
    assert!((command.y - expected_y).abs() < 1e-12);
    assert_eq!(
        command.matrix,
        Some([
            final_scale_x * cosine,
            -final_scale_x * sine,
            final_scale_y * sine,
            final_scale_y * cosine,
        ])
    );
    assert_eq!(
        (bridge.state.translate_x, bridge.state.translate_y),
        (3.0, 4.0)
    );
    assert_eq!((bridge.state.scale_x, bridge.state.scale_y), (1.0, 0.75));
    assert_eq!(bridge.state.angle, angle);
    assert_eq!((bridge.state.pivot_x, bridge.state.pivot_y), (2.0, 3.0));
    assert_eq!(bridge.state.matrix, None);
    // The explicit sub-one alpha is temporary and Purple restores 1.0,
    // not the previously installed 0.6 value.
    assert_eq!(bridge.state.alpha, 1.0);
}

#[test]
fn ui_text_native_clipped_lines_inherit_alpha_and_restore_it_on_error() {
    let runtime = StellaLua::new("/tmp").unwrap();
    register_test_sprite_sheet(&runtime, &["LINE_1", "LINE_2", "AFTER_ERROR"]);
    runtime
        .execute_source(
            r#"
                line_calls = {}
                function make_line(name)
                    return {
                        draw = function(self, x, y, sx, sy, angle)
                            table.insert(line_calls, {
                                self = self, x = x, y = y,
                                sx = sx, sy = sy, angle = angle
                            })
                            res.drawSprite(name, 0, 0)
                        end
                    }
                end
                clipped_text = {
                    visible = true,
                    x = 4, y = 5, scaleX = 0.5, scaleY = 0.25,
                    clipped = true,
                    lines = { make_line("LINE_1"), make_line("LINE_2") }
                }
                setRenderState(3, 4, 2, 3, 0.9, 6, 7, 0.6)
                drawUITextNative(clipped_text, 10, 20, 2, 3, 0.4, 0.25)

                bad_clipped_text = {
                    visible = true,
                    x = 0, y = 0, scaleX = 1, scaleY = 1,
                    clipped = true, lines = { {} }
                }
                bad_line_fails = not pcall(
                    drawUITextNative, bad_clipped_text, 0, 0, 1, 1, 0, 0.35
                )
                res.drawSprite("AFTER_ERROR", 0, 0)
                "#,
        )
        .unwrap();

    let environment = game_environment(runtime.lua()).unwrap();
    assert!(environment.get::<bool>("bad_line_fails").unwrap());
    let calls = environment.get::<mlua::Table>("line_calls").unwrap();
    assert_eq!(calls.raw_len(), 2);
    let first = calls.raw_get::<mlua::Table>(1).unwrap();
    let angle = 0.4_f64;
    let expected_x = 10.0 + 2.0 * angle.cos() * 4.0 - 3.0 * angle.sin() * 5.0;
    let expected_y = 20.0 + 2.0 * angle.sin() * 4.0 + 3.0 * angle.cos() * 5.0;
    assert!((first.get::<f64>("x").unwrap() - expected_x).abs() < 1e-12);
    assert!((first.get::<f64>("y").unwrap() - expected_y).abs() < 1e-12);
    assert_eq!(first.get::<f64>("sx").unwrap(), 1.0);
    assert_eq!(first.get::<f64>("sy").unwrap(), 0.75);
    assert_eq!(first.get::<f64>("angle").unwrap(), angle);

    let bridge = runtime.render.lock().unwrap();
    assert!(bridge.text_commands.is_empty());
    assert_eq!(bridge.commands.len(), 3);
    assert_eq!(bridge.commands[0].sprite, "LINE_1");
    assert_eq!(bridge.commands[1].sprite, "LINE_2");
    assert_eq!(bridge.commands[2].sprite, "AFTER_ERROR");
    assert_eq!(bridge.commands[0].state.alpha, 0.25);
    assert_eq!(bridge.commands[1].state.alpha, 0.25);
    assert_eq!(bridge.commands[2].state.alpha, 1.0);
    assert_eq!(bridge.state.alpha, 1.0);
    // Clipped mode does not install the composed text scale/rotation.
    assert_eq!((bridge.state.scale_x, bridge.state.scale_y), (2.0, 3.0));
    assert_eq!(bridge.state.angle, f64::from(0.9_f32));
}
