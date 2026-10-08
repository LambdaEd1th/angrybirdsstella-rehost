//! Native IFont selection and drawUITextNative's observable field callbacks.

use super::*;

struct Fonts(std::path::PathBuf);

impl Fonts {
    fn new() -> Self {
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let index = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!("stella-selected-font-{unique}-{index}"));
        for directory in ["data/first", "data/second", "appdata"] {
            fs::create_dir_all(root.join(directory)).unwrap();
        }
        for (path, width) in [("first/F.dat", 2), ("second/F.dat", 9), ("ALT.dat", 7)] {
            fs::write(
                root.join("data").join(path),
                test_bitmap_font_with_glyph("atlas.pvr", width),
            )
            .unwrap();
        }
        // A real RGBA8888 PVR atlas makes the FONT constructor fixture valid;
        // glyphs fit inside its 16x8 image and do not rely on missing-texture
        // diagnostics or preloaded host assets.
        let mut atlas = [
            52_u32,
            8,
            16,
            0,
            0x12,
            16 * 8 * 4,
            32,
            0x0000_00ff,
            0x0000_ff00,
            0x00ff_0000,
            0xff00_0000,
            u32::from_le_bytes(*b"PVR!"),
            1,
        ]
        .into_iter()
        .flat_map(u32::to_le_bytes)
        .collect::<Vec<_>>();
        atlas.extend_from_slice(&[255; 16 * 8 * 4]);
        let decoded =
            stella_assets::native_image::decode_native_texture(&atlas, Some("pvr")).unwrap();
        assert_eq!((decoded.width, decoded.height), (16, 8));
        for path in ["atlas.pvr", "first/atlas.pvr", "second/atlas.pvr"] {
            fs::write(root.join("data").join(path), &atlas).unwrap();
        }
        Self(root)
    }

    fn runtime(&self) -> StellaLua {
        StellaLua::new(self.0.join("data")).unwrap()
    }
}

impl Drop for Fonts {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn glyph_width(command: &TextRenderCommand) -> i16 {
    match command.font_binding.as_ref().unwrap() {
        TextFontBinding::Bitmap {
            font,
            decoded_image,
            image_owner,
            ..
        } => {
            assert!(decoded_image.is_some(), "fixture atlas must be decoded");
            assert!(image_owner.is_some(), "fixture image must be owned");
            font.glyphs[0].width
        }
        TextFontBinding::System(_) => panic!("expected bitmap font"),
    }
}

#[test]
fn clip_text_uses_selected_allocation_and_empty_input_skips_its_probe() {
    let files = Fonts::new();
    let runtime = files.runtime();
    runtime
        .execute_source(
            r#"
        res.createBitmapFont('first/F.dat'); res.useFont('F')
        clipText('', 'A', 20); first_width = clippedText.widestLine
        res.createBitmapFont('second/F.dat', true)
        local ok, err = pcall(clipText, '', 'A', 20)
        retired_clip = not ok and tostring(err):find('released IFont', 1, true) ~= nil
        empty_clip = pcall(clipText, '', '', 20)
        empty_width = clippedText.widestLine
        res.useFont('F'); clipText('', 'A', 20)
        second_width = clippedText.widestLine
    "#,
        )
        .unwrap();
    let environment = game_environment(runtime.lua()).unwrap();
    assert_eq!(environment.get::<i32>("first_width").unwrap(), 2);
    assert!(environment.get::<bool>("retired_clip").unwrap());
    assert!(environment.get::<bool>("empty_clip").unwrap());
    assert_eq!(environment.get::<i32>("empty_width").unwrap(), 0);
    assert_eq!(environment.get::<i32>("second_width").unwrap(), 9);
}

#[test]
fn host_language_reload_owns_the_old_selection_until_it_selects_the_new_face() {
    let files = Fonts::new();
    let runtime = files.runtime();
    runtime
        .execute_source(
            r#"
        res.createBitmapFont('first/F.dat'); res.useFont('F')
        function setLocale() end
        function refreshCurrentLocale()
            res.createBitmapFont('second/F.dat', true)
            during_reload_width = res.getStringWidth('A')
        end
    "#,
        )
        .unwrap();
    runtime.set_preferred_language("fr_FR").unwrap();
    let environment = game_environment(runtime.lua()).unwrap();
    assert_eq!(environment.get::<i32>("during_reload_width").unwrap(), 2);
    runtime
        .execute_source(
            r#"
        after_reload_width = res.getStringWidth('A')
        res.createBitmapFont('first/F.dat', true)
        local ok, err = pcall(res.getStringWidth, 'A')
        outside_reload_retired = not ok and tostring(err):find('released IFont', 1, true) ~= nil
    "#,
        )
        .unwrap();
    assert_eq!(environment.get::<i32>("after_reload_width").unwrap(), 9);
    assert!(environment.get::<bool>("outside_reload_retired").unwrap());
}

#[test]
fn release_keeps_the_selected_name_and_missing_use_does_not_clear_it() {
    let files = Fonts::new();
    let runtime = files.runtime();
    runtime
        .execute_source("res.createBitmapFont('first/F.dat'); res.useFont('F'); res.releaseFont('F'); res.useFont('MISSING')")
        .unwrap();
    let resources = runtime.resource_runtime.lock().unwrap();
    assert_eq!(resources.current_font.as_deref(), Some("F"));
    assert!(!resources.bitmap_fonts.contains("F"));
}

#[test]
fn forced_bitmap_replacement_reports_retired_selection_until_explicit_use() {
    let files = Fonts::new();
    let runtime = files.runtime();
    runtime
        .execute_source(
            r#"
                res.createBitmapFont('first/F.dat'); res.useFont('F')
                res.drawString('', 'A', 0, 0)
                res.createBitmapFont('second/F.dat', true)
                retired_queries = 0
                for _, f in ipairs({
                    res.getStringWidth, res.getFontMaxAscending,
                    res.getFontMaxDescending, res.getFontLeading,
                    res.getFontTracking, res.getFontHeight
                }) do
                    local ok, err = pcall(f, 'A')
                    if not ok and tostring(err):find('released IFont', 1, true) then
                        retired_queries = retired_queries + 1
                    end
                end
                retired_draw_ok = pcall(res.drawString, '', 'A', 0, 0)
                retired_3d_ok = pcall(drawString3D, '', 'A', 0, 0, 0, 0, 0, 0, 1)
                res.useFont('F'); res.drawString('', 'A', 0, 0)
            "#,
        )
        .unwrap();
    let environment = game_environment(runtime.lua()).unwrap();
    assert_eq!(environment.get::<i64>("retired_queries").unwrap(), 6);
    assert!(!environment.get::<bool>("retired_draw_ok").unwrap());
    assert!(!environment.get::<bool>("retired_3d_ok").unwrap());
    let bridge = runtime.render.lock().unwrap();
    assert_eq!(bridge.text_commands.len(), 2);
    assert_eq!(glyph_width(&bridge.text_commands[0]), 2);
    assert_eq!(glyph_width(&bridge.text_commands[1]), 9);
}

#[test]
fn same_named_creation_after_release_does_not_revive_the_old_selection() {
    let files = Fonts::new();
    let runtime = files.runtime();
    runtime
        .execute_source(
            r#"
                res.createBitmapFont('first/F.dat'); res.useFont('F')
                res.releaseFont('F'); res.createBitmapFont('second/F.dat')
                local ok, err = pcall(res.getFontLeading)
                retired = not ok and tostring(err):find('released IFont', 1, true) ~= nil
                res.useFont('F'); res.drawString('', 'A', 0, 0)
            "#,
        )
        .unwrap();
    let environment = game_environment(runtime.lua()).unwrap();
    assert!(environment.get::<bool>("retired").unwrap());
    assert_eq!(
        glyph_width(&runtime.render.lock().unwrap().text_commands[0]),
        9
    );
}

#[test]
fn same_named_system_replacement_requires_a_new_selection() {
    let files = Fonts::new();
    let runtime = files.runtime();
    runtime
        .execute_source(
            r#"
                res.createSystemFont('F', 'Arial', 12, 255, 255, 255, 255)
                res.useFont('F'); res.drawString('', 'A', 0, 0)
                res.createSystemFont('F', 'Arial', 18, 255, 255, 255, 255, true)
                local ok, err = pcall(res.getStringWidth, 'A')
                retired = not ok and tostring(err):find('released IFont', 1, true) ~= nil
                res.useFont('F'); res.drawString('', 'A', 0, 0)
            "#,
        )
        .unwrap();
    let environment = game_environment(runtime.lua()).unwrap();
    assert!(environment.get::<bool>("retired").unwrap());
    let bridge = runtime.render.lock().unwrap();
    let sizes: Vec<_> = bridge
        .text_commands
        .iter()
        .map(|command| match command.font_binding.as_ref().unwrap() {
            TextFontBinding::System(font) => font.size,
            TextFontBinding::Bitmap { .. } => panic!("expected system font"),
        })
        .collect();
    assert_eq!(sizes, [12, 18]);
}

#[test]
fn changing_font_kind_does_not_retarget_the_selected_pointer() {
    let files = Fonts::new();
    let runtime = files.runtime();
    runtime
        .execute_source(
            r#"
                res.createBitmapFont('first/F.dat'); res.useFont('F')
                res.createSystemFont('F', 'Arial', 12, 255, 255, 255, 255, true)
                local ok, err = pcall(res.getFontLeading)
                bitmap_retired = not ok and tostring(err):find('released IFont', 1, true) ~= nil
                res.useFont('F'); res.drawString('', 'A', 0, 0)
                res.createBitmapFont('second/F.dat', true)
                ok, err = pcall(res.getFontLeading)
                system_retired = not ok and tostring(err):find('released IFont', 1, true) ~= nil
                res.useFont('F'); res.drawString('', 'A', 0, 0)
            "#,
        )
        .unwrap();
    let environment = game_environment(runtime.lua()).unwrap();
    assert!(environment.get::<bool>("bitmap_retired").unwrap());
    assert!(environment.get::<bool>("system_retired").unwrap());
    let bridge = runtime.render.lock().unwrap();
    assert!(matches!(
        bridge.text_commands[0].font_binding,
        Some(TextFontBinding::System(_))
    ));
    assert_eq!(glyph_width(&bridge.text_commands[1]), 9);
}

#[test]
fn missing_selection_and_failed_constructor_preserve_the_live_font() {
    let files = Fonts::new();
    let runtime = files.runtime();
    runtime
        .execute_source(
            r#"
                res.createBitmapFont('first/F.dat'); res.useFont('F')
                width_before = res.getStringWidth('A')
                res.useFont('MISSING')
                assert(not pcall(res.createBitmapFont, 'absent/F.dat', true))
                res.createBitmapFont('second/F.dat')
                width_after = res.getStringWidth('A')
                res.drawString('', 'A', 0, 0)
            "#,
        )
        .unwrap();
    let environment = game_environment(runtime.lua()).unwrap();
    assert_eq!(
        environment.get::<f64>("width_before").unwrap(),
        environment.get::<f64>("width_after").unwrap()
    );
    assert_eq!(
        glyph_width(&runtime.render.lock().unwrap().text_commands[0]),
        2
    );
}

#[test]
fn ui_missing_font_fails_before_anchor_property_access() {
    let files = Fonts::new();
    let runtime = files.runtime();
    runtime
        .execute_source(
            r#"
        anchor_called = false
        local text = setmetatable({visible=true, x=0, y=0, scaleX=1, scaleY=1,
            font='MISSING', width=10, vanchor='TOP', group='', text='A'}, {
            __index=function(_, key)
                if key == 'hanchor' then
                    anchor_called = true
                    res.createBitmapFont('first/F.dat'); res.useFont('F')
                    return 'LEFT'
                end
            end
        })
        draw_ok, draw_error = pcall(drawUITextNative, text, 0, 0)
        draw_error = tostring(draw_error)
    "#,
        )
        .unwrap();
    let environment = game_environment(runtime.lua()).unwrap();
    assert!(!environment.get::<bool>("draw_ok").unwrap());
    assert!(!environment.get::<bool>("anchor_called").unwrap());
    assert!(
        environment
            .get::<String>("draw_error")
            .unwrap()
            .contains("No font is set while trying to get font leading")
    );
}

#[test]
fn ui_raw_properties_ignore_index_metamethods() {
    let files = Fonts::new();
    let runtime = files.runtime();
    runtime
        .execute_source(
            r#"
        res.createBitmapFont('first/F.dat')
        property_reads = 0
        local text = setmetatable({visible=true, x=0, y=0, scaleX=1, scaleY=1,
            font='F', width=10, hanchor='LEFT', vanchor='TOP', group='', text='A'}, {
            __index=function() property_reads=property_reads+1; return nil end
        })
        drawUITextNative(text, 0, 0)
    "#,
        )
        .unwrap();
    let environment = game_environment(runtime.lua()).unwrap();
    assert_eq!(environment.get::<i64>("property_reads").unwrap(), 0);
    assert_eq!(
        glyph_width(&runtime.render.lock().unwrap().text_commands[0]),
        2
    );
}

#[test]
fn ui_raw_font_does_not_select_an_inherited_font() {
    let files = Fonts::new();
    let runtime = files.runtime();
    runtime
        .execute_source(
            r#"
        res.createBitmapFont('first/F.dat'); res.createBitmapFont('ALT.dat'); res.useFont('F')
        local text = setmetatable({visible=true, x=0, y=0, scaleX=1, scaleY=1,
            width=10, hanchor='LEFT', vanchor='TOP', group='', text='A'}, {
            __index=function(_, key) if key=='font' then return 'ALT' end end
        })
        drawUITextNative(text, 0, 0)
    "#,
        )
        .unwrap();
    let bridge = runtime.render.lock().unwrap();
    assert_eq!(bridge.text_commands[0].font, "F");
    assert_eq!(glyph_width(&bridge.text_commands[0]), 2);
}

#[test]
fn ui_raw_numbers_strings_truthiness_and_missing_width_match_lua51() {
    let files = Fonts::new();
    let runtime = files.runtime();
    runtime
        .execute_source(
            r#"
        res.createBitmapFont('first/F.dat')
        drawUITextNative({visible=0, x='1.75', y='2.25', scaleX='2', scaleY='3',
            font='F', floorCoordinates='false', group=123, text=65}, 0, 0)
    "#,
        )
        .unwrap();
    let bridge = runtime.render.lock().unwrap();
    assert_eq!(bridge.text_commands.len(), 1);
    let command = &bridge.text_commands[0];
    assert_eq!(command.text, "65");
    assert_eq!((command.x, command.y), (0.0, 0.0));
    assert_eq!((command.scale_x, command.scale_y), (2.0, 3.0));
    assert_eq!(
        (
            command.horizontal_anchor.as_str(),
            command.vertical_anchor.as_str()
        ),
        ("LEFT", "TOP")
    );
}

#[test]
fn ui_empty_raw_text_still_installs_context() {
    let files = Fonts::new();
    let runtime = files.runtime();
    runtime
        .execute_source(
            r#"
        res.createBitmapFont('first/F.dat')
        drawUITextNative({visible=true, scaleX=2, scaleY=3, font='F'}, 0, 0, 3, 4, 0.4, 0.25)
    "#,
        )
        .unwrap();
    let bridge = runtime.render.lock().unwrap();
    assert!(bridge.text_commands.is_empty());
    assert_eq!(
        (
            bridge.state.scale_x,
            bridge.state.scale_y,
            bridge.state.alpha
        ),
        (6.0, 12.0, 1.0)
    );
}

#[test]
fn ui_anchor_parser_merges_both_axes_before_drawing() {
    let files = Fonts::new();
    let runtime = files.runtime();
    runtime
        .execute_source(
            r#"
        res.createBitmapFont('first/F.dat')
        drawUITextNative({visible=true, x=0, y=0, scaleX=1, scaleY=1, font='F', width=10,
            hanchor='BOTTOM', vanchor='RIGHT', group='', text='A'}, 0, 0)
    "#,
        )
        .unwrap();
    let bridge = runtime.render.lock().unwrap();
    assert_eq!(
        (
            bridge.text_commands[0].horizontal_anchor.as_str(),
            bridge.text_commands[0].vertical_anchor.as_str()
        ),
        ("RIGHT", "BOTTOM")
    );
}

#[test]
fn invalid_ui_anchor_throws_after_installing_context_without_alpha_restore() {
    let files = Fonts::new();
    let runtime = files.runtime();
    runtime
        .execute_source(
            r#"
        res.createBitmapFont('first/F.dat')
        draw_ok, draw_error = pcall(drawUITextNative, {
            visible=true, x=0, y=0, scaleX=2, scaleY=3, font='F', width=10,
            hanchor='INVALID', vanchor='TOP', group='', text='A'
        }, 0, 0, 3, 4, 0.4, 0.25)
        draw_error = tostring(draw_error)
    "#,
        )
        .unwrap();
    let environment = game_environment(runtime.lua()).unwrap();
    assert!(!environment.get::<bool>("draw_ok").unwrap());
    assert!(
        environment
            .get::<String>("draw_error")
            .unwrap()
            .contains("Invalid anchor: INVALID")
    );
    let bridge = runtime.render.lock().unwrap();
    assert!(bridge.text_commands.is_empty());
    assert_eq!(
        (
            bridge.state.scale_x,
            bridge.state.scale_y,
            bridge.state.alpha
        ),
        (6.0, 12.0, 0.25)
    );
}

#[test]
fn ui_numeric_font_and_c_string_text_select_native_values() {
    let files = Fonts::new();
    fs::write(
        files.0.join("data/123.dat"),
        test_bitmap_font_with_glyph("atlas.pvr", 5),
    )
    .unwrap();
    let runtime = files.runtime();
    runtime
        .execute_source(
            r#"
        res.createBitmapFont('123.dat')
        drawUITextNative({visible=true, x=0, y=0, scaleX=1, scaleY=1,
            font=123, text='A\0ignored'}, 0, 0)
    "#,
        )
        .unwrap();
    let bridge = runtime.render.lock().unwrap();
    assert_eq!(bridge.text_commands[0].font, "123");
    assert_eq!(bridge.text_commands[0].text, "A");
    assert_eq!(glyph_width(&bridge.text_commands[0]), 5);
}

#[test]
fn ui_clipped_refreshes_receiver_and_uses_raw_line_indices() {
    let files = Fonts::new();
    let runtime = files.runtime();
    runtime
        .execute_source(
            r#"
        inherited_index_reads = 0
        local lines = setmetatable({}, {__index=function(_, key)
            if key==2 then
                inherited_index_reads=inherited_index_reads+1
                return {draw=function() end}
            end
        end})
        lines[1]=setmetatable({marker='OLD'}, {__index=function(_, key)
            if key=='draw' then
                lines[1]={marker='NEW'}
                return function(receiver) observed_receiver=receiver.marker end
            end
        end})
        drawUITextNative({visible=true, x=0, y=0, scaleX=1, scaleY=1,
            clipped=true, lines=lines}, 0, 0)
    "#,
        )
        .unwrap();
    let environment = game_environment(runtime.lua()).unwrap();
    assert_eq!(
        environment.get::<String>("observed_receiver").unwrap(),
        "NEW"
    );
    assert_eq!(environment.get::<i64>("inherited_index_reads").unwrap(), 0);
}

#[test]
fn ui_clipped_callback_error_preserves_alpha_and_receiver_type_is_rechecked() {
    let files = Fonts::new();
    let runtime = files.runtime();
    runtime
        .execute_source(
            r#"
        called=false
        local lines={}
        lines[1]=setmetatable({}, {__index=function(_, key)
            if key=='draw' then lines[1]=false; return function() called=true end end
        end})
        draw_ok=pcall(drawUITextNative, {visible=true,x=0,y=0,scaleX=1,scaleY=1,
            clipped=true,lines=lines}, 0, 0, 1, 1, 0, 0.35)
    "#,
        )
        .unwrap();
    let environment = game_environment(runtime.lua()).unwrap();
    assert!(!environment.get::<bool>("draw_ok").unwrap());
    assert!(!environment.get::<bool>("called").unwrap());
    assert_eq!(
        runtime.render.lock().unwrap().state.alpha,
        f64::from(0.35_f32)
    );
}

#[test]
fn ui_clipped_callable_draw_value_uses_lua_call_semantics() {
    let files = Fonts::new();
    let runtime = files.runtime();
    runtime
        .execute_source(
            r#"
        local callable=setmetatable({}, {__call=function(_, receiver, x, y, sx, sy, angle)
            assert(receiver.marker=='LINE' and x==2 and y==3 and sx==1 and sy==1 and angle==0)
            called=true
        end})
        drawUITextNative({visible=true,x=0,y=0,scaleX=1,scaleY=1,clipped=true,
            lines={{marker='LINE',draw=callable}}}, 2, 3, 1, 1, 0, 0.5)
    "#,
        )
        .unwrap();
    assert!(
        game_environment(runtime.lua())
            .unwrap()
            .get::<bool>("called")
            .unwrap()
    );
    assert_eq!(runtime.render.lock().unwrap().state.alpha, 1.0);
}
