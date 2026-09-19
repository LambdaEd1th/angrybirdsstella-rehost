//! SDK3.14.1 piggyback admission and application-thread session mutation.
use super::*;
use crate::facebook_graph::{GraphTransport, wire};
use crate::{SocialPlatformDispatcher, SocialPlatformRequestOwner, SocialPlatformTask};
use serde_json::Value;
use std::{collections::VecDeque, sync::Weak};

mod completion;
pub(super) mod system_account;

const DISTANT_PAST: f64 = -62_135_769_600.0;

pub(super) struct State {
    token: Option<token_cache::CachedToken>,
    attempted_extension: f64,
    attempted_permissions: f64,
    pub dispatcher: Option<SocialPlatformDispatcher>,
    pending: VecDeque<SocialPlatformTask>,
    last_error: Option<SocialPlatformError>,
    system_account: Option<Arc<dyn system_account::FacebookSystemAccountAdapter>>,
    #[cfg(test)]
    clock: Option<Arc<dyn Fn() -> f64 + Send + Sync>>,
}

impl Default for State {
    fn default() -> Self {
        Self {
            token: None,
            attempted_extension: DISTANT_PAST,
            attempted_permissions: DISTANT_PAST,
            dispatcher: None,
            pending: VecDeque::new(),
            last_error: None,
            system_account: None,
            #[cfg(test)]
            clock: None,
        }
    }
}

impl State {
    pub fn install(&mut self, token: token_cache::CachedToken) {
        self.token = Some(token);
        self.attempted_extension = DISTANT_PAST;
        self.attempted_permissions = DISTANT_PAST;
        self.pending.clear();
        self.last_error = None;
    }

    fn now(&self) -> f64 {
        #[cfg(test)]
        if let Some(clock) = &self.clock {
            return clock();
        }
        token_cache::now()
    }

    fn admit_extension(&mut self, now: f64) -> Result<bool, SocialPlatformError> {
        let token = self
            .token
            .as_ref()
            .ok_or(SocialPlatformError::NotLoggedIn)?;
        let extend = now - self.attempted_extension > 3600.0 && token.should_extend(now)?;
        if extend {
            self.attempted_extension = now;
        }
        Ok(extend)
    }

    fn admit(&mut self, now: f64) -> Result<(bool, bool), SocialPlatformError> {
        let extend = self.admit_extension(now)?;
        let token = self
            .token
            .as_ref()
            .ok_or(SocialPlatformError::NotLoggedIn)?;
        let permissions =
            now - self.attempted_permissions > 3600.0 && token.should_refresh_permissions(now)?;
        if permissions {
            self.attempted_permissions = now;
        }
        Ok((extend, permissions))
    }
}

pub(super) fn is_open(status: FacebookSessionState) -> bool {
    matches!(
        status,
        FacebookSessionState::Open | FacebookSessionState::OpenTokenExtended
    )
}

fn owns(session: &Session, owner: &SocialPlatformRequestOwner) -> bool {
    is_open(session.status)
        && session
            .graph
            .as_ref()
            .is_some_and(|graph| graph.owns_request(owner))
}

impl FacebookOAuthSession {
    /// A background refresh error does not turn a completed profile request
    /// into an authentication failure. Hosts can report it independently.
    pub fn take_refresh_error(&self) -> Option<SocialPlatformError> {
        self.state
            .lock()
            .expect("Facebook session lock poisoned")
            .refresh
            .last_error
            .take()
    }
    /// Drain SDK work for standalone embeddings without an installed event
    /// dispatcher. Call only on the application's thread, before consumers.
    /// The game's runtime instead installs its own dispatcher automatically.
    pub fn dispatch_pending_sdk_completions(&self) {
        loop {
            let task = self
                .state
                .lock()
                .expect("Facebook session lock poisoned")
                .refresh
                .pending
                .pop_front();
            let Some(task) = task else {
                break;
            };
            task.run();
        }
    }
}

pub(super) fn transport(
    state: &Arc<Mutex<Session>>,
    config: &FacebookOAuthConfig,
    cache: FacebookTokenCache,
) -> Arc<GraphTransport> {
    let state = Arc::downgrade(state);
    let config = config.clone();
    Arc::new(move |path, token, owner| {
        let shared = state.upgrade().ok_or(SocialPlatformError::Cancelled)?;
        let (extension, permissions, dispatcher, adapter) = {
            let mut session = shared.lock().map_err(|_| SocialPlatformError::Transport)?;
            if !owns(&session, owner) {
                return Err(SocialPlatformError::Cancelled);
            }
            let now = session.refresh.now();
            let (extension, permissions) = session.refresh.admit(now)?;
            (
                extension,
                permissions,
                session.refresh.dispatcher.clone(),
                session.refresh.system_account.clone(),
            )
        };
        let repair = system_account::Admission {
            state: state.clone(),
            owner: owner.clone(),
            cache: cache.clone(),
            dispatcher: dispatcher.clone(),
            adapter,
            app_id: config.app_id.clone(),
        };
        // The permission admission reserves its attempt even when an existing
        // request already has the same graphPath2DE92C.
        let permissions = permissions && path != "me/permissions";
        let mut paths = vec![path];
        if extension {
            paths.push("method/auth.extendSSOAccessToken");
        }
        if permissions {
            paths.push("me/permissions");
        }
        let responses = (|| -> Result<Vec<wire::Response>, SocialPlatformError> {
            Ok(if paths.len() == 1 {
                vec![wire::send(
                    &format!(
                        "{}/{path}?format=json&sdk=ios&access_token={}",
                        config.graph_root.trim_end_matches('/'),
                        encode_query(token)
                    ),
                    None,
                )?]
            } else {
                let body = wire::multipart(&config.app_id, &paths, token);
                let response = wire::send(config.graph_root.trim_end_matches('/'), Some(&body))?;
                wire::unpack(response, paths.len())?
            })
        })();
        let (primary, should_close, extension_result, permissions_result) = match responses {
            Ok(responses) => {
                // Admit every response before waiting: separate native request
                // tasks can renew concurrently, even inside one HTTP batch.
                let pending: Vec<_> = responses
                    .iter()
                    .map(|response| {
                        let connection_error = if paths.len() == 1 {
                            !(200..300).contains(&response.status)
                        } else {
                            matches!(response.error, Some(SocialPlatformError::Http(_)))
                        };
                        repair.schedule(response, connection_error)
                    })
                    .collect();
                let mut requests = responses.into_iter().zip(pending);
                let (primary_response, primary_pending) =
                    requests.next().expect("primary response");
                let mut remaining = Vec::new();
                for (index, (response, pending)) in requests.enumerate() {
                    if repair.adapter.is_some() {
                        repair.complete_auxiliary(response, pending, extension && index == 0);
                    } else {
                        remaining.push((response, pending));
                    }
                }
                let (primary, primary_close) =
                    system_account::resolve(primary_response, primary_pending.wait());
                if repair.adapter.is_some() {
                    (primary, primary_close, None, None)
                } else {
                    let mut completed = Vec::with_capacity(remaining.len());
                    for (response, pending) in remaining {
                        completed.push(system_account::resolve(response, pending.wait()));
                    }
                    let mut close = primary_close;
                    let extension_result = if extension {
                        let (result, extension_close) = completed.remove(0);
                        close |= extension_close;
                        Some(result)
                    } else {
                        None
                    };
                    let permissions_result = permissions.then(|| completed.remove(0).0);
                    (primary, close, extension_result, permissions_result)
                }
            }
            Err(error) => (
                Err(error),
                false,
                extension.then_some(Err(error)),
                permissions.then_some(Err(error)),
            ),
        };
        let task_state = state.clone();
        let task_owner = owner.clone();
        let task_cache = cache.clone();
        let rest_root = config.rest_root.clone();
        let app_id = config.app_id.clone();
        let task = SocialPlatformTask::new(move || {
            apply(
                &task_state,
                &task_owner,
                &task_cache,
                should_close,
                extension_result,
                permissions_result,
            );
            completion::start(&task_state, &task_owner, &task_cache, rest_root, app_id);
        });
        // Even a non-piggyback or failed request checks extension admission at
        // completion2DD104. This task precedes the service consumer, but starts
        // independent REST transport without waiting for its response2DDE60.
        queue_task(&shared, owner, &dispatcher, task)?;
        primary
    })
}

fn queue_task(
    state: &Arc<Mutex<Session>>,
    owner: &SocialPlatformRequestOwner,
    dispatcher: &Option<SocialPlatformDispatcher>,
    task: SocialPlatformTask,
) -> Result<(), SocialPlatformError> {
    {
        let mut session = state.lock().map_err(|_| SocialPlatformError::Transport)?;
        if !owns(&session, owner) {
            return Err(SocialPlatformError::Cancelled);
        }
        if dispatcher.is_none() {
            session.refresh.pending.push_back(task.clone());
        }
    }
    // Retain the delivery lifetime from request admission. Reinstallation must
    // not reroute an old request through a newly installed platform sink.
    if let Some(dispatcher) = dispatcher {
        dispatcher(task);
    }
    Ok(())
}

fn invalid_session(response: &wire::Response) -> bool {
    if (200..300).contains(&response.status) && response.error.is_none() {
        return false;
    }
    // 3261E4 only reads NSNumber values inside body.error NSDictionary.
    // Top-level legacy error_code/code fields do not classify this NSError.
    let Some(error) = response.value.get("error").and_then(Value::as_object) else {
        return false;
    };
    let code = error.get("code").and_then(error_int_value);
    let subcode = error.get("error_subcode").and_then(error_int_value);
    matches!(code, Some(102 | 190)) && !matches!(subcode, Some(459 | 65000))
}

fn error_int_value(value: &Value) -> Option<i32> {
    match value {
        Value::Bool(value) => Some(i32::from(*value)),
        Value::Number(value) => Some(value.as_i64().map_or_else(
            || {
                value
                    .as_u64()
                    .map_or_else(|| value.as_f64().unwrap_or(0.0) as i64 as i32, |v| v as i32)
            },
            |v| v as i32,
        )),
        _ => None,
    }
}

fn apply(
    state: &Weak<Mutex<Session>>,
    owner: &SocialPlatformRequestOwner,
    cache: &FacebookTokenCache,
    close: bool,
    extension: Option<Result<Value, SocialPlatformError>>,
    permissions: Option<Result<Value, SocialPlatformError>>,
) {
    let Some(state) = state.upgrade() else {
        return;
    };
    let mut session = state.lock().expect("Facebook session lock poisoned");
    if !owns(&session, owner) {
        return;
    }
    for result in [&extension, &permissions] {
        if let Some(Err(error)) = result {
            session.refresh.last_error = Some(*error);
        }
    }
    if close {
        if let Some(graph) = &session.graph {
            graph.close();
        }
        session.status = FacebookSessionState::Closed;
        cache.clear();
        return;
    }
    let now = session.refresh.now();
    if let Some(Ok(value)) = extension {
        let token = match value.get("access_token") {
            None => Some(None),
            Some(Value::String(token)) => Some(Some(token.as_str())),
            _ => None,
        };
        let expiry = value.get("expires_at").map_or(0.0, double_value);
        if let Some(token) = token
            && expiry != 0.0
        {
            let metadata = session.refresh.token.as_mut().expect("open session token");
            metadata.extend(token, expiry, now);
            let token = metadata.token.clone();
            cache.cache(metadata);
            session
                .graph
                .as_ref()
                .expect("open session graph")
                .replace_token(&token);
            session.status = FacebookSessionState::OpenTokenExtended;
        }
    }
    if let Some(Ok(value)) = permissions
        && let Some((all, granted)) = parse_permissions(&value)
    {
        protocol::update_declined(&mut session.declined, &all, &granted);
        session.granted = granted.clone();
        let metadata = session.refresh.token.as_mut().expect("open session token");
        metadata.refresh_permissions(granted, now);
        cache.cache(metadata);
        session.refresh.attempted_permissions = now;
    }
}

fn double_value(value: &Value) -> f64 {
    match value {
        Value::Number(n) => n.as_f64().unwrap_or(0.0),
        Value::String(s) => token_cache::numeric_prefix(s, true),
        Value::Bool(b) => f64::from(u8::from(*b)),
        _ => 0.0,
    }
}

fn parse_permissions(value: &Value) -> Option<(Vec<String>, Vec<String>)> {
    let data = value.get("data")?.as_array()?;
    if data.is_empty() {
        return None;
    }
    if data.len() == 1 && data[0].get("permission").is_none() {
        let all: Vec<_> = data[0].as_object()?.keys().cloned().collect();
        return (!all.is_empty()).then(|| (all.clone(), all));
    }
    let mut all = Vec::new();
    let mut granted = Vec::new();
    for item in data {
        let name = item.get("permission")?.as_str()?;
        all.push(name.to_owned());
        if item.get("status").and_then(Value::as_str) == Some("granted") {
            granted.push(name.to_owned());
        }
    }
    Some((all, granted))
}

#[cfg(test)]
mod tests;
