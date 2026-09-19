//! Native SocialManager registration plus an opt-in local social provider.

use crate::*;
use serde::{Deserialize, Serialize};
use std::{collections::VecDeque, fs, io::Read, time::Duration};

mod avatar;
mod avatar_cache;
pub(in crate::game_lua::platform_services) mod friends_store;
mod platform;

use avatar::{AvatarStage, LoadResult};

#[derive(Clone, Debug)]
enum Completion {
    Connected,
    FriendsProgress,
    Leaderboard {
        level: String,
        request_id: String,
    },
    ScorePosted {
        level: String,
        request_id: String,
    },
    AvatarDownloaded {
        account_id: String,
    },
    AvatarResult {
        generation: u64,
        accounts: Vec<String>,
        result: Result<PathBuf, avatar_cache::CacheError>,
    },
}

#[derive(Clone, Debug)]
enum OnlineCompletion {
    PlatformSdk {
        current: Arc<Mutex<bool>>,
        task: SocialPlatformTask,
    },
    Platform(Box<platform::PlatformCompletion>),
    NativeFriends {
        network: Option<SocialNetwork>,
        client: super::skynest_account::friends_support::FriendsClient,
        result:
            Result<Vec<LocalSocialFriend>, super::skynest_account::friends_support::FriendsError>,
    },
    AvatarFetched {
        url: String,
        result: Result<PathBuf, avatar_cache::CacheError>,
    },
    Connected(Result<OnlineConnectResponse, String>),
    FriendsProgress(Result<OnlineFriendsResponse, String>),
    Leaderboard {
        level: String,
        request_id: String,
        result: Result<OnlineLeaderboardResponse, String>,
    },
    ScorePosted {
        level: String,
        request_id: String,
        result: Result<(), String>,
    },
    ProgressPosted {
        progress: String,
        result: Result<(), String>,
    },
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct OnlineConnectResponse {
    #[serde(default = "default_social_network")]
    network: String,
    local_player: OnlineLocalPlayer,
    #[serde(default)]
    friends: Vec<LocalSocialFriend>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct OnlineLocalPlayer {
    account_id: String,
    #[serde(default)]
    name: String,
    #[serde(default)]
    profile: serde_json::Value,
}

#[derive(Clone, Debug, Deserialize)]
struct OnlineFriendsResponse {
    #[serde(default)]
    friends: Vec<LocalSocialFriend>,
}

#[derive(Clone, Debug, Deserialize)]
struct OnlineLeaderboardResponse {
    #[serde(default)]
    entries: Vec<OnlineLeaderboardEntry>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct OnlineLeaderboardEntry {
    points: f64,
    rank: i64,
    #[serde(default = "default_player_name")]
    nickname: String,
    account_id: String,
    #[serde(default)]
    local_player: bool,
}

fn default_social_network() -> String {
    "facebook".to_owned()
}

fn default_player_name() -> String {
    "n/a".to_owned()
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
struct LocalSocialDocument {
    #[serde(default)]
    scores: BTreeMap<String, f64>,
    #[serde(default)]
    progress: Option<String>,
    #[serde(default)]
    friends: Vec<LocalSocialFriend>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(in crate::game_lua::platform_services) struct LocalSocialFriend {
    account_id: String,
    #[serde(default)]
    name: String,
    #[serde(default)]
    nickname: String,
    #[serde(default)]
    progress: String,
    #[serde(default)]
    scores: BTreeMap<String, f64>,
    /// Compatible transport carries the original native UserProfile JSON.
    #[serde(default, skip_serializing_if = "serde_json::Value::is_null")]
    profile: serde_json::Value,
}

impl LocalSocialFriend {
    fn display_name(&self) -> &str {
        if self.name.is_empty() {
            &self.nickname
        } else {
            &self.name
        }
    }

    fn leaderboard_name(&self) -> &str {
        if self.nickname.is_empty() {
            self.display_name()
        } else {
            &self.nickname
        }
    }
}

#[derive(Debug)]
struct SocialState {
    #[cfg(test)]
    native_friends_completions: u64,
    provider_generation: u64,
    local_provider: bool,
    connected: bool,
    local_account_id: String,
    local_player_name: String,
    local_profile: serde_json::Value,
    persistence_path: PathBuf,
    document: LocalSocialDocument,
    friends_store: Option<friends_store::FriendsStore>,
    avatars: BTreeMap<String, AvatarStage>,
    avatar_paths: BTreeMap<String, PathBuf>,
    pending_avatars: BTreeMap<String, Vec<String>>,
    avatar_cache: Option<avatar_cache::AvatarCache>,
    completions: VecDeque<Completion>,
}

impl Drop for SocialState {
    fn drop(&mut self) {
        // Workers retain only the completion queue and cache backend, so the
        // last runtime owner must retire I/O before those workers finish.
        if let Some(cache) = &self.avatar_cache {
            cache.retire();
        }
    }
}

#[derive(Clone)]
pub(crate) struct SocialRuntime {
    state: Arc<Mutex<SocialState>>,
    compatible_url: Arc<Mutex<Option<String>>>,
    online_completions: Arc<Mutex<VecDeque<(u64, OnlineCompletion)>>>,
    account: super::skynest_account::SkynestAccountRuntime,
    storage: super::skynest_storage::SkynestStorageRuntime,
    data_root: Arc<PathBuf>,
    resource_runtime: Arc<Mutex<ResourceRuntime>>,
    application_events: ApplicationEventScheduler,
    platform: Arc<Mutex<platform::PlatformState>>,
}

impl std::fmt::Debug for SocialRuntime {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("SocialRuntime")
    }
}

impl SocialRuntime {
    #[cfg(test)]
    pub(crate) fn native_friends_completions_for_test(&self) -> u64 {
        self.state.lock().unwrap().native_friends_completions
    }
    #[cfg(test)]
    pub(crate) fn online_completion_count_probe(&self) -> impl Fn() -> usize + use<> {
        let queue = self.online_completions.clone();
        move || {
            queue
                .lock()
                .expect("social online completion lock poisoned")
                .len()
        }
    }

    fn new(
        persistence_path: PathBuf,
        data_root: Arc<PathBuf>,
        resource_runtime: Arc<Mutex<ResourceRuntime>>,
        application_events: ApplicationEventScheduler,
        account: super::skynest_account::SkynestAccountRuntime,
        storage: super::skynest_storage::SkynestStorageRuntime,
    ) -> Self {
        Self {
            state: Arc::new(Mutex::new(SocialState {
                #[cfg(test)]
                native_friends_completions: 0,
                provider_generation: 0,
                local_provider: false,
                connected: false,
                local_account_id: "local-player".to_owned(),
                local_player_name: "Stella Player".to_owned(),
                local_profile: serde_json::Value::Null,
                persistence_path,
                document: LocalSocialDocument::default(),
                friends_store: None,
                avatars: BTreeMap::new(),
                avatar_paths: BTreeMap::new(),
                pending_avatars: BTreeMap::new(),
                avatar_cache: None,
                completions: VecDeque::new(),
            })),
            compatible_url: Arc::new(Mutex::new(None)),
            online_completions: Arc::new(Mutex::new(VecDeque::new())),
            data_root,
            resource_runtime,
            application_events,
            account,
            storage,
            platform: Arc::new(Mutex::new(platform::PlatformState::default())),
        }
    }

    pub(crate) fn enable_local_provider(&self) -> LuaResult<()> {
        let switching = self
            .compatible_url
            .lock()
            .map_err(|_| runtime_error("social URL lock poisoned"))?
            .take()
            .is_some()
            || self
                .state
                .lock()
                .map_err(|_| runtime_error("social state lock poisoned"))?
                .friends_store
                .is_some();
        if switching {
            self.retire_platform_jobs()?;
            self.unload_all_avatars()?;
        }
        let mut state = self
            .state
            .lock()
            .map_err(|_| runtime_error("social state lock poisoned"))?;
        if switching {
            if let Some(cache) = state.avatar_cache.take() {
                cache.retire();
            }
            state.provider_generation = state.provider_generation.wrapping_add(1);
            state.connected = false;
            state.local_account_id = "local-player".to_owned();
            state.local_player_name = "Stella Player".to_owned();
            state.local_profile = serde_json::Value::Null;
            state.friends_store = None;
            state.document = LocalSocialDocument::default();
            state.avatars.clear();
            state.avatar_paths.clear();
            state.pending_avatars.clear();
            state.completions.clear();
        }
        if state.persistence_path.is_file() {
            let bytes = fs::read(&state.persistence_path).map_err(runtime_error)?;
            state.document = serde_json::from_slice(&bytes).map_err(runtime_error)?;
        }
        state.local_provider = true;
        persist(&state)
    }

    pub(crate) fn set_compatible_url(&self, url: &str) -> LuaResult<()> {
        let url = url.trim();
        let host = url
            .strip_prefix("http://")
            .or_else(|| url.strip_prefix("https://"));
        let Some(host) = host else {
            return Err(runtime_error(
                "social URL must use the http or https scheme",
            ));
        };
        if host.is_empty() || host.starts_with('/') {
            return Err(runtime_error("social URL is missing a host"));
        }
        {
            let mut current = self
                .compatible_url
                .lock()
                .map_err(|_| runtime_error("social URL lock poisoned"))?;
            if current.as_deref() == Some(url) {
                return Ok(());
            }
            *current = Some(url.to_owned());
        }
        self.retire_platform_jobs()?;
        self.unload_all_avatars()?;
        let mut state = self
            .state
            .lock()
            .map_err(|_| runtime_error("social state lock poisoned"))?;
        if let Some(cache) = state.avatar_cache.take() {
            cache.retire();
        }
        state.provider_generation = state.provider_generation.wrapping_add(1);
        state.local_provider = false;
        state.connected = false;
        state.local_profile = serde_json::Value::Null;
        state.friends_store = None;
        state.document.friends.clear();
        state.avatars.clear();
        state.avatar_paths.clear();
        state.pending_avatars.clear();
        state.completions.clear();
        Ok(())
    }

    fn compatible_url(&self) -> Option<String> {
        self.compatible_url
            .lock()
            .expect("social URL lock poisoned")
            .clone()
    }

    fn is_connected(&self) -> bool {
        self.state
            .lock()
            .expect("social state lock poisoned")
            .connected
    }

    fn local_account_id(&self) -> String {
        let state = self.state.lock().expect("social state lock poisoned");
        if state.connected {
            state.local_account_id.clone()
        } else {
            String::new()
        }
    }

    fn friends(&self) -> Vec<LocalSocialFriend> {
        let state = self.state.lock().expect("social state lock poisoned");
        if let Some(store) = &state.friends_store {
            if store
                .context
                .as_ref()
                .is_some_and(|context| !context.context_is_current())
            {
                return Vec::new();
            }
            return store.friends.values().cloned().collect();
        }
        if (state.local_provider || self.compatible_url().is_some()) && state.connected {
            state.document.friends.clone()
        } else {
            Vec::new()
        }
    }

    fn friend_account_id(&self, query: &str) -> String {
        let query = query.to_lowercase();
        self.friends()
            .into_iter()
            .find(|friend| friend.display_name().to_lowercase().contains(&query))
            .map(|friend| friend.account_id)
            .unwrap_or_default()
    }

    fn is_known_account(&self, account_id: &str) -> bool {
        account_id == self.local_account_id()
            || self
                .friends()
                .iter()
                .any(|friend| friend.account_id == account_id)
    }

    fn connect(&self, lua: &Lua) -> LuaResult<()> {
        if let Some(url) = self.compatible_url() {
            return self.spawn_online("connect", move || {
                OnlineCompletion::Connected(request_online_json(
                    &url,
                    &serde_json::json!({"operation": "connect"}),
                ))
            });
        }
        let mut state = self.state.lock().expect("social state lock poisoned");
        if state.local_provider && !state.connected {
            state.connected = true;
            state.completions.push_back(Completion::Connected);
            self.application_events.post(ApplicationEvent::SocialLocal);
        }
        let native = !state.local_provider;
        drop(state);
        if native {
            self.connect_native_platform(lua)?;
        }
        Ok(())
    }

    fn queue_if_local(&self, completion: Completion) {
        let mut state = self.state.lock().expect("social state lock poisoned");
        if state.local_provider && state.connected {
            state.completions.push_back(completion);
            self.application_events.post(ApplicationEvent::SocialLocal);
        }
    }

    fn set_progress(&self, progress: String) -> LuaResult<()> {
        if let Some(url) = self.compatible_url() {
            let posted = progress.clone();
            return self.spawn_online("progress", move || OnlineCompletion::ProgressPosted {
                progress,
                result: request_online_empty(
                    &url,
                    &serde_json::json!({"operation": "setProgress", "progress": posted}),
                ),
            });
        }
        let mut state = self
            .state
            .lock()
            .map_err(|_| runtime_error("social state lock poisoned"))?;
        if state.local_provider && state.connected {
            state.document.progress = Some(progress);
            persist(&state)?;
        }
        Ok(())
    }

    fn post_score(&self, level: String, score: f32, request_id: String) -> LuaResult<()> {
        if let Some(url) = self.compatible_url() {
            let completion_level = level.clone();
            let completion_request_id = request_id.clone();
            return self.spawn_online("score", move || OnlineCompletion::ScorePosted {
                level: completion_level,
                request_id: completion_request_id,
                result: request_online_empty(
                    &url,
                    &serde_json::json!({
                        "operation": "postScore",
                        "level": level,
                        "points": score,
                        "requestId": request_id,
                    }),
                ),
            });
        }
        let mut state = self
            .state
            .lock()
            .map_err(|_| runtime_error("social state lock poisoned"))?;
        if state.local_provider && state.connected {
            let score = f64::from(score);
            let previous = state.document.scores.entry(level.clone()).or_insert(score);
            *previous = (*previous).max(score);
            persist(&state)?;
            state
                .completions
                .push_back(Completion::ScorePosted { level, request_id });
            self.application_events.post(ApplicationEvent::SocialLocal);
        }
        Ok(())
    }

    fn get_friends_progress(&self, lua: &Lua) -> LuaResult<()> {
        self.synchronize_native_context()?;
        if self
            .state
            .lock()
            .map_err(|_| runtime_error("social state lock poisoned"))?
            .friends_store
            .is_some()
        {
            return self.get_native_friends_progress(lua);
        }
        if let Some(url) = self.compatible_url() {
            return self.spawn_online("friends", move || {
                OnlineCompletion::FriendsProgress(request_online_json(
                    &url,
                    &serde_json::json!({"operation": "getFriendsProgress"}),
                ))
            });
        }
        self.queue_if_local(Completion::FriendsProgress);
        Ok(())
    }

    fn fetch_leaderboard(&self, level: String, request_id: String) -> LuaResult<()> {
        if let Some(url) = self.compatible_url() {
            let completion_level = level.clone();
            let completion_request_id = request_id.clone();
            return self.spawn_online("leaderboard", move || OnlineCompletion::Leaderboard {
                level: completion_level,
                request_id: completion_request_id,
                result: request_online_json(
                    &url,
                    &serde_json::json!({
                        "operation": "fetchLeaderboard",
                        "level": level,
                        "requestId": request_id,
                    }),
                ),
            });
        }
        self.queue_if_local(Completion::Leaderboard { level, request_id });
        Ok(())
    }

    fn spawn_online(
        &self,
        operation: &str,
        task: impl FnOnce() -> OnlineCompletion + Send + 'static,
    ) -> LuaResult<()> {
        let generation = self
            .state
            .lock()
            .expect("social state lock poisoned")
            .provider_generation;
        let queue = Arc::clone(&self.online_completions);
        let application_events = self.application_events.clone();
        std::thread::Builder::new()
            .name(format!("stella-social-{operation}"))
            .spawn(move || {
                let completion = task();
                let mut completions = queue
                    .lock()
                    .expect("social online completion lock poisoned");
                completions.push_back((generation, completion));
                application_events.post(ApplicationEvent::SocialOnline);
            })
            .map_err(|_| runtime_error("Creating thread failed"))?;
        Ok(())
    }

    fn pop_pending(&self) -> Option<Completion> {
        self.state
            .lock()
            .expect("social state lock poisoned")
            .completions
            .pop_front()
    }

    fn pop_online_pending(&self) -> Option<OnlineCompletion> {
        let (generation, completion) = self
            .online_completions
            .lock()
            .expect("social online completion lock poisoned")
            .pop_front()?;
        (matches!(&completion, OnlineCompletion::PlatformSdk { .. })
            || generation
                == self
                    .state
                    .lock()
                    .expect("social state lock poisoned")
                    .provider_generation)
            .then_some(completion)
    }

    pub(crate) fn discard_local_completion(&self) {
        let _ = self.pop_pending();
    }

    pub(crate) fn discard_online_completion(&self) {
        let _ = self.pop_online_pending();
    }
}

const ONLINE_REQUEST_TIMEOUT: Duration = Duration::from_secs(30);

fn request_online_response(
    url: &str,
    payload: &serde_json::Value,
) -> Result<ureq::http::Response<ureq::Body>, String> {
    let body = serde_json::to_string(payload).map_err(|error| error.to_string())?;
    ureq::Agent::config_builder()
        .http_status_as_error(false)
        .timeout_global(Some(ONLINE_REQUEST_TIMEOUT))
        .build()
        .new_agent()
        .post(url)
        .header("Content-Type", "application/json")
        .send(body)
        .map_err(|error| error.to_string())
}

fn request_online_json<T: for<'de> Deserialize<'de>>(
    url: &str,
    payload: &serde_json::Value,
) -> Result<T, String> {
    let mut response = request_online_response(url, payload)?;
    let status = response.status().as_u16();
    let mut body = Vec::new();
    response
        .body_mut()
        .as_reader()
        .read_to_end(&mut body)
        .map_err(|error| error.to_string())?;
    if status != 200 {
        return Err(format!(
            "social HTTP {status}: {}",
            String::from_utf8_lossy(&body)
        ));
    }
    serde_json::from_slice(&body).map_err(|error| error.to_string())
}

fn request_online_empty(url: &str, payload: &serde_json::Value) -> Result<(), String> {
    let mut response = request_online_response(url, payload)?;
    let status = response.status().as_u16();
    let mut body = Vec::new();
    response
        .body_mut()
        .as_reader()
        .read_to_end(&mut body)
        .map_err(|error| error.to_string())?;
    if status == 200 {
        Ok(())
    } else {
        Err(format!(
            "social HTTP {status}: {}",
            String::from_utf8_lossy(&body)
        ))
    }
}

fn persist(state: &SocialState) -> LuaResult<()> {
    if !state.local_provider {
        return Ok(());
    }
    if let Some(parent) = state.persistence_path.parent() {
        fs::create_dir_all(parent).map_err(runtime_error)?;
    }
    let bytes = serde_json::to_vec_pretty(&state.document).map_err(runtime_error)?;
    fs::write(&state.persistence_path, bytes).map_err(runtime_error)
}

pub(super) struct SkynestServices {
    pub(super) account: super::skynest_account::SkynestAccountRuntime,
    pub(super) storage: super::skynest_storage::SkynestStorageRuntime,
}

pub(super) fn install(
    lua: &Lua,
    globals: &mlua::Table,
    persistence_path: PathBuf,
    data_root: Arc<PathBuf>,
    resource_runtime: Arc<Mutex<ResourceRuntime>>,
    application_events: ApplicationEventScheduler,
    skynest: SkynestServices,
) -> LuaResult<SocialRuntime> {
    let runtime = SocialRuntime::new(
        persistence_path,
        data_root,
        resource_runtime,
        application_events,
        skynest.account,
        skynest.storage,
    );
    let social = lua.create_table()?;
    let login_runtime = runtime.clone();
    lua.set_named_registry_value(
        friends_store::LOGIN_REGISTRY_KEY,
        lua.create_function(move |lua, ()| login_runtime.initialize_friends_store(lua))?,
    )?;
    let logout_runtime = runtime.clone();
    lua.set_named_registry_value(
        friends_store::LOGOUT_REGISTRY_KEY,
        lua.create_function(move |_, network: i32| {
            logout_runtime.logout_account_platform(network)
        })?,
    )?;
    let connected_runtime = runtime.clone();
    social.set(
        "native_isConnectedToSocialNetwork",
        lua.create_function(move |_, _: MultiValue| Ok(connected_runtime.is_connected()))?,
    )?;
    let connect_runtime = runtime.clone();
    social.set(
        "native_connectToSocialNetwork",
        lua.create_function(move |lua, _: MultiValue| connect_runtime.connect(lua))?,
    )?;
    let friends_runtime = runtime.clone();
    social.set(
        "native_getFriendsProgress",
        lua.create_function(move |lua, _: MultiValue| friends_runtime.get_friends_progress(lua))?,
    )?;
    let unload_all_runtime = runtime.clone();
    social.set(
        "native_unloadAllAvatars",
        lua.create_function(move |_, _: MultiValue| unload_all_runtime.unload_all_avatars())?,
    )?;
    let score_runtime = runtime.clone();
    social.set(
        "native_postScores",
        lua.create_function(move |_, args: MultiValue| {
            let level = native_required_string(&args, 0, "native_postScores")?;
            let score = native_required_number(&args, 1, "native_postScores")? as f32;
            let request_id = native_required_string(&args, 2, "native_postScores")?;
            score_runtime.post_score(level, score, request_id)
        })?,
    )?;
    let leaderboard_runtime = runtime.clone();
    social.set(
        "native_fetchLeaderboard",
        lua.create_function(move |_, args: MultiValue| {
            let level = native_required_string(&args, 0, "native_fetchLeaderboard")?;
            let request_id = native_required_string(&args, 1, "native_fetchLeaderboard")?;
            leaderboard_runtime.fetch_leaderboard(level, request_id)
        })?,
    )?;
    let progress_runtime = runtime.clone();
    social.set(
        "native_setProgress",
        lua.create_function(move |_, args: MultiValue| {
            progress_runtime.set_progress(native_required_string(&args, 0, "native_setProgress")?)
        })?,
    )?;
    let load_runtime = runtime.clone();
    social.set(
        "native_loadAvatar",
        lua.create_function(move |lua, args: MultiValue| {
            let account_id = native_required_string(&args, 0, "native_loadAvatar")?;
            if load_runtime.load_avatar(&account_id)? == LoadResult::Loaded {
                lua.globals()
                    .get::<mlua::Table>("SocialManager")?
                    .get::<mlua::Function>("onAvatarImageLoaded")?
                    .call::<()>(account_id)?;
            }
            Ok(())
        })?,
    )?;
    let unload_runtime = runtime.clone();
    social.set(
        "native_unloadAvatar",
        lua.create_function(move |_, args: MultiValue| {
            let account_id = native_required_string(&args, 0, "native_unloadAvatar")?;
            unload_runtime.unload_avatar(&account_id)
        })?,
    )?;
    social.set(
        "native_getSocialNetworkName",
        lua.create_function(|_, _: MultiValue| Ok("facebook"))?,
    )?;
    social.set("native_getFriendAccountId", {
        let friend_runtime = runtime.clone();
        lua.create_function(move |_, args: MultiValue| {
            let query = native_required_string(&args, 0, "native_getFriendAccountId")?;
            Ok(friend_runtime.friend_account_id(&query))
        })?
    })?;
    let local_user_runtime = runtime.clone();
    social.set(
        "native_getLocalUserAccountId",
        lua.create_function(move |_, _: MultiValue| Ok(local_user_runtime.local_account_id()))?,
    )?;
    let get_friends_runtime = runtime.clone();
    social.set(
        "native_getFriends",
        lua.create_function(move |lua, _: MultiValue| {
            let result = lua.create_table()?;
            let mut output_index = 1;
            for friend in get_friends_runtime.friends() {
                if friend.display_name().is_empty() {
                    continue;
                }
                let entry = lua.create_table()?;
                let name = friend.display_name().to_owned();
                entry.set("accountId", friend.account_id)?;
                entry.set("name", name)?;
                result.raw_set(output_index, entry)?;
                output_index += 1;
            }
            Ok(result)
        })?,
    )?;
    globals.set("SocialManager", social)?;
    Ok(runtime)
}

/// Deliver local-provider results with the exact native callback arity
/// recovered from sub_1000C50D0, sub_1000C3A5C, sub_1000C4998 and
/// sub_1000C4648.
pub(super) fn dispatch_account_login(lua: &Lua) -> LuaResult<()> {
    if let Some(callback) =
        lua.named_registry_value::<Option<mlua::Function>>(friends_store::LOGIN_REGISTRY_KEY)?
    {
        callback.call::<()>(())?;
    }
    Ok(())
}

pub(super) fn dispatch_account_logout(lua: &Lua, network: Option<SocialNetwork>) -> LuaResult<()> {
    if let Some(callback) =
        lua.named_registry_value::<Option<mlua::Function>>(friends_store::LOGOUT_REGISTRY_KEY)?
    {
        callback.call::<()>(network.map_or(0, |network| network as i32))?;
    }
    Ok(())
}

pub(crate) fn dispatch_local_completion(lua: &Lua, runtime: &SocialRuntime) -> LuaResult<()> {
    runtime.synchronize_native_context()?;
    let native = lua.globals().get::<mlua::Table>("SocialManager")?;
    let Some(completion) = runtime.pop_pending() else {
        return Ok(());
    };
    match completion {
        Completion::Connected => {
            native
                .get::<mlua::Function>("onSocialNetworkConnected")?
                .call::<()>("facebook")?;
        }
        Completion::FriendsProgress => {
            let friends = lua.create_table()?;
            let mut output_index = 1;
            for friend in runtime.friends() {
                if friend.account_id.is_empty() {
                    continue;
                }
                let entry = lua.create_table()?;
                let nickname = friend.leaderboard_name().to_owned();
                entry.set("accountId", friend.account_id)?;
                entry.set("nickname", nickname)?;
                entry.set("progress", friend.progress)?;
                friends.raw_set(output_index, entry)?;
                output_index += 1;
            }
            native
                .get::<mlua::Function>("onFriendsProgressUpdated")?
                .call::<()>((true, friends))?;
        }
        Completion::Leaderboard { level, request_id } => {
            let leaderboard = lua.create_table()?;
            let state = runtime
                .state
                .lock()
                .map_err(|_| runtime_error("social state lock poisoned"))?;
            let mut entries = Vec::new();
            if let Some(score) = state.document.scores.get(&level).copied() {
                entries.push((
                    score,
                    "Stella Player".to_owned(),
                    "local-player".to_owned(),
                    true,
                ));
            }
            for friend in &state.document.friends {
                if let Some(score) = friend.scores.get(&level).copied() {
                    entries.push((
                        score,
                        friend.leaderboard_name().to_owned(),
                        friend.account_id.clone(),
                        false,
                    ));
                }
            }
            drop(state);
            entries.sort_by(|left, right| {
                right
                    .0
                    .total_cmp(&left.0)
                    .then_with(|| left.2.cmp(&right.2))
            });
            for (index, (score, nickname, account_id, local_player)) in
                entries.into_iter().enumerate()
            {
                let player = lua.create_table()?;
                player.set("points", score)?;
                player.set("rank", index + 1)?;
                player.set("nickname", nickname)?;
                player.set("accountId", account_id)?;
                player.set("localPlayer", local_player)?;
                leaderboard.raw_set(index + 1, player)?;
            }
            native
                .get::<mlua::Function>("onLeaderboardFetched")?
                .call::<()>((true, level, leaderboard, request_id))?;
        }
        Completion::ScorePosted { level, request_id } => {
            native
                .get::<mlua::Function>("onScorePosted")?
                .call::<()>((true, level, request_id))?;
        }
        Completion::AvatarDownloaded { account_id } => {
            if runtime.finish_avatar_download(&account_id)? {
                native
                    .get::<mlua::Function>("onAvatarDownloadedToCache")?
                    .call::<()>(account_id)?;
            }
        }
        Completion::AvatarResult {
            generation,
            accounts,
            result,
        } => {
            for account_id in accounts {
                let success = {
                    let mut state = runtime
                        .state
                        .lock()
                        .map_err(|_| runtime_error("social state lock poisoned"))?;
                    if state.provider_generation != generation {
                        return Ok(());
                    }
                    if state.avatars.get(&account_id) != Some(&AvatarStage::Downloading) {
                        continue;
                    }
                    match &result {
                        Ok(path) => {
                            state.avatar_paths.insert(account_id.clone(), path.clone());
                            state
                                .avatars
                                .insert(account_id.clone(), AvatarStage::Cached);
                            true
                        }
                        Err(_) => {
                            state.avatars.insert(account_id.clone(), AvatarStage::New);
                            false
                        }
                    }
                };
                if success {
                    native
                        .get::<mlua::Function>("onAvatarDownloadedToCache")?
                        .call::<()>(account_id)?;
                }
            }
        }
    }
    Ok(())
}

pub(crate) fn dispatch_online_completion(lua: &Lua, runtime: &SocialRuntime) -> LuaResult<()> {
    runtime.synchronize_native_context()?;
    let native = lua.globals().get::<mlua::Table>("SocialManager")?;
    let Some(completion) = runtime.pop_online_pending() else {
        return Ok(());
    };
    match completion {
        OnlineCompletion::PlatformSdk { current, task } => {
            let active = *current.lock().expect("platform SDK lifetime lock poisoned");
            if active {
                task.run();
            }
        }
        OnlineCompletion::Platform(completion) => runtime.finish_platform(lua, *completion)?,
        OnlineCompletion::NativeFriends {
            client,
            result,
            network,
        } => {
            #[cfg(test)]
            {
                runtime.state.lock().unwrap().native_friends_completions += 1;
            }
            if !client.is_current() {
                return Ok(());
            }
            match result {
                Ok(friends) => runtime.finish_native_friends(lua, &client, friends, network)?,
                Err(error) => {
                    // Store's native failure callback is nullsub_334: retain
                    // previous records and do not invent a Lua completion.
                    eprintln!(
                        "native friends refresh failed ({}): {}",
                        error.code, error.detail
                    );
                }
            }
        }
        OnlineCompletion::AvatarFetched { url, result } => {
            let mut state = runtime
                .state
                .lock()
                .map_err(|_| runtime_error("social state lock poisoned"))?;
            let Some(accounts) = state.pending_avatars.remove(&url) else {
                return Ok(());
            };
            let mut result = result;
            if let (Ok(path), Some(cache)) = (&mut result, &state.avatar_cache) {
                if path.is_file() {
                    cache.touch(path.clone()).map_err(runtime_error)?;
                } else {
                    *path = PathBuf::new();
                }
            }
            let generation = state.provider_generation;
            state.completions.push_back(Completion::AvatarResult {
                generation,
                accounts,
                result,
            });
            runtime
                .application_events
                .post(ApplicationEvent::SocialLocal);
        }
        OnlineCompletion::Connected(result) => {
            let network = match result {
                Ok(response) if !response.local_player.account_id.is_empty() => {
                    let mut state = runtime
                        .state
                        .lock()
                        .map_err(|_| runtime_error("social state lock poisoned"))?;
                    state.connected = true;
                    state.local_account_id = response.local_player.account_id;
                    state.local_profile = response.local_player.profile;
                    state.local_player_name = if response.local_player.name.is_empty() {
                        "n/a".to_owned()
                    } else {
                        response.local_player.name
                    };
                    state.document.friends = response.friends;
                    normalized_social_network(&response.network).to_owned()
                }
                Ok(_) | Err(_) => {
                    let mut state = runtime
                        .state
                        .lock()
                        .map_err(|_| runtime_error("social state lock poisoned"))?;
                    state.connected = false;
                    "unknown".to_owned()
                }
            };
            native
                .get::<mlua::Function>("onSocialNetworkConnected")?
                .call::<()>(network)?;
        }
        OnlineCompletion::FriendsProgress(result) => {
            let (success, friends) = match result {
                Ok(response) => {
                    runtime
                        .state
                        .lock()
                        .map_err(|_| runtime_error("social state lock poisoned"))?
                        .document
                        .friends = response.friends.clone();
                    (true, response.friends)
                }
                Err(_) => (false, Vec::new()),
            };
            native
                .get::<mlua::Function>("onFriendsProgressUpdated")?
                .call::<()>((success, friends_progress_table(lua, friends)?))?;
        }
        OnlineCompletion::Leaderboard {
            level,
            request_id,
            result,
        } => match result {
            Ok(response) => {
                native
                    .get::<mlua::Function>("onLeaderboardFetched")?
                    .call::<()>((
                        true,
                        level,
                        online_leaderboard_table(lua, response.entries)?,
                        request_id,
                    ))?;
            }
            Err(_) => {
                // sub_1000C4848 uses the native failure overload with
                // exactly `(false, level)` and intentionally drops the
                // retained request id.
                native
                    .get::<mlua::Function>("onLeaderboardFetched")?
                    .call::<()>((false, level))?;
            }
        },
        OnlineCompletion::ScorePosted {
            level,
            request_id,
            result,
        } => {
            native.get::<mlua::Function>("onScorePosted")?.call::<()>((
                result.is_ok(),
                level,
                request_id,
            ))?;
        }
        OnlineCompletion::ProgressPosted { progress, result } => {
            if result.is_ok() {
                runtime
                    .state
                    .lock()
                    .map_err(|_| runtime_error("social state lock poisoned"))?
                    .document
                    .progress = Some(progress);
            }
        }
    }
    Ok(())
}

fn normalized_social_network(network: &str) -> &str {
    match network {
        "facebook" => "facebook",
        "sinaweibo" => "sinaweibo",
        _ => "unknown",
    }
}

fn friends_progress_table(lua: &Lua, friends: Vec<LocalSocialFriend>) -> LuaResult<mlua::Table> {
    let table = lua.create_table()?;
    let mut output_index = 1;
    for friend in friends {
        if friend.account_id.is_empty() {
            continue;
        }
        let entry = lua.create_table()?;
        let nickname = friend.leaderboard_name().to_owned();
        entry.set("accountId", friend.account_id)?;
        entry.set("nickname", nickname)?;
        entry.set("progress", friend.progress)?;
        table.raw_set(output_index, entry)?;
        output_index += 1;
    }
    Ok(table)
}

fn online_leaderboard_table(
    lua: &Lua,
    entries: Vec<OnlineLeaderboardEntry>,
) -> LuaResult<mlua::Table> {
    let table = lua.create_table()?;
    for (index, entry) in entries.into_iter().enumerate() {
        let player = lua.create_table()?;
        player.set("points", entry.points)?;
        player.set("rank", entry.rank)?;
        player.set("nickname", entry.nickname)?;
        player.set("accountId", entry.account_id)?;
        player.set("localPlayer", entry.local_player)?;
        table.raw_set(index + 1, player)?;
    }
    Ok(table)
}
