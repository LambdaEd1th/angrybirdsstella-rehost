//! 2DD104 / 2DD8C4 / 2DDA0C system-account completion chain.
use super::*;
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::time::Duration;

pub type FacebookSystemAccountCompletion<T> =
    Box<dyn FnOnce(Result<T, SocialPlatformError>) + Send>;

/// ACAccountCredentialRenewResult returned by the actual account store.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(isize)]
pub enum FacebookSystemAuthorization {
    Renewed = 0,
    Rejected = 1,
    Failed = 2,
}

/// An embedding host's real account-store operations. Installation does not
/// initiate account access. Every method is called on the application thread;
/// asynchronous completions may arrive on another thread and are marshalled
/// back through the captured SDK dispatcher. There are no successful defaults.
pub trait FacebookSystemAccountAdapter: Send + Sync + 'static {
    fn can_request_access_without_ui(&self) -> bool;
    fn renew_system_authorization(
        &self,
        completion: FacebookSystemAccountCompletion<FacebookSystemAuthorization>,
    );
    /// 2F4948 restores access with nil requested permissions, isReauthorize=false,
    /// and the session's last system audience (zero for a cache-restored session).
    fn restore_account_access(
        &self,
        app_id: &str,
        default_audience: i32,
        completion: FacebookSystemAccountCompletion<String>,
    );
    /// SDK adapter policy for its next renewal, not a system settings mutation.
    fn set_force_blocking_renew(&self, force: bool);
}

impl FacebookOAuthSession {
    /// Supply an actual account-store capability for cached loginType1 tokens.
    /// Worker requests can wait for its callback; standalone hosts must continue
    /// draining SDK completions on their application thread while they wait.
    pub fn set_system_account_adapter(&self, adapter: Arc<dyn FacebookSystemAccountAdapter>) {
        self.state
            .lock()
            .expect("Facebook session lock poisoned")
            .refresh
            .system_account = Some(adapter);
    }
}

#[derive(Clone, Copy)]
enum Kind {
    Expired,
    Password,
    RenewOnly,
}

#[derive(Clone, Copy)]
pub(super) enum Outcome {
    Original,
    Repaired,
    Cancelled,
}

pub(super) fn resolve(
    response: wire::Response,
    outcome: Outcome,
) -> (Result<Value, SocialPlatformError>, bool) {
    match outcome {
        Outcome::Original => {
            let close = invalid_session(&response);
            (response.into_result(), close)
        }
        Outcome::Repaired => (Err(SocialPlatformError::GraphRetryRequired), false),
        Outcome::Cancelled => (Err(SocialPlatformError::Cancelled), false),
    }
}

pub(super) enum Pending {
    Ready,
    Waiting {
        receiver: Receiver<Outcome>,
        state: Weak<Mutex<Session>>,
        owner: SocialPlatformRequestOwner,
        // Keep the adapter alive without adapter -> callback -> adapter cycles.
        _adapter: Arc<dyn FacebookSystemAccountAdapter>,
    },
}

impl Pending {
    pub fn wait(self) -> Outcome {
        let Self::Waiting {
            receiver,
            state,
            owner,
            _adapter,
        } = self
        else {
            return Outcome::Original;
        };
        loop {
            match receiver.recv_timeout(Duration::from_millis(100)) {
                Ok(outcome) => return outcome,
                Err(RecvTimeoutError::Disconnected) => return Outcome::Cancelled,
                Err(RecvTimeoutError::Timeout) => {
                    let Some(state) = state.upgrade() else {
                        return Outcome::Cancelled;
                    };
                    if !owns(
                        &state.lock().expect("Facebook session lock poisoned"),
                        &owner,
                    ) {
                        return Outcome::Cancelled;
                    }
                }
            }
        }
    }
}

#[derive(Clone)]
pub(super) struct Admission {
    pub state: Weak<Mutex<Session>>,
    pub owner: SocialPlatformRequestOwner,
    pub cache: FacebookTokenCache,
    pub dispatcher: Option<SocialPlatformDispatcher>,
    pub adapter: Option<Arc<dyn FacebookSystemAccountAdapter>>,
    pub app_id: String,
}

impl Admission {
    pub fn complete_auxiliary(&self, response: wire::Response, pending: Pending, extension: bool) {
        let waiting = matches!(&pending, Pending::Waiting { .. });
        let admission = self.clone();
        let finish = move || {
            let outcome = pending.wait();
            if matches!(outcome, Outcome::Cancelled) {
                return;
            }
            let (result, close) = resolve(response, outcome);
            let Some(state) = admission.state.upgrade() else {
                return;
            };
            let owner = admission.owner.clone();
            let dispatcher = admission.dispatcher.clone();
            let task = SocialPlatformTask::new(move || {
                let (extension_result, permissions_result) = if extension {
                    (Some(result), None)
                } else {
                    (None, Some(result))
                };
                apply(
                    &admission.state,
                    &admission.owner,
                    &admission.cache,
                    extension && close,
                    extension_result,
                    permissions_result,
                );
            });
            let _ = queue_task(&state, &owner, &dispatcher, task);
        };
        // A held auxiliary account operation must not hold the primary consumer.
        // Even immediate outcomes use the same application-thread apply task.
        if !waiting {
            finish();
        } else if std::thread::Builder::new()
            .name("stella-facebook-repair".into())
            .spawn(finish)
            .is_err()
            && let Some(state) = self.state.upgrade()
        {
            let mut session = state.lock().expect("Facebook session lock poisoned");
            if owns(&session, &self.owner) {
                session.refresh.last_error = Some(SocialPlatformError::Transport);
            }
        }
    }

    pub fn schedule(&self, response: &wire::Response, connection_error: bool) -> Pending {
        let Some(adapter) = &self.adapter else {
            return Pending::Ready;
        };
        let error = response.value.get("error");
        let code = error.and_then(|e| e.get("code")).and_then(error_int_value);
        let subcode = error
            .and_then(|e| e.get("error_subcode"))
            .and_then(error_int_value);
        let kind = if connection_error && code == Some(200) {
            Kind::RenewOnly
        } else if invalid_session(response) {
            match subcode {
                Some(463) => Kind::Expired,
                Some(460) => Kind::Password,
                _ => Kind::RenewOnly,
            }
        } else {
            return Pending::Ready;
        };
        let (sender, receiver) = mpsc::channel();
        let context = Context {
            state: self.state.clone(),
            owner: self.owner.clone(),
            cache: self.cache.clone(),
            dispatcher: self.dispatcher.clone(),
            adapter: Arc::downgrade(adapter),
            app_id: self.app_id.clone(),
            sender,
        };
        context.enqueue(move |context| context.begin(kind));
        Pending::Waiting {
            receiver,
            state: self.state.clone(),
            owner: self.owner.clone(),
            _adapter: adapter.clone(),
        }
    }
}

struct Context {
    state: Weak<Mutex<Session>>,
    owner: SocialPlatformRequestOwner,
    cache: FacebookTokenCache,
    dispatcher: Option<SocialPlatformDispatcher>,
    adapter: Weak<dyn FacebookSystemAccountAdapter>,
    app_id: String,
    sender: Sender<Outcome>,
}

impl Context {
    fn enqueue(self, operation: impl FnOnce(Self) + Send + 'static) {
        let Some(state) = self.state.upgrade() else {
            return;
        };
        let owner = self.owner.clone();
        let dispatcher = self.dispatcher.clone();
        let task = SocialPlatformTask::new(move || {
            let Some(state) = self.state.upgrade() else {
                return;
            };
            if owns(
                &state.lock().expect("Facebook session lock poisoned"),
                &self.owner,
            ) {
                operation(self);
            }
        });
        let _ = queue_task(&state, &owner, &dispatcher, task);
    }

    fn finish(self, outcome: Outcome) {
        let _ = self.sender.send(outcome);
    }

    fn record_error(&self, error: SocialPlatformError) {
        if let Some(state) = self.state.upgrade() {
            let mut session = state.lock().expect("Facebook session lock poisoned");
            if owns(&session, &self.owner) {
                session.refresh.last_error = Some(error);
            }
        }
    }

    fn begin(self, kind: Kind) {
        let Some(state) = self.state.upgrade() else {
            return;
        };
        let system = state
            .lock()
            .expect("Facebook session lock poisoned")
            .refresh
            .token
            .as_ref()
            .is_some_and(token_cache::CachedToken::is_system_account);
        if !system {
            self.finish(Outcome::Original);
            return;
        }
        let Some(adapter) = self.adapter.upgrade() else {
            return;
        };
        if matches!(kind, Kind::Password) {
            adapter.set_force_blocking_renew(true);
            self.finish(Outcome::Original);
            return;
        }
        let restore = matches!(kind, Kind::Expired) && adapter.can_request_access_without_ui();
        adapter.renew_system_authorization(Box::new(move |result| {
            self.enqueue(move |context| context.renewed(restore, result));
        }));
    }

    fn renewed(
        self,
        restore: bool,
        result: Result<FacebookSystemAuthorization, SocialPlatformError>,
    ) {
        if let Err(error) = result {
            self.record_error(error);
        }
        if !restore || result != Ok(FacebookSystemAuthorization::Renewed) {
            self.finish(Outcome::Original);
            return;
        }
        let Some(adapter) = self.adapter.upgrade() else {
            return;
        };
        let app_id = self.app_id.clone();
        adapter.restore_account_access(&app_id, 0, Box::new(move |result| {
            self.enqueue(move |context| {
                match result {
                    Ok(token) => {
                        apply(&context.state, &context.owner, &context.cache, false,
                            Some(Ok(serde_json::json!({"access_token":token,"expires_at":64_092_211_200.0}))), None);
                        context.finish(Outcome::Repaired);
                    }
                    Err(error) => {
                        context.record_error(error);
                        context.finish(Outcome::Original);
                    }
                }
            });
        }));
    }
}
