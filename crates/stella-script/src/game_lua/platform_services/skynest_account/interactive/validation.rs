//! Native delayed editing validation, separate from final form submission.

use super::*;

impl SkynestAccountRuntime {
    pub(crate) fn invalidate_validation(
        &self,
        id: u64,
        _field: AccountValidationField,
        _generation: u64,
    ) -> bool {
        // UIKit invalidates the NSTimer, not a worker it already started.
        // The host owns those timers. Do not silently fix the original
        // same-owner response race by discarding an older editing revision.
        self.interactive
            .borrow()
            .ui
            .as_ref()
            .is_some_and(|ui| ui.id == id)
    }

    pub(crate) fn take_validation_results(&self) -> Vec<AccountValidationResult> {
        let mut state = self.interactive.borrow_mut();
        let owner = state.ui.as_ref().map(|ui| ui.id);
        state
            .validation_results
            .drain(..)
            .filter(|result| Some(result.id) == owner)
            .collect()
    }

    pub(crate) fn validate_field(
        &self,
        id: u64,
        field: AccountValidationField,
        generation: u64,
        text: &str,
    ) -> LuaResult<bool> {
        let view = {
            let state = self.interactive.borrow();
            let Some(ui) = state.ui.as_ref().filter(|ui| ui.id == id) else {
                return Ok(false);
            };
            ui.view
        };
        // 100776610/100776774 cache NSString.UTF8String using strlen, so a
        // public caller has the same first-NUL boundary as the native host.
        let text = text.split_once('\0').map_or(text, |(prefix, _)| prefix);
        // 1007768D8/100776960 clear the timer and skip empty cached strings.
        if text.is_empty() {
            return Ok(false);
        }
        if field == AccountValidationField::Password {
            let (error, valid) = password_feedback(view, text);
            self.interactive
                .borrow_mut()
                .validation_results
                .push_back(AccountValidationResult {
                    id,
                    view,
                    field,
                    generation,
                    error,
                    valid,
                });
            return Ok(true);
        }
        // Native local email checks happen in the worker. Their visible
        // result still arrives asynchronously, even with no HTTP request.
        if !native_email_syntax(text) {
            self.queue_local(Completion::interactive(
                InteractiveCompletion::EmailValidation {
                    id,
                    generation,
                    result: Ok(1),
                },
            ));
            return Ok(true);
        }
        let Some(config) = self.prepared_online_config()? else {
            // Explicit provider required; never contact a retired service or
            // pretend an unknown email belongs to a local/guest account.
            self.queue_local(Completion::interactive(
                InteractiveCompletion::EmailValidation {
                    id,
                    generation,
                    result: Err(LoginError { status: -1 }),
                },
            ));
            return Ok(true);
        };
        let session = self.session.clone();
        let owner = session.request_owner(ProviderLevel::Level1);
        let email = text.to_owned();
        spawn_online(
            Arc::clone(&self.online_completions),
            self.application_events.clone(),
            ApplicationEvent::SkynestAccountOnline,
            owner,
            move |owner| {
                OnlineCompletion::Interactive(OnlineResult::EmailValidation {
                    id,
                    generation,
                    result: request_email_validation(&config, &session, owner, &email),
                })
            },
        )?;
        Ok(true)
    }

    pub(super) fn dispatch_email_validation(
        &self,
        owner: RequestOwner,
        id: u64,
        generation: u64,
        result: Result<u32, LoginError>,
    ) {
        if !self
            .interactive
            .borrow()
            .ui
            .as_ref()
            .is_some_and(|ui| ui.id == id)
        {
            return;
        }
        // 10075A658 posts its result at 10075A6E4; its catch 10075A714 also
        // posts a separate UI callback. Do not change the current form here.
        self.queue_local_owned(
            owner,
            Completion::interactive(InteractiveCompletion::EmailValidation {
                id,
                generation,
                result,
            }),
        );
    }

    pub(super) fn finish_email_validation(
        &self,
        id: u64,
        generation: u64,
        result: Result<u32, LoginError>,
    ) {
        let mut state = self.interactive.borrow_mut();
        let Some(ui) = state.ui.as_mut().filter(|ui| ui.id == id) else {
            return;
        };
        let code = match result {
            Ok(code) => code,
            Err(_) => {
                // Native catches every exception, not just transport status
                // -1; all select NoNetwork through 10075AD10 -> 10075A2F0.
                ui.view = AccountView::NoNetworkConnectivity;
                ui.busy = false;
                ui.field_error = None;
                return;
            }
        };
        // 10075B15C reads state +92 at DELIVERY. Navigation, progress, another
        // edit or another worker in this owner does not cancel the old check.
        // 1007584CC skips the +92 store for state 12: progress changes the
        // provider's visible UIView, NOT this retained controller view. The
        // host's progress view ignores field feedback; mapping still uses it.
        let view = ui.view;
        let (error, valid) = email_feedback(view, code);
        state.validation_results.push_back(AccountValidationResult {
            id,
            view,
            field: AccountValidationField::Email,
            generation,
            error,
            valid,
        });
    }
}

fn native_email_syntax(email: &str) -> bool {
    let bytes = email.as_bytes();
    if !(1..=256).contains(&bytes.len()) {
        return false;
    }
    const ALLOWED: &[u8] =
        b"abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789!#$%&'*+-/=?^_`{|}~.@";
    if !bytes.iter().all(|byte| ALLOWED.contains(byte)) {
        return false;
    }
    // 10072EF6C..10072EFB0 track the LAST @ and dot, not an RFC parser.
    // Multiple @, adjacent punctuation and a final dot can pass this gate.
    matches!((bytes.iter().rposition(|&b| b == b'@'), bytes.iter().rposition(|&b| b == b'.')),
        (Some(at), Some(dot)) if at > 0 && at <= dot)
}

fn password_feedback(view: AccountView, password: &str) -> (Option<AccountFieldError>, bool) {
    // 10072FFBC / 10075A0B4 use UTF-8 byte count, and no network request.
    if password.len() >= 8 {
        return (None, true);
    }
    let message = if password.is_empty() {
        5
    } else if view == AccountView::SignIn {
        6
    } else {
        4
    };
    let field = match view {
        AccountView::SignIn => 19,
        AccountView::Register2 => 17,
        _ => 23,
    };
    (Some(AccountFieldError { field, message }), false)
}

fn email_feedback(view: AccountView, code: u32) -> (Option<AccountFieldError>, bool) {
    let mut valid = code == 0;
    let error = match view {
        AccountView::SignIn => match code {
            2 => {
                valid = true;
                None
            }
            1 => Some(AccountFieldError {
                field: 18,
                message: 1,
            }),
            _ => Some(AccountFieldError {
                field: 18,
                message: 3,
            }),
        },
        AccountView::Register2 => match code {
            0 => None,
            2 => Some(AccountFieldError {
                field: 16,
                message: 2,
            }),
            _ => Some(AccountFieldError {
                field: 16,
                message: 1,
            }),
        },
        AccountView::ForgotPassword => {
            valid |= code == 2;
            Some(AccountFieldError {
                field: 15,
                message: if code == 2 { 2 } else { 1 },
            })
        }
        _ => None,
    };
    // These booleans do not undo the preceding error: iOS provider slots
    // 10072E61C/624 tailcall RET-only 100763948/94C. Preserve both outputs.
    (error, valid)
}

fn request_email_validation(
    config: &IdentityConfig,
    session: &IdentitySession,
    owner: &mut RequestOwner,
    email: &str,
) -> Result<u32, LoginError> {
    let mut response = session
        .execute_form_for_owner(
            config,
            owner,
            ProviderLevel::Level1,
            "abid/validate/email",
            &form_body(&[("email", email.to_owned())]),
        )
        .map_err(LoginError::from)?;
    let status = response.status().as_u16();
    if !(200..300).contains(&status) {
        return Err(LoginError {
            status: i32::from(status),
        });
    }
    let mut bytes = Vec::new();
    response
        .body_mut()
        .as_reader()
        .read_to_end(&mut bytes)
        .map_err(|_| LoginError { status: -1 })?;
    let json: serde_json::Value =
        serde_json::from_slice(&bytes).map_err(|_| LoginError { status: -1 })?;
    let code = native_validation_code(&json).ok_or(LoginError { status: -1 })?;
    Ok(match code {
        0 | 3 | 4 => code,
        10 => 2,
        _ => 1,
    })
}

fn native_validation_code(json: &serde_json::Value) -> Option<u32> {
    // 10055D400 requires an object with a code key, 10055D244 requires Number
    // and returns its low W word. Parser integer i64s stay exact; doubles go
    // through signed FCVTZS X,D first (10055AE64), then are narrowed to W.
    let number = json.as_object()?.get("code")?.as_number()?;
    if let Some(value) = number.as_i64() {
        return Some(value as u32);
    }
    if number.as_u64().is_some() {
        return None;
    }
    number.as_f64().map(|value| (value as i64) as u32)
}

#[cfg(test)]
mod tests;
