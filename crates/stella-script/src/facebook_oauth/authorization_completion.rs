//! Shared native FBSession callback handling319BD4/31C68C.
use super::*;

impl FacebookOAuthSession {
    pub(super) fn handle_authorization_params(
        &self,
        params: BTreeMap<String, String>,
        dialog_id: Option<&str>,
    ) -> Result<bool, SocialPlatformError> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| SocialPlatformError::Transport)?;
        if let Some(id) = dialog_id
            && (state.auth_logger_id != id || state.pending_login_type != 4)
        {
            return Ok(false);
        }
        let login_type = state.pending_login_type as i32;
        if dialog_id.is_none() {
            state.pending_login_type = 0;
        }
        if state.status != FacebookSessionState::Opening {
            return Ok(false);
        }
        if let Some(token) = params.get("access_token") {
            let graph = match FacebookGraphSession::new(&self.config.graph_root, token) {
                Ok(graph) => graph
                    .with_transport(state.transport.clone().expect("OAuth transport installed")),
                Err(error) => {
                    state.status = FacebookSessionState::ClosedLoginFailed;
                    state.completion = Some(Err(error));
                    return Ok(true);
                }
            };
            let requested = protocol::permissions(&self.config);
            // 77BD58 combines FacebookService's retained Graph user with the
            // CURRENT active session token. Reauthorization does not itself
            // erase the service's profile cache; only open logout779720 does.
            if let Some(user) = &state.retained_user {
                graph.publish_user_profile(&SocialPlatformProfile {
                    user: user.clone(),
                    access_token: token.clone(),
                    client_id: String::new(),
                    request_owner: None,
                })?;
            }
            let granted = params
                .get("granted_scopes")
                .map(|s| s.split(',').map(str::to_owned).collect())
                .unwrap_or_else(|| requested.clone());
            protocol::update_declined(&mut state.declined, &requested, &granted);
            let now = token_cache::now();
            let token_data = token_cache::CachedToken::from_response(
                token.clone(),
                granted.clone(),
                &params,
                login_type,
                now,
            );
            self.token_cache.cache(&token_data);
            state.refresh.install(token_data);
            state.granted = granted;
            state.graph = Some(Arc::new(graph));
            state.status = FacebookSessionState::Open;
            state.login_profile_pending = true;
            state.completion = Some(Ok(()));
        } else if params
            .get("error")
            .is_some_and(|error| error == "service_disabled_use_browser")
        {
            // 31C68C retries with Safari enabled and retains the auth logger.
            // No intermediate login callback is sent; this retry explicitly
            // disables inline fallback even when an adapter is installed.
            let logger_id = state.auth_logger_id.clone();
            state.pending_login_type = 3;
            drop(state);
            let launched = protocol::retry_authorization_url(&self.config, &logger_id)
                .and_then(|url| (self.launch)(&url));
            if !matches!(launched, Ok(true)) {
                let mut state = self
                    .state
                    .lock()
                    .map_err(|_| SocialPlatformError::Transport)?;
                if state.status == FacebookSessionState::Opening
                    && state.auth_logger_id == logger_id
                {
                    state.pending_login_type = 0;
                    state.status = FacebookSessionState::ClosedLoginFailed;
                    state.completion = Some(Err(launched
                        .err()
                        .unwrap_or(SocialPlatformError::Unavailable)));
                }
            }
            return Ok(true);
        } else {
            state.status = FacebookSessionState::ClosedLoginFailed;
            // URL cancellation has an NSError31C68C, unlike implicit resume.
            // service_disabled explicitly disables every retry strategy.
            state.completion = Some(Err(
                if params
                    .get("error")
                    .is_some_and(|error| error == "service_disabled")
                {
                    SocialPlatformError::Unavailable
                } else if params.contains_key("error_message") || params.contains_key("error_code")
                {
                    SocialPlatformError::Graph
                } else {
                    SocialPlatformError::Cancelled
                },
            ));
        }
        Ok(true)
    }
}
