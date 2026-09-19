//! Real booted Lua + platform input + wgpu overlay, synthetic appdata only.
use super::*;
use stella_script::AppRatingChoice;

fn rating_touch(app: &mut StellaApp, phase: TouchPhase, id: u64, x: f64, y: f64) {
    assert!(
        app.app_rating_window_event(&WindowEvent::Touch(Touch {
            device_id: DeviceId::dummy(),
            phase,
            location: PhysicalPosition::new(x, y),
            force: None,
            id,
        }))
        .unwrap()
    );
}

#[test]
fn apprater_overlay_restores_account_pixels_and_never_changes_game_capture_or_input() {
    let Some(sandbox) = ShippedDataSandbox::new() else {
        return;
    };
    let mut app = sandbox.app();
    app.renderer = Some(GpuRenderer::headless(app.resolution).unwrap());
    open_sign_in(&mut app);
    app.paint_account_overlay(1024, 768).unwrap();
    assert!(app.platform_overlay.is_some());
    let background = RgbaImage::from_pixel(1024, 768, Rgba([73, 97, 142, 255]));
    let account_before = app
        .renderer
        .as_ref()
        .unwrap()
        .composite_window_overlay_for_test(&background);
    let game_before = app.renderer.as_ref().unwrap().read_game_rgba().unwrap();
    let account_owner = app.runtime.account_ui().unwrap();
    let catalog = app
        .runtime
        .sprite_catalog_snapshot_since(0)
        .unwrap()
        .revision;
    let captures = app.assets.captures.bindings.len();
    app.runtime
        .execute_source("for i=1,6 do _G.Apprater.showAlert(true, 'Game resumed') end")
        .unwrap();
    let prompt = app.runtime.app_rating_prompt().unwrap();
    app.synchronize_account_ui().unwrap();
    let image = app
        .app_rating_ui
        .paint(&app.runtime, true)
        .unwrap()
        .unwrap();
    assert!(
        image
            .pixels()
            .any(|pixel| pixel[3] == 255 && pixel[0] > 180)
    );
    app.paint_account_overlay(1024, 768).unwrap();
    assert!(app.renderer.as_ref().unwrap().has_window_overlay_for_test());
    assert_eq!(app.platform_overlay, Some(PlatformOverlay::AppRating));
    let rating_pixels = app
        .renderer
        .as_ref()
        .unwrap()
        .composite_window_overlay_for_test(&background);
    assert_ne!(rating_pixels, account_before);
    assert!(
        rating_pixels
            .pixels()
            .zip(image.pixels())
            .filter(|(_, expected)| expected[3] == 255)
            .all(|(pixel, expected)| pixel == expected)
    );
    assert_eq!(app.runtime.account_ui(), Some(account_owner.clone()));
    assert_eq!(
        app.renderer.as_ref().unwrap().read_game_rgba().unwrap(),
        game_before
    );
    assert!(app.runtime.sprite_catalog_snapshot_since(catalog).is_none());
    assert_eq!(app.assets.captures.bindings.len(), captures);
    let rect = app
        .app_rating_ui
        .button_rect(AppRatingChoice::Later)
        .unwrap();
    let (x, y) = (
        f64::from(rect.x + rect.width / 2.0),
        f64::from(rect.y + rect.height / 2.0),
    );
    rating_touch(&mut app, TouchPhase::Started, 77, x, y);
    rating_touch(&mut app, TouchPhase::Cancelled, 77, x, y);
    assert_eq!(app.runtime.app_rating_prompt(), Some(prompt.clone()));
    assert!(app.touches.is_empty());
    assert!(app.primary_touch.is_none());
    rating_touch(&mut app, TouchPhase::Started, 78, x, y);
    rating_touch(&mut app, TouchPhase::Ended, 79, x, y);
    assert!(app.runtime.app_rating_prompt().is_some());
    rating_touch(&mut app, TouchPhase::Ended, 78, x, y);
    assert!(app.runtime.app_rating_prompt().is_none());
    assert_eq!(app.runtime.account_ui(), Some(account_owner));
    // Rating cleared the shared GPU overlay cache. The unchanged account
    // form must repaint instead of retaining the last rating-alert pixels.
    assert!(
        app.account_painter
            .paint(&app.runtime, &app.account_ui, 1024, 768, 0.0)
            .unwrap()
            .is_some()
    );
    app.account_painter.invalidate();
    app.paint_account_overlay(1024, 768).unwrap();
    assert!(app.platform_overlay.is_some());
    assert_eq!(app.platform_overlay, Some(PlatformOverlay::Account));
    assert_eq!(
        app.renderer
            .as_ref()
            .unwrap()
            .composite_window_overlay_for_test(&background),
        account_before
    );
    assert_eq!(
        app.renderer.as_ref().unwrap().read_game_rgba().unwrap(),
        game_before
    );
    assert!(app.touches.is_empty());
    assert!(!app.cursor_down);
    let artifacts = unique_directory("stella-apprater-visual-qa");
    rating_pixels
        .save(artifacts.join("app-rating.png"))
        .unwrap();
    eprintln!("app rating visual QA: {}", artifacts.display());
    assert!(
        !app.runtime
            .answer_app_rating(prompt.id, AppRatingChoice::Rate)
            .unwrap()
    );
}
