//! Default game login: Facebook application, then Safari (31AC14/325788).
use super::*;

impl FacebookOAuthSession {
    /// Install a host app-switch capability. The host must support multitasking
    /// and have registered this application's callback scheme before supplying
    /// it (325788). Return the actual open-URL result. A missing/declining app
    /// falls through to the supplied browser launcher; it never logs in a user.
    pub fn set_facebook_application_launcher(
        &self,
        launch: impl Fn(&str) -> Result<bool, SocialPlatformError> + Send + Sync + 'static,
    ) {
        self.state
            .lock()
            .expect("Facebook session lock poisoned")
            .application_launcher = Some(Arc::new(launch));
    }

    pub(super) fn start_authorization(
        &self,
        mut browser_url: String,
        logger_id: &str,
        application: Option<Arc<BrowserLauncher>>,
    ) -> SocialLoginRequest {
        if let Some(application) = application {
            let launched = protocol::application_authorization_url(&self.config, logger_id)
                .and_then(|url| application(&url));
            let mut state = self.state.lock().expect("Facebook session lock poisoned");
            if state.auth_logger_id != logger_id {
                return SocialLoginRequest::Ready(Err(SocialPlatformError::Cancelled));
            }
            if let Some(result) = state.completion.take() {
                return SocialLoginRequest::Ready(result);
            }
            // A launcher can synchronously deliver the app callback, including
            // a browser retry. Do not launch that already-started browser twice.
            if state.pending_login_type != 2 || state.status != FacebookSessionState::Opening {
                return if refresh::is_open(state.status) {
                    SocialLoginRequest::Ready(Ok(()))
                } else if state.status == FacebookSessionState::Opening {
                    SocialLoginRequest::AwaitingCallback
                } else {
                    SocialLoginRequest::Ready(Err(SocialPlatformError::Cancelled))
                };
            }
            if matches!(launched, Ok(true)) {
                return SocialLoginRequest::AwaitingCallback;
            }
            state.pending_login_type = 3;
            drop(state);
            browser_url = match protocol::retry_authorization_url(&self.config, logger_id) {
                Ok(url) => url,
                Err(error) => return self.finish_authorization_launch(logger_id, Err(error)),
            };
        }
        self.finish_authorization_launch(logger_id, (self.launch)(&browser_url))
    }

    fn finish_authorization_launch(
        &self,
        logger_id: &str,
        launched: Result<bool, SocialPlatformError>,
    ) -> SocialLoginRequest {
        let mut state = self.state.lock().expect("Facebook session lock poisoned");
        if state.auth_logger_id != logger_id {
            return SocialLoginRequest::Ready(Err(SocialPlatformError::Cancelled));
        }
        if let Some(result) = state.completion.take() {
            return SocialLoginRequest::Ready(result);
        }
        if !matches!(launched, Ok(true))
            && let Some(adapter) = state.dialog_adapter.clone()
        {
            drop(state);
            return self.start_login_dialog(logger_id, adapter);
        }
        match launched {
            Ok(true) => SocialLoginRequest::AwaitingCallback,
            result => {
                state.pending_login_type = 0;
                state.status = FacebookSessionState::ClosedLoginFailed;
                SocialLoginRequest::Ready(Err(result
                    .err()
                    .unwrap_or(SocialPlatformError::Unavailable)))
            }
        }
    }
}
