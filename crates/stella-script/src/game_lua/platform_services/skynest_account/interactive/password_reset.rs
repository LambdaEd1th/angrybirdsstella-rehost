//! Native reset-email request and its retained UI completion, not a login.

use super::*;

impl SkynestAccountRuntime {
    pub(crate) fn submit_password_reset(&self, id: u64, email: &str) -> LuaResult<bool> {
        let request_id = {
            let mut state = self.interactive.borrow_mut();
            let Some(ui) = state
                .ui
                .as_mut()
                .filter(|ui| ui.id == id && ui.view == AccountView::ForgotPassword && !ui.busy)
            else {
                return Ok(false);
            };
            ui.field_error = None;
            // 100775858 validates empty/whitespace-only input at submission.
            // Required-vs-invalid text is view-owned. The separate two-second
            // email-validation timer must not become an invented blocking
            // request, RFC regex, or trim of the submitted address here.
            if email.trim().is_empty() {
                ui.field_error = Some(AccountFieldError {
                    field: 15,
                    message: 1,
                });
                return Ok(true);
            }
            // 100759D94 sets state 12 before starting the request worker.
            ui.busy = true;
            state.next_reset_request_id = state.next_reset_request_id.wrapping_add(1).max(1);
            let request_id = state.next_reset_request_id;
            state.reset_request = Some(request_id);
            request_id
        };
        let Some(config) = self.prepared_online_config()? else {
            // No retired endpoint or fabricated success. Retain the native
            // asynchronous transport-error continuation even without a host.
            self.queue_local(Completion::interactive(
                InteractiveCompletion::PasswordReset {
                    id,
                    request_id,
                    view: AccountView::NoNetworkConnectivity,
                },
            ));
            return Ok(true);
        };
        let session = self.session.clone();
        let owner = session.request_owner(ProviderLevel::Level1);
        let language = crate::preferred_languages::host_preferred_languages()
            .into_iter()
            .next()
            .unwrap_or_else(|| "en".to_owned());
        let locale = password_reset_locale(&language).to_owned();
        let email = email.to_owned();
        let result = spawn_online(
            Arc::clone(&self.online_completions),
            self.application_events.clone(),
            ApplicationEvent::SkynestAccountOnline,
            owner,
            move |owner| {
                OnlineCompletion::Interactive(OnlineResult::PasswordReset {
                    id,
                    request_id,
                    result: request_password_reset(&config, &session, owner, &email, &locale),
                })
            },
        );
        if result.is_err() {
            let mut state = self.interactive.borrow_mut();
            if state.reset_request == Some(request_id) {
                state.reset_request = None;
                if let Some(ui) = state.ui.as_mut().filter(|ui| ui.id == id) {
                    ui.busy = false;
                }
            }
        }
        result.map(|_| true)
    }

    pub(super) fn dispatch_password_reset(
        &self,
        owner: RequestOwner,
        id: u64,
        request_id: u64,
        result: Result<(), LoginError>,
    ) {
        let mut state = self.interactive.borrow_mut();
        if state.reset_request != Some(request_id) {
            return;
        }
        let Some(ui) = state
            .ui
            .as_mut()
            .filter(|ui| ui.id == id && ui.view == AccountView::ForgotPassword && ui.busy)
        else {
            return;
        };
        let view = match result {
            Ok(()) => AccountView::PasswordResetEmailSent,
            Err(LoginError { status: -1 }) => AccountView::NoNetworkConnectivity,
            Err(_) => {
                // 10075B534..10075B560: every non-transport exception returns
                // to ForgotPassword (7), then sets field/message 15/1.
                ui.busy = false;
                ui.field_error = Some(AccountFieldError {
                    field: 15,
                    message: 1,
                });
                state.reset_request = None;
                return;
            }
        };
        drop(state);
        // 10075B3B4 and 10075B5C4 post success/network continuations separately;
        // 10075C0B0 selects state 8 and 10075BC70 selects network state 13.
        self.queue_local_owned(
            owner,
            Completion::interactive(InteractiveCompletion::PasswordReset {
                id,
                request_id,
                view,
            }),
        );
    }

    pub(super) fn finish_password_reset(&self, id: u64, request_id: u64, view: AccountView) {
        let mut state = self.interactive.borrow_mut();
        if state.reset_request != Some(request_id) {
            return;
        }
        let Some(ui) = state
            .ui
            .as_mut()
            .filter(|ui| ui.id == id && ui.view == AccountView::ForgotPassword && ui.busy)
        else {
            return;
        };
        ui.view = view;
        ui.busy = false;
        ui.field_error = None;
        state.reset_request = None;
    }
}

pub(super) fn password_reset_locale(language: &str) -> &str {
    // 100771104 uses the provider's override or first preferred OS language;
    // these are its exact substitutions, not generic locale normalization.
    match language {
        "pt" => "pt_BR",
        "zh-Hans" => "zh_CN",
        "zh-Hant" => "zh_TW",
        other => other,
    }
}

fn request_password_reset(
    config: &IdentityConfig,
    session: &IdentitySession,
    owner: &mut RequestOwner,
    email: &str,
    locale: &str,
) -> Result<(), LoginError> {
    // 10072E634 uses email + locale form fields and shared provider headers.
    // The operation is deliberately NOT one of the four identity/3.0 routes.
    let mut response = session
        .execute_form_for_owner(
            config,
            owner,
            ProviderLevel::Level1,
            "abid/reset/password",
            &form_body(&[("email", email.to_owned()), ("locale", locale.to_owned())]),
        )
        .map_err(LoginError::from)?;
    let status = response.status().as_u16();
    // Native consumes the response, but parses no JSON (including empty 204).
    // Discarding it also avoids retaining a service's echoed address/secrets.
    std::io::copy(&mut response.body_mut().as_reader(), &mut std::io::sink())
        .map_err(|_| LoginError { status: -1 })?;
    if (200..300).contains(&status) {
        Ok(())
    } else {
        Err(LoginError {
            status: i32::from(status),
        })
    }
}

#[cfg(test)]
mod tests;
