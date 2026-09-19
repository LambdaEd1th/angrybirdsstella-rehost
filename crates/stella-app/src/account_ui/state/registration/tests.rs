//! Pure native-form/desktop-input checks. No clipboard, URLs, or remote service.

use super::*;
use std::{
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};

const TODAY: RegistrationBirthday = RegistrationBirthday {
    day: 5,
    month: 9,
    year: 2026,
};
const EMAIL_REQUIRED: (&str, &str) = (
    "rovio_id_validate_email_required",
    "Please enter your email address",
);
const PASSWORD_REQUIRED: (&str, &str) = (
    "rovio_id_validate_password_required",
    "Please enter a password",
);
const PASSWORD_SHORT: (&str, &str) = (
    "rovio_id_password_help_text",
    "Password must contain at least 8 characters",
);

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
    ui.sync(Some(snapshot(71, view)));
    ui.set_calendar_today(TODAY);
    ui
}

fn click(ui: &mut AccountUi, name: Option<&str>) -> Option<Command> {
    ui.press(name);
    ui.release(name)
}

fn press(ui: &mut AccountUi, key: NamedKey) -> Option<Command> {
    ui.key(&Key::Named(key), None, ModifiersState::empty())
}

fn shortcut(ui: &mut AccountUi, key: &str, modifier: ModifiersState) -> Option<Command> {
    ui.key(&Key::Character(key.into()), None, modifier)
}

fn enter(ui: &mut AccountUi, field: Field, value: &str) {
    ui.focus(Some(field));
    shortcut(ui, "a", ModifiersState::CONTROL);
    ui.text(value);
}

#[test]
fn repeated_impossible_date_repaints_errors_even_when_snapshot_is_identical() {
    let fixture = Fixture::register2();
    let id = fixture.runtime.account_ui().unwrap().id;
    fixture
        .runtime
        .account_ui_action(id, AccountUiAction::Back)
        .unwrap();
    let mut ui = fixture.ui();
    ui.registration.values = [Some(31), Some(2), Some(2000)];
    ui.execute(&fixture.runtime, Command::Submit).unwrap();
    let first = fixture.runtime.account_ui();
    assert_eq!(ui.registration.date_errors, [true, true, false]);
    ui.registration.values[1] = Some(4);
    ui.registration.date_errors = [false; 3];
    ui.execute(&fixture.runtime, Command::Submit).unwrap();
    assert_eq!(fixture.runtime.account_ui(), first);
    assert_eq!(ui.registration.date_errors, [true, true, false]);
}

/// Each runtime has its own data/appdata roots and no configured endpoint.
struct Fixture {
    runtime: StellaLua,
    root: PathBuf,
}

impl Fixture {
    fn register2() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(1);
        let root = std::env::temp_dir().join(format!(
            "stella-register-input-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        let data = root.join("data");
        std::fs::create_dir_all(&data).unwrap();
        let runtime = StellaLua::new(data).unwrap();
        runtime
            .execute_source("_G.SkynestAccount.native_login(true, false, true)")
            .unwrap();
        let initial = runtime.account_ui().unwrap();
        assert_eq!(initial.view, AccountView::Register1);
        assert!(
            runtime
                .submit_account_birthday(
                    initial.id,
                    RegistrationBirthday {
                        day: 1,
                        month: 1,
                        year: 1900,
                    }
                )
                .unwrap()
        );
        assert_eq!(runtime.account_ui().unwrap().view, AccountView::Register2);
        Self { runtime, root }
    }

    fn ui(&self) -> AccountUi {
        let mut ui = AccountUi::default();
        ui.sync(self.runtime.account_ui());
        ui
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

#[test]
fn birthday_fields_start_empty_open_today_and_outside_close_does_not_fill_them() {
    let mut ui = ui(AccountView::Register1);
    assert_eq!(ui.registration.values, [None; 3]);
    assert_eq!(ui.registration.picker, None);
    for (name, tag, row) in [
        ("dayTextField", 0, 4),
        ("monthTextField", 1, 8),
        ("yearTextField", 2, 126),
    ] {
        assert_eq!(click(&mut ui, Some(name)), None);
        assert_eq!(ui.registration.picker, Some(tag));
        assert_eq!(ui.registration.row, row);
        assert_eq!(ui.registration.values, [None; 3]);
        assert_eq!(click(&mut ui, None), None);
        assert_eq!(ui.registration.picker, None);
        assert_eq!(ui.registration.values, [None; 3]);
    }
    ui.focus(Some(Field::Email));
    assert_eq!(
        ui.focused(),
        None,
        "birthday nib controls are not text editors"
    );
}

#[test]
fn visible_selection_updates_immediately_and_second_field_tap_commits_then_closes() {
    let mut ui = ui(AccountView::Register1);
    click(&mut ui, Some("dayTextField"));
    ui.scroll_picker(0);
    assert_eq!(ui.registration.values[0], None);
    ui.scroll_picker(2);
    assert_eq!(ui.registration.values[0], Some(7));
    assert_eq!(ui.registration.picker, Some(0));
    assert_eq!(ui.registration.row, 6);
    assert_eq!(click(&mut ui, Some("dayTextField")), None);
    assert_eq!(ui.registration.values[0], Some(7));
    assert_eq!(ui.registration.picker, None);
    click(&mut ui, Some("dayTextField"));
    assert_eq!(
        ui.registration.row, 6,
        "reopen restores displayed selection"
    );

    click(&mut ui, Some("monthTextField"));
    assert_eq!(ui.registration.picker, Some(1));
    assert_eq!(ui.registration.values[1], None);
    assert_eq!(click(&mut ui, Some("monthTextField")), None);
    assert_eq!(ui.registration.values[1], Some(9));
    assert_eq!(ui.registration.picker, None);

    click(&mut ui, Some("yearTextField"));
    assert_eq!(click(&mut ui, Some("pickerRowMinus1")), None);
    assert_eq!(ui.registration.values[2], Some(2025));
    assert_eq!(
        ui.registration.picker, None,
        "picker-body tap dismisses after selecting"
    );
}

#[test]
fn picker_boundaries_and_february_keep_native_independent_day_rows() {
    let mut ui = ui(AccountView::Register1);
    for (name, tag, count, first, last) in [
        ("dayTextField", 0, 31, 1, 31),
        ("monthTextField", 1, 12, 1, 12),
        ("yearTextField", 2, 127, 1900, 2026),
    ] {
        click(&mut ui, Some(name));
        assert_eq!(ui.registration.row_count(tag), count);
        assert_eq!(press(&mut ui, NamedKey::Home), None);
        assert_eq!(ui.registration.row, 0);
        assert_eq!(ui.registration.values[usize::from(tag)], Some(first));
        assert_eq!(press(&mut ui, NamedKey::ArrowUp), None);
        assert_eq!(ui.registration.row, 0);
        assert_eq!(press(&mut ui, NamedKey::End), None);
        assert_eq!(ui.registration.row, count - 1);
        assert_eq!(ui.registration.values[usize::from(tag)], Some(last));
        ui.scroll_picker(i32::MAX);
        assert_eq!(ui.registration.row, count - 1);
    }
    click(&mut ui, Some("monthTextField"));
    press(&mut ui, NamedKey::Home);
    press(&mut ui, NamedKey::ArrowDown);
    assert_eq!(ui.registration.values[1], Some(2));
    assert_eq!(ui.registration.values[0], Some(31));
    assert_eq!(ui.registration.row_count(0), 31);
    press(&mut ui, NamedKey::Enter);
    assert_eq!(ui.registration.picker, None);
    click(&mut ui, Some("dayTextField"));
    assert_eq!(ui.registration.row, 30);
    assert_eq!(ui.registration.values[0], Some(31));
}

#[test]
fn picker_keys_consume_input_and_escape_closes_without_filling_an_untouched_field() {
    let mut ui = ui(AccountView::Register1);
    click(&mut ui, Some("dayTextField"));
    assert_eq!(
        ui.key(
            &Key::Character("a".into()),
            Some("a"),
            ModifiersState::empty()
        ),
        None
    );
    assert_eq!(press(&mut ui, NamedKey::Tab), None);
    assert_eq!(ui.focused(), None);
    assert_eq!(ui.registration.values[0], None);
    assert_eq!(press(&mut ui, NamedKey::Escape), None);
    assert_eq!(ui.registration.picker, None);
    assert_eq!(ui.registration.values[0], None);
    click(&mut ui, Some("dayTextField"));
    assert_eq!(press(&mut ui, NamedKey::Enter), None);
    assert_eq!(ui.registration.values[0], Some(5));
    assert_eq!(ui.registration.picker, None);
    assert_eq!(
        press(&mut ui, NamedKey::Escape),
        Some(Command::Action(AccountUiAction::Cancel))
    );
}

#[test]
fn native_continue_marks_missing_dates_and_dismisses_before_submitting() {
    let mut ui = ui(AccountView::Register1);
    click(&mut ui, Some("monthTextField"));
    assert_eq!(click(&mut ui, Some("continueButton")), None);
    assert_eq!(ui.registration.date_errors, [true; 3]);
    assert_eq!(ui.registration.picker, None);
    assert_eq!(ui.registration.values, [None; 3]);
    ui.registration.values = [Some(1), None, Some(2000)];
    assert_eq!(click(&mut ui, Some("continueButton")), None);
    assert_eq!(ui.registration.date_errors, [false, true, false]);
    ui.registration.values[1] = Some(1);
    click(&mut ui, Some("yearTextField"));
    assert_eq!(click(&mut ui, Some("continueButton")), None);
    assert_eq!(ui.registration.date_errors, [false; 3]);
    assert_eq!(ui.registration.picker, None);
    assert_eq!(
        click(&mut ui, Some("continueButton")),
        Some(Command::Submit)
    );
}

#[test]
fn legal_links_close_open_picker_before_generating_literal_native_url() {
    for (name, url) in [
        ("eulaLabel", "http://www.rovio.com/eula"),
        ("privacyPolicyLabel", "http://www.rovio.com/privacy"),
    ] {
        let mut ui = ui(AccountView::Register1);
        click(&mut ui, Some("dayTextField"));
        assert_eq!(click(&mut ui, Some(name)), None);
        assert_eq!(ui.registration.picker, None);
        assert_eq!(ui.registration.values[0], None);
        assert_eq!(click(&mut ui, Some(name)), Some(Command::OpenUrl(url)));
    }
}

#[test]
fn native_registration_button_outlets_do_not_inherit_wrong_signin_actions() {
    let mut ui = ui(AccountView::Register1);
    assert_eq!(
        click(&mut ui, Some("backButton")),
        Some(Command::Action(AccountUiAction::Cancel))
    );
    assert_eq!(
        click(&mut ui, Some("questionButton")),
        Some(Command::Action(AccountUiAction::Back))
    );
    assert_eq!(click(&mut ui, Some("registerLabel")), None);
    ui.press(Some("backButton"));
    assert_eq!(ui.release(Some("questionButton")), None);
    ui.sync(Some(snapshot(71, AccountView::Register2)));
    assert_eq!(
        click(&mut ui, Some("backButton")),
        Some(Command::Action(AccountUiAction::Back))
    );
    assert_eq!(
        click(&mut ui, Some("closeButton")),
        Some(Command::Action(AccountUiAction::Cancel))
    );
    assert_eq!(
        click(&mut ui, Some("registerButton")),
        Some(Command::Submit)
    );
}

#[test]
fn retained_gender_and_birthdate_survive_same_owner_navigation_but_not_replacement() {
    let mut ui = ui(AccountView::Register2);
    assert!(!ui.registration.female);
    assert_eq!(click(&mut ui, Some("gender_female_button")), None);
    assert!(ui.registration.female);
    ui.registration.values = [Some(5), Some(9), Some(2000)];
    ui.sync(Some(snapshot(71, AccountView::Register1)));
    assert_eq!(ui.registration.values, [Some(5), Some(9), Some(2000)]);
    assert!(ui.registration.female);
    ui.sync(Some(snapshot(71, AccountView::Register2)));
    assert!(ui.registration.female);
    click(&mut ui, Some("gender_male_button"));
    assert!(!ui.registration.female);
    let mut busy = snapshot(71, AccountView::Register2);
    busy.busy = true;
    ui.sync(Some(busy));
    click(&mut ui, Some("gender_female_button"));
    assert!(!ui.registration.female);
    ui.sync(Some(snapshot(72, AccountView::Register1)));
    assert_eq!(ui.registration.values, [None; 3]);
    assert!(!ui.registration.female);
}

#[test]
fn register2_return_and_ime_follow_native_field_delegate_not_submit() {
    let mut ui = ui(AccountView::Register2);
    enter(&mut ui, Field::Email, "bird@example.invalid");
    assert_eq!(press(&mut ui, NamedKey::Enter), None);
    assert_eq!(ui.focused(), Some(Field::Password));
    assert_eq!(press(&mut ui, NamedKey::Enter), None);
    assert_eq!(ui.focused(), None);
    assert_eq!(
        ui.key(&Key::Named(NamedKey::Tab), None, ModifiersState::SHIFT),
        None
    );
    assert_eq!(ui.focused(), Some(Field::Password));
    ui.focus(Some(Field::Email));
    ui.preedit("邮箱", Some((0, 6)));
    assert_eq!(press(&mut ui, NamedKey::Enter), None);
    assert_eq!(press(&mut ui, NamedKey::Tab), None);
    assert_eq!(ui.focused(), Some(Field::Email));
    assert_eq!(ui.email.preedit, "邮箱");
    assert!(!ui.busy());
    assert_eq!(ui.snapshot.as_ref().unwrap().view, AccountView::Register2);
}

#[test]
fn register2_clipboard_commands_are_explicit_and_secure_text_never_exports() {
    for modifier in [ModifiersState::CONTROL, ModifiersState::SUPER] {
        let mut ui = ui(AccountView::Register2);
        enter(&mut ui, Field::Email, "鸟@example.invalid");
        assert_eq!(
            shortcut(&mut ui, "c", modifier),
            None,
            "empty selection is not copied"
        );
        assert_eq!(shortcut(&mut ui, "a", modifier), None);
        assert_eq!(shortcut(&mut ui, "c", modifier), Some(Command::Copy));
        assert_eq!(shortcut(&mut ui, "x", modifier), Some(Command::Cut));
        assert_eq!(ui.copyable_selection(), Some("鸟@example.invalid"));
        assert_eq!(
            ui.email.text(),
            "鸟@example.invalid",
            "commands alone do not modify text"
        );
        assert_eq!(shortcut(&mut ui, "v", modifier), Some(Command::Paste));

        enter(&mut ui, Field::Password, "private 🐦 text");
        shortcut(&mut ui, "a", modifier);
        assert_eq!(shortcut(&mut ui, "c", modifier), None);
        assert_eq!(shortcut(&mut ui, "x", modifier), None);
        assert_eq!(ui.copyable_selection(), None);
        assert_eq!(shortcut(&mut ui, "v", modifier), Some(Command::Paste));
        assert_eq!(ui.password.text(), "private 🐦 text");
        ui.preedit("秘密", Some((0, 6)));
        assert_eq!(
            shortcut(&mut ui, "v", modifier),
            None,
            "IME keeps input ownership"
        );
    }
}

#[test]
fn register2_checks_raw_email_first_then_required_password_without_trimming() {
    let fixture = Fixture::register2();
    let mut ui = fixture.ui();
    ui.password_help = true;
    ui.execute(&fixture.runtime, Command::Submit).unwrap();
    assert_eq!(ui.email_error, Some(EMAIL_REQUIRED));
    assert!(ui.email_popup);
    assert_eq!(ui.password_error, None);
    assert!(!ui.busy());
    enter(&mut ui, Field::Email, " ");
    ui.execute(&fixture.runtime, Command::Submit).unwrap();
    assert_eq!(
        ui.email_error, None,
        "raw nonempty email is accepted by wrapper"
    );
    assert_eq!(ui.password_error, Some(PASSWORD_REQUIRED));
    assert!(ui.password_popup);
    assert!(!ui.password_help);
    assert!(!ui.busy());
    assert_eq!(
        fixture.runtime.account_ui().unwrap().view,
        AccountView::Register2
    );
}

#[test]
fn register2_native_password_error_ids_restore_required_or_short_messages() {
    for (message, expected) in [(5, PASSWORD_REQUIRED), (4, PASSWORD_SHORT)] {
        let mut ui = ui(AccountView::Register2);
        let mut next = snapshot(71, AccountView::Register2);
        next.field_error = Some(stella_script::AccountFieldError { field: 17, message });
        ui.sync(Some(next));
        assert_eq!(ui.password_error, Some(expected));
        assert!(ui.password_popup);
        assert_eq!(ui.email_error, None);
        enter(&mut ui, Field::Password, "corrected");
        assert_eq!(ui.password_error, None);
        assert!(!ui.password_popup);
    }
}

#[test]
fn register2_password_threshold_counts_utf8_bytes_not_characters() {
    for (password, accepted) in [
        ("1234567", false),
        ("éééa", false),
        ("éééé", true),
        ("🐦🐦", true),
        ("        ", true),
        ("12345678", true),
    ] {
        let fixture = Fixture::register2();
        let mut ui = fixture.ui();
        enter(&mut ui, Field::Email, "not-an-email");
        enter(&mut ui, Field::Password, password);
        ui.execute(&fixture.runtime, Command::Submit).unwrap();
        assert_eq!(
            ui.busy(),
            accepted,
            "password byte length {}",
            password.len()
        );
        assert_eq!(
            ui.password_error,
            if accepted { None } else { Some(PASSWORD_SHORT) }
        );
        assert_eq!(
            ui.email_error, None,
            "form submit must not invent an email regex"
        );
        assert_eq!(
            fixture.runtime.account_ui().unwrap().view,
            AccountView::Register2
        );
        // No endpoint is configured: accepted means queued native submission,
        // not a successful account, nor a network request to a retired service.
        assert_eq!(ui.password.text(), password);
    }
}

#[test]
fn old_register_email_error_is_not_an_extra_submit_gate() {
    let fixture = Fixture::register2();
    let mut ui = fixture.ui();
    enter(&mut ui, Field::Email, "unchanged@example.invalid");
    enter(&mut ui, Field::Password, "eight bytes");
    ui.email_error = Some((
        "rovio_id_validate_email_invalid",
        "Please enter a valid email address",
    ));
    ui.email_popup = true;
    ui.execute(&fixture.runtime, Command::Submit).unwrap();
    assert!(ui.busy());
    assert_eq!(ui.email_error, None);
    assert!(!ui.email_popup);
    assert_eq!(ui.snapshot.as_ref().unwrap().view, AccountView::Register2);
}
