//! Platform social-service boundary, separate from the game's Skynest identity.
//! All successful data comes from a provider response; an absent provider is
//! represented explicitly and never supplies a synthetic empty success.

use std::{
    collections::BTreeMap,
    fmt,
    sync::{Arc, Mutex},
};

type PlatformTaskFn = Box<dyn FnOnce() + Send>;

/// A native SDK completion to run once on the application's event thread.
/// Clones share ownership; captured credentials are never included in Debug.
#[derive(Clone)]
pub struct SocialPlatformTask(Arc<Mutex<Option<PlatformTaskFn>>>);

impl SocialPlatformTask {
    pub fn new(task: impl FnOnce() + Send + 'static) -> Self {
        Self(Arc::new(Mutex::new(Some(Box::new(task)))))
    }

    pub fn run(self) {
        let task = self.0.lock().expect("platform task lock poisoned").take();
        if let Some(task) = task {
            task();
        }
    }
}

impl fmt::Debug for SocialPlatformTask {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("SocialPlatformTask")
    }
}

pub type SocialPlatformDispatcher = Arc<dyn Fn(SocialPlatformTask) + Send + Sync>;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
#[repr(i32)]
pub enum SocialNetwork {
    Facebook = 1,
    SinaWeibo = 2,
    GameCenter = 3,
    KakaoTalk = 4,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SocialFriendDetails {
    Identifiers,
    Profiles,
}

/// Native social::User fields (100787634); these are platform IDs, not the
/// public account IDs in Skynest's game-friend map.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SocialPlatformUser {
    pub id: String,
    pub username: String,
    pub name: String,
    pub avatar_url: String,
    pub custom_params: BTreeMap<String, String>,
}

/// Native GetUserProfileResponse includes a separate platform access token.
/// Never derive Debug/Serialize for that credential-bearing response.
#[derive(Clone, Default)]
pub struct SocialPlatformProfile {
    pub user: SocialPlatformUser,
    pub access_token: String,
    pub client_id: String,
    /// Opaque provenance supplied by the platform transport. It prevents an
    /// old session's response from publishing into a replacement session.
    pub request_owner: Option<SocialPlatformRequestOwner>,
}

#[derive(Clone, Default)]
pub struct SocialPlatformRequestOwner(pub(crate) Arc<()>);

impl SocialPlatformRequestOwner {
    pub(crate) fn matches(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }
}

impl fmt::Debug for SocialPlatformProfile {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SocialPlatformProfile")
            .field("user", &self.user)
            .finish_non_exhaustive()
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SocialPlatformFriends {
    pub users: Vec<SocialPlatformUser>,
    pub next_page: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SocialPlatformError {
    Unavailable,
    NotLoggedIn,
    Http(u16),
    Transport,
    InvalidResponse,
    InvalidConfiguration,
    Graph,
    /// FBSDK code5 with Graph190/subcode65000: the token was repaired, but
    /// the original request has no successful result and was not replayed.
    GraphRetryRequired,
    Cancelled,
}

impl fmt::Display for SocialPlatformError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unavailable => f.write_str("platform social service is unavailable"),
            Self::NotLoggedIn => f.write_str("platform social session is not open"),
            Self::Http(status) => write!(f, "platform social request returned HTTP {status}"),
            Self::Transport => f.write_str("platform social transport failed"),
            Self::InvalidConfiguration => f.write_str("platform social configuration is invalid"),
            Self::Graph => f.write_str("platform social Graph request failed"),
            Self::GraphRetryRequired => f.write_str("platform social Graph request requires retry"),
            Self::InvalidResponse => f.write_str("platform social response is invalid"),
            Self::Cancelled => f.write_str("platform social request was cancelled"),
        }
    }
}
impl std::error::Error for SocialPlatformError {}

/// A native synchronous cached result or a deferred network request. Keeping
/// this distinction preserves callbacks that occur before the caller returns.
pub enum SocialProfileRequest {
    Ready(Result<SocialPlatformProfile, SocialPlatformError>),
    Pending(Box<dyn FnOnce() -> Result<SocialPlatformProfile, SocialPlatformError> + Send>),
}
impl SocialProfileRequest {
    pub fn execute(self) -> Result<SocialPlatformProfile, SocialPlatformError> {
        match self {
            Self::Ready(result) => result,
            Self::Pending(request) => request(),
        }
    }
}

/// Platform login admission runs on the application thread. A host that owns
/// interactive authentication may return a pending operation; only its actual
/// successful completion permits the native profile/connect chain to proceed.
pub enum SocialLoginRequest {
    Ready(Result<(), SocialPlatformError>),
    Pending(Box<dyn FnOnce() -> Result<(), SocialPlatformError> + Send>),
    /// An external application owns the request. Completion is delivered on
    /// the application thread by an incoming URL or activation notification.
    AwaitingCallback,
}

/// An actual host social session, authenticated independently from Skynest.
/// Pending requests return real responses and publish their cache on the app
/// thread; session replacement retires old deliveries.
pub trait SocialPlatformProvider: Send + Sync + 'static {
    /// Install an application-thread SDK completion sink. Its lifetime is
    /// independent of Skynest requests; replacement retires queued SDK work.
    fn set_application_dispatcher(&self, _dispatcher: SocialPlatformDispatcher) {}
    fn is_logged_in(&self) -> bool;
    fn prepare_login(self: Arc<Self>) -> SocialLoginRequest;
    /// SDK activation is independent of the Skynest account's access level.
    /// Token-only providers have no external authorization to cancel.
    fn application_resumed(&self) {}
    /// Incoming platform URL, distinct from an HTTP request completion.
    /// Embedded authorization is an optional host capability. Unsupported
    /// providers explicitly return unhandled; no login success is fabricated.
    fn handle_login_dialog_event(
        &self,
        _event: &crate::FacebookLoginDialogEvent,
    ) -> Result<bool, SocialPlatformError> {
        Ok(false)
    }

    fn handle_open_url(&self, _url: &str) -> Result<bool, SocialPlatformError> {
        Ok(false)
    }
    /// Consume an external login callback once, on the application thread.
    fn take_login_completion(&self) -> Option<Result<(), SocialPlatformError>> {
        None
    }
    /// FacebookService77A0DC starts its account-name lookup before invoking
    /// the login callback. Drain this admission before the connection profile.
    fn take_login_profile_request(self: Arc<Self>) -> Option<SocialProfileRequest> {
        None
    }
    /// Synchronous platform logout before the selected Skynest identity clears.
    /// Implementations must perform their own credential/cache cleanup.
    fn logout(&self) -> Result<(), SocialPlatformError>;
    fn user_profile(&self) -> Result<SocialPlatformProfile, SocialPlatformError>;
    /// Capture cached-versus-network admission on the application thread.
    /// The returned request runs on a worker; successful cache publication
    /// belongs to publish_user_profile on the application thread.
    fn prepare_user_profile(self: Arc<Self>) -> SocialProfileRequest;
    fn publish_user_profile(
        &self,
        profile: &SocialPlatformProfile,
    ) -> Result<(), SocialPlatformError>;
    /// SDK refresh can change the live token before a same-session profile
    /// completes. Return the profile that downstream consumers should receive.
    fn publish_completed_profile(
        &self,
        profile: &SocialPlatformProfile,
    ) -> Result<SocialPlatformProfile, SocialPlatformError> {
        self.publish_user_profile(profile)?;
        Ok(profile.clone())
    }
    fn friends(
        &self,
        details: SocialFriendDetails,
    ) -> Result<SocialPlatformFriends, SocialPlatformError>;
}
