//! Native birthday gate, registration worker, and retained confirmation tokens.

use super::*;
use mlua::{Function, Table};

#[derive(Default)]
pub(super) struct RegistrationState {
    // Native byte_100C0DA80 starts at 1 and can only become 0. Keep this for
    // the lifetime of the runtime, not the lifetime of an individual dialog.
    blocked: bool,
    birthday: Option<RegistrationBirthday>,
    request: Option<u64>,
    // The confirmation page can outlive another credential installation.
    // Keep the original installation authority, not merely the UI id.
    accepted: Option<(u64, RequestOwner, IdentityConfig, AccessResponse)>,
}

impl RegistrationState {
    pub(super) fn invalidate_request(&mut self) {
        self.request = None;
        self.accepted = None;
    }

    pub(super) fn replace_owner(&mut self) {
        self.invalidate_request();
        self.birthday = None;
    }

    pub(super) fn visible_view(&self, view: AccountView) -> AccountView {
        // 1007584CC normalizes state 1/2 to failure once the age gate closes.
        if self.blocked && view == AccountView::Register1 {
            AccountView::RegistrationFailure
        } else {
            view
        }
    }
}

impl SkynestAccountRuntime {
    pub(crate) fn calendar_today(&self) -> LuaResult<RegistrationBirthday> {
        // Lua's stock OS library uses the platform localtime implementation.
        // A fresh minimal VM avoids trusting shipped/replaced os.date globals,
        // adds no date dependency, and has no game storage or network provider.
        let calendar = Lua::new_with(mlua::StdLib::OS, mlua::LuaOptions::default())?;
        let date: Function = calendar.globals().get::<Table>("os")?.get("date")?;
        let today: Table = date.call("*t")?;
        Ok(RegistrationBirthday {
            day: today.get("day")?,
            month: today.get("month")?,
            year: today.get("year")?,
        })
    }

    pub(crate) fn submit_birthday(
        &self,
        id: u64,
        birthday: RegistrationBirthday,
    ) -> LuaResult<bool> {
        Ok(self.submit_birthday_on(id, birthday, self.calendar_today()?))
    }

    fn submit_birthday_on(
        &self,
        id: u64,
        birthday: RegistrationBirthday,
        today: RegistrationBirthday,
    ) -> bool {
        let mut state = self.interactive.borrow_mut();
        let Some(ui) = state
            .ui
            .as_mut()
            .filter(|ui| ui.id == id && ui.view == AccountView::Register1 && !ui.busy)
        else {
            return false;
        };
        ui.field_error = birthday_error(birthday);
        if ui.field_error.is_some() {
            return true;
        }
        // 100758968: invalid dates do not change the sticky gate; a valid
        // date younger than thirteen permanently closes it for this runtime.
        state.registration.blocked |= !at_least_thirteen(birthday, today);
        state.registration.birthday = Some(birthday);
        let next = if state.registration.blocked {
            AccountView::RegistrationFailure
        } else {
            AccountView::Register2
        };
        state.ui.as_mut().expect("active owner").view = next;
        true
    }

    pub(crate) fn submit_registration(
        &self,
        id: u64,
        email: &str,
        password: &str,
        gender: AccountGender,
    ) -> LuaResult<bool> {
        let (request_id, birthday) = {
            let mut state = self.interactive.borrow_mut();
            let Some(birthday) = state.registration.birthday else {
                return Ok(false);
            };
            if state.registration.blocked {
                return Ok(false);
            }
            let Some(ui) = state
                .ui
                .as_mut()
                .filter(|ui| ui.id == id && ui.view == AccountView::Register2 && !ui.busy)
            else {
                return Ok(false);
            };
            // 1007743A4: raw email is required, then UTF-8 password bytes >=8.
            // No trim, RFC regex, extra password policy or automatic validator.
            ui.field_error = if email.is_empty() {
                Some(AccountFieldError {
                    field: 16,
                    message: 1,
                })
            } else if password.len() < 8 {
                Some(AccountFieldError {
                    field: 17,
                    // 10072FFBC -> 10075A0B4: empty=5, short=4 for state3.
                    message: if password.is_empty() { 5 } else { 4 },
                })
            } else {
                None
            };
            if ui.field_error.is_some() {
                return Ok(true);
            }
            ui.busy = true;
            let request = self.begin_login_job()?;
            state.registration.request = Some(request);
            (request, birthday)
        };
        let Some(config) = self.prepared_online_config()? else {
            self.queue_local(Completion::interactive(
                InteractiveCompletion::Registration {
                    id,
                    request_id,
                    install_owner: self.session.request_owner(ProviderLevel::Level2),
                    result: Err(LoginError { status: -1 }),
                },
            ));
            return Ok(true);
        };
        let session = self.session.clone();
        let install_owner = session.request_owner(ProviderLevel::Level2);
        let upgrade_guest = session.profile().is_some_and(|profile| profile.is_guest());
        // Ordinary registration uses parent Level1 HTTP, but still may not
        // install its retained Level2 tokens over a subsequently chosen user.
        let owner = if upgrade_guest {
            install_owner
        } else {
            install_owner.epoch_only()
        };
        let language = crate::preferred_languages::host_preferred_languages()
            .into_iter()
            .next()
            .unwrap_or_else(|| "en".to_owned());
        let locale = password_reset::password_reset_locale(&language).to_owned();
        let email = email.to_owned();
        let password = password.to_owned();
        let result = spawn_online(
            Arc::clone(&self.online_completions),
            self.application_events.clone(),
            ApplicationEvent::SkynestAccountOnline,
            owner,
            move |owner| {
                let result = request_registration(
                    &config,
                    &session,
                    owner,
                    &email,
                    &password,
                    birthday,
                    gender,
                    &locale,
                    upgrade_guest,
                );
                OnlineCompletion::Interactive(OnlineResult::Registration {
                    id,
                    request_id,
                    install_owner: if upgrade_guest { *owner } else { install_owner },
                    config,
                    result,
                })
            },
        );
        if result.is_err() {
            self.finish_stale_login_job(request_id);
            let mut state = self.interactive.borrow_mut();
            if state.registration.request == Some(request_id) {
                state.registration.invalidate_request();
                if let Some(ui) = state.ui.as_mut().filter(|ui| ui.id == id) {
                    ui.busy = false;
                }
            }
        }
        result.map(|_| true)
    }

    pub(super) fn dispatch_registration(
        &self,
        owner: RequestOwner,
        id: u64,
        request_id: u64,
        install_owner: RequestOwner,
        config: IdentityConfig,
        result: Result<AccessResponse, LoginError>,
    ) {
        if !self.session.request_owner_is_current(install_owner) {
            self.cancel_stale_registration(id, request_id);
            return;
        }
        let state = self.interactive.borrow();
        if state.registration.request != Some(request_id)
            || !state
                .ui
                .as_ref()
                .is_some_and(|ui| ui.id == id && ui.view == AccountView::Register2 && ui.busy)
        {
            return;
        }
        drop(state);
        // Both success (10075C230) and exception (10075C3EC) post the UI
        // continuation separately. Neither completes the game's login yet.
        self.queue_local_owned(
            owner,
            Completion::interactive(InteractiveCompletion::Registration {
                id,
                request_id,
                install_owner,
                result: result.map(|access| (config, access)),
            }),
        );
    }

    pub(super) fn finish_registration(
        &self,
        id: u64,
        request_id: u64,
        install_owner: RequestOwner,
        result: Result<(IdentityConfig, AccessResponse), LoginError>,
    ) {
        if !self.session.request_owner_is_current(install_owner) {
            self.cancel_stale_registration(id, request_id);
            return;
        }
        let mut state = self.interactive.borrow_mut();
        if state.registration.request != Some(request_id)
            || !state
                .ui
                .as_ref()
                .is_some_and(|ui| ui.id == id && ui.view == AccountView::Register2 && ui.busy)
        {
            return;
        }
        state.registration.request = None;
        let (view, error) = match result {
            Ok((config, access)) => {
                state.registration.accepted = Some((request_id, install_owner, config, access));
                (AccountView::ThanksForRegistering, None)
            }
            Err(LoginError { status: -1 }) => (AccountView::NoNetworkConnectivity, None),
            Err(LoginError { status: 400 | 412 }) => (
                AccountView::Register2,
                Some(AccountFieldError {
                    field: 16,
                    message: 1,
                }),
            ),
            Err(LoginError { status }) => {
                if status == 451 {
                    state.registration.blocked = true;
                }
                (AccountView::RegistrationFailure, None)
            }
        };
        let ui = state.ui.as_mut().expect("active owner");
        ui.view = view;
        ui.field_error = error;
        ui.busy = false;
    }

    pub(super) fn confirm_registration(&self, id: u64) -> bool {
        let mut state = self.interactive.borrow_mut();
        if !state.ui.as_ref().is_some_and(|ui| {
            ui.id == id && ui.view == AccountView::ThanksForRegistering && !ui.busy
        }) {
            return false;
        }
        let Some((login_job, owner, config, access)) = state.registration.accepted.take() else {
            return false;
        };
        // 100758324 hides now; 10075E978 delivers the retained token later.
        state.ui = None;
        state.registration.replace_owner();
        drop(state);
        if !self.session.request_owner_is_current(owner) {
            self.finish_stale_login_job(login_job);
            return true;
        }
        self.queue_local_owned(
            owner,
            Completion::interactive(InteractiveCompletion::Tokens {
                login_job,
                config,
                access,
            }),
        );
        true
    }

    pub(super) fn cancel_stale_registration(&self, id: u64, request_id: u64) {
        let mut state = self.interactive.borrow_mut();
        if state.registration.request == Some(request_id)
            && let Some(ui) = state.ui.as_mut().filter(|ui| ui.id == id)
        {
            ui.busy = false;
            state.registration.invalidate_request();
        }
        drop(state);
        self.finish_stale_login_job(request_id);
    }
}

fn birthday_error(date: RegistrationBirthday) -> Option<AccountFieldError> {
    let leap = date.year % 4 == 0 && (date.year % 100 != 0 || date.year % 400 == 0);
    let days = match date.month {
        2 => {
            if leap {
                29
            } else {
                28
            }
        }
        4 | 6 | 9 | 11 => 30,
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        _ => 0,
    };
    if date.year > 0 && date.day > 0 && date.day <= days {
        return None;
    }
    // Native emits every missing-field error (year, day, month), or day+month
    // for an impossible positive date. Snapshot currently retains the last;
    // desktop local validation is responsible for all highlighted date fields.
    let field = if date.month == 0 {
        13
    } else if date.day == 0 {
        12
    } else if date.year <= 0 {
        14
    } else {
        13
    };
    Some(AccountFieldError { field, message: 7 })
}

fn at_least_thirteen(birthday: RegistrationBirthday, today: RegistrationBirthday) -> bool {
    (i64::from(today.year), today.month, today.day)
        >= (i64::from(birthday.year) + 13, birthday.month, birthday.day)
}

#[allow(clippy::too_many_arguments)]
fn request_registration(
    config: &IdentityConfig,
    session: &IdentitySession,
    owner: &mut RequestOwner,
    email: &str,
    password: &str,
    birthday: RegistrationBirthday,
    gender: AccountGender,
    locale: &str,
    upgrade_guest: bool,
) -> Result<AccessResponse, LoginError> {
    let operation = if upgrade_guest {
        "guest/upgrade"
    } else {
        "abid/register"
    };
    let mut fields = vec![
        ("email", email.to_owned()),
        ("password", password.to_owned()),
        (
            "birthday",
            format!("{}-{}-{}", birthday.year, birthday.month, birthday.day),
        ),
        (
            "gender",
            match gender {
                AccountGender::Male => "male",
                AccountGender::Female => "female",
            }
            .to_owned(),
        ),
    ];
    if !upgrade_guest || !locale.is_empty() {
        fields.push(("locale", locale.to_owned()));
    }
    if upgrade_guest {
        // 100729804 uses id.accountUUID here, despite this field sharing its
        // name with the distinct device hash in Level1/session access.
        fields.push((
            "persistentGuid",
            config
                .identifiers
                .installation_id()
                .map_err(session::SessionError::from)?,
        ));
    }
    let level = if upgrade_guest {
        ProviderLevel::Level2
    } else {
        ProviderLevel::Level1
    };
    let response = session
        .execute_form_for_owner(config, owner, level, operation, &form_body(&fields))
        .map_err(LoginError::from)?;
    session::parse_access_response(response).map_err(LoginError::from)
}

#[cfg(test)]
mod tests;
