//! UIKit editing timers and field presentation. These use real elapsed time,
//! not scaled game time, and survive focus/page changes within the same owner.

use super::*;
use std::time::Duration;
use stella_script::{AccountValidationField, AccountValidationResult};

const DELAY: Duration = Duration::from_secs(2);
const PASSWORD_HELP: (&str, &str) = (
    "rovio_id_password_help_text",
    "Password must contain at least 8 characters",
);

#[derive(Default)]
pub(super) struct Validation {
    now: Duration,
    deadlines: [Option<Duration>; 2],
    generation: [u64; 2],
    /// Native wrapper view_address/view_password. Never included in snapshots,
    /// logging, save data, or renderer cache keys. Timer reads the latest cache.
    cached: [String; 2],
    submitted: [bool; 3], // SignIn, Register2, ForgotPassword
    signin_email_error: bool,
    signin_password_error: bool,
    email_error_text: Option<(&'static str, &'static str)>,
    password_error_text: Option<(&'static str, &'static str)>,
}

fn input_view(view: AccountView) -> Option<usize> {
    match view {
        AccountView::SignIn => Some(0),
        AccountView::Register2 => Some(1),
        AccountView::ForgotPassword => Some(2),
        _ => None,
    }
}

fn native_string(value: &str) -> String {
    // All four original callbacks use UTF8String then strlen.
    value.split('\0').next().unwrap_or("").to_owned()
}

fn email_message(message: u32) -> (&'static str, &'static str) {
    match message {
        2 => (
            "rovio_id_email_taken",
            "This email address has already been registered",
        ),
        3 => ("rovio_id_wrong_email", "Email address not found"),
        _ => (
            "rovio_id_validate_email_invalid",
            "Please enter a valid email address",
        ),
    }
}

impl AccountUi {
    pub(super) fn retain_password_error_text(&mut self) {
        if self.password_error.is_some() {
            // UIKit hides this label on editing; it does not erase its text.
            self.validation.password_error_text = self.password_error;
        }
    }

    pub(super) fn reset_validation_view(&mut self) {
        // A newly instantiated nib gets its own default password popup. The
        // SignIn retained flags and wrapper-wide email errorText are separate.
        self.validation.password_error_text = None;
    }

    pub(crate) fn set_validation_clock(&mut self, now: Duration) {
        self.validation.now = self.validation.now.max(now);
    }

    pub(super) fn edit_validation_field(&mut self) {
        let Some(view) = self.snapshot.as_ref().map(|s| s.view) else {
            return;
        };
        let Some(index) = input_view(view) else {
            return;
        };
        let Some(field) = self.focus else { return };
        let slot = usize::from(field == Field::Password);
        let text = if slot == 0 {
            self.email_editor().display(false).0
        } else {
            self.password.display(false).0
        };
        self.validation.cached[slot] = native_string(&text);
        self.validation.generation[slot] = self.validation.generation[slot].wrapping_add(1);
        self.validation.deadlines[slot] = self.validation.now.checked_add(DELAY);
        if slot == 0 {
            self.validation.submitted = [false; 3];
            if index == 0 {
                self.validation.signin_email_error = false;
            }
        } else {
            self.validation.submitted[index] = false;
            if index == 0 {
                self.validation.signin_password_error = false;
            }
        }
    }

    pub(super) fn restore_native_validation_text(&mut self, view: AccountView) {
        if matches!(view, AccountView::SignIn | AccountView::Register2) {
            self.email = Editor::default();
            self.email.replace(&self.validation.cached[0]);
            self.password = Editor::default();
            self.password.replace(&self.validation.cached[1]);
            if view == AccountView::SignIn {
                if self.validation.signin_email_error {
                    self.email_error = self.validation.email_error_text;
                }
                if self.validation.signin_password_error {
                    self.password_error = Some(if self.validation.cached[1].len() <= 7 {
                        PASSWORD_HELP
                    } else {
                        ("rovio_id_wrong_password", "Wrong password")
                    });
                }
            }
        }
    }

    pub(super) fn remember_native_field_error(&mut self, field: u32) {
        if matches!(field, 15 | 16 | 18) {
            self.validation.email_error_text = self.email_error;
        }
        if !self.busy()
            && self
                .snapshot
                .as_ref()
                .is_some_and(|s| s.view == AccountView::SignIn)
        {
            match field {
                18 => self.validation.signin_email_error = true,
                19 => self.validation.signin_password_error = true,
                _ => {}
            }
        }
    }

    pub(super) fn validation_submission(&mut self, view: AccountView) {
        if let Some(index) = input_view(view) {
            self.validation.submitted[index] = true;
        }
        // Submit does not invalidate either scheduled timer. Required SignIn
        // checks happen before copying; Register2 copies even empty fields.
        match view {
            AccountView::Register2 => {
                self.validation.cached = [
                    native_string(self.email.text()),
                    native_string(self.password.text()),
                ];
            }
            AccountView::SignIn
                if !self.email.text().is_empty() && !self.password.text().is_empty() =>
            {
                self.validation.cached = [
                    native_string(self.email.text()),
                    native_string(self.password.text()),
                ];
            }
            AccountView::ForgotPassword => self.validation.cached[0].clear(),
            _ => {}
        }
    }

    pub(super) fn decorate_submission_errors(&mut self) {
        if self.email_popup && self.email_error.is_some() {
            self.email_border = true;
        }
        if self.password_popup && self.password_error.is_some() {
            self.password_border = true;
            if self
                .snapshot
                .as_ref()
                .is_some_and(|s| s.view == AccountView::Register2)
                && self.password.text().is_empty()
                && self.email_error.is_some()
            {
                self.email_border = true;
            }
        }
    }

    pub(super) fn invalidate_submitted_email_timers(&mut self) {
        let Some(index) = self.snapshot.as_ref().and_then(|s| input_view(s.view)) else {
            return;
        };
        if self.validation.submitted[index] {
            self.validation.deadlines[0] = None;
            if index != 2 {
                self.validation.deadlines[1] = None;
            }
            self.validation.submitted[index] = false;
        }
    }

    pub(crate) fn advance_validation(
        &mut self,
        runtime: &StellaLua,
    ) -> Result<(), stella_script::ScriptError> {
        for result in runtime.take_account_validation_results() {
            self.apply_validation_result(result);
        }
        let Some(id) = self.owner_id() else {
            return Ok(());
        };
        for (slot, field) in [
            AccountValidationField::Email,
            AccountValidationField::Password,
        ]
        .into_iter()
        .enumerate()
        {
            if self.validation.deadlines[slot].is_some_and(|at| at <= self.validation.now) {
                self.validation.deadlines[slot] = None;
                if !self.validation.cached[slot].is_empty() {
                    runtime.validate_account_field(
                        id,
                        field,
                        self.validation.generation[slot],
                        &self.validation.cached[slot],
                    )?;
                }
            }
        }
        // The password callback is synchronous and local in the original.
        for result in runtime.take_account_validation_results() {
            self.apply_validation_result(result);
        }
        Ok(())
    }

    fn apply_validation_result(&mut self, result: AccountValidationResult) {
        if self.owner_id() != Some(result.id) {
            return;
        }
        // C++ copies each in-flight email request, but does not cancel it on
        // another edit. Preserve same-owner arrival order, not a newer-text
        // filter. The valid boolean is an iOS provider no-op, not a reset.
        let Some(error) = result.error else { return };
        if matches!(error.field, 15 | 16 | 18) {
            // 1007729A8 stores errorText before any UIView class check, even
            // when progress/help cannot show the field error itself.
            self.validation.email_error_text = Some(email_message(error.message));
        }
        // Progress preserves the C++ controller's logical view, but replaces
        // its UIKit form. Its provider cannot paint errors into the old form
        // or consume that form's submitted flags/timers. Workers still run.
        if self.busy() {
            return;
        }
        let Some(index) = self.snapshot.as_ref().and_then(|s| input_view(s.view)) else {
            return;
        };
        let submitted = self.validation.submitted[index];
        match error.field {
            15 | 16 | 18 => {
                self.email_error = self.validation.email_error_text;
                if index == 0 {
                    self.validation.signin_email_error = true;
                }
                if submitted {
                    self.email_popup = true;
                    self.email_border = true;
                    self.invalidate_submitted_email_timers();
                }
            }
            17 | 19 if index != 2 => {
                self.password_help = false;
                if index == 0 {
                    self.validation.signin_password_error = true;
                }
                if !submitted {
                    self.password_error = Some(PASSWORD_HELP);
                } else if index == 0 {
                    self.password_error = Some(("rovio_id_wrong_password", "Wrong password"));
                    self.password_popup = true;
                    self.password_border = true;
                    self.validation.submitted[index] = false;
                } else if index == 1 {
                    // 100773044 submitted Register2 branch only installs red
                    // backgrounds, retaining prior local error text/visibility.
                    // awakeFromNib 10076E510 initializes this separate popup
                    // to required, while editing only hides its existing text.
                    self.password_error.get_or_insert(
                        self.validation.password_error_text.unwrap_or((
                            "rovio_id_validate_password_required",
                            "Please enter a password",
                        )),
                    );
                    self.password_border = true;
                    self.validation.submitted[index] = false;
                }
            }
            _ => return,
        }
        self.dirty();
    }
}

#[cfg(test)]
mod tests;
