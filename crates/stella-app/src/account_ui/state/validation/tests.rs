//! Native delayed-validation presentation: no window, clipboard, or endpoint.
//! Timers are advanced deterministically; fixtures own their entire data tree.

use super::*;
use std::{
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};
use stella_script::AccountFieldError;

const EMAIL_INVALID: (&str, &str) = (
    "rovio_id_validate_email_invalid",
    "Please enter a valid email address",
);
const EMAIL_TAKEN: (&str, &str) = (
    "rovio_id_email_taken",
    "This email address has already been registered",
);
const WRONG_EMAIL: (&str, &str) = ("rovio_id_wrong_email", "Email address not found");
const WRONG_PASSWORD: (&str, &str) = ("rovio_id_wrong_password", "Wrong password");
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
            "stella-account-validation-ui-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed),
        ));
        let data = root.join("data");
        std::fs::create_dir_all(&data).unwrap();
        let runtime = StellaLua::new(data).unwrap();
        runtime
            .execute_source("_G.SkynestAccount.native_login(true, false, false)")
            .unwrap();
        Self { runtime, root }
    }

    fn ui(&self) -> AccountUi {
        let mut ui = AccountUi::default();
        assert!(ui.sync(self.runtime.account_ui()));
        ui
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

fn ui(view: AccountView) -> AccountUi {
    let mut ui = AccountUi::default();
    ui.sync(Some(snapshot(7, view)));
    ui
}

fn clock(ui: &mut AccountUi, millis: u64) {
    ui.set_validation_clock(Duration::from_millis(millis));
}

fn edit(ui: &mut AccountUi, field: Field, text: &str) {
    ui.focus(Some(field));
    ui.editor_mut().unwrap().select_all();
    ui.text(text);
}

fn feedback(ui: &AccountUi, field: u32, message: u32) -> AccountValidationResult {
    AccountValidationResult {
        id: ui.owner_id().unwrap(),
        view: ui.snapshot.as_ref().unwrap().view,
        field: if matches!(field, 17 | 19 | 23) {
            AccountValidationField::Password
        } else {
            AccountValidationField::Email
        },
        generation: 0,
        error: Some(AccountFieldError { field, message }),
        valid: false,
    }
}

fn deliver(ui: &mut AccountUi, field: u32, message: u32) {
    ui.apply_validation_result(feedback(ui, field, message));
}

fn key(ui: &mut AccountUi, key: NamedKey) -> Option<Command> {
    ui.key(&Key::Named(key), None, ModifiersState::empty())
}

#[test]
fn independent_two_second_timers_reset_only_the_edited_field() {
    let fixture = Fixture::new();
    let mut ui = fixture.ui();
    edit(&mut ui, Field::Email, "not-an-email");
    clock(&mut ui, 400);
    edit(&mut ui, Field::Password, "short");
    clock(&mut ui, 900);
    edit(&mut ui, Field::Email, "still-invalid");
    assert_eq!(
        ui.validation.deadlines,
        [
            Some(Duration::from_millis(2900)),
            Some(Duration::from_millis(2400))
        ]
    );
    assert_eq!(ui.validation.generation, [2, 1]);
    clock(&mut ui, 2399);
    ui.advance_validation(&fixture.runtime).unwrap();
    assert_eq!(ui.password_error, None);
    clock(&mut ui, 2400);
    ui.advance_validation(&fixture.runtime).unwrap();
    assert_eq!(ui.password_error, Some(PASSWORD_HELP));
    assert!(!ui.password_popup && !ui.password_border);
    assert_eq!(
        ui.validation.deadlines,
        [Some(Duration::from_millis(2900)), None]
    );
    assert!(fixture.runtime.take_account_validation_results().is_empty());
}

#[test]
fn empty_edits_still_schedule_but_expire_without_field_feedback() {
    let fixture = Fixture::new();
    let mut ui = fixture.ui();
    edit(&mut ui, Field::Email, "invalid");
    edit(&mut ui, Field::Password, "short");
    clock(&mut ui, 500);
    edit(&mut ui, Field::Email, "");
    edit(&mut ui, Field::Password, "");
    assert_eq!(ui.validation.cached, ["", ""]);
    assert_eq!(
        ui.validation.deadlines,
        [Some(Duration::from_millis(2500)); 2]
    );
    clock(&mut ui, 2500);
    ui.advance_validation(&fixture.runtime).unwrap();
    assert_eq!(ui.validation.deadlines, [None; 2]);
    assert_eq!((ui.email_error, ui.password_error), (None, None));
    assert!(fixture.runtime.take_account_validation_results().is_empty());
    assert_eq!(
        fixture.runtime.account_ui().unwrap().view,
        AccountView::SignIn
    );
}

#[test]
fn invalid_email_timer_enters_local_listener_only_at_deadline_and_delivers_asynchronously() {
    let fixture = Fixture::new();
    let mut ui = fixture.ui();
    edit(&mut ui, Field::Email, "not-an-email");
    clock(&mut ui, 1999);
    ui.advance_validation(&fixture.runtime).unwrap();
    fixture.runtime.update(0.0).unwrap();
    ui.advance_validation(&fixture.runtime).unwrap();
    assert_eq!(ui.email_error, None);

    clock(&mut ui, 2000);
    ui.advance_validation(&fixture.runtime).unwrap();
    assert_eq!(ui.validation.deadlines[0], None);
    assert_eq!(ui.email_error, None);
    fixture.runtime.update(0.0).unwrap();
    ui.advance_validation(&fixture.runtime).unwrap();
    assert_eq!(ui.email_error, Some(EMAIL_INVALID));
    assert!(!ui.email_popup && !ui.email_border);
    assert_eq!(
        fixture.runtime.account_ui().unwrap().view,
        AccountView::SignIn
    );
}

#[test]
fn blur_return_same_owner_page_and_progress_do_not_cancel_timers() {
    let mut ui = ui(AccountView::SignIn);
    edit(&mut ui, Field::Email, "invalid");
    clock(&mut ui, 400);
    edit(&mut ui, Field::Password, "short");
    let deadlines = ui.validation.deadlines;
    ui.focus(Some(Field::Email));
    assert_eq!(key(&mut ui, NamedKey::Enter), None);
    assert_eq!(ui.focus, Some(Field::Password));
    assert_eq!(key(&mut ui, NamedKey::Enter), None);
    assert_eq!(ui.focus, None);
    assert_eq!(ui.validation.deadlines, deadlines);
    for view in [
        AccountView::Help1,
        AccountView::Register2,
        AccountView::SignIn,
    ] {
        assert!(!ui.sync(Some(snapshot(7, view))));
        assert_eq!(ui.validation.deadlines, deadlines);
        assert_eq!(ui.validation.cached, ["invalid", "short"]);
    }
    let mut busy = snapshot(7, AccountView::SignIn);
    busy.busy = true;
    ui.sync(Some(busy));
    assert_eq!(ui.validation.deadlines, deadlines);
    assert_eq!(ui.validation.generation, [1, 1]);
    ui.sync(Some(snapshot(8, AccountView::SignIn)));
    assert_eq!(ui.validation.deadlines, [None; 2]);
    assert_eq!(ui.validation.cached, ["", ""]);
    assert_eq!(ui.validation.generation, [0; 2]);
}

#[test]
fn timers_fire_during_progress_but_the_current_native_view_has_no_fields() {
    let fixture = Fixture::new();
    let mut ui = fixture.ui();
    edit(&mut ui, Field::Password, "short");
    let mut busy = ui.snapshot.clone().unwrap();
    busy.busy = true;
    ui.sync(Some(busy));
    clock(&mut ui, 2000);
    ui.advance_validation(&fixture.runtime).unwrap();
    assert_eq!(ui.validation.deadlines[1], None);
    assert!(fixture.runtime.take_account_validation_results().is_empty());
    assert_eq!(ui.password_error, None);
    assert!(!ui.password_popup && !ui.password_border);
    assert!(ui.busy());
}

#[test]
fn progress_email_feedback_retains_wrapper_text_without_consuming_form_state() {
    let mut ui = ui(AccountView::SignIn);
    edit(&mut ui, Field::Email, "invalid");
    edit(&mut ui, Field::Password, "short");
    deliver(&mut ui, 18, 1);
    ui.validation.submitted = [true; 3];
    let deadlines = ui.validation.deadlines;
    let mut busy = snapshot(7, AccountView::SignIn);
    busy.busy = true;
    ui.sync(Some(busy));
    let revision = ui.revision;
    deliver(&mut ui, 18, 3);
    assert_eq!(ui.validation.email_error_text, Some(WRONG_EMAIL));
    assert_eq!(ui.email_error, None);
    assert!(!ui.email_popup && !ui.email_border);
    assert!(ui.validation.signin_email_error);
    assert!(!ui.validation.signin_password_error);
    assert_eq!(ui.validation.submitted, [true; 3]);
    assert_eq!(ui.validation.deadlines, deadlines);
    assert_eq!(ui.revision, revision);
}

#[test]
fn submit_preserves_timers_and_native_cache_copy_order() {
    let mut ui = ui(AccountView::SignIn);
    edit(&mut ui, Field::Email, "cached");
    edit(&mut ui, Field::Password, "cached-password");
    let deadlines = ui.validation.deadlines;
    // Direct editor changes isolate the submit callback from editing-changed.
    ui.email.select_all();
    ui.email.replace("new-email");
    ui.password.select_all();
    ui.password.replace("");
    ui.validation_submission(AccountView::SignIn);
    assert_eq!(ui.validation.cached, ["cached", "cached-password"]);
    assert_eq!(ui.validation.submitted, [true, false, false]);
    ui.validation_submission(AccountView::Register2);
    assert_eq!(ui.validation.cached, ["new-email", ""]);
    ui.password.replace("new-password");
    ui.validation_submission(AccountView::SignIn);
    assert_eq!(ui.validation.cached, ["new-email", "new-password"]);
    ui.validation_submission(AccountView::ForgotPassword);
    assert_eq!(ui.validation.cached, ["", "new-password"]);
    assert_eq!(ui.validation.deadlines, deadlines);
    assert_eq!(ui.validation.submitted, [true; 3]);
}

#[test]
fn actual_required_submit_keeps_pending_timers_and_submit_flag() {
    let fixture = Fixture::new();
    let mut ui = fixture.ui();
    edit(&mut ui, Field::Email, "invalid");
    edit(&mut ui, Field::Password, "");
    let deadlines = ui.validation.deadlines;
    ui.execute(&fixture.runtime, Command::Submit).unwrap();
    assert_eq!(ui.validation.deadlines, deadlines);
    assert_eq!(ui.validation.submitted, [true, false, false]);
    assert_eq!(ui.password_error, Some(PASSWORD_REQUIRED));
    assert!(ui.password_popup && ui.password_border);
}

#[test]
fn shared_cache_restores_unsubmitted_email_and_password_across_pages() {
    let mut ui = ui(AccountView::SignIn);
    edit(&mut ui, Field::Email, "draft@first.invalid");
    edit(&mut ui, Field::Password, "draft-secret");
    ui.sync(Some(snapshot(7, AccountView::Register2)));
    assert_eq!(ui.email.text(), "draft@first.invalid");
    assert_eq!(ui.password.text(), "draft-secret");
    ui.sync(Some(snapshot(7, AccountView::ForgotPassword)));
    assert_eq!(ui.forgot_email.text(), "");
    edit(&mut ui, Field::Email, "draft@reset.invalid");
    ui.sync(Some(snapshot(7, AccountView::SignIn)));
    assert_eq!(ui.email.text(), "draft@reset.invalid");
    assert_eq!(ui.password.text(), "draft-secret");
    assert_eq!(ui.validation.submitted, [false; 3]);
}

#[test]
fn editing_clears_only_its_visuals_with_native_submit_flag_asymmetry() {
    let mut ui = ui(AccountView::SignIn);
    ui.email_error = Some(EMAIL_INVALID);
    ui.email_popup = true;
    ui.email_border = true;
    ui.password_error = Some(WRONG_PASSWORD);
    ui.password_popup = true;
    ui.password_border = true;
    ui.validation.submitted = [true; 3];
    edit(&mut ui, Field::Password, "short");
    assert_eq!(ui.validation.submitted, [false, true, true]);
    assert_eq!(ui.password_error, None);
    assert!(!ui.password_popup && !ui.password_border);
    assert_eq!(ui.email_error, Some(EMAIL_INVALID));
    assert!(ui.email_popup && ui.email_border);
    edit(&mut ui, Field::Email, "invalid");
    assert_eq!(ui.validation.submitted, [false; 3]);
    assert_eq!(ui.email_error, None);
    assert!(!ui.email_popup && !ui.email_border);
}

#[test]
fn before_submit_feedback_changes_icons_without_opening_bubbles_or_red_borders() {
    for view in [
        AccountView::SignIn,
        AccountView::Register2,
        AccountView::ForgotPassword,
    ] {
        let mut ui = ui(view);
        deliver(&mut ui, 18, 1);
        assert_eq!(ui.email_error, Some(EMAIL_INVALID));
        assert!(!ui.email_popup && !ui.email_border);
        if view != AccountView::ForgotPassword {
            ui.password_help = true;
            deliver(&mut ui, 19, 6);
            assert_eq!(ui.password_error, Some(PASSWORD_HELP));
            assert!(!ui.password_popup && !ui.password_border && !ui.password_help);
        }
        // Already-open presentation is not forcibly hidden by another reply.
        ui.email_popup = true;
        ui.email_border = true;
        deliver(&mut ui, 18, 3);
        assert_eq!(ui.email_error, Some(WRONG_EMAIL));
        assert!(ui.email_popup && ui.email_border);
    }
}

#[test]
fn valid_boolean_is_a_noop_and_cannot_suppress_an_error() {
    let mut ui = ui(AccountView::SignIn);
    deliver(&mut ui, 18, 3);
    ui.email_popup = true;
    ui.email_border = true;
    let revision = ui.revision;
    for valid in [false, true] {
        let mut result = feedback(&ui, 18, 1);
        result.error = None;
        result.valid = valid;
        ui.apply_validation_result(result);
        assert_eq!(ui.email_error, Some(WRONG_EMAIL));
        assert!(ui.email_popup && ui.email_border);
        assert_eq!(ui.revision, revision);
    }
    let mut result = feedback(&ui, 16, 2);
    result.valid = true;
    ui.apply_validation_result(result);
    assert_eq!(ui.email_error, Some(EMAIL_TAKEN));
}

#[test]
fn submitted_email_error_cancels_both_timers_except_forgot_password() {
    for (view, index) in [
        (AccountView::SignIn, 0),
        (AccountView::Register2, 1),
        (AccountView::ForgotPassword, 2),
    ] {
        let mut ui = ui(view);
        ui.validation.deadlines = [Some(Duration::from_secs(4)); 2];
        ui.validation.submitted = [true; 3];
        deliver(&mut ui, 15, 1);
        assert!(ui.email_popup && ui.email_border);
        assert_eq!(ui.validation.deadlines[0], None);
        assert_eq!(
            ui.validation.deadlines[1],
            (index == 2).then_some(Duration::from_secs(4))
        );
        let mut flags = [true; 3];
        flags[index] = false;
        assert_eq!(ui.validation.submitted, flags);
    }
}

#[test]
fn submitted_password_error_keeps_timers_and_has_view_specific_presentation() {
    let mut signin = ui(AccountView::SignIn);
    signin.validation.submitted = [true; 3];
    signin.validation.deadlines = [Some(Duration::from_secs(4)); 2];
    deliver(&mut signin, 19, 6);
    assert_eq!(signin.password_error, Some(WRONG_PASSWORD));
    assert!(signin.password_popup && signin.password_border);
    assert_eq!(signin.validation.submitted, [false, true, true]);
    assert_eq!(
        signin.validation.deadlines,
        [Some(Duration::from_secs(4)); 2]
    );

    for previous in [None, Some(PASSWORD_HELP)] {
        for popup in [false, true] {
            let mut register = ui(AccountView::Register2);
            register.password_error = previous;
            register.password_popup = popup;
            register.validation.submitted[1] = true;
            deliver(&mut register, 17, 4);
            assert_eq!(
                register.password_error,
                Some(previous.unwrap_or(PASSWORD_REQUIRED))
            );
            assert_eq!(register.password_popup, popup);
            assert!(register.password_border);
            assert!(!register.validation.submitted[1]);
        }
    }
}

#[test]
fn register2_edit_hides_password_error_but_preserves_its_text_for_submitted_feedback() {
    let mut ui = ui(AccountView::Register2);
    deliver(&mut ui, 17, 4);
    assert_eq!(ui.password_error, Some(PASSWORD_HELP));
    edit(&mut ui, Field::Password, "another-short-input");
    assert_eq!(ui.password_error, None);
    assert_eq!(ui.validation.password_error_text, Some(PASSWORD_HELP));
    assert!(!ui.password_popup && !ui.password_border);
    ui.validation_submission(AccountView::Register2);
    deliver(&mut ui, 17, 4);
    assert_eq!(ui.password_error, Some(PASSWORD_HELP));
    assert!(ui.password_border);
    assert!(!ui.password_popup);
}

#[test]
fn new_register2_nib_discards_hidden_password_label_text_and_uses_required_default() {
    let mut ui = ui(AccountView::Register2);
    deliver(&mut ui, 17, 4);
    edit(&mut ui, Field::Password, "edited");
    assert_eq!(ui.validation.password_error_text, Some(PASSWORD_HELP));
    ui.sync(Some(snapshot(7, AccountView::Register1)));
    ui.sync(Some(snapshot(7, AccountView::Register2)));
    assert_eq!(ui.validation.password_error_text, None);
    assert_eq!(ui.password_error, None);
    ui.validation_submission(AccountView::Register2);
    deliver(&mut ui, 17, 4);
    assert_eq!(ui.password_error, Some(PASSWORD_REQUIRED));
    assert!(ui.password_border);
    assert!(!ui.password_popup);
}

#[test]
fn forgot_password_and_non_input_views_ignore_password_feedback() {
    for view in [
        AccountView::ForgotPassword,
        AccountView::Help1,
        AccountView::Register1,
        AccountView::NoNetworkConnectivity,
    ] {
        for submitted in [false, true] {
            let mut ui = ui(view);
            ui.validation.submitted = [submitted; 3];
            let revision = ui.revision;
            for field in [17, 19, 23] {
                deliver(&mut ui, field, 4);
            }
            assert_eq!(ui.password_error, None);
            assert!(!ui.password_popup && !ui.password_border);
            assert_eq!(ui.revision, revision);
            assert_eq!(ui.validation.submitted, [submitted; 3]);
        }
    }
}

#[test]
fn only_register2_empty_password_submit_can_color_an_existing_email_icon_border() {
    for view in [AccountView::SignIn, AccountView::Register2] {
        for password in ["", "short"] {
            let mut ui = ui(view);
            ui.email_error = Some(EMAIL_INVALID);
            ui.password.replace(password);
            ui.password_error = Some(PASSWORD_REQUIRED);
            ui.password_popup = true;
            ui.decorate_submission_errors();
            assert!(ui.password_border);
            assert!(!ui.email_popup);
            assert_eq!(
                ui.email_border,
                view == AccountView::Register2 && password.is_empty()
            );
        }
    }
}

#[test]
fn in_flight_old_generations_apply_in_arrival_order_to_current_owner_view() {
    let mut ui = ui(AccountView::SignIn);
    edit(&mut ui, Field::Email, "old");
    let mut old = feedback(&ui, 18, 3);
    old.generation = 1;
    edit(&mut ui, Field::Email, "new");
    ui.sync(Some(snapshot(7, AccountView::Register2)));
    assert_eq!(ui.validation.generation[0], 2);
    ui.apply_validation_result(old); // Result.view intentionally remains SignIn.
    assert_eq!(ui.email_error, Some(WRONG_EMAIL));
    let mut next = old;
    next.error.as_mut().unwrap().message = 1;
    ui.apply_validation_result(next);
    assert_eq!(ui.email_error, Some(EMAIL_INVALID));
    let revision = ui.revision;
    next.id = 999;
    next.error.as_mut().unwrap().message = 2;
    ui.apply_validation_result(next);
    assert_eq!(ui.email_error, Some(EMAIL_INVALID));
    assert_eq!(ui.revision, revision);
}

#[test]
fn signin_retains_error_icons_across_help_without_reopening_popups_or_borders() {
    let mut ui = ui(AccountView::SignIn);
    edit(&mut ui, Field::Password, "short");
    deliver(&mut ui, 18, 3);
    deliver(&mut ui, 19, 6);
    ui.email_popup = true;
    ui.password_popup = true;
    ui.email_border = true;
    ui.password_border = true;
    ui.sync(Some(snapshot(7, AccountView::Help1)));
    ui.sync(Some(snapshot(7, AccountView::SignIn)));
    assert_eq!(ui.email_error, Some(WRONG_EMAIL));
    assert_eq!(ui.password_error, Some(PASSWORD_HELP));
    assert!(!ui.email_popup && !ui.password_popup && !ui.email_border && !ui.password_border);
    edit(&mut ui, Field::Email, "changed");
    edit(&mut ui, Field::Password, "long-enough-secret");
    ui.sync(Some(snapshot(7, AccountView::Help1)));
    ui.sync(Some(snapshot(7, AccountView::SignIn)));
    assert_eq!((ui.email_error, ui.password_error), (None, None));
}

#[test]
fn submitted_snapshot_errors_use_the_same_retained_signin_icon_state() {
    for (field, message) in [(18, 3), (19, 6)] {
        let mut ui = ui(AccountView::SignIn);
        edit(&mut ui, Field::Email, "nobody@example.invalid");
        edit(&mut ui, Field::Password, "long-enough-password");
        let mut error = snapshot(7, AccountView::SignIn);
        error.field_error = Some(AccountFieldError { field, message });
        ui.sync(Some(error));
        ui.sync(Some(snapshot(7, AccountView::Help1)));
        ui.sync(Some(snapshot(7, AccountView::SignIn)));
        if field == 18 {
            assert_eq!(ui.email_error, Some(WRONG_EMAIL));
        } else {
            assert_eq!(ui.password_error, Some(WRONG_PASSWORD));
        }
        assert!(!ui.email_popup && !ui.password_popup && !ui.email_border && !ui.password_border);
    }
}

#[test]
fn marked_text_changes_restart_timer_but_ime_cursor_only_changes_do_not() {
    let fixture = Fixture::new();
    let mut ui = fixture.ui();
    edit(&mut ui, Field::Password, "x");
    clock(&mut ui, 100);
    ui.preedit("中", Some((0, 3)));
    assert_eq!(ui.validation.cached[1], "x中");
    assert_eq!(ui.password.text(), "x");
    assert_eq!(
        ui.validation.deadlines[1],
        Some(Duration::from_millis(2100))
    );
    let generation = ui.validation.generation[1];
    clock(&mut ui, 900);
    ui.preedit("中", Some((3, 3)));
    assert_eq!(ui.validation.generation[1], generation);
    assert_eq!(
        ui.validation.deadlines[1],
        Some(Duration::from_millis(2100))
    );
    clock(&mut ui, 2100);
    ui.advance_validation(&fixture.runtime).unwrap();
    assert_eq!(ui.password_error, Some(PASSWORD_HELP));
    assert_eq!(ui.password.preedit, "中"); // Native has no marked-text timer pause.
    ui.text("中");
    assert_eq!(ui.validation.cached[1], "x中");
    assert!(ui.password.preedit.is_empty());
    assert_eq!(ui.password_error, None);
    assert_eq!(
        ui.validation.deadlines[1],
        Some(Duration::from_millis(4100))
    );
}

#[test]
fn ime_cancel_restores_cache_to_visible_committed_text_and_restarts_edit_timer() {
    // Desktop adaptation: removing marked text changes field contents, unlike
    // moving its cursor. Do not later validate text the user just cancelled.
    for blur in [false, true] {
        let mut ui = ui(AccountView::SignIn);
        edit(&mut ui, Field::Password, "committed");
        ui.preedit("撤销", Some((0, 6)));
        clock(&mut ui, 700);
        if blur {
            ui.focus(None);
        } else {
            assert_eq!(key(&mut ui, NamedKey::Escape), None);
        }
        assert!(ui.password.preedit.is_empty());
        assert_eq!(ui.validation.cached[1], "committed");
        assert_eq!(
            ui.validation.deadlines[1],
            Some(Duration::from_millis(2700))
        );
    }
}

#[test]
fn caret_and_selection_are_not_edits_but_actual_deletion_is() {
    let mut ui = ui(AccountView::SignIn);
    edit(&mut ui, Field::Email, "e\u{301}x");
    deliver(&mut ui, 18, 3);
    clock(&mut ui, 700);
    let deadlines = ui.validation.deadlines;
    let generation = ui.validation.generation;
    key(&mut ui, NamedKey::Home);
    key(&mut ui, NamedKey::Backspace); // No content to delete at start.
    key(&mut ui, NamedKey::End);
    ui.key(&Key::Character("a".into()), None, ModifiersState::CONTROL);
    assert_eq!(ui.validation.deadlines, deadlines);
    assert_eq!(ui.validation.generation, generation);
    assert_eq!(ui.email_error, Some(WRONG_EMAIL));
    key(&mut ui, NamedKey::Backspace);
    assert_eq!(ui.email.text(), "");
    assert_eq!(ui.validation.cached[0], "");
    assert_eq!(
        ui.validation.deadlines[0],
        Some(Duration::from_millis(2700))
    );
    assert_eq!(ui.email_error, None);
}

#[test]
fn native_utf8_cache_uses_strlen_boundary_and_clock_does_not_move_backwards() {
    let mut ui = ui(AccountView::SignIn);
    clock(&mut ui, 900);
    edit(&mut ui, Field::Password, "x");
    clock(&mut ui, 100);
    ui.preedit("中\0ignored", Some((3, 3)));
    assert_eq!(ui.validation.now, Duration::from_millis(900));
    assert_eq!(ui.validation.cached[1], "x中");
    assert_eq!(
        ui.validation.deadlines[1],
        Some(Duration::from_millis(2900))
    );
}
