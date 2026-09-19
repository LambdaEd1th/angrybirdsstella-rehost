//! Recovered registration controls; wheel/arrow picker input is desktop glue.

use super::*;
use stella_script::{AccountGender, RegistrationBirthday};

#[derive(Default)]
pub(crate) struct Registration {
    pub values: [Option<i32>; 3],
    pub today: Option<RegistrationBirthday>,
    pub picker: Option<u8>,
    pub row: i32,
    pub female: bool,
    pub date_errors: [bool; 3],
}

impl Registration {
    pub fn row_count(&self, tag: u8) -> i32 {
        super::super::layout::registration::picker_row_count(
            tag,
            self.today.map_or(1900, |date| date.year),
        ) as i32
    }

    fn open(&mut self, tag: u8) {
        let today = self.today.map_or(1900, |date| match tag {
            0 => date.day as i32,
            1 => date.month as i32,
            _ => date.year,
        });
        let value = self.values[usize::from(tag)].unwrap_or(today);
        self.row =
            (value - if tag == 2 { 1900 } else { 1 }).clamp(0, (self.row_count(tag) - 1).max(0));
        self.picker = Some(tag);
        self.date_errors[usize::from(tag)] = false;
    }

    fn commit(&mut self) {
        if let Some(tag) = self.picker {
            self.values[usize::from(tag)] = Some(self.row + if tag == 2 { 1900 } else { 1 });
        }
    }

    fn birthday(&self) -> RegistrationBirthday {
        RegistrationBirthday {
            day: self.values[0].unwrap_or(0).max(0) as u32,
            month: self.values[1].unwrap_or(0).max(0) as u32,
            year: self.values[2].unwrap_or(0),
        }
    }
}

impl AccountUi {
    pub(crate) fn set_calendar_today(&mut self, today: RegistrationBirthday) {
        self.registration.today = Some(today);
    }

    pub(crate) fn scroll_picker(&mut self, rows: i32) {
        if self.busy() || rows == 0 {
            return;
        }
        if let Some(tag) = self.registration.picker {
            self.registration.row = self
                .registration
                .row
                .saturating_add(rows)
                .clamp(0, (self.registration.row_count(tag) - 1).max(0));
            // didSelectRow writes while visible, but does not dismiss it.
            self.registration.commit();
            self.dirty();
        }
    }

    pub(super) fn registration_key(&mut self, key: &Key) -> bool {
        if self.registration.picker.is_none() {
            return false;
        }
        match key {
            Key::Named(NamedKey::ArrowUp) => self.scroll_picker(-1),
            Key::Named(NamedKey::ArrowDown) => self.scroll_picker(1),
            Key::Named(NamedKey::Home) => self.scroll_picker(-i32::MAX),
            Key::Named(NamedKey::End) => self.scroll_picker(i32::MAX),
            Key::Named(NamedKey::Enter) => {
                self.registration.commit();
                self.registration.picker = None;
                self.dirty();
            }
            _ => {}
        }
        true
    }

    /// Outer Option indicates this registration control consumed the tap.
    pub(super) fn registration_release(&mut self, name: Option<&str>) -> Option<Option<Command>> {
        let view = self.snapshot.as_ref()?.view;
        if self.busy() {
            return None;
        }
        if view == AccountView::Register1 {
            let tag = match name {
                Some("dayTextField") => Some(0),
                Some("monthTextField") => Some(1),
                Some("yearTextField") => Some(2),
                _ => None,
            };
            if let Some(tag) = tag {
                if self.registration.picker == Some(tag) {
                    self.registration.commit();
                    self.registration.picker = None;
                } else {
                    self.registration.open(tag);
                }
                return Some(None);
            }
            if self.registration.picker.is_some() {
                let offset = match name {
                    Some("pickerRowMinus2") => Some(-2),
                    Some("pickerRowMinus1") => Some(-1),
                    Some("pickerRow0") => Some(0),
                    Some("pickerRowPlus1") => Some(1),
                    Some("pickerRowPlus2") => Some(2),
                    _ => None,
                };
                if let Some(offset) = offset {
                    self.scroll_picker(offset);
                    self.registration.commit();
                }
                self.registration.picker = None;
                if name == Some("continueButton") {
                    self.registration.date_errors =
                        self.registration.values.map(|value| value.is_none());
                    return Some(None);
                }
                // 10076C384: first tap on a link with a picker open only hides it.
                if matches!(name, Some("eulaLabel" | "privacyPolicyLabel")) || offset.is_some() {
                    return Some(None);
                }
            }
            return match name {
                Some("backButton") => Some(Some(Command::Action(AccountUiAction::Cancel))),
                Some("continueButton") => {
                    self.registration.date_errors =
                        self.registration.values.map(|value| value.is_none());
                    Some(
                        (!self.registration.date_errors.contains(&true)).then_some(Command::Submit),
                    )
                }
                Some("eulaLabel") => Some(Some(Command::OpenUrl(
                    super::super::layout::registration::TERMS_URL,
                ))),
                Some("privacyPolicyLabel") => Some(Some(Command::OpenUrl(
                    super::super::layout::registration::PRIVACY_URL,
                ))),
                // The title shares SignIn's link outlet name but is not a link.
                Some("registerLabel") => Some(None),
                _ => None,
            };
        }
        if view == AccountView::Register2 {
            match name {
                Some("gender_male_button") => self.registration.female = false,
                Some("gender_female_button") => self.registration.female = true,
                Some("registerButton") => return Some(Some(Command::Submit)),
                _ => return None,
            }
            return Some(None);
        }
        None
    }

    pub(super) fn submit_registration(
        &mut self,
        runtime: &StellaLua,
        snapshot: &AccountUiSnapshot,
    ) -> Result<(), stella_script::ScriptError> {
        if snapshot.view == AccountView::Register1 {
            self.registration.date_errors = self.registration.values.map(|value| value.is_none());
            if self.registration.picker.take().is_some()
                || self.registration.date_errors.contains(&true)
            {
                return Ok(());
            }
            runtime.submit_account_birthday(snapshot.id, self.registration.birthday())?;
            // A second invalid date can produce an identical public snapshot.
            // Submit still replays native showInvalidDayError after resetting
            // the backgrounds; do not depend on snapshot-delta notification.
            if runtime
                .account_ui()
                .is_some_and(|ui| ui.view == AccountView::Register1 && ui.field_error.is_some())
            {
                self.registration.date_errors[0] = true;
                self.registration.date_errors[1] = true;
            }
            return Ok(());
        }
        if self.email.text().is_empty() {
            self.email_popup = true;
            self.email_error = Some((
                "rovio_id_validate_email_required",
                "Please enter your email address",
            ));
        } else if self.password.text().len() < 8 {
            self.password_popup = true;
            self.password_help = false;
            self.password_error = Some(if self.password.text().is_empty() {
                (
                    "rovio_id_validate_password_required",
                    "Please enter a password",
                )
            } else {
                (
                    "rovio_id_password_help_text",
                    "Password must contain at least 8 characters",
                )
            });
        } else {
            // Native wrapper counts UTF8 bytes, not Unicode codepoints; it
            // doesn't trim, require a checkbox, or block on an old email icon.
            runtime.submit_account_registration(
                snapshot.id,
                self.email.text(),
                self.password.text(),
                if self.registration.female {
                    AccountGender::Female
                } else {
                    AccountGender::Male
                },
            )?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests;
