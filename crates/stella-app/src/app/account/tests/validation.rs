//! Real app-clock delivery and desktop lifecycle boundaries, without sleeps,
//! an OS clipboard, external endpoints, or changes to the player's AppData.

use super::*;

fn elapsed(app: &mut StellaApp, seconds: u64) {
    // Shift the clock origin; exercise the production synchronize method, not
    // AccountUi::advance_validation or a synthetic field-result injection.
    app.account_started -= Duration::from_secs(seconds);
    app.synchronize_account_ui().unwrap();
}

#[test]
fn modal_password_timer_uses_real_elapsed_time_without_a_game_update() {
    let Some(sandbox) = ShippedDataSandbox::new() else {
        return;
    };
    let mut app = sandbox.app();
    open_sign_in(&mut app);
    let _ = paint(&mut app, 0.0);
    app.account_ui.focus(Some(Field::Password));
    ime(&mut app, Ime::Commit("short".to_owned()));
    app.account_ui.focus(None);
    let frames = app
        .runtime
        .lua()
        .globals()
        .get::<i64>("modal_frames")
        .unwrap();
    let before = paint(&mut app, 0.0);
    assert_ne!(
        app.account_painter.hit(685.0, 347.0),
        Some("passwordErrorButton")
    );
    elapsed(&mut app, 5);
    let after = paint(&mut app, 0.0);
    assert_eq!(
        app.account_painter.hit(685.0, 347.0),
        Some("passwordErrorButton")
    );
    assert_ne!(before, after);
    assert_eq!(
        app.runtime
            .lua()
            .globals()
            .get::<i64>("modal_frames")
            .unwrap(),
        frames
    );
    assert_eq!(app.runtime.account_ui().unwrap().view, AccountView::SignIn);
    assert!(!app.account_ui.busy());
}

#[test]
fn inactive_desktop_defers_expired_edit_timer_until_resume_without_resetting_owner_clock() {
    let Some(sandbox) = ShippedDataSandbox::new() else {
        return;
    };
    let mut app = sandbox.app();
    open_sign_in(&mut app);
    app.account_ui.focus(Some(Field::Password));
    ime(&mut app, Ime::Commit("short".to_owned()));
    app.account_ui.focus(None);
    app.will_resign_active();
    let owner = app.account_owner;
    elapsed(&mut app, 5);
    let clock_origin = app.account_started;
    let _ = paint(&mut app, 0.0);
    assert_ne!(
        app.account_painter.hit(685.0, 347.0),
        Some("passwordErrorButton")
    );
    assert!(app.runtime.take_account_validation_results().is_empty());
    app.did_become_active();
    app.synchronize_account_ui().unwrap();
    let _ = paint(&mut app, 0.0);
    assert_eq!(
        app.account_painter.hit(685.0, 347.0),
        Some("passwordErrorButton")
    );
    assert_eq!(app.account_owner, owner);
    assert_eq!(app.account_started, clock_origin);
    assert!(app.fatal_error.is_none());
}

#[test]
fn modal_email_timer_posts_error_instead_of_contacting_an_unconfigured_provider() {
    let Some(sandbox) = ShippedDataSandbox::new() else {
        return;
    };
    let mut app = sandbox.app();
    open_sign_in(&mut app);
    app.account_ui.focus(Some(Field::Email));
    ime(&mut app, Ime::Commit("unused@example.invalid".to_owned()));
    let id = app.account_owner;
    elapsed(&mut app, 5);
    assert_eq!(app.runtime.account_ui().unwrap().view, AccountView::SignIn);
    // The timer posts its completion; it does not navigate synchronously or
    // silently replace online validation with the opt-in local guest service.
    app.runtime.update(0.0).unwrap();
    app.synchronize_account_ui().unwrap();
    assert_eq!(
        app.runtime.account_ui().unwrap().view,
        AccountView::NoNetworkConnectivity
    );
    assert_eq!(app.account_owner, id);
    assert!(!app.account_ui.busy());
    assert_eq!(
        app.runtime
            .lua()
            .globals()
            .get::<i64>("modal_login_failures")
            .unwrap(),
        0
    );
}
