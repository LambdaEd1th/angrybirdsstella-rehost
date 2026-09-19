//! Account input/controller tests: no fonts, window, service, or player saves.

use super::*;
use std::{
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};
use stella_script::AccountFieldError;

const EMAIL_REQUIRED: (&str, &str) = (
    "rovio_id_validate_email_required",
    "Please enter your email address",
);
const EMAIL_INVALID: (&str, &str) = (
    "rovio_id_validate_email_invalid",
    "Please enter a valid email address",
);
const PASSWORD_REQUIRED: (&str, &str) = (
    "rovio_id_validate_password_required",
    "Please enter a password",
);

struct Fixture {
    runtime: StellaLua,
    root: PathBuf,
}

impl Fixture {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(1);
        let root = std::env::temp_dir().join(format!(
            "stella-account-input-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        let data = root.join("data");
        std::fs::create_dir_all(&data).unwrap();
        let runtime = StellaLua::new(data).unwrap();
        runtime
            .execute_source(
                r#"
                _G.account_input_successes = 0
                _G.account_input_failures = 0
                _G.SkynestAccount.onLoginSuccess = function()
                    _G.account_input_successes = _G.account_input_successes + 1
                end
                _G.SkynestAccount.onLoginFailure = function(code)
                    _G.account_input_failures = _G.account_input_failures + 1
                    _G.account_input_failure_code = code
                end
                _G.SkynestAccount.native_login(true, false, false)
                "#,
            )
            .unwrap();
        Self { runtime, root }
    }

    fn ui(&self) -> AccountUi {
        let mut ui = AccountUi::default();
        assert!(ui.sync(self.runtime.account_ui()));
        ui
    }

    fn counts(&self) -> (i64, i64) {
        let globals = self.runtime.lua().globals();
        (
            globals.get("account_input_successes").unwrap(),
            globals.get("account_input_failures").unwrap(),
        )
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

fn snapshot(id: u64, view: AccountView) -> AccountUiSnapshot {
    AccountUiSnapshot {
        id,
        view,
        busy: false,
        field_error: None,
    }
}

fn press(ui: &mut AccountUi, key: NamedKey) -> Option<Command> {
    ui.key(&Key::Named(key), None, ModifiersState::empty())
}

fn enter(ui: &mut AccountUi, field: Field, text: &str) {
    ui.focus(Some(field));
    ui.text(text);
}

#[test]
fn signin_required_fields_are_checked_email_first_without_trimming_or_regex() {
    let fixture = Fixture::new();
    let mut ui = fixture.ui();
    ui.execute(&fixture.runtime, Command::Submit).unwrap();
    assert_eq!(ui.email_error, Some(EMAIL_REQUIRED));
    assert_eq!(ui.password_error, None);
    assert_eq!(
        fixture.runtime.account_ui().unwrap().view,
        AccountView::SignIn
    );

    enter(&mut ui, Field::Email, "not-an-email");
    assert_eq!(ui.email_error, None);
    ui.execute(&fixture.runtime, Command::Submit).unwrap();
    assert_eq!(ui.email_error, None);
    assert_eq!(ui.password_error, Some(PASSWORD_REQUIRED));
    assert_eq!(
        fixture.runtime.account_ui().unwrap().view,
        AccountView::SignIn
    );

    // Native SignIn only checks raw length here; a single space is nonempty.
    enter(&mut ui, Field::Password, " ");
    assert_eq!(ui.password_error, None);
    ui.execute(&fixture.runtime, Command::Submit).unwrap();
    assert_eq!(
        fixture.runtime.account_ui().unwrap().view,
        AccountView::NoNetworkConnectivity
    );
    assert_eq!(ui.email.text(), "not-an-email");
    assert_eq!(ui.password.text(), " ");
    assert_eq!(fixture.counts(), (0, 0));
}

#[test]
fn signin_return_moves_email_to_password_then_blurs_without_submission() {
    let mut ui = AccountUi::default();
    ui.sync(Some(snapshot(1, AccountView::SignIn)));
    enter(&mut ui, Field::Password, "secret");
    enter(&mut ui, Field::Email, "stella@example.invalid");

    assert_eq!(press(&mut ui, NamedKey::Enter), None);
    assert_eq!(ui.focused(), Some(Field::Password));
    assert_eq!(press(&mut ui, NamedKey::Enter), None);
    assert_eq!(ui.focused(), None);
    assert!(!ui.busy());
    assert_eq!(ui.snapshot.as_ref().unwrap().view, AccountView::SignIn);

    ui.focus(Some(Field::Email));
    ui.preedit("中文", Some((0, 6)));
    assert_eq!(press(&mut ui, NamedKey::Enter), None);
    assert_eq!(ui.focused(), Some(Field::Email));
    assert_eq!(ui.email.preedit, "中文");
    assert_eq!(ui.email.text(), "stella@example.invalid");
    assert_eq!(press(&mut ui, NamedKey::Escape), None);
    assert!(ui.email.preedit.is_empty());
    assert_eq!(
        press(&mut ui, NamedKey::Escape),
        Some(Command::Action(AccountUiAction::Cancel))
    );

    ui.sync(Some(snapshot(1, AccountView::ForgotPassword)));
    ui.focus(Some(Field::Email));
    assert_eq!(press(&mut ui, NamedKey::Enter), None);
    assert_eq!(ui.focused(), None);
}

#[test]
fn owner_changes_clear_credentials_but_same_owner_help_and_errors_retain_them() {
    let mut ui = AccountUi::default();
    assert!(!ui.visible());
    assert_eq!(press(&mut ui, NamedKey::Escape), None);
    assert!(ui.sync(Some(snapshot(41, AccountView::SignIn))));
    enter(&mut ui, Field::Email, "private@example.invalid");
    enter(&mut ui, Field::Password, "retained secret");
    ui.preedit("composing secret", Some((0, 16)));
    ui.pressed = Some("submit".to_owned());
    let mut error = snapshot(41, AccountView::SignIn);
    error.field_error = Some(AccountFieldError {
        field: 19,
        message: 6,
    });
    assert!(!ui.sync(Some(error)));
    assert_eq!(
        ui.password_error,
        Some(("rovio_id_wrong_password", "Wrong password"))
    );
    assert_eq!(ui.email.text(), "private@example.invalid");
    assert_eq!(ui.password.text(), "retained secret");

    assert!(!ui.sync(Some(snapshot(41, AccountView::Help1))));
    assert_eq!(ui.focused(), None);
    assert!(ui.email.preedit.is_empty());
    assert!(ui.password.preedit.is_empty());
    assert_eq!(ui.pressed, None);
    assert_eq!(ui.password_error, None);
    assert!(!ui.sync(Some(snapshot(41, AccountView::SignIn))));
    assert_eq!(ui.email.text(), "private@example.invalid");
    assert_eq!(ui.password.text(), "retained secret");

    assert!(ui.sync(Some(snapshot(42, AccountView::SignIn))));
    assert_eq!(ui.email.text(), "");
    assert_eq!(ui.password.text(), "");
    assert_eq!(ui.email.cursor(), 0);
    assert_eq!(ui.password.cursor(), 0);
    enter(&mut ui, Field::Password, "another secret");
    assert!(ui.sync(None));
    assert!(!ui.visible());
    assert_eq!(ui.password.text(), "");
    assert_eq!(ui.focused(), None);
}

#[test]
fn repeated_server_snapshot_does_not_restore_field_errors_cleared_by_editing() {
    for (field, message, focus, expected) in [
        (15, 1, Field::Email, EMAIL_INVALID),
        (
            18,
            3,
            Field::Email,
            ("rovio_id_wrong_email", "Email address not found"),
        ),
        (
            16,
            2,
            Field::Email,
            (
                "rovio_id_email_taken",
                "This email address has already been registered",
            ),
        ),
        (
            19,
            6,
            Field::Password,
            ("rovio_id_wrong_password", "Wrong password"),
        ),
        (
            17,
            4,
            Field::Password,
            (
                "rovio_id_password_help_text",
                "Password must contain at least 8 characters",
            ),
        ),
    ] {
        let mut ui = AccountUi::default();
        let mut next = snapshot(
            12,
            if matches!(field, 16 | 17) {
                AccountView::Register2
            } else {
                AccountView::SignIn
            },
        );
        next.field_error = Some(AccountFieldError { field, message });
        ui.sync(Some(next.clone()));
        assert_eq!(
            if focus == Field::Email {
                ui.email_error
            } else {
                ui.password_error
            },
            Some(expected)
        );
        enter(&mut ui, focus, "correction");
        assert_eq!(ui.email_error, None);
        assert_eq!(ui.password_error, None);
        let revision = ui.revision;
        assert!(!ui.sync(Some(next)));
        assert_eq!(ui.revision, revision);
        assert_eq!(ui.email_error, None);
        assert_eq!(ui.password_error, None);
    }
}

#[test]
fn busy_snapshot_clears_composition_and_gates_text_keys_and_submit_but_not_cancel() {
    let fixture = Fixture::new();
    let mut ui = fixture.ui();
    ui.execute(
        &fixture.runtime,
        Command::Action(AccountUiAction::ForgotPassword),
    )
    .unwrap();
    enter(&mut ui, Field::Email, "stella@example.invalid");
    ui.preedit("unfinished", Some((0, 10)));
    ui.pressed = Some("submit".to_owned());
    ui.execute(&fixture.runtime, Command::Submit).unwrap();
    assert!(ui.busy());
    assert_eq!(ui.focused(), None);
    assert!(ui.forgot_email.preedit.is_empty());
    assert_eq!(ui.pressed, None);
    // Even a stale host focus must not bypass the busy gate.
    ui.focus(Some(Field::Email));
    ui.text("must not be entered");
    ui.preedit("must not compose", Some((0, 4)));
    for key in [
        NamedKey::Backspace,
        NamedKey::Delete,
        NamedKey::Tab,
        NamedKey::Enter,
    ] {
        assert_eq!(press(&mut ui, key), None);
    }
    assert_eq!(ui.forgot_email.text(), "stella@example.invalid");
    assert!(ui.forgot_email.preedit.is_empty());
    ui.execute(&fixture.runtime, Command::Submit).unwrap();
    assert!(ui.busy());
    assert!(fixture.runtime.account_ui().unwrap().busy);
    assert_eq!(fixture.counts(), (0, 0));
    assert_eq!(
        press(&mut ui, NamedKey::Escape),
        Some(Command::Action(AccountUiAction::Cancel))
    );
}

#[test]
fn cancellation_hides_and_clears_immediately_but_notifies_on_two_later_frames() {
    let fixture = Fixture::new();
    let mut ui = fixture.ui();
    enter(&mut ui, Field::Email, "stella@example.invalid");
    enter(&mut ui, Field::Password, "secret");
    let command = press(&mut ui, NamedKey::Escape).unwrap();
    assert_eq!(command, Command::Action(AccountUiAction::Cancel));
    ui.execute(&fixture.runtime, command).unwrap();
    assert!(!ui.visible());
    assert!(fixture.runtime.account_ui().is_none());
    assert_eq!(ui.email.text(), "");
    assert_eq!(ui.password.text(), "");
    assert_eq!(ui.focused(), None);
    assert_eq!(fixture.counts(), (0, 0));

    fixture.runtime.update(0.0).unwrap();
    assert_eq!(fixture.counts(), (0, 0));
    fixture.runtime.update(0.0).unwrap();
    assert_eq!(fixture.counts(), (0, 1));
    assert_eq!(
        fixture
            .runtime
            .lua()
            .globals()
            .get::<String>("account_input_failure_code")
            .unwrap(),
        "ERROR_USER_CANCELLED_LOGIN"
    );
    ui.execute(&fixture.runtime, Command::Action(AccountUiAction::Cancel))
        .unwrap();
    fixture.runtime.update(0.0).unwrap();
    assert_eq!(fixture.counts(), (0, 1));
}

#[test]
fn forgot_password_distinguishes_required_and_whitespace_errors_before_transport() {
    let fixture = Fixture::new();
    let mut ui = fixture.ui();
    ui.execute(
        &fixture.runtime,
        Command::Action(AccountUiAction::ForgotPassword),
    )
    .unwrap();
    ui.execute(&fixture.runtime, Command::Submit).unwrap();
    assert_eq!(ui.email_error, Some(EMAIL_REQUIRED));
    assert_eq!(ui.password_error, None);
    assert_eq!(
        fixture.runtime.account_ui().unwrap().view,
        AccountView::ForgotPassword
    );
    assert!(!ui.busy());

    enter(&mut ui, Field::Email, " \u{2003} ");
    ui.execute(&fixture.runtime, Command::Submit).unwrap();
    assert_eq!(ui.email_error, Some(EMAIL_INVALID));
    assert!(!fixture.runtime.account_ui().unwrap().busy);
    assert_eq!(fixture.counts(), (0, 0));

    ui.forgot_email.select_all();
    ui.text("stella@example.invalid");
    assert_eq!(ui.email_error, None);
    ui.execute(&fixture.runtime, Command::Submit).unwrap();
    assert!(ui.busy());
    fixture.runtime.update(0.0).unwrap();
    ui.sync(fixture.runtime.account_ui());
    assert_eq!(
        ui.snapshot.as_ref().unwrap().view,
        AccountView::NoNetworkConnectivity
    );
    assert!(!ui.busy());
    assert_eq!(fixture.counts(), (0, 0));
}

#[test]
fn help_and_other_non_input_views_do_not_focus_or_edit_hidden_credentials() {
    for view in [
        AccountView::Help1,
        AccountView::Help2,
        AccountView::Help3,
        AccountView::NoNetworkConnectivity,
        AccountView::PasswordResetEmailSent,
        AccountView::AccountNotVerified,
        AccountView::ThanksForRegistering,
        AccountView::RegistrationFailure,
    ] {
        let mut ui = AccountUi::default();
        ui.sync(Some(snapshot(9, AccountView::SignIn)));
        enter(&mut ui, Field::Email, "retained@example.invalid");
        enter(&mut ui, Field::Password, "retained secret");
        ui.sync(Some(snapshot(9, view)));
        assert_eq!(ui.focused(), None);
        assert_eq!(press(&mut ui, NamedKey::Tab), None);
        assert_eq!(ui.focused(), None, "{view:?}");
        // A stale widget focus request must not bypass the keyboard guard.
        for field in [Field::Email, Field::Password] {
            ui.focus(Some(field));
            assert_eq!(ui.focused(), None, "{view:?}/{field:?}");
            ui.text("hidden edit");
            ui.preedit("hidden composition", Some((0, 5)));
            assert_eq!(
                ui.key(
                    &Key::Character("x".into()),
                    Some("x"),
                    ModifiersState::empty()
                ),
                None
            );
            assert_eq!(press(&mut ui, NamedKey::Backspace), None);
        }
        assert_eq!(ui.email.text(), "retained@example.invalid");
        assert_eq!(ui.password.text(), "retained secret");
        assert!(ui.email.preedit.is_empty());
        assert!(ui.password.preedit.is_empty());
        assert_eq!(press(&mut ui, NamedKey::Enter), None);
        assert_eq!(ui.focused(), None);
        ui.sync(Some(snapshot(9, AccountView::SignIn)));
        assert_eq!(ui.email.text(), "retained@example.invalid");
        assert_eq!(ui.password.text(), "retained secret");
    }
}

#[test]
fn clipboard_shortcuts_require_explicit_input_and_never_export_secure_selection() {
    let mut ui = AccountUi::default();
    ui.sync(Some(snapshot(1, AccountView::SignIn)));
    let shortcut = |ui: &mut AccountUi, key: &str, modifier| {
        ui.key(&Key::Character(key.into()), None, modifier)
    };
    for modifier in [ModifiersState::CONTROL, ModifiersState::SUPER] {
        assert_eq!(shortcut(&mut ui, "v", modifier), None);
        enter(&mut ui, Field::Email, "mail@example.invalid");
        assert_eq!(shortcut(&mut ui, "c", modifier), None);
        assert_eq!(shortcut(&mut ui, "x", modifier), None);
        assert_eq!(shortcut(&mut ui, "a", modifier), None);
        assert_eq!(ui.copyable_selection(), Some("mail@example.invalid"));
        assert_eq!(shortcut(&mut ui, "C", modifier), Some(Command::Copy));
        assert_eq!(shortcut(&mut ui, "x", modifier), Some(Command::Cut));
        // Dispatch commands alone must not mutate text or access an OS clipboard.
        assert_eq!(ui.email.text(), "mail@example.invalid");
        enter(&mut ui, Field::Password, "秘密👩‍🚀");
        assert_eq!(shortcut(&mut ui, "a", modifier), None);
        assert_eq!(ui.copyable_selection(), None);
        assert_eq!(shortcut(&mut ui, "c", modifier), None);
        assert_eq!(shortcut(&mut ui, "X", modifier), None);
        assert_eq!(shortcut(&mut ui, "V", modifier), Some(Command::Paste));
        assert_eq!(ui.password.text(), "秘密👩‍🚀");
        ui.sync(None);
        ui.sync(Some(snapshot(2, AccountView::SignIn)));
    }
}

#[test]
fn ime_owns_navigation_and_clipboard_until_commit_or_escape() {
    let mut ui = AccountUi::default();
    ui.sync(Some(snapshot(1, AccountView::SignIn)));
    enter(&mut ui, Field::Email, "base");
    ui.email.select_all();
    ui.preedit("候选", Some((0, 6)));
    for key in [
        NamedKey::Tab,
        NamedKey::Enter,
        NamedKey::Backspace,
        NamedKey::ArrowRight,
    ] {
        assert_eq!(press(&mut ui, key), None);
        assert_eq!(ui.focused(), Some(Field::Email));
        assert_eq!(ui.email.preedit, "候选");
        assert_eq!(ui.email.text(), "base");
    }
    for key in ["c", "x", "v"] {
        assert_eq!(
            ui.key(&Key::Character(key.into()), None, ModifiersState::SUPER),
            None
        );
    }
    assert_eq!(ui.copyable_selection(), None);
    assert_eq!(press(&mut ui, NamedKey::Escape), None);
    assert!(ui.email.preedit.is_empty());
    assert_eq!(ui.email.text(), "base");
    assert_eq!(
        press(&mut ui, NamedKey::Escape),
        Some(Command::Action(AccountUiAction::Cancel))
    );
}

#[test]
fn desktop_reverse_tab_starts_last_field_and_caret_motion_retains_validation_errors() {
    let mut ui = AccountUi::default();
    ui.sync(Some(snapshot(1, AccountView::SignIn)));
    ui.key(&Key::Named(NamedKey::Tab), None, ModifiersState::SHIFT);
    assert_eq!(ui.focused(), Some(Field::Password));
    press(&mut ui, NamedKey::Tab);
    assert_eq!(ui.focused(), Some(Field::Email));
    ui.text("invalid");
    ui.email_error = Some(EMAIL_INVALID);
    press(&mut ui, NamedKey::ArrowLeft);
    assert_eq!(ui.email_error, Some(EMAIL_INVALID));
    ui.key(&Key::Character("a".into()), None, ModifiersState::CONTROL);
    assert_eq!(ui.email_error, Some(EMAIL_INVALID));
    press(&mut ui, NamedKey::Delete);
    assert_eq!(ui.email_error, None);
    assert_eq!(ui.email.text(), "");
}

#[test]
fn native_error_bubble_tap_dismisses_without_clearing_error_and_icon_reopens_it() {
    let fixture = Fixture::new();
    let mut ui = fixture.ui();
    ui.execute(&fixture.runtime, Command::Submit).unwrap();
    assert!(ui.email_popup);
    assert_eq!(ui.email_error, Some(EMAIL_REQUIRED));
    ui.press(None);
    assert_eq!(ui.release(None), None);
    assert!(!ui.email_popup);
    assert_eq!(ui.email_error, Some(EMAIL_REQUIRED));
    ui.press(Some("emailErrorButton"));
    assert_eq!(ui.release(Some("emailErrorButton")), None);
    assert!(ui.email_popup);
    ui.press(Some("errorPopup"));
    assert_eq!(ui.release(Some("errorPopup")), None);
    assert!(!ui.email_popup);
    enter(&mut ui, Field::Email, "edited");
    assert_eq!(ui.email_error, None);
    ui.press(Some("emailErrorButton"));
    ui.release(Some("emailErrorButton"));
    assert!(!ui.email_popup);
}

#[test]
fn reset_email_is_separate_and_new_reset_nib_does_not_reuse_signin_or_old_reset_input() {
    let fixture = Fixture::new();
    let mut ui = fixture.ui();
    enter(&mut ui, Field::Email, "login@example.invalid");
    ui.execute(
        &fixture.runtime,
        Command::Action(AccountUiAction::ForgotPassword),
    )
    .unwrap();
    assert_eq!(ui.email_editor().text(), "");
    enter(&mut ui, Field::Email, "reset@example.invalid");
    assert_eq!(ui.email.text(), "login@example.invalid");
    ui.execute(&fixture.runtime, Command::Action(AccountUiAction::Back))
        .unwrap();
    // Forgot editing updates the wrapper's shared view_address even without
    // submitting. The newly allocated SignIn nib restores that shared cache.
    assert_eq!(ui.email_editor().text(), "reset@example.invalid");
    ui.execute(
        &fixture.runtime,
        Command::Action(AccountUiAction::ForgotPassword),
    )
    .unwrap();
    assert_eq!(ui.email_editor().text(), "");
    // Even invalid request submission clears the native retained SignIn email.
    ui.execute(&fixture.runtime, Command::Submit).unwrap();
    assert_eq!(ui.email.text(), "");
    assert_eq!(ui.email_error, Some(EMAIL_REQUIRED));
    assert!(!ui.busy());
}

#[test]
fn reset_visible_error_blocks_resubmission_until_edit_without_inventing_a_regex() {
    let fixture = Fixture::new();
    let mut ui = fixture.ui();
    ui.execute(
        &fixture.runtime,
        Command::Action(AccountUiAction::ForgotPassword),
    )
    .unwrap();
    enter(&mut ui, Field::Email, "anything-nonempty");
    ui.email_error = Some(EMAIL_INVALID);
    ui.email_popup = false;
    ui.execute(&fixture.runtime, Command::Submit).unwrap();
    assert!(ui.email_popup);
    assert!(!ui.busy());
    ui.text("-edited");
    assert_eq!(ui.email_error, None);
    ui.execute(&fixture.runtime, Command::Submit).unwrap();
    assert!(ui.busy());
}
