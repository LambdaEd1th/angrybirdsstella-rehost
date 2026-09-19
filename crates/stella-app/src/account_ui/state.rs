use stella_script::{AccountUiAction, AccountUiSnapshot, AccountView, StellaLua};
use winit::keyboard::{Key, ModifiersState, NamedKey};

use super::editor::Editor;

mod registration;
mod validation;
pub(super) use registration::Registration;
use validation::Validation;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Field {
    Email,
    Password,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Command {
    Action(AccountUiAction),
    Submit,
    Paste,
    Copy,
    Cut,
    OpenUrl(&'static str),
}

#[derive(Default)]
pub(crate) struct AccountUi {
    pub(super) snapshot: Option<AccountUiSnapshot>,
    pub(super) email: Editor,
    pub(super) forgot_email: Editor,
    pub(super) password: Editor,
    pub(super) focus: Option<Field>,
    pub(super) email_error: Option<(&'static str, &'static str)>,
    pub(super) password_error: Option<(&'static str, &'static str)>,
    pub(super) email_popup: bool,
    pub(super) password_popup: bool,
    pub(super) password_help: bool,
    pub(super) email_border: bool,
    pub(super) password_border: bool,
    validation: Validation,
    pub(super) registration: Registration,
    pub(super) pressed: Option<String>,
    pub(super) pointer: [f32; 2],
    pub(super) revision: u64,
}

impl AccountUi {
    pub(crate) fn visible(&self) -> bool {
        self.snapshot.is_some()
    }

    pub(crate) fn owner_id(&self) -> Option<u64> {
        self.snapshot.as_ref().map(|s| s.id)
    }

    pub(crate) fn busy(&self) -> bool {
        self.snapshot.as_ref().is_some_and(|snapshot| snapshot.busy)
    }

    pub(crate) fn focused(&self) -> Option<Field> {
        self.focus
    }

    pub(crate) fn sync(&mut self, next: Option<AccountUiSnapshot>) -> bool {
        if self.snapshot == next {
            return false;
        }
        let changed_owner = self.snapshot.as_ref().map(|s| s.id) != next.as_ref().map(|s| s.id);
        let changed_view = self.snapshot.as_ref().map(|s| s.view) != next.as_ref().map(|s| s.view);
        if changed_owner {
            self.email = Editor::default();
            self.forgot_email = Editor::default();
            self.password = Editor::default();
            self.registration = Registration::default();
            self.validation = Validation::default();
        }
        if changed_view
            && next
                .as_ref()
                .is_some_and(|s| s.view == AccountView::ForgotPassword && s.field_error.is_none())
        {
            // 100773494 sets the old view's address before allocating a new
            // SForgotPasswordView nib. A new/reset-back page starts blank;
            // the submitted-error callback separately restores its address.
            self.forgot_email = Editor::default();
        }
        if changed_view || changed_owner || next.as_ref().is_some_and(|s| s.busy) {
            self.focus(None);
            self.reset_validation_view();
            self.pressed = None;
            self.email_error = None;
            self.password_error = None;
            self.email_popup = false;
            self.password_popup = false;
            self.password_help = false;
            self.email_border = false;
            self.password_border = false;
            self.registration.picker = None;
        }
        if changed_view && let Some(view) = next.as_ref().map(|s| s.view) {
            self.restore_native_validation_text(view);
        }
        self.snapshot = next;
        // Native field/message decoding is separate from retaining text.
        // A repeated snapshot must not restore an error cleared by editing.
        if let Some(error) = self.snapshot.as_ref().and_then(|s| s.field_error) {
            match error.field {
                12..=14 => {
                    let index = match error.field {
                        12 => 0,
                        13 => 1,
                        _ => 2,
                    };
                    self.registration.date_errors[index] = true;
                    // showInvalidDayError (100772F1C) marks day and month.
                    if error.field == 13 && self.registration.values.iter().all(Option::is_some) {
                        self.registration.date_errors[0] = true;
                    }
                }
                15 | 16 | 18 => {
                    self.email_popup = true;
                    self.email_border = true;
                    self.invalidate_submitted_email_timers();
                    self.email_error = Some(match error.message {
                        2 => (
                            "rovio_id_email_taken",
                            "This email address has already been registered",
                        ),
                        3 => ("rovio_id_wrong_email", "Email address not found"),
                        _ => (
                            "rovio_id_validate_email_invalid",
                            "Please enter a valid email address",
                        ),
                    });
                    self.remember_native_field_error(error.field);
                }
                17 | 19 => {
                    self.password_popup = true;
                    self.password_border = true;
                    self.password_help = false;
                    self.password_error = Some(match (error.field, error.message) {
                        (17, 5) => (
                            "rovio_id_validate_password_required",
                            "Please enter a password",
                        ),
                        (17, 4) => (
                            "rovio_id_password_help_text",
                            "Password must contain at least 8 characters",
                        ),
                        _ => ("rovio_id_wrong_password", "Wrong password"),
                    });
                    self.remember_native_field_error(error.field);
                }
                _ => {}
            }
        }
        self.dirty();
        changed_owner
    }

    pub(super) fn dirty(&mut self) {
        self.revision = self.revision.wrapping_add(1);
    }

    pub(crate) fn focus(&mut self, next: Option<Field>) {
        if let Some(field) = next {
            let allowed = self.snapshot.as_ref().is_some_and(|s| {
                !s.busy
                    && matches!(
                        (s.view, field),
                        (AccountView::SignIn | AccountView::Register2, _)
                            | (AccountView::ForgotPassword, Field::Email)
                    )
            });
            if !allowed {
                return;
            }
        }
        if self.focus != next {
            self.clear_preedit();
            self.focus = next;
            self.dirty();
        }
    }

    pub(super) fn email_editor(&self) -> &Editor {
        if self
            .snapshot
            .as_ref()
            .is_some_and(|s| s.view == AccountView::ForgotPassword)
        {
            &self.forgot_email
        } else {
            &self.email
        }
    }

    pub(super) fn email_editor_mut(&mut self) -> &mut Editor {
        if self
            .snapshot
            .as_ref()
            .is_some_and(|s| s.view == AccountView::ForgotPassword)
        {
            &mut self.forgot_email
        } else {
            &mut self.email
        }
    }

    fn editor_mut(&mut self) -> Option<&mut Editor> {
        match self.focus {
            Some(Field::Email) => Some(self.email_editor_mut()),
            Some(Field::Password) => Some(&mut self.password),
            None => None,
        }
    }

    fn clear_field_error(&mut self) {
        match self.focus {
            Some(Field::Email) => {
                self.email_error = None;
                self.email_popup = false;
                self.email_border = false;
            }
            Some(Field::Password) => {
                self.retain_password_error_text();
                self.password_error = None;
                self.password_popup = false;
                self.password_help = false;
                self.password_border = false;
            }
            None => {}
        }
    }

    pub(crate) fn text(&mut self, text: &str) {
        if self.busy() {
            return;
        }
        if let Some(editor) = self.editor_mut() {
            editor.replace(text);
            self.clear_field_error();
            self.edit_validation_field();
            self.dirty();
        }
    }

    pub(crate) fn preedit(&mut self, text: &str, cursor: Option<(usize, usize)>) {
        if self.busy() {
            return;
        }
        if let Some(editor) = self.editor_mut() {
            let changed_text = editor.preedit != text;
            editor.set_preedit(text, cursor);
            if changed_text {
                self.clear_field_error();
                self.edit_validation_field();
            }
            self.dirty();
        }
    }

    pub(crate) fn clear_preedit(&mut self) {
        let changed_text = self
            .editor_mut()
            .is_some_and(|editor| !editor.preedit.is_empty());
        self.email.clear_preedit();
        self.forgot_email.clear_preedit();
        self.password.clear_preedit();
        if changed_text {
            // Desktop IME adaptation: cancellation changes visible contents,
            // so it is an edit, not a bare focus event. Do not validate marked
            // text after the host has removed it from the input field.
            self.clear_field_error();
            self.edit_validation_field();
        }
        self.dirty();
    }

    pub(crate) fn key(
        &mut self,
        key: &Key,
        text: Option<&str>,
        modifiers: ModifiersState,
    ) -> Option<Command> {
        if !self.visible() {
            return None;
        }
        if *key == Key::Named(NamedKey::Escape) {
            if self.registration.picker.take().is_some() {
                self.dirty();
                return None;
            }
            if self
                .editor_mut()
                .is_some_and(|editor| !editor.preedit.is_empty())
            {
                self.clear_preedit();
                return None;
            }
            return Some(Command::Action(AccountUiAction::Cancel));
        }
        if self.busy() {
            return None;
        }
        if self.registration_key(key) {
            return None;
        }
        // Let the desktop IME own candidate navigation/confirmation until it
        // commits. Tab must not discard marked text and move another field.
        if self
            .editor_mut()
            .is_some_and(|editor| !editor.preedit.is_empty())
        {
            return None;
        }
        if *key == Key::Named(NamedKey::Tab) {
            if !self.snapshot.as_ref().is_some_and(|s| {
                matches!(
                    s.view,
                    AccountView::SignIn | AccountView::Register2 | AccountView::ForgotPassword
                )
            }) {
                return None;
            }
            let has_password = self
                .snapshot
                .as_ref()
                .is_some_and(|s| matches!(s.view, AccountView::SignIn | AccountView::Register2));
            // Desktop focus-cycle adaptation, not a recovered UIKit shortcut:
            // reverse traversal from no focus starts at the last input field.
            let next = if has_password
                && (self.focus == Some(Field::Email)
                    || (self.focus.is_none() && modifiers.shift_key()))
            {
                Some(Field::Password)
            } else {
                Some(Field::Email)
            };
            self.focus(next);
            return None;
        }
        // Active SSignInView delegate 10077005C: email Return focuses the
        // password field; password Return resigns it. No implicit submit.
        if *key == Key::Named(NamedKey::Enter) {
            let sign_in = self
                .snapshot
                .as_ref()
                .is_some_and(|s| matches!(s.view, AccountView::SignIn | AccountView::Register2));
            self.focus(if sign_in && self.focus == Some(Field::Email) {
                Some(Field::Password)
            } else {
                None
            });
            return None;
        }
        let secure = self.focus == Some(Field::Password);
        let editor = self.editor_mut()?;
        let command = modifiers.super_key() || modifiers.control_key();
        if let Key::Character(key) = key
            && command
        {
            if key.eq_ignore_ascii_case("v") {
                return Some(Command::Paste);
            }
            if key.eq_ignore_ascii_case("c") || key.eq_ignore_ascii_case("x") {
                // Reject secure export before it can even request host
                // clipboard access. An empty selection is also a no-op.
                if secure || editor.selection().is_empty() {
                    return None;
                }
                return Some(if key.eq_ignore_ascii_case("c") {
                    Command::Copy
                } else {
                    Command::Cut
                });
            }
        }
        let previous_len = editor.text().len();
        match key {
            Key::Named(NamedKey::ArrowLeft) => editor.move_horizontal(false, modifiers.shift_key()),
            Key::Named(NamedKey::ArrowRight) => editor.move_horizontal(true, modifiers.shift_key()),
            Key::Named(NamedKey::Home) => editor.move_to(0, modifiers.shift_key()),
            Key::Named(NamedKey::End) => editor.move_to(editor.text().len(), modifiers.shift_key()),
            Key::Named(NamedKey::Backspace) => editor.erase(false),
            Key::Named(NamedKey::Delete) => editor.erase(true),
            Key::Character(key) if command && key.eq_ignore_ascii_case("a") => editor.select_all(),
            _ => {
                if !command && let Some(text) = text {
                    self.text(text);
                }
                return None;
            }
        }
        // Native clear handlers are registered for editing-changed, not caret
        // navigation. Do not erase a server error just by selecting text.
        if editor.text().len() != previous_len {
            self.clear_field_error();
            self.edit_validation_field();
        }
        self.dirty();
        None
    }

    /// Secure native text fields do not export their selection. Clipboard
    /// access is a user-triggered desktop operation, never an automatic read.
    pub(crate) fn copyable_selection(&self) -> Option<&str> {
        let email = self.email_editor();
        if self.focus != Some(Field::Email) || self.busy() || !email.preedit.is_empty() {
            return None;
        }
        let selection = email.selection();
        (!selection.is_empty()).then(|| &email.text()[selection])
    }

    pub(crate) fn move_pointer(&mut self, x: f32, y: f32) {
        self.pointer = [x, y];
    }

    pub(crate) fn pointer(&self) -> [f32; 2] {
        self.pointer
    }

    pub(crate) fn press(&mut self, name: Option<&str>) {
        self.pressed = name.map(str::to_owned);
        self.dirty();
    }

    pub(crate) fn pressed(&self) -> Option<&str> {
        self.pressed.as_deref()
    }

    pub(crate) fn release(&mut self, name: Option<&str>) -> Option<Command> {
        let pressed = self.pressed.take();
        self.dirty();
        if pressed.as_deref() != name {
            return None;
        }
        if let Some(command) = self.registration_release(name) {
            return command;
        }
        // Native root tap handlers hide the bubbles without clearing error
        // icons/borders, and reopen only the visible icon that was tapped.
        if !matches!(name, Some("emailTextField" | "passwordTextField")) {
            self.email_popup = name == Some("emailErrorButton") && self.email_error.is_some();
            self.password_popup =
                name == Some("passwordErrorButton") && self.password_error.is_some();
            self.password_help =
                name == Some("passwordTooltipButton") && self.password_error.is_none();
        }
        match name? {
            "signInButton" | "sendRequestButton" => Some(Command::Submit),
            "closeButton" => Some(Command::Action(AccountUiAction::Cancel)),
            "backButton" | "questionButton" => Some(Command::Action(AccountUiAction::Back)),
            "nextButton" | "okButton" => Some(Command::Action(AccountUiAction::Continue)),
            "forgotPasswordLabel" => Some(Command::Action(AccountUiAction::ForgotPassword)),
            "registerLabel" => Some(Command::Action(AccountUiAction::Register)),
            _ => None,
        }
    }

    pub(crate) fn execute(
        &mut self,
        runtime: &StellaLua,
        command: Command,
    ) -> Result<(), stella_script::ScriptError> {
        let Some(snapshot) = self.snapshot.clone() else {
            return Ok(());
        };
        match command {
            Command::Action(action) => {
                runtime.account_ui_action(snapshot.id, action)?;
            }
            Command::Submit if !snapshot.busy => {
                self.validation_submission(snapshot.view);
                if matches!(
                    snapshot.view,
                    AccountView::Register1 | AccountView::Register2
                ) {
                    self.submit_registration(runtime, &snapshot)?;
                    self.decorate_submission_errors();
                    self.sync(runtime.account_ui());
                    self.dirty();
                    return Ok(());
                }
                // Native signInAction validates raw NSString length, in this
                // order. It does not trim either value or invent a regex.
                if snapshot.view == AccountView::ForgotPassword {
                    // requestNewPasswordAction clears the retained SignIn
                    // address; reset entry itself is a separate native field.
                    self.email = Editor::default();
                }
                if self.email_editor().text().is_empty() {
                    self.email_popup = true;
                    self.email_error = Some((
                        "rovio_id_validate_email_required",
                        "Please enter your email address",
                    ));
                } else if snapshot.view == AccountView::SignIn && self.password.text().is_empty() {
                    self.password_popup = true;
                    self.password_error = Some((
                        "rovio_id_validate_password_required",
                        "Please enter a password",
                    ));
                } else if snapshot.view == AccountView::SignIn {
                    runtime.submit_account_login(
                        snapshot.id,
                        self.email.text(),
                        self.password.text(),
                    )?;
                } else if snapshot.view == AccountView::ForgotPassword {
                    if self.forgot_email.text().trim().is_empty() || self.email_error.is_some() {
                        self.email_popup = true;
                        self.email_error = Some((
                            "rovio_id_validate_email_invalid",
                            "Please enter a valid email address",
                        ));
                    } else {
                        runtime
                            .submit_account_password_reset(snapshot.id, self.forgot_email.text())?;
                    }
                }
            }
            Command::Submit => {}
            Command::Paste | Command::Copy | Command::Cut | Command::OpenUrl(_) => {}
        }
        if command == Command::Submit {
            self.decorate_submission_errors();
        }
        self.sync(runtime.account_ui());
        self.dirty();
        Ok(())
    }
}

#[cfg(test)]
mod tests;
