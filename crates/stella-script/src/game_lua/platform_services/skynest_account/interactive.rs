//! SkynestLoginUI owner, control transitions and credential-login completions.
//! UIKit presentation belongs to the host; this is not a guest-login fallback.

use super::*;

mod password_reset;
mod registration;
mod validation;

#[derive(Default)]
pub(super) struct InteractiveState {
    next_id: u64,
    ui: Option<AccountUiSnapshot>,
    initial_view: Option<AccountView>,
    login_request: Option<u64>,
    next_reset_request_id: u64,
    reset_request: Option<u64>,
    registration: registration::RegistrationState,
    validation_results: VecDeque<AccountValidationResult>,
}

pub(super) enum InteractiveCompletion {
    EmailValidation {
        id: u64,
        generation: u64,
        result: Result<u32, LoginError>,
    },
    Cancelled,
    CancelFailure,
    Tokens {
        login_job: u64,
        config: IdentityConfig,
        access: AccessResponse,
    },
    LoginReady {
        login_job: u64,
        owner: session::OwnProfileOwner,
        profile: ProfileResponse,
    },
    ProfileFailure {
        login_job: u64,
        owner: session::OwnProfileOwner,
    },
    PasswordReset {
        id: u64,
        request_id: u64,
        view: AccountView,
    },
    Registration {
        id: u64,
        request_id: u64,
        install_owner: RequestOwner,
        result: Result<(IdentityConfig, AccessResponse), LoginError>,
    },
}

pub(super) enum OnlineResult {
    EmailValidation {
        id: u64,
        generation: u64,
        result: Result<u32, LoginError>,
    },
    Tokens {
        id: u64,
        request_id: u64,
        config: IdentityConfig,
        result: Result<AccessResponse, LoginError>,
    },
    Profile {
        login_job: u64,
        owner: session::OwnProfileOwner,
        result: Result<ProfileResponse, String>,
    },
    PasswordReset {
        id: u64,
        request_id: u64,
        result: Result<(), LoginError>,
    },
    Registration {
        id: u64,
        request_id: u64,
        install_owner: RequestOwner,
        config: IdentityConfig,
        result: Result<AccessResponse, LoginError>,
    },
}

/// Native CloudServiceException status. Do not retain/log server bodies from
/// credential requests: a misconfigured service may echo submitted secrets.
pub(super) struct LoginError {
    status: i32,
}

impl From<session::SessionError> for LoginError {
    fn from(error: session::SessionError) -> Self {
        Self {
            status: error.status,
        }
    }
}

impl SkynestAccountRuntime {
    pub(super) fn clear_interactive_owner(&self) {
        let mut state = self.interactive.borrow_mut();
        // Keep owner ids monotonic across logout: old host input must not hit
        // a newly opened dialog which happens to reuse the same numeric id.
        state.ui = None;
        state.initial_view = None;
        state.login_request = None;
        state.reset_request = None;
        state.validation_results.clear();
        // Native age gate byte_100C0DA80 belongs to the runtime, not the
        // account session. Keep it sticky when discarding only this owner.
        state.registration.replace_owner();
    }

    pub(super) fn begin_interactive(&self, register: bool) -> LuaResult<()> {
        self.begin_login_job()?;
        let mut state = self.interactive.borrow_mut();
        state.next_id = state.next_id.wrapping_add(1).max(1);
        state.login_request = None;
        state.reset_request = None;
        state.registration.replace_owner();
        state.validation_results.clear();
        let view = if register {
            AccountView::Register1
        } else {
            AccountView::SignIn
        };
        // 1000A3E04 inverts register for 10074736C; the latter normalizes
        // state 1 to Register1 (2). Shipped registration gate is initialized 1.
        state.initial_view = Some(view);
        state.ui = Some(AccountUiSnapshot {
            id: state.next_id,
            view: state.registration.visible_view(view),
            busy: false,
            field_error: None,
        });
        Ok(())
    }

    pub(super) fn begin_unavailable_social(&self) -> LuaResult<()> {
        // The Facebook method is independent of both email UI and guest
        // access. Until its platform provider exists, never report a guest
        // account as a successful social login.
        let login_job = self.begin_login_job()?;
        self.queue_local_owned(
            self.session.request_owner(ProviderLevel::Level2),
            Completion::LoginUnavailable { login_job },
        );
        Ok(())
    }

    pub(crate) fn ui_snapshot(&self) -> Option<AccountUiSnapshot> {
        self.interactive.borrow().ui.clone()
    }

    pub(crate) fn ui_action(&self, id: u64, action: AccountUiAction) -> LuaResult<bool> {
        if action == AccountUiAction::Continue && self.confirm_registration(id) {
            return Ok(true);
        }
        let mut state = self.interactive.borrow_mut();
        let initial = state.initial_view.unwrap_or(AccountView::SignIn);
        let Some(ui) = state.ui.as_mut().filter(|ui| ui.id == id) else {
            return Ok(false);
        };
        if action == AccountUiAction::Cancel
            || (action == AccountUiAction::Continue && ui.view == AccountView::RegistrationFailure)
        {
            // 1007581EC hides synchronously. The retained callback remains
            // queued even if a new dialog replaces this owner before delivery.
            state.ui = None;
            state.login_request = None;
            state.reset_request = None;
            state.registration.replace_owner();
            state.validation_results.clear();
            drop(state);
            self.queue_local(Completion::interactive(InteractiveCompletion::Cancelled));
            return Ok(true);
        }
        let next = match action {
            AccountUiAction::Back => match ui.view {
                AccountView::SignIn | AccountView::Register1 => Some(AccountView::Help1),
                AccountView::Register2 | AccountView::RegistrationFailure => {
                    Some(AccountView::Register1)
                }
                AccountView::ForgotPassword | AccountView::NoNetworkConnectivity => Some(initial),
                _ => None,
            },
            AccountUiAction::Continue => match ui.view {
                AccountView::ForgotPassword
                | AccountView::PasswordResetEmailSent
                | AccountView::Help3
                | AccountView::NoNetworkConnectivity
                | AccountView::AccountNotVerified => Some(initial),
                AccountView::Help1 => Some(AccountView::Help2),
                AccountView::Help2 => Some(AccountView::Help3),
                // Register1/2 have separate field-bearing controller methods.
                // This button must not invent an accepted registration.
                _ => None,
            },
            AccountUiAction::Register => Some(AccountView::Register1),
            AccountUiAction::ForgotPassword => Some(AccountView::ForgotPassword),
            AccountUiAction::Cancel => unreachable!(),
        };
        if let Some(next) = next {
            ui.view = next;
            ui.busy = false;
            ui.field_error = None;
            state.login_request = None;
            if matches!(next, AccountView::SignIn | AccountView::Register1) {
                state.initial_view = Some(next);
            }
            state.reset_request = None;
            state.registration.invalidate_request();
            let next = state.registration.visible_view(next);
            state.ui.as_mut().expect("active owner").view = next;
        }
        Ok(true)
    }

    pub(crate) fn submit_login(&self, id: u64, email: &str, password: &str) -> LuaResult<bool> {
        let config = self.prepared_online_config()?;
        let request_id = {
            let mut state = self.interactive.borrow_mut();
            let Some(ui) = state
                .ui
                .as_mut()
                .filter(|ui| ui.id == id && ui.view == AccountView::SignIn && !ui.busy)
            else {
                return Ok(false);
            };
            ui.field_error = None;
            if config.is_none() {
                // The desktop has no implicit retired endpoint. Same visible
                // error state as the native request's transport status -1.
                ui.view = AccountView::NoNetworkConnectivity;
                return Ok(true);
            }
            ui.busy = true;
            let request_id = self.begin_login_job()?;
            state.login_request = Some(request_id);
            request_id
        };
        let config = config.expect("checked above");
        let session = self.session.clone();
        let owner = session.request_owner(ProviderLevel::Level2);
        let email = email.to_owned();
        let password = password.to_owned();
        let result = spawn_online(
            Arc::clone(&self.online_completions),
            self.application_events.clone(),
            ApplicationEvent::SkynestAccountOnline,
            owner,
            move |owner| {
                let result = request_login(&config, &session, owner, &email, &password);
                OnlineCompletion::Interactive(OnlineResult::Tokens {
                    id,
                    request_id,
                    config,
                    result,
                })
            },
        );
        if result.is_err()
            && let Some(ui) = self
                .interactive
                .borrow_mut()
                .ui
                .as_mut()
                .filter(|ui| ui.id == id)
        {
            ui.busy = false;
        }
        if result.is_err() {
            self.finish_stale_login_job(request_id);
        }
        result.map(|_| true)
    }

    pub(super) fn dispatch_interactive(
        &self,
        lua: &Lua,
        request_owner: RequestOwner,
        completion: InteractiveCompletion,
    ) -> LuaResult<()> {
        match completion {
            InteractiveCompletion::EmailValidation {
                id,
                generation,
                result,
            } => {
                self.finish_email_validation(id, generation, result);
            }
            InteractiveCompletion::Cancelled => self.queue_local_owned(
                request_owner,
                Completion::interactive(InteractiveCompletion::CancelFailure),
            ),
            InteractiveCompletion::CancelFailure => notify_account_failure(
                lua,
                &self.state,
                "ERROR_USER_CANCELLED_LOGIN",
                "User cancelled login",
            )?,
            InteractiveCompletion::Tokens {
                login_job,
                config,
                access,
            } => {
                if self.active_login_job.get() != Some(login_job) {
                    return Ok(());
                }
                // 10075E52C hides first; retain the returned credentials for
                // direct own-profile GET before installing them (10074F650/67C).
                let session = self.session.clone();
                let Some(mut owner) = session.own_profile_owner_for_request(request_owner) else {
                    self.finish_stale_login_job(login_job);
                    return Ok(());
                };
                let queue = Arc::clone(&self.online_completions);
                spawn_online(
                    queue,
                    self.application_events.clone(),
                    ApplicationEvent::SkynestAccountOnline,
                    request_owner.epoch_only(),
                    move |_| {
                        let result = request_login_profile(&config, &session, &mut owner, access);
                        OnlineCompletion::Interactive(OnlineResult::Profile {
                            login_job,
                            owner,
                            result,
                        })
                    },
                )?;
            }
            InteractiveCompletion::LoginReady {
                login_job,
                owner,
                profile: _profile,
            } => {
                if self.session.own_profile_owner_is_current(owner)
                    && self.complete_login_job(login_job)
                {
                    notify_login_success(lua, &self.state)?;
                } else {
                    self.finish_stale_login_job(login_job);
                }
            }
            InteractiveCompletion::ProfileFailure { login_job, owner } => {
                if self.session.own_profile_owner_is_current(owner)
                    && self.complete_login_job(login_job)
                {
                    notify_login_failure(lua, &self.state, "Unable to retrieve account profile")?;
                } else {
                    self.finish_stale_login_job(login_job);
                }
            }
            InteractiveCompletion::PasswordReset {
                id,
                request_id,
                view,
            } => self.finish_password_reset(id, request_id, view),
            InteractiveCompletion::Registration {
                id,
                request_id,
                install_owner,
                result,
            } => self.finish_registration(id, request_id, install_owner, result),
        }
        Ok(())
    }

    pub(super) fn dispatch_interactive_online(
        &self,
        _lua: &Lua,
        request_owner: RequestOwner,
        result: OnlineResult,
    ) -> LuaResult<()> {
        match result {
            OnlineResult::EmailValidation {
                id,
                generation,
                result,
            } => {
                self.dispatch_email_validation(request_owner, id, generation, result);
            }
            OnlineResult::Tokens {
                id,
                request_id,
                config,
                result,
            } => {
                let mut state = self.interactive.borrow_mut();
                if state.login_request != Some(request_id) {
                    return Ok(());
                }
                state.login_request = None;
                let initial = state.initial_view.unwrap_or(AccountView::SignIn);
                let Some(ui) = state.ui.as_mut().filter(|ui| ui.id == id) else {
                    return Ok(());
                };
                ui.busy = false;
                match result {
                    Ok(access) => {
                        state.ui = None;
                        drop(state);
                        self.queue_local_owned(
                            request_owner,
                            Completion::interactive(InteractiveCompletion::Tokens {
                                login_job: request_id,
                                config,
                                access,
                            }),
                        );
                    }
                    Err(error) => {
                        // 10075DB90: these errors stay inside LoginUIProvider;
                        // they do not complete the game's login callback.
                        match error.status {
                            -1 => ui.view = AccountView::NoNetworkConnectivity,
                            412 => ui.view = AccountView::AccountNotVerified,
                            404 => {
                                ui.view = AccountView::SignIn;
                                ui.field_error = Some(AccountFieldError {
                                    field: 18,
                                    message: 3,
                                });
                                state.initial_view = Some(AccountView::SignIn);
                            }
                            _ => {
                                ui.view = initial;
                                ui.field_error = Some(AccountFieldError {
                                    field: 19,
                                    message: 6,
                                });
                            }
                        }
                    }
                }
            }
            OnlineResult::Profile {
                login_job,
                owner,
                result,
            } => {
                if !self.session.own_profile_owner_is_current(owner) {
                    self.finish_stale_login_job(login_job);
                    return Ok(());
                }
                self.queue_local_owned(
                    // The payload's OwnProfileOwner includes this worker's
                    // successful profile publication; retain its own check.
                    request_owner,
                    Completion::interactive(match result {
                        Ok(profile) => InteractiveCompletion::LoginReady {
                            login_job,
                            owner,
                            profile,
                        },
                        Err(_) => InteractiveCompletion::ProfileFailure { login_job, owner },
                    }),
                )
            }
            OnlineResult::PasswordReset {
                id,
                request_id,
                result,
            } => self.dispatch_password_reset(request_owner, id, request_id, result),
            OnlineResult::Registration {
                id,
                request_id,
                install_owner,
                config,
                result,
            } => self.dispatch_registration(
                request_owner,
                id,
                request_id,
                install_owner,
                config,
                result,
            ),
        }
        Ok(())
    }

    fn cancel_stale_login_submission(&self, id: u64, request_id: u64) {
        let mut state = self.interactive.borrow_mut();
        if state.login_request == Some(request_id)
            && let Some(ui) = state.ui.as_mut().filter(|ui| ui.id == id)
        {
            ui.busy = false;
            state.login_request = None;
        }
        drop(state);
        self.finish_stale_login_job(request_id);
    }

    pub(super) fn discard_stale_interactive_online(&self, result: OnlineResult) {
        match result {
            OnlineResult::Tokens { id, request_id, .. } => {
                self.cancel_stale_login_submission(id, request_id)
            }
            OnlineResult::Profile { login_job, .. } => self.finish_stale_login_job(login_job),
            OnlineResult::Registration { id, request_id, .. } => {
                self.cancel_stale_registration(id, request_id)
            }
            // Level1 text/reset results retain native owner/request filtering.
            OnlineResult::EmailValidation { .. } | OnlineResult::PasswordReset { .. } => {}
        }
    }

    pub(super) fn discard_stale_interactive(&self, completion: InteractiveCompletion) {
        match completion {
            InteractiveCompletion::Tokens { login_job, .. }
            | InteractiveCompletion::LoginReady { login_job, .. }
            | InteractiveCompletion::ProfileFailure { login_job, .. } => {
                self.finish_stale_login_job(login_job)
            }
            InteractiveCompletion::Registration { id, request_id, .. } => {
                self.cancel_stale_registration(id, request_id)
            }
            InteractiveCompletion::EmailValidation { .. }
            | InteractiveCompletion::PasswordReset { .. }
            | InteractiveCompletion::Cancelled
            | InteractiveCompletion::CancelFailure => {}
        }
    }
}

fn request_login(
    config: &IdentityConfig,
    session: &IdentitySession,
    owner: &mut RequestOwner,
    email: &str,
    password: &str,
) -> Result<AccessResponse, LoginError> {
    // 100722C64 uses only email/password form fields. 10066E978 adds both
    // provider-scoped headers, not client secrets in the credential body.
    let response = session
        .execute_form_for_owner(
            config,
            owner,
            ProviderLevel::Level2,
            "abid/login",
            &form_body(&[
                ("email", email.to_owned()),
                ("password", password.to_owned()),
            ]),
        )
        .map_err(LoginError::from)?;
    session::parse_access_response(response).map_err(LoginError::from)
}

fn request_login_profile(
    config: &IdentityConfig,
    session: &IdentitySession,
    owner: &mut session::OwnProfileOwner,
    access: AccessResponse,
) -> Result<ProfileResponse, String> {
    let before = session
        .login_profile_identity(*owner)
        .ok_or_else(|| "identity request cancelled".to_owned())?;
    // Separate native direct GET (100672054), not common auth/401 replay.
    let response = agent()
        .get(config.endpoint.request_url("profile/own"))
        .header("X-Access-Token", &access.access_token)
        .call()
        .map_err(|_| "identity profile transport error".to_owned())?;
    let profile = session::parse_profile_response(response).map_err(|error| error.to_string())?;
    let prepared = session
        .prepare_login_profile(owner, &profile, before)
        .map_err(|error| error.to_string())?
        .ok_or_else(|| "identity request cancelled".to_owned())?;
    session
        .fetch_avatar_assets(*owner, &profile.avatar_assets)
        .inspect_err(|error| eprintln!("identity personal avatar fetch failed: {error}"))?;
    if !session
        .finish_login_profile(owner, &access, &config.identifiers, prepared)
        .map_err(|error| error.to_string())?
    {
        return Err("identity request cancelled".to_owned());
    };
    Ok(profile)
}

#[cfg(test)]
mod tests;
