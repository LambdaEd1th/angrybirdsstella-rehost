//! FBSessionInlineWebViewLoginStategy32F3AC and FBLoginDialog30CF04.
use super::*;

/// The host owns an embedded web view and reports its actual navigation/load
/// events on the application thread. This request is also the ownership key
/// used to reject callbacks from a dialog replaced by a newer login.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FacebookLoginDialogRequest {
    pub authorization_url: String,
    pub request_id: String,
}

/// Actual host-view events. Deliver through StellaLua's platform dialog entry
/// point when installed in a runtime, so SDK completion also starts the native
/// FacebookService profile request and pending Friends consumer callbacks.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FacebookLoginDialogEvent {
    Navigation {
        request_id: String,
        url: String,
        link_clicked: bool,
    },
    Cancel {
        request_id: String,
    },
    LoadFailure {
        request_id: String,
        domain: String,
        code: i64,
    },
}

/// Explicit embedded-dialog capability. No default renderer, endpoint, cookie
/// store or successful operation is installed. The host must provide both UI
/// operations and forward navigation, cancellation and load errors to session
/// methods with the request ID. `dismiss` must dismiss this request's view only.
pub trait FacebookLoginDialogAdapter: Send + Sync {
    fn show(&self, request: &FacebookLoginDialogRequest) -> Result<(), SocialPlatformError>;
    fn dismiss(&self, request: &FacebookLoginDialogRequest, success: bool);
}

#[derive(Clone)]
pub(super) struct Adapter {
    endpoint: String,
    host: Arc<dyn FacebookLoginDialogAdapter>,
}
#[derive(Clone)]
pub(super) struct Active {
    request: FacebookLoginDialogRequest,
    adapter: Adapter,
}

impl FacebookOAuthSession {
    /// Select an explicit inline OAuth endpoint (native m.facebook.com/dialog/
    /// oauth2FA34C/31C58C) and a host view. Installation does not open a dialog.
    pub fn set_login_dialog_adapter(
        &self,
        authorization_url: &str,
        adapter: Arc<dyn FacebookLoginDialogAdapter>,
    ) -> Result<(), SocialPlatformError> {
        let mut config = self.config.clone();
        config.authorization_url = authorization_url.to_owned();
        protocol::validate_config(&config)?;
        self.state
            .lock()
            .map_err(|_| SocialPlatformError::Transport)?
            .dialog_adapter = Some(Adapter {
            endpoint: authorization_url.to_owned(),
            host: adapter,
        });
        Ok(())
    }

    pub(super) fn start_login_dialog(&self, id: &str, adapter: Adapter) -> SocialLoginRequest {
        let url = match protocol::dialog_authorization_url(&self.config, id, &adapter.endpoint) {
            Ok(url) => url,
            Err(error) => return SocialLoginRequest::Ready(Err(error)),
        };
        let active = Active {
            request: FacebookLoginDialogRequest {
                authorization_url: url,
                request_id: id.to_owned(),
            },
            adapter,
        };
        {
            let mut state = self.state.lock().expect("Facebook session lock poisoned");
            if state.auth_logger_id != id {
                return SocialLoginRequest::Ready(Err(SocialPlatformError::Cancelled));
            }
            state.pending_login_type = 4; // Set before show31C58C.
            state.active_dialog = Some(active.clone());
        }
        let shown = active.adapter.host.show(&active.request);
        let mut state = self.state.lock().expect("Facebook session lock poisoned");
        if state.auth_logger_id != id {
            return SocialLoginRequest::Ready(Err(SocialPlatformError::Cancelled));
        }
        if let Some(result) = state.completion.take() {
            return SocialLoginRequest::Ready(result);
        }
        match shown {
            Ok(()) => SocialLoginRequest::AwaitingCallback,
            Err(error) => {
                state.active_dialog = None;
                state.pending_login_type = 0;
                state.status = FacebookSessionState::ClosedLoginFailed;
                drop(state);
                active.adapter.host.dismiss(&active.request, false);
                SocialLoginRequest::Ready(Err(error))
            }
        }
    }

    pub(super) fn retire_login_dialog(&self) {
        let active = self
            .state
            .lock()
            .expect("Facebook session lock poisoned")
            .active_dialog
            .take();
        if let Some(active) = active {
            active.adapter.host.dismiss(&active.request, false);
        }
    }

    fn active_login_dialog(
        &self,
        id: &str,
        take: bool,
    ) -> Result<Option<Active>, SocialPlatformError> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| SocialPlatformError::Transport)?;
        if state.status != FacebookSessionState::Opening
            || state.auth_logger_id != id
            || state.pending_login_type != 4
        {
            return Ok(None);
        }
        Ok(if take {
            state.active_dialog.take()
        } else {
            state.active_dialog.clone()
        })
    }

    /// Return true when the navigation was consumed (the host must not load
    /// it), false for an ordinary in-view navigation or an obsolete request.
    /// Link clicks outside the initial URL use the actual browser launcher.
    pub fn handle_login_dialog_navigation(
        &self,
        id: &str,
        url: &str,
        link_clicked: bool,
    ) -> Result<bool, SocialPlatformError> {
        let Some(active) = self.active_login_dialog(id, false)? else {
            return Ok(false);
        };
        let Some(resource) = url.strip_prefix("fbconnect:") else {
            if link_clicked && url != active.request.authorization_url {
                return match (self.launch)(url) {
                    Ok(true) => Ok(true),
                    Ok(false) => Err(SocialPlatformError::Unavailable),
                    Err(error) => Err(error),
                };
            }
            return Ok(false);
        };
        if resource.starts_with("//cancel") {
            if string_from_url(url, "error_code=").is_some() {
                // FBDialog2E7020 dismissWithError only notifies the base
                // delegate; FBLoginDialog sets a distinct loginDelegate.
                // No fbDialogNotLogin callback is emitted by this route.
                if let Some(active) = self.active_login_dialog(id, true)? {
                    active.adapter.host.dismiss(&active.request, false);
                }
                return Err(SocialPlatformError::Graph);
            }
            return self.cancel_login_dialog(id);
        }
        let token = string_from_url(url, "access_token=");
        if token.as_deref().is_none_or(str::is_empty) {
            //30CF04 calls dialogDidCancel then dismisses again after its
            // delegate callback. Keep the observable operation order.
            let cancelled = self.cancel_login_dialog(id)?;
            active.adapter.host.dismiss(&active.request, false);
            return Ok(cancelled);
        }
        let mut params = protocol::url_params(url)?;
        params.insert("access_token".into(), token.unwrap());
        //31E158 inserts nil expirationDate.timeIntervalSinceNow (=0) when
        // expires_in is absent, overriding an unrelated absolute expires.
        params
            .entry("expires_in".into())
            .or_insert_with(|| "0".into());
        let Some(active) = self.active_login_dialog(id, true)? else {
            return Ok(false);
        };
        let result = self.handle_authorization_params(params, Some(id));
        active.adapter.host.dismiss(&active.request, true); // Success follows delegate30CF04.
        result
    }

    pub fn cancel_login_dialog(&self, id: &str) -> Result<bool, SocialPlatformError> {
        let Some(active) = self.active_login_dialog(id, true)? else {
            return Ok(false);
        };
        active.adapter.host.dismiss(&active.request, false); // Cancel precedes delegate30D0D0.
        self.handle_authorization_params(
            BTreeMap::from([(
                "error".into(),
                "com.facebook.sdk:InlineLoginCancelled".into(),
            )]),
            Some(id),
        )
    }

    /// Native30D15C ignores only these two exact domain/code pairs. Other
    /// failures dismiss the view then emit the unsuccessful login delegate.
    pub fn handle_login_dialog_load_error(
        &self,
        id: &str,
        domain: &str,
        code: i64,
    ) -> Result<bool, SocialPlatformError> {
        if (domain == "NSURLErrorDomain" && code == -999)
            || (domain == "WebKitErrorDomain" && code == 102)
        {
            return Ok(false);
        }
        let Some(active) = self.active_login_dialog(id, true)? else {
            return Ok(false);
        };
        active.adapter.host.dismiss(&active.request, false);
        //31E274 feeds ErrorLoginNotCancelled into31C68C. The latter wraps
        // it in UserLoginCancelled; do not invent a different outer error.
        self.handle_authorization_params(
            BTreeMap::from([(
                "error".into(),
                "com.facebook.sdk:ErrorLoginNotCancelled".into(),
            )]),
            Some(id),
        )
    }
}

/// FBDialog2E76D4 takes the FIRST needle, requires ?/#/& immediately before
/// it, stops at '&', and percent-decodes without treating '+' as a space.
fn string_from_url(url: &str, needle: &str) -> Option<String> {
    let index = url.find(needle)?;
    if index != 0 && !matches!(url.as_bytes()[index - 1], b'?' | b'#' | b'&') {
        return None;
    }
    let value = url[index + needle.len()..].split('&').next().unwrap();
    // NSString.stringByReplacingPercentEscapesUsingEncoding: returns nil
    // for malformed escapes/UTF-8. FBLoginDialog treats nil as cancellation.
    protocol::percent_decode(value, false).ok()
}
