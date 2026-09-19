//! Theme regressions grouped by recovered ThemeManager/GameLua members.

mod animation;
mod camera_lifecycle;
mod lifecycle;
mod motion;
mod particles;
mod reference_camera;
mod render;
mod repeat;
mod spawn;
mod sprites;

/// Small binding tests supply the same state a real level publishes. These
/// are explicit fixture values, not an alternate production draw algorithm.
fn configure_theme_camera_fixture(runtime: &super::StellaLua) {
    let (width, height) = {
        let bridge = runtime.render.lock().unwrap();
        (bridge.screen_width, bridge.screen_height)
    };
    runtime
        .execute_source(&format!(
            r#"
        screen = {{ x = {width} / 40, y = {height} / 40 }}
        deviceModel = "ios"
        objects = {{ castleCameraData = {{
            ipad = {{ sx = 20, sy = 20 }},
            ios = {{ px = {width} / 40, py = {height} / 40 }}
        }} }}
        originalCameras = {{ [2] = {{ sx = 20, sy = 20 }} }}
        gameCamera = {{
            resolutionCorrectedCameras = {{ [2] = {{ sx = 20, sy = 20 }} }},
            endCameraIndex = 2
        }}
        leftLimitWorld = 0; rightLimitWorld = {width} / 20
        topLimitWorld = 0; bottomLimitWorld = {height} / 20
    "#
        ))
        .unwrap();
    let mut bridge = runtime.render.lock().unwrap();
    bridge.theme_camera.scale = 20.0;
    bridge.theme_camera.scale_y = 20.0;
    bridge.theme_camera.original_scale_ratio = 1.0;
    bridge.resolution_camera_scale = 20.0;
}
