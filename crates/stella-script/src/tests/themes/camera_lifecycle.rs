use super::super::*;

fn camera_runtime() -> StellaLua {
    let runtime = StellaLua::new("/tmp").unwrap();
    register_test_sprite_sheet(&runtime, &["CAMERA_BG", "CAMERA_FG"]);
    runtime
        .execute_source(
            r#"
        deviceModel = "ios"
        screen = { x = 100, y = 80 }
        objects = {
            castleCameraData = {
                ipad = { sx = 4, sy = 6 }, ios = { px = 10, py = 20 }
            },
            birdCameraData = { ios = { px = 30, py = 40 } }
        }
        originalCameras = { [2] = { sx = 8, sy = 9 } }
        gameCamera = {
            resolutionCorrectedCameras = { [2] = { sx = 5 } },
            endCameraIndex = 2
        }
        blockTable = { themes = { lifecycle = { bgLayers = {}, fgLayers = {} } } }
        leftLimitWorld = 0; rightLimitWorld = 256
        topLimitWorld = 0; bottomLimitWorld = 192
        setWorldScale(5)
        setTopLeft(0, 0)
        setTheme("lifecycle")
    "#,
        )
        .unwrap();
    runtime
}

#[test]
fn theme_constructor_refresh_and_first_draw_have_distinct_state_transitions() {
    let runtime = camera_runtime();
    {
        let bridge = runtime.render.lock().unwrap();
        assert!(!bridge.theme_camera.valid);
        assert_eq!(bridge.theme_camera.scale, 1.0);
        assert_eq!(bridge.theme_camera.scale_y, 1.0);
        assert_eq!(bridge.theme_camera.current_scale, 0.0);
        assert_eq!(bridge.theme_camera.original_scale_ratio, 0.0);
        assert_eq!(bridge.resolution_camera_scale, 0.0);
    }
    runtime
        .execute_source("native_refreshThemeSystem()")
        .unwrap();
    {
        let bridge = runtime.render.lock().unwrap();
        assert!(!bridge.theme_camera.valid);
        assert_eq!((bridge.theme_camera.x, bridge.theme_camera.y), (0.0, 0.0));
        assert_eq!(
            (bridge.theme_camera.scale, bridge.theme_camera.scale_y),
            (4.0, 6.0)
        );
        assert_eq!(bridge.theme_camera.original_scale_ratio, 2.0);
        assert_eq!(bridge.resolution_camera_scale, 5.0);
    }
    runtime
        .execute_source(
            r#"
        objects.castleCameraData.ios.px = 17
        objects.castleCameraData.ios.py = 23
        gameCamera.resolutionCorrectedCameras[2].sx = 7
        drawBackgroundNative(99) -- empty/out-of-range pass still initializes
    "#,
        )
        .unwrap();
    {
        let bridge = runtime.render.lock().unwrap();
        assert!(bridge.theme_camera.valid);
        assert_eq!((bridge.theme_camera.x, bridge.theme_camera.y), (17.0, 23.0));
        assert_eq!(bridge.theme_camera.saved_y, 23.0);
        assert_eq!(
            (bridge.theme_camera.screen_x, bridge.theme_camera.screen_y),
            (100.0, 80.0)
        );
        assert_eq!(bridge.theme_camera.current_scale, 5.0);
        assert_eq!(bridge.resolution_camera_scale, 7.0);
    }
    runtime
        .execute_source(
            r#"
        objects.castleCameraData.ios.px = 99
        objects.castleCameraData.ios.py = 101
        native_refreshThemeSystem()
        drawForegroundNative()
    "#,
        )
        .unwrap();
    {
        let bridge = runtime.render.lock().unwrap();
        assert_eq!((bridge.theme_camera.x, bridge.theme_camera.y), (17.0, 0.0));
        assert_eq!(bridge.theme_camera.saved_y, 23.0);
    }
    runtime.execute_source("drawBackgroundNative(-1)").unwrap();
    assert_eq!(runtime.render.lock().unwrap().theme_camera.y, 23.0);
    runtime
        .execute_source("native_resetThemeSystem(); drawForegroundNative()")
        .unwrap();
    // The lazy branch follows the foreground zero store on this first pass.
    assert_eq!(runtime.render.lock().unwrap().theme_camera.y, 101.0);
    runtime.execute_source("drawForegroundNative()").unwrap();
    assert_eq!(runtime.render.lock().unwrap().theme_camera.y, 0.0);
}

#[test]
fn theme_lazy_world_and_relative_offsets_run_once_for_only_the_first_pass() {
    let runtime = camera_runtime();
    runtime
        .execute_source(
            r#"
        local theme = blockTable.themes.lifecycle
        theme.bgLayers = {{ sprite = "CAMERA_BG", relativeY = 0.75,
            worldW = 0.2, worldH = 0.2, velY = 4 }}
        theme.fgLayers = {{ sprite = "CAMERA_FG", relativeY = 0.25,
            offsetY = 13, worldW = 0.2, worldH = 0.2 }}
        setTheme("lifecycle")
        native_refreshThemeSystem()
        drawBackgroundNative(-1)
    "#,
        )
        .unwrap();
    let (offset_x, random_index) = {
        let bridge = runtime.render.lock().unwrap();
        assert_eq!(native_offset_y(&bridge.theme_background_layers[0]), 96.0);
        assert_eq!(native_offset_y(&bridge.theme_foreground_layers[0]), 13.0);
        (
            bridge.theme_background_layers[0].offset_x,
            bridge.particle_random.index,
        )
    };
    runtime
        .render
        .lock()
        .unwrap()
        .advance_native_theme_frame(1.0);
    runtime
        .execute_source(
            r#"
        for i = 1, 5 do drawBackgroundNative(-1); drawForegroundNative() end
    "#,
        )
        .unwrap();
    {
        let bridge = runtime.render.lock().unwrap();
        assert_eq!(bridge.theme_background_layers[0].offset_x, offset_x);
        assert_eq!(native_offset_y(&bridge.theme_background_layers[0]), 100.0);
        assert_eq!(native_offset_y(&bridge.theme_foreground_layers[0]), 13.0);
        assert_eq!(bridge.particle_random.index, random_index);
    }
    runtime
        .execute_source(
            "native_resetThemeSystem(); drawForegroundNative(); drawBackgroundNative(-1)",
        )
        .unwrap();
    let bridge = runtime.render.lock().unwrap();
    assert_eq!(native_offset_y(&bridge.theme_foreground_layers[0]), -96.0);
    assert_eq!(native_offset_y(&bridge.theme_background_layers[0]), 100.0);
    assert_eq!(bridge.particle_random.index, random_index + 2);
}

#[test]
fn theme_lazy_latch_precedes_required_reference_errors_but_follows_screen_lookup() {
    let runtime = camera_runtime();
    runtime
        .execute_source(
            r#"
        local saved = screen
        screen = nil
        assert(not pcall(drawBackgroundNative, -1))
        screen = saved
    "#,
        )
        .unwrap();
    assert!(!runtime.render.lock().unwrap().theme_camera.valid);
    runtime
        .execute_source(
            r#"
        local saved = objects.castleCameraData.ios
        objects.castleCameraData.ios = nil
        setmetatable(objects.castleCameraData, { __index = function() error("no metamethod") end })
        local ok, err = pcall(drawBackgroundNative, -1)
        assert(not ok and not tostring(err):find("no metamethod"))
        objects.castleCameraData.ios = saved
        drawBackgroundNative(-1)
    "#,
        )
        .unwrap();
    {
        let bridge = runtime.render.lock().unwrap();
        assert!(bridge.theme_camera.valid);
        assert_eq!(bridge.theme_camera.x, 0.0); // repaired table does not retry
        assert_eq!(bridge.theme_camera.screen_x, 100.0);
    }
    runtime
        .execute_source("native_resetThemeSystem(); drawBackgroundNative(-1)")
        .unwrap();
    assert_eq!(runtime.render.lock().unwrap().theme_camera.x, 10.0);
}

#[test]
fn theme_camera_optional_boolean_switches_and_unordered_comparison_match_native() {
    let runtime = camera_runtime();
    runtime
        .execute_source(
            r#"
        g_useLowerCameraAsThemeReferencePoint = 1
        g_useZeroAsThemeReferencePointX = "true"
        g_useZeroAsThemeReferencePointY = 1
        drawBackgroundNative(-1)
    "#,
        )
        .unwrap();
    assert_eq!(runtime.render.lock().unwrap().theme_camera.x, 10.0);
    runtime
        .execute_source(
            r#"
        g_useLowerCameraAsThemeReferencePoint = true
        objects.castleCameraData.ios.px = 0/0
        objects.birdCameraData.ios.py = 0/0
        native_resetThemeSystem(); drawBackgroundNative(-1)
    "#,
        )
        .unwrap();
    let bridge = runtime.render.lock().unwrap();
    assert_eq!(bridge.theme_camera.x, 30.0);
    assert!(bridge.theme_camera.y.is_nan());
    assert!(bridge.theme_camera.valid); // NaN does not clear the lifecycle latch
}

#[test]
fn symbolic_theme_offsets_before_refresh_are_numeric_zero_not_screen_edges() {
    let runtime = camera_runtime();
    runtime
        .execute_source(
            r#"
        blockTable.themes.lifecycle.bgLayers = {
            { sprite = "CAMERA_BG", offsetY = "top" },
            { sprite = "CAMERA_BG", offsetY = "bottom" },
            { sprite = "CAMERA_BG", offsetY = 0 }
        }
        setTheme("lifecycle")
        drawBackgroundNative(-1)
    "#,
        )
        .unwrap();
    let bridge = runtime.render.lock().unwrap();
    let layers = &bridge.theme_background_layers;
    assert_eq!(native_offset_y(&layers[0]), 0.0);
    assert_eq!(native_offset_y(&layers[1]), 0.0);
    assert_eq!(layers[0].cached_draw_world_y, layers[2].cached_draw_world_y);
    assert_eq!(layers[1].cached_draw_world_y, layers[2].cached_draw_world_y);
    assert_ne!(layers[0].cached_draw_world_y, 0.0);
}

#[test]
fn native_level_load_resets_the_lazy_latch_without_replacing_camera_scales() {
    let runtime = camera_runtime();
    runtime
        .execute_source("native_refreshThemeSystem(); drawBackgroundNative(-1)")
        .unwrap();
    {
        let mut bridge = runtime.render.lock().unwrap();
        bridge.theme_camera.effect_x = 12.0;
        bridge.theme_camera.effect_y = 13.0;
    }
    // reset runs before file lookup and therefore also survives a bad path.
    runtime
        .execute_source("assert(not pcall(loadLevel, 'missing-theme-lifecycle-regression'))")
        .unwrap();
    let bridge = runtime.render.lock().unwrap();
    assert!(!bridge.theme_camera.valid);
    assert_eq!(
        (bridge.theme_camera.effect_x, bridge.theme_camera.effect_y),
        (0.0, 0.0)
    );
    assert_eq!(
        (bridge.theme_camera.x, bridge.theme_camera.saved_y),
        (10.0, 20.0)
    );
    assert_eq!(
        (bridge.theme_camera.scale, bridge.theme_camera.scale_y),
        (4.0, 6.0)
    );
    assert_eq!(bridge.resolution_camera_scale, 5.0);
}

#[test]
fn theme_update_uses_last_draw_limits_and_does_not_initialize_the_camera() {
    let runtime = camera_runtime();
    runtime
        .execute_source("native_refreshThemeSystem()")
        .unwrap();
    runtime
        .render
        .lock()
        .unwrap()
        .advance_native_theme_frame(0.1);
    {
        let bridge = runtime.render.lock().unwrap();
        assert!(!bridge.theme_camera.valid);
        assert_eq!(bridge.theme_camera.current_scale, 0.0);
        assert_eq!(bridge.theme_camera.world_limits.right, Some(0.0));
    }
    runtime
        .execute_source("drawBackgroundNative(-1); rightLimitWorld = 1; setWorldScale(9)")
        .unwrap();
    runtime
        .render
        .lock()
        .unwrap()
        .advance_native_theme_frame(0.1);
    {
        let bridge = runtime.render.lock().unwrap();
        assert_eq!(bridge.theme_camera.current_scale, 5.0);
        assert_eq!(bridge.theme_camera.world_limits.right, Some(256.0));
    }
    runtime.execute_source("drawBackgroundNative(-1)").unwrap();
    let bridge = runtime.render.lock().unwrap();
    assert_eq!(bridge.theme_camera.current_scale, 9.0);
    assert_eq!(bridge.theme_camera.world_limits.right, Some(1.0));
}
