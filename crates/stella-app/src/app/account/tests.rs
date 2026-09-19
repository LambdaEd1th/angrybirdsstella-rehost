//! Real app/modal integration without an OS window or non-loopback services.

use super::*;
use image::{Rgba, RgbaImage};
use stella_script::AccountView;
use winit::{
    dpi::PhysicalPosition,
    event::{DeviceId, Ime, Touch},
};

mod apprater;
mod registration;
mod validation;

struct ShippedDataSandbox {
    root: PathBuf,
    data: PathBuf,
}

impl ShippedDataSandbox {
    fn new() -> Option<Self> {
        let shipped = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../runtime/data");
        if !shipped.join("scripts/game.lua").is_file() {
            eprintln!("account integration requires shipped runtime/data; skipped");
            return None;
        }
        let root = unique_directory("stella-account-integration");
        let data = root.join("data");
        fs::create_dir(root.join("appdata")).unwrap();
        let shipped = shipped.canonicalize().unwrap();
        #[cfg(unix)]
        std::os::unix::fs::symlink(shipped, &data).unwrap();
        #[cfg(windows)]
        std::os::windows::fs::symlink_dir(shipped, &data).unwrap();
        Some(Self { root, data })
    }

    fn app(&self) -> StellaApp {
        let mut app = StellaApp::new_with_missing_global_diagnostics(
            self.data.clone(),
            GameResolution::default(),
            false,
            PlatformServiceOptions {
                local_services: true,
                ..PlatformServiceOptions::default()
            },
        )
        .unwrap();
        // Keep the actual booted resource/input/native-service graph. Replace
        // only callbacks in this isolated VM, so menus cannot consume probes.
        app.runtime
            .execute_source(
                r#"
                _G.modal_pauses, _G.modal_resumes, _G.modal_frames = 0, 0, 0
                function gamePaused() _G.modal_pauses = _G.modal_pauses + 1 end
                function gameResumed() _G.modal_resumes = _G.modal_resumes + 1 end
                function update() _G.modal_frames = _G.modal_frames + 1 end
                function draw() end
                _G.modal_login_failures = 0
                _G.SkynestAccount.onLoginFailure = function()
                    _G.modal_login_failures = _G.modal_login_failures + 1
                end
                "#,
            )
            .unwrap();
        app.did_become_active();
        assert!(app.active);
        assert!(app.runtime.audio_output_state().started);
        assert!(app.fatal_error.is_none());
        app
    }
}

impl Drop for ShippedDataSandbox {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

fn unique_directory(label: &str) -> PathBuf {
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    // Artifacts may outlive a test process. Reserve atomically without ever
    // deleting another run's output if a process/time-derived name collides.
    for attempt in 0..1024 {
        let path =
            std::env::temp_dir().join(format!("{label}-{}-{nonce}-{attempt}", std::process::id()));
        match fs::create_dir(&path) {
            Ok(()) => return path,
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => panic!("create isolated account test directory: {error}"),
        }
    }
    panic!("could not reserve an isolated account test directory");
}

fn open_sign_in(app: &mut StellaApp) {
    app.runtime
        .execute_source("_G.SkynestAccount.native_login(true, false, false)")
        .unwrap();
    app.synchronize_account_ui().unwrap();
    assert_eq!(app.runtime.account_ui().unwrap().view, AccountView::SignIn);
}

fn paint(app: &mut StellaApp, seconds: f64) -> RgbaImage {
    app.account_painter
        .paint(&app.runtime, &app.account_ui, 1024, 768, seconds)
        .unwrap()
        .expect("test changes the UI before repaint")
}

fn pointer(app: &mut StellaApp, x: f64, y: f64) {
    assert!(
        app.account_window_event(&WindowEvent::CursorMoved {
            device_id: DeviceId::dummy(),
            position: PhysicalPosition::new(x, y),
        })
        .unwrap()
    );
}

fn mouse(app: &mut StellaApp, state: ElementState, button: MouseButton) {
    assert!(
        app.account_window_event(&WindowEvent::MouseInput {
            device_id: DeviceId::dummy(),
            state,
            button,
        })
        .unwrap()
    );
}

fn click(app: &mut StellaApp, x: f64, y: f64) {
    pointer(app, x, y);
    mouse(app, ElementState::Pressed, MouseButton::Left);
    mouse(app, ElementState::Released, MouseButton::Left);
}

fn ime(app: &mut StellaApp, event: Ime) {
    assert!(app.account_window_event(&WindowEvent::Ime(event)).unwrap());
}

fn touch(app: &mut StellaApp, phase: TouchPhase, id: u64, x: f64, y: f64) {
    assert!(
        app.account_window_event(&WindowEvent::Touch(Touch {
            device_id: DeviceId::dummy(),
            phase,
            location: PhysicalPosition::new(x, y),
            force: None,
            id,
        }))
        .unwrap()
    );
}

fn assert_game_input_clear(app: &StellaApp) {
    // Explicit error, not shipped game.lua's intentionally disabled assert.
    app.runtime
        .execute_source(
            r#"
        if touchcount ~= 0 or keyHold.LBUTTON or keyPressed.LBUTTON
            or keyReleased.LBUTTON or keyHold.KEY_BACK or keyPressed.KEY_BACK
            or keyReleased.KEY_BACK then error('modal input reached the game') end
    "#,
        )
        .unwrap();
    assert!(!app.cursor_down);
    assert!(app.touches.is_empty());
}

#[test]
fn modal_events_block_game_input_without_pausing_audio_and_replacement_clears_pending_input() {
    let Some(sandbox) = ShippedDataSandbox::new() else {
        return;
    };
    let mut app = sandbox.app();
    let audio = app.runtime.audio_output_state();
    app.cursor_down = true;
    app.primary_touch = Some(77);
    app.touches = vec![(77, 31, 47)];
    app.modifiers = ModifiersState::SHIFT | ModifiersState::CONTROL;
    app.runtime.set_cursor(31.0, 47.0, true).unwrap();
    app.runtime.set_key("KEY_BACK", true).unwrap();
    app.runtime.set_touches(&app.touches).unwrap();
    app.runtime.update(0.0).unwrap();
    app.runtime.execute_source("if not keyHold.LBUTTON or touchcount ~= 1 then error('missing input precondition') end").unwrap();

    open_sign_in(&mut app);
    let first_owner = app.account_owner.unwrap();
    assert_eq!(app.primary_touch, None);
    assert!(app.modifiers.is_empty());
    app.runtime.update(0.0).unwrap();
    assert_game_input_clear(&app);
    let _ = paint(&mut app, 0.0);
    click(&mut app, 450.0, 293.0);
    assert_eq!(app.account_ui.focused(), Some(Field::Email));
    ime(&mut app, Ime::Enabled);
    ime(&mut app, Ime::Preedit("拼音".to_owned(), Some((0, 6))));
    ime(&mut app, Ime::Commit("stella@example.invalid".to_owned()));
    ime(&mut app, Ime::Disabled);
    pointer(&mut app, 900.0, 710.0);
    mouse(&mut app, ElementState::Pressed, MouseButton::Right);
    mouse(&mut app, ElementState::Released, MouseButton::Right);
    for delta in [
        MouseScrollDelta::LineDelta(0.0, 8.0),
        MouseScrollDelta::PixelDelta(PhysicalPosition::new(0.0, -20.0)),
    ] {
        assert!(
            app.account_window_event(&WindowEvent::MouseWheel {
                device_id: DeviceId::dummy(),
                delta,
                phase: TouchPhase::Moved,
            })
            .unwrap()
        );
    }
    app.runtime.execute_source("if cursor.x ~= 31 or cursor.y ~= 47 or cursor.wheelTriggered then error('modal pointer or wheel leaked') end").unwrap();
    touch(&mut app, TouchPhase::Started, 10, 450.0, 345.0);
    touch(&mut app, TouchPhase::Started, 11, 600.0, 345.0);
    assert_eq!(app.primary_touch, Some(10));
    touch(&mut app, TouchPhase::Moved, 10, 480.0, 345.0);
    touch(&mut app, TouchPhase::Cancelled, 10, 480.0, 345.0);
    assert_eq!(app.primary_touch, None);
    touch(&mut app, TouchPhase::Ended, 11, 600.0, 345.0);
    app.runtime.update(0.0).unwrap();
    assert_game_input_clear(&app);
    assert!(app.active);
    let after_audio = app.runtime.audio_output_state();
    assert!(after_audio.started);
    assert_eq!(after_audio.generation, audio.generation);
    assert_eq!(
        app.runtime
            .lua()
            .globals()
            .get::<i64>("modal_pauses")
            .unwrap(),
        0
    );
    assert_eq!(
        app.runtime
            .lua()
            .globals()
            .get::<i64>("modal_resumes")
            .unwrap(),
        1
    );
    assert!(
        app.runtime
            .lua()
            .globals()
            .get::<i64>("modal_frames")
            .unwrap()
            >= 3
    );

    // Replacement must clear host and native input, even when the previous
    // modal owned a touch/IME edit and the host already has queued game bytes.
    touch(&mut app, TouchPhase::Started, 88, 450.0, 293.0);
    ime(
        &mut app,
        Ime::Preedit("pending owner".to_owned(), Some((0, 5))),
    );
    app.modifiers = ModifiersState::SHIFT;
    app.runtime.set_key("LBUTTON", true).unwrap();
    app.runtime.set_touches(&[(99, 30, 40)]).unwrap();
    open_sign_in(&mut app);
    assert_ne!(app.account_owner, Some(first_owner));
    assert_eq!(app.account_painter.hit(450.0, 293.0), None);
    assert_eq!(app.primary_touch, None);
    assert!(app.modifiers.is_empty());
    assert_eq!(app.account_ui.focused(), None);
    assert_eq!(app.account_ui.pressed(), None);
    app.runtime.update(0.0).unwrap();
    assert_game_input_clear(&app);
    assert!(
        !app.account_window_event(&WindowEvent::RedrawRequested)
            .unwrap()
    );
    assert!(
        !app.account_window_event(&WindowEvent::Resized(PhysicalSize::new(800, 600)))
            .unwrap()
    );
    assert!(
        !app.account_window_event(&WindowEvent::Focused(false))
            .unwrap()
    );
    // The account router delegates lifecycle rather than pausing it itself.
    assert!(app.active);
    assert!(app.runtime.audio_output_state().started);
}

fn artifact_with_backdrop(overlay: &RgbaImage) -> RgbaImage {
    RgbaImage::from_fn(overlay.width(), overlay.height(), |x, y| {
        let src = overlay.get_pixel(x, y);
        let mut result = [0, 0, 0, 255];
        for (index, background) in [32u16, 150, 200].into_iter().enumerate() {
            result[index] = (u16::from(src[index])
                + (background * u16::from(255 - src[3]) + 127) / 255)
                .min(255) as u8;
        }
        Rgba(result)
    })
}

#[test]
fn real_signin_artwork_masks_passwords_and_cancel_removes_gpu_layer_without_game_capture_changes() {
    let Some(sandbox) = ShippedDataSandbox::new() else {
        return;
    };
    let mut app = sandbox.app();
    app.renderer = Some(GpuRenderer::headless(app.resolution).unwrap());
    let frame = app
        .assets
        .prepare_gpu_frame_at_resolution(app.resolution, &[], &[], &[], &[])
        .unwrap();
    app.renderer
        .as_mut()
        .unwrap()
        .render_offscreen(&app.assets, &frame, [32, 150, 200])
        .unwrap();
    let game_before = app.renderer.as_ref().unwrap().read_game_rgba().unwrap();
    let captures_before = app.assets.captures.bindings.len();
    let catalog_revision = app.resource_revision;
    let commands_before = app.runtime.has_frame_commands();
    open_sign_in(&mut app);
    let initial = paint(&mut app, 0.0);
    assert_eq!(initial.dimensions(), (1024, 768));
    assert!(
        initial
            .pixels()
            .any(|pixel| pixel[3] == 255 && pixel[0] > 180)
    );
    assert_eq!(
        app.account_painter.hit(450.0, 293.0),
        Some("emailTextField")
    );
    assert_eq!(app.account_painter.hit(510.0, 490.0), Some("signInButton"));
    click(&mut app, 450.0, 345.0);
    ime(&mut app, Ime::Commit("abcdefgh".to_owned()));
    let first_password = paint(&mut app, 0.0);
    assert_eq!(
        app.account_ui
            .key(&Key::Character("a".into()), None, ModifiersState::CONTROL),
        None
    );
    ime(&mut app, Ime::Commit("87654321".to_owned()));
    let second_password = paint(&mut app, 0.0);
    assert!(
        first_password == second_password,
        "equal-length plaintext passwords must have identical secure pixels"
    );
    ime(&mut app, Ime::Preedit("ijkl".to_owned(), Some((0, 4))));
    let first_composition = paint(&mut app, 0.0);
    ime(&mut app, Ime::Preedit("4321".to_owned(), Some((0, 4))));
    let second_composition = paint(&mut app, 0.0);
    assert!(
        first_composition == second_composition,
        "IME marked passwords must also remain masked"
    );
    ime(&mut app, Ime::Disabled);
    let secure = paint(&mut app, 0.0);
    // Force a new revision before the app's own upload gate, whose unchanged
    // painter result means retain (not hide) the preceding overlay.
    app.account_ui.focus(None);
    app.paint_account_overlay(1024, 768).unwrap();
    assert!(app.platform_overlay.is_some());
    assert!(app.renderer.as_ref().unwrap().has_window_overlay_for_test());
    assert!(
        app.renderer.as_ref().unwrap().read_game_rgba().unwrap() == game_before,
        "window overlay must not modify the game target"
    );
    assert_eq!(app.assets.captures.bindings.len(), captures_before);
    assert!(
        app.runtime
            .sprite_catalog_snapshot_since(catalog_revision)
            .is_none()
    );
    assert_eq!(app.runtime.has_frame_commands(), commands_before);

    let artifact_root = unique_directory("stella-account-visual-qa");
    artifact_with_backdrop(&initial)
        .save(artifact_root.join("sign-in.png"))
        .unwrap();
    artifact_with_backdrop(&secure)
        .save(artifact_root.join("sign-in-password-masked.png"))
        .unwrap();
    // Native forgotten-password label uses measured hit width; this point is
    // inside its left edge in each bundled localization.
    click(&mut app, 325.0, 390.0);
    assert_eq!(
        app.runtime.account_ui().unwrap().view,
        AccountView::ForgotPassword
    );
    // The page changed before repaint. A click at the old SignIn button
    // must not become a reset submission or touch an obsolete hit target.
    assert_eq!(app.account_painter.hit(510.0, 490.0), None);
    click(&mut app, 510.0, 490.0);
    assert!(!app.runtime.account_ui().unwrap().busy);
    let forgot = paint(&mut app, 0.0);
    artifact_with_backdrop(&forgot)
        .save(artifact_root.join("forgot-password.png"))
        .unwrap();
    click(&mut app, 240.0, 205.0);
    assert_eq!(app.runtime.account_ui().unwrap().view, AccountView::SignIn);
    let _ = paint(&mut app, 0.0);
    let owner = app.account_owner;
    // Closing through execute updates AccountUi before synchronize_account_ui;
    // separate account_owner/overlay guards must still clear the GPU layer.
    click(&mut app, 785.0, 205.0);
    assert!(owner.is_some());
    assert_eq!(app.account_owner, None);
    assert!(app.platform_overlay.is_none());
    assert!(!app.renderer.as_ref().unwrap().has_window_overlay_for_test());
    assert!(app.runtime.account_ui().is_none());
    assert!(
        app.renderer.as_ref().unwrap().read_game_rgba().unwrap() == game_before,
        "removing the overlay must not modify the game target"
    );
    assert_eq!(app.assets.captures.bindings.len(), captures_before);
    assert!(
        !app.account_window_event(&WindowEvent::Ime(Ime::Commit(
            "must not be consumed".to_owned()
        )))
        .unwrap()
    );
    assert_eq!(
        app.runtime
            .lua()
            .globals()
            .get::<i64>("modal_login_failures")
            .unwrap(),
        0
    );
    app.runtime.update(0.0).unwrap();
    assert_eq!(
        app.runtime
            .lua()
            .globals()
            .get::<i64>("modal_login_failures")
            .unwrap(),
        0
    );
    app.runtime.update(0.0).unwrap();
    assert_eq!(
        app.runtime
            .lua()
            .globals()
            .get::<i64>("modal_login_failures")
            .unwrap(),
        1
    );
    assert!(app.active);
    assert!(app.runtime.audio_output_state().started);
    eprintln!(
        "account UI visual QA (original artwork/fonts, synthetic backdrop): {}",
        artifact_root.display()
    );
}
