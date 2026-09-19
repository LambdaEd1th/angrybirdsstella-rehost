//! Real app consumption boundaries, beyond prepared-frame capture tests.

use super::*;

fn capture_app() -> StellaApp {
    let resolution = GameResolution::new(8, 4).unwrap();
    let unique = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let data = std::env::temp_dir().join(format!("stella-app-frame-{unique}/data"));
    fs::create_dir_all(&data).unwrap();
    // Even an unbooted VM initializes device identity beside its data root.
    // Never let a generic /tmp root address shared /appdata during a test.
    let runtime = StellaLua::new_with_resolution(&data, 8, 4).unwrap();
    runtime
        .execute_source(
            r#"
                frame = 0
                function update() frame = frame + 1 end
                function draw()
                    if frame == 1 then
                        drawRect(1, 0, 0, 1, 0, 0, 8, 4, true)
                    else
                        setRenderState(0, 0, 0.5, 1, 0, 0, 0, 1)
                        res.drawSprite("CAP", 0, 0)
                        setRenderState(0, 0, 1, 1, 0, 0, 0, 1)
                        drawRect(0, 0, 1, 1, 4, 0, 8, 4, true)
                    end
                    res.captureSprite("CAP")
                end
            "#,
        )
        .unwrap();
    let account_painter = crate::account_ui::AccountPainter::new(data, &runtime);
    StellaApp {
        runtime,
        assets: AssetCatalog {
            root: PathBuf::new(),
            font_root: PathBuf::new(),
            regions: HashMap::new(),
            composites: HashMap::new(),
            masked_textures: HashMap::new(),
            fonts: HashMap::new(),
            textures: HashMap::new(),
            system_labels: SystemLabelPool::default(),
            captures: CapturedTextureCatalog::default(),
        },
        render_commands: Vec::new(),
        text_commands: Vec::new(),
        rect_commands: Vec::new(),
        capture_commands: Vec::new(),
        rendered_frame_ready: false,
        screenshot_share_requests: Vec::new(),
        background_color: [0; 3],
        frame_clear_clip: None,
        resource_revision: 0,
        resolution,
        window: None,
        renderer: Some(GpuRenderer::headless(resolution).unwrap()),
        audio: None,
        audio_clock: AudioOutputClock::default(),
        last_tick: Instant::now(),
        accumulator: Duration::ZERO,
        cursor: (0.0, 0.0),
        cursor_down: false,
        touches: Vec::new(),
        primary_touch: None,
        modifiers: ModifiersState::empty(),
        account_ui: crate::account_ui::AccountUi::default(),
        app_rating_ui: crate::apprater_ui::AppRatingUi::default(),
        account_painter,
        account_ime_enabled: false,
        account_owner: None,
        platform_overlay: None,
        account_clipboard: None,
        account_started: Instant::now(),
        active: true,
        close_request: super::super::window::CloseRequest::default(),
        fatal_error: None,
    }
}

fn assert_red_left_blue_right(rgba: &[u8]) {
    for y in 0..4 {
        for x in 0..8 {
            let offset = (y * 8 + x) * 4;
            let expected = if x < 4 {
                [255, 0, 0, 255]
            } else {
                [0, 0, 255, 255]
            };
            assert_eq!(&rgba[offset..offset + 4], &expected, "pixel ({x}, {y})");
        }
    }
}

#[test]
fn window_capture_frame_is_consumed_once_before_any_present_retries() {
    let mut app = capture_app();
    for _ in 0..2 {
        app.last_tick = Instant::now() - DISPLAY_LINK_STEP;
        app.advance();
        assert!(app.fatal_error.is_none(), "{:?}", app.fatal_error);
        assert!(app.rendered_frame_ready);
        assert!(app.capture_commands.is_empty());
    }
    let expected = app.renderer.as_ref().unwrap().read_game_rgba().unwrap();
    assert_red_left_blue_right(&expected);
    let generations = app.assets.captures.bindings.clone();
    // RedrawRequested uses this same gate before presenting the completed
    // framebuffer. Repeated surface retries may not recompile old draws.
    for _ in 0..3 {
        app.render_game_frame_if_needed().unwrap();
        assert_eq!(
            app.renderer.as_ref().unwrap().read_game_rgba().unwrap(),
            expected
        );
        for (logical, texture) in &generations {
            assert_eq!(app.assets.captures.bindings[logical].source, texture.source);
        }
    }
}

#[test]
fn screenshot_final_capture_reads_completed_frame_without_replaying_draws() {
    let mut app = capture_app();
    let unique = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let path = std::env::temp_dir().join(format!("stella-capture-final-{unique}.png"));
    app.save_screenshot(&path, 2, &[], &[], &[]).unwrap();
    let image = image::open(&path).unwrap().to_rgba8();
    assert_eq!(image.dimensions(), (8, 4));
    assert_red_left_blue_right(image.as_raw());
    std::fs::remove_file(path).unwrap();
}

fn advance_checked(app: &mut StellaApp) -> Vec<u8> {
    app.last_tick = Instant::now() - DISPLAY_LINK_STEP;
    app.advance();
    assert!(app.fatal_error.is_none(), "{:?}", app.fatal_error);
    assert!(!app.runtime.has_frame_commands());
    assert!(app.rendered_frame_ready);
    app.renderer.as_ref().unwrap().read_game_rgba().unwrap()
}

#[test]
fn update_draws_and_capture_execute_before_the_implicit_frame_clear() {
    let mut app = capture_app();
    app.runtime
        .execute_source(
            r#"
                function update()
                    drawRect(1, 0, 0, 1, 0, 0, 8, 4, true)
                    drawRect(0, 0, 1, 1, 4, 0, 8, 4, true)
                    res.captureSprite("UPDATE_CAP")
                    -- This clear is after the capture and is not its source.
                    setBGColor(0, 255, 0)
                    clearScreen()
                end
                function draw()
                    res.drawSprite("UPDATE_CAP", 0, 0)
                end
            "#,
        )
        .unwrap();
    for _ in 0..3 {
        assert_red_left_blue_right(&advance_checked(&mut app));
    }
}

#[test]
fn update_capture_uses_host_backing_even_without_previous_surface_presentation() {
    // Native retainedBacking=false does not promise pixels after iOS present.
    // This checks the deterministic wgpu backing, and especially that a missed
    // desktop present does not discard a preceding ordinary draw callback.
    let mut app = capture_app();
    app.runtime
        .execute_source(
            r#"
                function update()
                    frame = frame + 1
                    if frame > 1 then
                        drawRect(0, 0, 1, 1, 4, 0, 8, 4, true)
                        res.captureSprite("UPDATE_CAP")
                    end
                end
                function draw()
                    if frame == 1 then
                        drawRect(1, 0, 0, 1, 0, 0, 8, 4, true)
                    else
                        res.drawSprite("UPDATE_CAP", 0, 0)
                    end
                end
            "#,
        )
        .unwrap();
    let first = advance_checked(&mut app);
    assert!(
        first
            .as_chunks::<4>()
            .0
            .iter()
            .all(|pixel| *pixel == [255, 0, 0, 255])
    );
    assert_red_left_blue_right(&advance_checked(&mut app));
}

#[test]
fn calls_between_display_updates_keep_their_immediate_order() {
    let mut app = capture_app();
    app.runtime
        .execute_source("function update() end; function draw() end")
        .unwrap();
    advance_checked(&mut app);
    app.runtime
        .execute_source(
            r#"
                drawRect(1, 0, 0, 1, 0, 0, 8, 4, true)
                res.captureSprite("BEFORE_UPDATE")
                function update()
                    drawRect(0, 0, 1, 1, 0, 0, 8, 4, true)
                    res.captureSprite("IN_UPDATE")
                end
                function draw()
                    setRenderState(0, 0, 0.5, 1, 0, 0, 0, 1)
                    res.drawSprite("BEFORE_UPDATE", 0, 0)
                    res.drawSprite("IN_UPDATE", 8, 0)
                    setRenderState(0, 0, 1, 1, 0, 0, 0, 1)
                end
            "#,
        )
        .unwrap();
    assert_red_left_blue_right(&advance_checked(&mut app));
}

#[test]
fn screenshot_consumes_update_capture_after_an_ordinary_frame() {
    let mut app = capture_app();
    app.runtime
        .execute_source(
            r#"
                function update()
                    frame = frame + 1
                    if frame == 2 then
                        drawRect(0, 0, 1, 1, 4, 0, 8, 4, true)
                        res.captureSprite("FROM_ORDINARY_FRAME")
                    end
                end
                function draw()
                    if frame == 1 then
                        drawRect(1, 0, 0, 1, 0, 0, 8, 4, true)
                    else
                        res.drawSprite("FROM_ORDINARY_FRAME", 0, 0)
                    end
                end
            "#,
        )
        .unwrap();
    let unique = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let path = std::env::temp_dir().join(format!("stella-update-capture-{unique}.png"));
    app.save_screenshot(&path, 2, &[], &[], &[]).unwrap();
    let image = image::open(&path).unwrap().to_rgba8();
    assert_red_left_blue_right(image.as_raw());
    std::fs::remove_file(path).unwrap();
}

#[test]
fn implicit_clear_latches_update_clip_before_draw_replaces_it() {
    let mut app = capture_app();
    app.runtime
        .execute_source(
            r#"
                function update()
                    setBGColor(255, 0, 0)
                    clearScreen()
                    res.setClipRect(4, 0, 4, 4)
                    setBGColor(0, 0, 255)
                end
                function draw()
                    -- This affects later calls, not the preceding host clear.
                    res.setClipRect(0, 0, 8, 4)
                end
            "#,
        )
        .unwrap();
    assert_red_left_blue_right(&advance_checked(&mut app));
    assert_eq!(app.frame_clear_clip, Some([4, 0, 8, 4]));
    assert_eq!(app.runtime.framebuffer_clip_rect(), Some([0, 0, 8, 4]));
}

#[test]
fn empty_or_wrapped_native_clear_clip_does_not_mean_full_framebuffer() {
    for clip in ["2, 0, 0, 4", "-2147483648, 0, 4294967296, 4"] {
        let mut app = capture_app();
        app.runtime
            .execute_source(&format!(
                r#"
                    function update()
                        setBGColor(255, 0, 0)
                        clearScreen()
                        res.setClipRect({clip})
                        setBGColor(0, 0, 255)
                    end
                    function draw() end
                "#,
            ))
            .unwrap();
        let rgba = advance_checked(&mut app);
        assert!(
            rgba.as_chunks::<4>()
                .0
                .iter()
                .all(|pixel| *pixel == [255, 0, 0, 255])
        );
    }
}

#[test]
fn clear_screen_bypasses_perspective_left_by_a_failed_3d_text_call() {
    let mut app = capture_app();
    app.runtime
        .execute_source(
            r#"
                function update() end
                function draw()
                    drawRect(1, 0, 0, 1, 0, 0, 8, 4, true)
                    local ok = pcall(drawString3D, "MISSING", "A", 1, 2, 3, 0.75, 0, 0, 0.5)
                    if ok then error("expected the missing-font error") end
                    setBGColor(0, 0, 255)
                    clearScreen()
                end
            "#,
        )
        .unwrap();
    let rgba = advance_checked(&mut app);
    assert!(
        rgba.as_chunks::<4>()
            .0
            .iter()
            .all(|pixel| *pixel == [0, 0, 255, 255])
    );
}

#[test]
fn disabled_lua_draw_still_executes_update_and_the_host_clear() {
    let mut app = capture_app();
    app.runtime
        .execute_source(
            r#"
                function update()
                    drawRect(1, 0, 0, 1, 0, 0, 8, 4, true)
                    res.captureSprite("DISABLED_DRAW_CAPTURE")
                    setBGColor(0, 0, 255)
                    setGameRenderingDisabled(true)
                end
                function draw() error("disabled draw was called") end
            "#,
        )
        .unwrap();
    let rgba = advance_checked(&mut app);
    assert!(
        rgba.as_chunks::<4>()
            .0
            .iter()
            .all(|pixel| *pixel == [0, 0, 255, 255])
    );
    assert_eq!(app.assets.captures.bindings.len(), 1);
}

#[test]
fn update_capture_survives_draw_time_release_and_same_name_recreation() {
    let mut app = capture_app();
    app.runtime
        .execute_source(
            r#"
                function update()
                    drawRect(1, 0, 0, 1, 0, 0, 8, 4, true)
                    res.captureSprite("CAP")
                end
                function draw()
                    setRenderState(0, 0, 0.5, 1, 0, 0, 0, 1)
                    res.drawSprite("CAP", 0, 0)
                    setRenderState(0, 0, 1, 1, 0, 0, 0, 1)
                    res.releaseSpriteSheet("CAP", false)
                    drawRect(0, 0, 1, 1, 4, 0, 8, 4, true)
                    res.captureSprite("CAP")
                end
            "#,
        )
        .unwrap();
    assert_red_left_blue_right(&advance_checked(&mut app));
    assert_eq!(app.assets.captures.bindings.len(), 2);
}

#[test]
fn resizing_consumes_pending_old_extent_capture_before_replacing_target() {
    let mut app = capture_app();
    app.runtime
        .execute_source(
            r#"
                drawRect(1, 0, 0, 1, 0, 0, 8, 4, true)
                drawRect(0, 0, 1, 1, 4, 0, 8, 4, true)
                res.captureSprite("BEFORE_RESIZE")
                function update() end
                function resolutionChanged() end
                function draw() res.drawSprite("BEFORE_RESIZE", 0, 0) end
            "#,
        )
        .unwrap();
    app.resize_runtime_target(GameResolution::new(12, 4).unwrap())
        .unwrap();
    assert!(!app.runtime.has_frame_commands());
    let rgba = advance_checked(&mut app);
    for y in 0..4 {
        assert_red_left_blue_right(&rgba[y * 12 * 4..y * 12 * 4 + 8 * 4].repeat(4));
        assert_eq!(&rgba[y * 12 * 4 + 8 * 4..(y + 1) * 12 * 4], &[255; 16]);
    }
    let capture = app.assets.captures.bindings.values().next().unwrap();
    assert_eq!((capture.width, capture.height), (8, 4));
}
