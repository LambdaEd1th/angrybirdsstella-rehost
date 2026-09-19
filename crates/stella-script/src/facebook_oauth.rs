//! Purple's external-browser FBSession login path (SDK 3.14.1).
//!
//! The embedding host supplies a browser launcher and explicitly selected
//! replacement endpoints. Returned credentials feed the real Graph provider.
//! System-account/app-switch/inline-dialog strategies are separate adapters.
//! A host-selected token cache can restore sessions across process restarts.

use crate::{
    FacebookGraphSession, SocialFriendDetails, SocialLoginRequest, SocialPlatformError,
    SocialPlatformFriends, SocialPlatformProfile, SocialPlatformProvider, SocialProfileRequest,
    facebook_graph::encode_query,
};
use std::{
    collections::BTreeMap,
    fmt,
    sync::{Arc, Mutex},
    time::{SystemTime, UNIX_EPOCH},
};

mod application;
mod authorization_completion;
mod dialog;
pub use dialog::{
    FacebookLoginDialogAdapter, FacebookLoginDialogEvent, FacebookLoginDialogRequest,
};
mod protocol;
mod refresh;
mod token_cache;
pub use refresh::system_account::{
    FacebookSystemAccountAdapter, FacebookSystemAccountCompletion, FacebookSystemAuthorization,
};
pub use token_cache::{FacebookTokenCache, FacebookTokenCacheError};

/// Explicit endpoints and application registration for a compatible OAuth
/// service. No historical application credentials are embedded or inferred.
#[derive(Clone, Debug)]
pub struct FacebookOAuthConfig {
    pub graph_root: String,
    /// Explicit API origin/version root for standalone SDK REST requests.
    /// Native Graph and REST use different domains. None leaves this host
    /// capability unavailable; no REST origin is guessed from graph_root.
    pub rest_root: Option<String>,
    /// Full browser authorization endpoint, including its `/oauth` path.
    pub authorization_url: String,
    pub app_id: String,
    pub url_scheme_suffix: String,
    /// FacebookService779E68 requests birthday by default. Only a supplied
    /// permission_request_birthday value other than "true" disables it.
    pub request_birthday: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u32)]
pub enum FacebookSessionState {
    Created = 0,
    CreatedTokenLoaded = 1,
    Opening = 2,
    ClosedLoginFailed = 257,
    Closed = 258,
    Open = 513,
    OpenTokenExtended = 514,
}

type BrowserLauncher = dyn Fn(&str) -> Result<bool, SocialPlatformError> + Send + Sync;

/// One active external-browser Facebook session. The launcher must return the
/// actual host acceptance result. `false` is an unavailable browser strategy;
/// it never supplies a successful login. Callbacks run on the application
/// thread through SocialPlatformProvider, not a polling worker.
pub struct FacebookOAuthSession {
    config: FacebookOAuthConfig,
    launch: Box<BrowserLauncher>,
    state: Arc<Mutex<Session>>,
    token_cache: FacebookTokenCache,
}

struct Session {
    status: FacebookSessionState,
    refresh: refresh::State,
    transport: Option<Arc<crate::facebook_graph::GraphTransport>>,
    pending_login_type: u32,
    auth_logger_id: String,
    application_launcher: Option<Arc<BrowserLauncher>>,
    dialog_adapter: Option<dialog::Adapter>,
    active_dialog: Option<dialog::Active>,
    graph: Option<Arc<FacebookGraphSession>>,
    retained_user: Option<crate::SocialPlatformUser>,
    completion: Option<Result<(), SocialPlatformError>>,
    login_profile_pending: bool,
    granted: Vec<String>,
    declined: Vec<String>,
}

impl Session {
    fn open_cached(
        &mut self,
        config: &FacebookOAuthConfig,
        token: token_cache::CachedToken,
    ) -> Result<(), SocialPlatformError> {
        let graph = FacebookGraphSession::new(&config.graph_root, &token.token)?
            .with_transport(self.transport.clone().expect("OAuth transport installed"));
        if let Some(user) = &self.retained_user {
            graph.publish_user_profile(&SocialPlatformProfile {
                user: user.clone(),
                access_token: token.token.clone(),
                client_id: String::new(),
                request_owner: None,
            })?;
        }
        self.status = FacebookSessionState::CreatedTokenLoaded;
        self.granted = token.granted_permissions();
        self.refresh.install(token);
        self.declined.clear();
        self.graph = Some(Arc::new(graph));
        self.status = FacebookSessionState::Open;
        self.pending_login_type = 0;
        self.completion = None;
        self.login_profile_pending = true;
        Ok(())
    }
}

impl fmt::Debug for FacebookOAuthSession {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("FacebookOAuthSession")
    }
}

impl FacebookOAuthSession {
    pub fn new(
        config: FacebookOAuthConfig,
        launch: impl Fn(&str) -> Result<bool, SocialPlatformError> + Send + Sync + 'static,
    ) -> Result<Self, SocialPlatformError> {
        Self::new_with_cache(config, FacebookTokenCache::memory(), launch)
    }

    /// Construct the FacebookService with an application-owned SDK cache.
    /// A valid cached session opens without UI7792B8/31EE94 and schedules its
    /// service profile request for installation in the runtime. A missing or
    /// rejected cache remains Created; construction never launches a browser.
    pub fn new_with_cache(
        config: FacebookOAuthConfig,
        token_cache: FacebookTokenCache,
        launch: impl Fn(&str) -> Result<bool, SocialPlatformError> + Send + Sync + 'static,
    ) -> Result<Self, SocialPlatformError> {
        protocol::validate_config(&config)?;
        let state = Arc::new(Mutex::new(Session {
            status: FacebookSessionState::Created,
            refresh: Default::default(),
            transport: None,
            pending_login_type: 0,
            auth_logger_id: String::new(),
            application_launcher: None,
            dialog_adapter: None,
            active_dialog: None,
            graph: None,
            retained_user: None,
            completion: None,
            login_profile_pending: false,
            granted: Vec::new(),
            declined: Vec::new(),
        }));
        state
            .lock()
            .expect("Facebook session lock poisoned")
            .transport = Some(refresh::transport(&state, &config, token_cache.clone()));
        if let Some(token) = token_cache
            .admitted(&protocol::permissions(&config))
            .map_err(|_| SocialPlatformError::InvalidResponse)?
        {
            state
                .lock()
                .expect("Facebook session lock poisoned")
                .open_cached(&config, token)?;
        }
        Ok(Self {
            config,
            launch: Box::new(launch),
            state,
            token_cache,
        })
    }

    pub fn session_state(&self) -> FacebookSessionState {
        self.state
            .lock()
            .expect("Facebook session lock poisoned")
            .status
    }

    /// Cache I/O failure does not turn a valid native login into an auth error.
    /// Hosts can report it separately even when they moved the cache handle
    /// into this provider instead of retaining a clone.
    pub fn take_token_cache_error(&self) -> Option<FacebookTokenCacheError> {
        self.token_cache.take_error()
    }

    pub fn granted_permissions(&self) -> Vec<String> {
        self.state
            .lock()
            .expect("Facebook session lock poisoned")
            .granted
            .clone()
    }

    pub fn declined_permissions(&self) -> Vec<String> {
        self.state
            .lock()
            .expect("Facebook session lock poisoned")
            .declined
            .clone()
    }

    fn graph(&self) -> Result<Arc<FacebookGraphSession>, SocialPlatformError> {
        let state = self
            .state
            .lock()
            .map_err(|_| SocialPlatformError::Transport)?;
        if !refresh::is_open(state.status) {
            return Err(SocialPlatformError::NotLoggedIn);
        }
        state.graph.clone().ok_or(SocialPlatformError::NotLoggedIn)
    }

    /// FBSession.close preserves token/cache storage; FacebookService.logout
    /// below clears those only while the session is open.
    pub fn close(&self) {
        let mut state = self.state.lock().expect("Facebook session lock poisoned");
        if let Some(graph) = &state.graph {
            graph.close();
        }
        if state.status == FacebookSessionState::Opening {
            state.status = FacebookSessionState::ClosedLoginFailed;
            // 319B58 passes nil NSError. 7831E4 converts that to true, even
            // without an account; the following profile request fails closed.
            state.completion = Some(Ok(()));
        } else {
            state.status = FacebookSessionState::Closed;
        }
    }
}

impl SocialPlatformProvider for FacebookOAuthSession {
    fn set_application_dispatcher(&self, dispatcher: crate::SocialPlatformDispatcher) {
        self.state
            .lock()
            .expect("Facebook session lock poisoned")
            .refresh
            .dispatcher = Some(dispatcher);
    }
    fn is_logged_in(&self) -> bool {
        self.graph().is_ok_and(|graph| graph.is_logged_in())
    }

    fn prepare_login(self: Arc<Self>) -> SocialLoginRequest {
        self.retire_login_dialog();
        let (url, logger_id, application_launcher) = {
            let mut state = self.state.lock().expect("Facebook session lock poisoned");
            // openActiveSession31EE94 creates a new session for each service
            // login. The Friends consumer rejects duplicate pending connects.
            if let Some(graph) = state.graph.take() {
                state.retained_user = graph.cached_user();
                graph.close();
            }
            // Cache admission occurs for every newly constructed FBSession,
            // including subsequent login requests on this service31EE94.
            match self
                .token_cache
                .admitted(&protocol::permissions(&self.config))
            {
                Ok(Some(token)) => {
                    return SocialLoginRequest::Ready(state.open_cached(&self.config, token));
                }
                Ok(None) => {}
                Err(_) => {
                    return SocialLoginRequest::Ready(Err(SocialPlatformError::InvalidResponse));
                }
            }
            let (url, logger_id) = match protocol::authorization_url(&self.config) {
                Ok(request) => request,
                Err(error) => return SocialLoginRequest::Ready(Err(error)),
            };
            state.status = FacebookSessionState::Opening;
            let application_launcher = state.application_launcher.clone();
            state.pending_login_type = if application_launcher.is_some() { 2 } else { 3 };
            state.auth_logger_id = logger_id.clone();
            state.completion = None;
            state.login_profile_pending = false;
            state.granted.clear();
            state.declined.clear();
            (url, logger_id, application_launcher)
        };
        self.start_authorization(url, &logger_id, application_launcher)
    }

    fn application_resumed(&self) {
        let mut state = self.state.lock().expect("Facebook session lock poisoned");
        // 31D8EC: pending 0/4 and states Created/257/258 are exempt. This
        // pending dialog4 remains live across background/resume transitions.
        if state.status == FacebookSessionState::Opening
            && !matches!(state.pending_login_type, 0 | 4)
        {
            state.status = FacebookSessionState::ClosedLoginFailed;
            state.completion = Some(Ok(()));
            state.pending_login_type = 0;
        }
    }

    fn handle_open_url(&self, url: &str) -> Result<bool, SocialPlatformError> {
        let Some(params) = protocol::callback_params(&self.config, url)? else {
            return Ok(false);
        };
        self.handle_authorization_params(params, None)
    }

    fn handle_login_dialog_event(
        &self,
        event: &FacebookLoginDialogEvent,
    ) -> Result<bool, SocialPlatformError> {
        match event {
            FacebookLoginDialogEvent::Navigation {
                request_id,
                url,
                link_clicked,
            } => self.handle_login_dialog_navigation(request_id, url, *link_clicked),
            FacebookLoginDialogEvent::Cancel { request_id } => self.cancel_login_dialog(request_id),
            FacebookLoginDialogEvent::LoadFailure {
                request_id,
                domain,
                code,
            } => self.handle_login_dialog_load_error(request_id, domain, *code),
        }
    }

    fn take_login_completion(&self) -> Option<Result<(), SocialPlatformError>> {
        self.state
            .lock()
            .expect("Facebook session lock poisoned")
            .completion
            .take()
    }

    fn take_login_profile_request(self: Arc<Self>) -> Option<SocialProfileRequest> {
        let mut state = self.state.lock().expect("Facebook session lock poisoned");
        if !std::mem::take(&mut state.login_profile_pending) {
            return None;
        }
        let graph = state.graph.clone()?;
        drop(state);
        Some(graph.prepare_user_profile())
    }

    fn logout(&self) -> Result<(), SocialPlatformError> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| SocialPlatformError::Transport)?;
        if refresh::is_open(state.status) {
            if let Some(graph) = &state.graph {
                graph.logout()?;
            }
            state.graph = None;
            state.retained_user = None;
            self.token_cache.clear();
            state.login_profile_pending = false;
            state.status = FacebookSessionState::Closed;
            state.granted.clear();
            state.declined.clear();
        }
        Ok(())
    }

    fn user_profile(&self) -> Result<SocialPlatformProfile, SocialPlatformError> {
        let profile = self.graph()?.user_profile()?;
        self.publish_completed_profile(&profile)
    }
    fn prepare_user_profile(self: Arc<Self>) -> SocialProfileRequest {
        match self.graph() {
            Ok(graph) => graph.prepare_user_profile(),
            Err(error) => SocialProfileRequest::Ready(Err(error)),
        }
    }
    fn publish_user_profile(
        &self,
        profile: &SocialPlatformProfile,
    ) -> Result<(), SocialPlatformError> {
        self.publish_completed_profile(profile).map(|_| ())
    }

    fn publish_completed_profile(
        &self,
        profile: &SocialPlatformProfile,
    ) -> Result<SocialPlatformProfile, SocialPlatformError> {
        self.dispatch_pending_sdk_completions();
        let state = self
            .state
            .lock()
            .map_err(|_| SocialPlatformError::Transport)?;
        let graph = state
            .graph
            .as_ref()
            .filter(|_| refresh::is_open(state.status))
            .ok_or(SocialPlatformError::Cancelled)?;
        if !profile
            .request_owner
            .as_ref()
            .is_some_and(|owner| graph.owns_request(owner))
        {
            return Err(SocialPlatformError::Cancelled);
        }
        let mut profile = profile.clone();
        profile.access_token = graph.current_token()?;
        graph.publish_user_profile(&profile)?;
        Ok(profile)
    }

    fn friends(
        &self,
        details: SocialFriendDetails,
    ) -> Result<SocialPlatformFriends, SocialPlatformError> {
        self.graph()?.friends(details)
    }
}

#[cfg(test)]
mod tests;
