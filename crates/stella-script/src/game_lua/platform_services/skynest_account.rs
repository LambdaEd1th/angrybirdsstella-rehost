//! Rovio Account/Identity Level 2 boundary for the retired Skynest backend.

use crate::*;
use mlua::{IntoLuaMulti, RegistryKey};
use serde::{Deserialize, Serialize};
use std::{
    cell::{Cell, RefCell},
    collections::VecDeque,
    fmt::Write as _,
    fs,
    io::Read,
    rc::Rc,
    time::Duration,
};

pub(super) mod avatar_support;
mod endpoint;
pub(in crate::game_lua::platform_services) mod friends_support;
mod identifiers;
mod interactive;
mod os_version;
#[cfg(test)]
mod queue_tests;
mod sdk_logger;
mod session;
pub(super) use session::{RegistryNamespace, StoreError};
mod signing;
mod storage_bridge;
mod timezone;
use endpoint::IdentityEndpoint;
use interactive::{InteractiveCompletion, InteractiveState};
use session::{IdentitySession, ProviderLevel, RequestOwner};
use signing::ClientSigning;
pub(super) use storage_bridge::{IdentityLifetime, StorageIdentity};

const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);

enum Completion {
    LoginUnavailable {
        login_job: u64,
    },
    LoginSucceeded {
        login_job: u64,
    },
    Interactive(Box<InteractiveCompletion>),
    ValidateNickname {
        callback: RegistryKey,
        is_valid: bool,
    },
}

impl Completion {
    fn interactive(value: InteractiveCompletion) -> Self {
        Self::Interactive(Box::new(value))
    }
}

// Capture before spawning, retain across every scheduler stage. Level1 owners
// contain only the account epoch; Level2 also identifies credential replacement.
// Neither owner is the native text-edit generation.
struct Queued<T> {
    owner: RequestOwner,
    value: T,
}

#[derive(Clone)]
struct IdentityConfig {
    endpoint: IdentityEndpoint,
    client_id: String,
    signing: ClientSigning,
    identifiers: Arc<identifiers::Identifiers>,
}

#[derive(Clone)]
struct AccessResponse {
    access_token: String,
    refresh_token: String,
    // Native flat constructor computes this before any UI/profile continuation.
    absolute_expiry: i64,
    segment: Option<String>,
}

#[derive(Clone, Debug, Deserialize)]
struct ProfileResponse {
    #[serde(skip)]
    raw: serde_json::Value,
    #[serde(skip)]
    avatar_assets: Vec<session::AvatarAsset>,
    #[serde(skip)]
    avatar_paths: BTreeMap<i32, String>,
    #[serde(default, rename = "publicAccountId")]
    public_account_id: String,
    #[serde(default)]
    personal: PersonalProfile,
    // Retained native collection for the remaining UserProfile consumers;
    // this checkpoint exposes only the separately computed active projection.
    #[allow(dead_code)]
    #[serde(default, rename = "socialNetworks")]
    social_networks: Vec<serde_json::Value>,
    #[serde(skip)]
    active_external_id: String,
    #[serde(skip)]
    active_social_network: Option<SocialNetwork>,
    #[serde(skip)]
    active_social_name: String,
    #[serde(skip)]
    connected_to_social_network: bool,
}

impl ProfileResponse {
    // Native 100743D18: an absent account is class2, a guest is class0;
    // otherwise a nonempty merged email or selected external id gives class1.
    fn is_guest(&self) -> bool {
        !self.public_account_id.is_empty()
            && self.personal.email.is_empty()
            && self.active_external_id.is_empty()
    }
}

#[derive(Clone, Debug, Default, Deserialize)]
struct PersonalProfile {
    #[serde(default, rename = "nickName")]
    nickname: String,
    #[serde(default)]
    email: String,
}

#[derive(Clone, Debug, Deserialize)]
struct NicknameResponse {
    #[serde(rename = "isValid")]
    is_valid: bool,
    #[serde(default, rename = "validationMsg")]
    validation_message: String,
}

enum OnlineCompletion {
    Login {
        login_job: u64,
        result: Result<ProfileResponse, String>,
    },
    Interactive(interactive::OnlineResult),
    ValidateNickname {
        request_id: u64,
        result: Result<NicknameResponse, String>,
    },
}

/// Identity-provider state and retained application-thread completions.
#[derive(Clone)]
pub(crate) struct SkynestAccountRuntime {
    state: Arc<Mutex<OfflineState>>,
    session: IdentitySession,
    completions: Rc<RefCell<VecDeque<Queued<Completion>>>>,
    compatible_url: Arc<Mutex<Option<IdentityEndpoint>>>,
    client_id: Arc<Mutex<String>>,
    client_signing: Arc<Mutex<ClientSigning>>,
    online_completions: Arc<Mutex<VecDeque<Queued<OnlineCompletion>>>>,
    callbacks: Rc<RefCell<BTreeMap<u64, RegistryKey>>>,
    next_request_id: Rc<Cell<u64>>,
    active_login_job: Rc<Cell<Option<u64>>>,
    application_events: ApplicationEventScheduler,
    interactive: Rc<RefCell<InteractiveState>>,
    registry_root: PathBuf,
    identifiers: Arc<identifiers::Identifiers>,
    bound_registry: Rc<RefCell<Option<PathBuf>>>,
}

impl SkynestAccountRuntime {
    fn new(
        state: Arc<Mutex<OfflineState>>,
        application_events: ApplicationEventScheduler,
        identifiers: Arc<identifiers::Identifiers>,
    ) -> Self {
        let session = state
            .lock()
            .expect("Skynest state lock poisoned")
            .identity_session
            .clone();
        session.bind_success_events(application_events.clone());
        let registry_root = state
            .lock()
            .expect("Skynest state lock poisoned")
            .persistence_path
            .parent()
            .expect("AppData service path")
            .join("identity-providers");
        Self {
            session,
            state,
            completions: Rc::new(RefCell::new(VecDeque::new())),
            compatible_url: Arc::new(Mutex::new(None)),
            client_id: Arc::new(Mutex::new("Purple".to_owned())),
            client_signing: Arc::default(),
            online_completions: Arc::new(Mutex::new(VecDeque::new())),
            callbacks: Rc::new(RefCell::new(BTreeMap::new())),
            next_request_id: Rc::new(Cell::new(1)),
            active_login_job: Rc::new(Cell::new(None)),
            application_events,
            interactive: Rc::new(RefCell::new(InteractiveState::default())),
            registry_root,
            identifiers,
            bound_registry: Rc::new(RefCell::new(None)),
        }
    }

    fn begin_login(&self) -> LuaResult<()> {
        if let Some(config) = self.prepared_online_config()? {
            let login_job = self.begin_login_job()?;
            let queue = Arc::clone(&self.online_completions);
            let session = self.session.clone();
            let owner = session.request_owner(ProviderLevel::Level2);
            let result = spawn_online(
                queue,
                self.application_events.clone(),
                ApplicationEvent::SkynestAccountOnline,
                owner,
                move |owner| OnlineCompletion::Login {
                    login_job,
                    result: session
                        .acquire_session_for_owner(&config, owner)
                        .map_err(|error| error.to_string()),
                },
            );
            if result.is_err() {
                self.finish_stale_login_job(login_job);
            }
            return result;
        }
        let login_job = self.begin_login_job()?;
        let completion = {
            let state = self
                .state
                .lock()
                .map_err(|_| runtime_error("Skynest state lock poisoned"))?;
            if state.local_provider {
                Completion::LoginSucceeded { login_job }
            } else {
                Completion::LoginUnavailable { login_job }
            }
        };
        self.queue_local_owned(
            self.session.request_owner(ProviderLevel::Level2),
            completion,
        );
        Ok(())
    }

    fn begin_login_job(&self) -> LuaResult<u64> {
        let id = self.next_request_id.get();
        self.next_request_id.set(id.wrapping_add(1).max(1));
        self.state
            .lock()
            .map_err(|_| runtime_error("Skynest state lock poisoned"))?
            .login_in_progress = true;
        self.active_login_job.set(Some(id));
        Ok(id)
    }

    fn finish_stale_login_job(&self, id: u64) {
        // Host cancellation terminal state: do not leave Progress stuck when
        // this request's renewal discovers another identity. A newer login
        // owns its own progress byte and must not be completed by this job.
        if self.complete_login_job(id) {
            self.state
                .lock()
                .expect("Skynest state lock poisoned")
                .login_in_progress = false;
        }
    }

    fn complete_login_job(&self, id: u64) -> bool {
        if self.active_login_job.get() != Some(id) {
            return false;
        }
        self.active_login_job.set(None);
        true
    }

    fn queue_local(&self, completion: Completion) {
        self.queue_local_owned(
            self.session.request_owner(ProviderLevel::Level1),
            completion,
        );
    }

    fn queue_local_owned(&self, owner: RequestOwner, completion: Completion) {
        self.completions.borrow_mut().push_back(Queued {
            owner,
            value: completion,
        });
        self.application_events
            .post(ApplicationEvent::SkynestAccountLocal);
    }

    pub(crate) fn enable_local_provider(&self) -> LuaResult<()> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| runtime_error("Skynest state lock poisoned"))?;
        state.enable_local_provider()
    }

    pub(crate) fn set_compatible_url(&self, url: &str) -> LuaResult<()> {
        let endpoint = IdentityEndpoint::parse(url).map_err(runtime_error)?;
        if self
            .compatible_url
            .lock()
            .map_err(|_| runtime_error("identity URL lock poisoned"))?
            .as_ref()
            == Some(&endpoint)
        {
            return Ok(());
        }
        self.detach_identity()?;
        *self
            .compatible_url
            .lock()
            .map_err(|_| runtime_error("identity URL lock poisoned"))? = Some(endpoint);
        Ok(())
    }

    pub(crate) fn set_compatible_client(
        &self,
        client_id: Option<&str>,
        client_signature: Option<&str>,
        client_salt: Option<&str>,
    ) -> LuaResult<()> {
        let signature = client_signature.unwrap_or_default().trim().to_owned();
        let salt = client_salt.unwrap_or_default().trim().to_owned();
        let signing = ClientSigning::literal(signature, salt);
        let current_id = self
            .client_id
            .lock()
            .map_err(|_| runtime_error("identity client-id lock poisoned"))?
            .clone();
        let client_id = nonempty(client_id).unwrap_or(current_id.clone());
        if client_id == current_id
            && signing
                == *self
                    .client_signing
                    .lock()
                    .map_err(|_| runtime_error("identity client-signing lock poisoned"))?
        {
            return Ok(());
        }
        self.detach_identity()?;
        *self
            .client_id
            .lock()
            .map_err(|_| runtime_error("identity client-id lock poisoned"))? = client_id;
        *self
            .client_signing
            .lock()
            .map_err(|_| runtime_error("identity client-signing lock poisoned"))? = signing;
        Ok(())
    }

    pub(crate) fn set_compatible_signing_key(&self, key: &[u8]) -> LuaResult<()> {
        let signing = ClientSigning::generated(key);
        if *self
            .client_signing
            .lock()
            .map_err(|_| runtime_error("identity client-signing lock poisoned"))?
            == signing
        {
            return Ok(());
        }
        self.detach_identity()?;
        *self
            .client_signing
            .lock()
            .map_err(|_| runtime_error("identity client-signing lock poisoned"))? = signing;
        Ok(())
    }

    fn online_config(&self) -> Option<IdentityConfig> {
        Some(IdentityConfig {
            identifiers: self.identifiers.clone(),
            endpoint: self
                .compatible_url
                .lock()
                .expect("identity URL lock poisoned")
                .clone()?,
            client_id: self
                .client_id
                .lock()
                .expect("identity client-id lock poisoned")
                .clone(),
            signing: self
                .client_signing
                .lock()
                .expect("identity client-signing lock poisoned")
                .clone(),
        })
    }

    fn queue_nickname_validation(&self, callback: RegistryKey, is_valid: bool) {
        self.queue_local_owned(
            self.session.request_owner(ProviderLevel::Level2),
            Completion::ValidateNickname { callback, is_valid },
        );
    }

    fn prepared_online_config(&self) -> LuaResult<Option<IdentityConfig>> {
        let Some(config) = self.online_config() else {
            return Ok(None);
        };
        let path = registry_path(&self.registry_root, &config);
        if self.bound_registry.borrow().as_ref() != Some(&path) {
            let store = session::RegistryStore::open(path.clone()).map_err(runtime_error)?;
            self.session
                .bind_store(Arc::new(store))
                .map_err(runtime_error)?;
            *self.bound_registry.borrow_mut() = Some(path);
        }
        Ok(Some(config))
    }

    fn queue_online_nickname_validation(
        &self,
        lua: &Lua,
        callback: mlua::Function,
        nickname: String,
        config: IdentityConfig,
    ) -> LuaResult<()> {
        let request_id = self.next_request_id.get();
        self.next_request_id.set(request_id.wrapping_add(1).max(1));
        self.callbacks
            .borrow_mut()
            .insert(request_id, lua.create_registry_value(callback)?);
        let session = self.session.clone();
        let owner = session.request_owner(ProviderLevel::Level2);
        let queue = Arc::clone(&self.online_completions);
        if let Err(error) = spawn_online(
            queue,
            self.application_events.clone(),
            ApplicationEvent::SkynestAccountOnline,
            owner,
            move |owner| OnlineCompletion::ValidateNickname {
                request_id,
                result: request_nickname_validation(&config, &session, owner, &nickname),
            },
        ) {
            if let Some(callback) = self.callbacks.borrow_mut().remove(&request_id) {
                lua.remove_registry_value(callback)?;
            }
            return Err(error);
        }
        Ok(())
    }

    fn pop_pending(&self) -> Option<Queued<Completion>> {
        self.completions.borrow_mut().pop_front()
    }

    fn pop_online_pending(&self) -> Option<Queued<OnlineCompletion>> {
        self.online_completions
            .lock()
            .expect("identity online completion queue lock poisoned")
            .pop_front()
    }

    pub(crate) fn discard_local_completion(&self) {
        let _ = self.pop_pending();
    }

    pub(crate) fn discard_online_completion(&self) {
        if let Some(Queued {
            value: OnlineCompletion::ValidateNickname { request_id, .. },
            ..
        }) = self.pop_online_pending()
        {
            let _ = self.callbacks.borrow_mut().remove(&request_id);
        }
    }

    fn clear_identity(&self, lua: &Lua) -> LuaResult<()> {
        // An explicit logout after startup must clear the selected provider's
        // durable cache even if no online operation has bound it yet. Failure
        // to open that cache must not skip invalidating live state/UI owners.
        let prepared = self.prepared_online_config();
        // Identity74969C selects the active external profile before base
        // logout671B2C erases it. No external/disconnect HTTP belongs here.
        let platform = super::social::dispatch_account_logout(
            lua,
            self.session
                .profile()
                .and_then(|profile| profile.active_social_network),
        );
        let result = self.session.logout();
        let cleared_owner = self.clear_account_owner();
        prepared?;
        platform?;
        cleared_owner?;
        result.map(|_| ()).map_err(runtime_error)
    }

    fn detach_identity(&self) -> LuaResult<()> {
        self.session.detach_store();
        *self.bound_registry.borrow_mut() = None;
        self.clear_account_owner()
    }

    pub(crate) fn submit_sdk_log(&self, level: SdkLogLevel, tag: &str, message: &str) -> bool {
        self.session
            .sdk_logger
            .submit(&self.session, level, tag, message)
    }

    pub(crate) fn sdk_log_snapshot(&self) -> SdkLogSnapshot {
        self.session.sdk_logger.snapshot()
    }

    pub(crate) fn check_sdk_log_thread(&self) -> LuaResult<()> {
        let snapshot = self.sdk_log_snapshot();
        if snapshot.fatal {
            return Err(runtime_error(format!(
                "SDK log thread terminated: {}",
                snapshot
                    .last_error
                    .as_deref()
                    .unwrap_or("unknown upload failure")
            )));
        }
        Ok(())
    }

    pub(crate) fn flush_sdk_log_timer(&self, generation: u64) {
        self.session
            .sdk_logger
            .flush_timer(&self.session, generation);
    }

    fn clear_account_owner(&self) -> LuaResult<()> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| runtime_error("Skynest state lock poisoned"))?;
        state.logged_in = false;
        state.login_in_progress = false;
        drop(state);
        self.active_login_job.set(None);
        self.clear_interactive_owner();
        Ok(())
    }

    #[cfg(test)]
    pub(crate) fn registry_path_for_test(&self) -> PathBuf {
        registry_path(
            &self.registry_root,
            &self.online_config().expect("configured test provider"),
        )
    }

    #[cfg(test)]
    pub(crate) fn seed_test_tokens(
        &self,
        level1: bool,
        access: &str,
        refresh: &str,
        segment: &str,
    ) {
        self.prepared_online_config()
            .expect("isolated fixture registry");
        let tokens = AccessResponse {
            access_token: access.to_owned(),
            refresh_token: refresh.to_owned(),
            segment: Some(segment.to_owned()),
            absolute_expiry: i64::MAX,
        };
        if level1 {
            self.session.install_level1_flat(&tokens);
        } else {
            self.session.install_flat(&tokens);
        }
    }
}

fn registry_path(root: &std::path::Path, config: &IdentityConfig) -> PathBuf {
    // Native has one fixed service. The desktop permits explicit providers:
    // isolate native-format registries without importing another origin's
    // credentials. Length-prefixed exact bytes avoid hash collisions/traversal.
    let service = config.endpoint.request_url("");
    let mut scope = Vec::new();
    for field in [service.as_bytes(), config.client_id.as_bytes()] {
        scope.extend_from_slice(&(field.len() as u64).to_le_bytes());
        scope.extend_from_slice(field);
    }
    let mut hex = String::with_capacity(scope.len() * 2);
    for byte in scope {
        write!(&mut hex, "{byte:02x}").expect("String write");
    }
    let mut path = root.to_path_buf();
    for part in hex.as_bytes().chunks(128) {
        path.push(std::str::from_utf8(part).expect("hex ASCII"));
    }
    path.join("fusion.registry")
}

fn nonempty(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
}

fn form_component(value: &str) -> String {
    let mut encoded = String::with_capacity(value.len());
    for byte in value.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => {
                encoded.push(char::from(byte));
            }
            b' ' => encoded.push('+'),
            _ => write!(encoded, "%{byte:02X}").expect("writing to String cannot fail"),
        }
    }
    encoded
}

fn form_body(fields: &[(&str, String)]) -> String {
    fields
        .iter()
        .map(|(name, value)| format!("{}={}", form_component(name), form_component(value)))
        .collect::<Vec<_>>()
        .join("&")
}

fn agent() -> ureq::Agent {
    ureq::Agent::config_builder()
        .http_status_as_error(false)
        // Compatible identity credentials are scoped to the explicitly chosen
        // provider. Do not follow a response to another origin or route.
        .max_redirects(0)
        .timeout_global(Some(REQUEST_TIMEOUT))
        .build()
        .new_agent()
}

fn read_json<T: for<'de> Deserialize<'de>>(
    mut response: ureq::http::Response<ureq::Body>,
) -> Result<T, String> {
    let status = response.status().as_u16();
    let mut body = Vec::new();
    response
        .body_mut()
        .as_reader()
        .read_to_end(&mut body)
        .map_err(|_| "identity response read failure".to_owned())?;
    // Own-profile (100672170) and nickname (100747E38) impose an additional
    // exact-200 check, unlike the common transport's general 2xx result.
    if status != 200 {
        return Err(format!("identity HTTP {status}"));
    }
    // Never propagate untrusted response text through Lua errors or logging.
    serde_json::from_slice(&body).map_err(|_| "identity response format failure".to_owned())
}

fn access_metadata(
    config: &IdentityConfig,
) -> Result<Vec<(&'static str, String)>, session::SessionError> {
    let installation_id = config.identifiers.installation_id()?;
    let signed = config.signing.credentials(&config.client_id)?;
    // 10067B7E8 supplies the same string pairs to Level1 form access and
    // active Level2 session JSON. Device identifiers remain explicit desktop
    // adaptations; no historical application credentials are implicit.
    Ok(vec![
        ("clientId", config.client_id.clone()),
        ("clientSignature", signed.signature),
        ("clientSalt", signed.salt),
        ("clientVersion", "1.1.6".to_owned()),
        ("persistentGuid", config.identifiers.persistent_guid.clone()),
        ("installationId", installation_id),
        ("deviceType", crate::native_device_info_model()),
        ("os", std::env::consts::OS.to_owned()),
        ("osVersion", os_version::current_version()?),
        ("sdkVersion", "1130200".to_owned()),
        ("fusionVersion", "66595".to_owned()),
        // Game bridge 10009ED78/8C supplies these exact strings; SDK default
        // "Apple" and the selected game/registration locale are separate paths.
        ("distributionChannel", "apple".to_owned()),
        ("locale", "en_EN".to_owned()),
        ("utcOffset", timezone::current_offset()?),
        // Config +48/+56 remain empty at 10009ED14..DA4, so the native
        // optional pair builder omits definition and buildId entirely.
    ])
}

fn request_nickname_validation(
    config: &IdentityConfig,
    session: &IdentitySession,
    owner: &mut RequestOwner,
    nickname: &str,
) -> Result<NicknameResponse, String> {
    let fields = [
        ("nickname", nickname.to_owned()),
        ("checkUnique", "true".to_owned()),
    ];
    let response = session
        .execute_form_for_owner(
            config,
            owner,
            ProviderLevel::Level2,
            "profile/nickname/validate",
            &form_body(&fields),
        )
        .map_err(|error| error.to_string())?;
    read_json(response)
}

fn spawn_online(
    queue: Arc<Mutex<VecDeque<Queued<OnlineCompletion>>>>,
    application_events: ApplicationEventScheduler,
    event: ApplicationEvent,
    mut owner: RequestOwner,
    task: impl FnOnce(&mut RequestOwner) -> OnlineCompletion + Send + 'static,
) -> LuaResult<()> {
    std::thread::Builder::new()
        .name("stella-identity".to_owned())
        .spawn(move || {
            let completion = task(&mut owner);
            let mut completions = queue
                .lock()
                .expect("identity online completion queue lock poisoned");
            completions.push_back(Queued {
                owner,
                value: completion,
            });
            application_events.post(event);
        })
        .map_err(|_| runtime_error("Creating thread failed"))?;
    Ok(())
}

#[derive(Debug, Default, Serialize, Deserialize)]
pub(super) struct LocalServiceDocument {
    #[serde(default)]
    pub(super) keys: BTreeMap<String, String>,
    #[serde(default)]
    pub(super) storage_hashes: BTreeMap<String, String>,
    #[serde(default)]
    pub(super) cloud_settings: Option<serde_json::Value>,
}

pub(super) struct OfflineState {
    pub(super) keys: BTreeMap<String, String>,
    pub(super) storage_hashes: BTreeMap<String, String>,
    pub(super) cloud_settings: Option<serde_json::Value>,
    pub(super) transaction_in_progress: bool,
    pub(super) local_provider: bool,
    pub(super) logged_in: bool,
    identity_session: IdentitySession,
    pub(super) persistence_path: PathBuf,
    login_in_progress: bool,
}

impl OfflineState {
    pub(super) fn new(persistence_path: PathBuf) -> Self {
        Self {
            keys: BTreeMap::new(),
            storage_hashes: BTreeMap::new(),
            cloud_settings: None,
            transaction_in_progress: false,
            local_provider: false,
            logged_in: false,
            identity_session: IdentitySession::default(),
            persistence_path,
            // SkynestAccountService's constructor finishes by calling
            // sub_1000A4AB0, which sets byte +0x41 before starting the
            // provider's automatic login. The Lua facade is installed later,
            // so the offline completion is delivered after service
            // announcement instead of being lost during construction.
            login_in_progress: true,
        }
    }

    fn enable_local_provider(&mut self) -> LuaResult<()> {
        if self.persistence_path.is_file() {
            let bytes = fs::read(&self.persistence_path).map_err(runtime_error)?;
            let document =
                serde_json::from_slice::<LocalServiceDocument>(&bytes).map_err(runtime_error)?;
            self.keys = document.keys;
            self.storage_hashes = document.storage_hashes;
            self.cloud_settings = document.cloud_settings;
        }
        self.local_provider = true;
        Ok(())
    }

    pub(super) fn identity_headers(&self) -> (Option<String>, Option<String>) {
        let tokens = self.identity_session.level2_tokens();
        (
            (!tokens.access_token.is_empty()).then_some(tokens.access_token),
            (!tokens.segment.is_empty()).then_some(tokens.segment),
        )
    }

    pub(super) fn persist(&self) -> LuaResult<()> {
        if !self.local_provider {
            return Ok(());
        }
        if let Some(parent) = self.persistence_path.parent() {
            fs::create_dir_all(parent).map_err(runtime_error)?;
        }
        let document = LocalServiceDocument {
            keys: self.keys.clone(),
            storage_hashes: self.storage_hashes.clone(),
            cloud_settings: self.cloud_settings.clone(),
        };
        let bytes = serde_json::to_vec_pretty(&document).map_err(runtime_error)?;
        fs::write(&self.persistence_path, bytes).map_err(runtime_error)
    }
}

pub(super) fn install(
    lua: &Lua,
    globals: &mlua::Table,
    state: Arc<Mutex<OfflineState>>,
    application_events: ApplicationEventScheduler,
) -> LuaResult<SkynestAccountRuntime> {
    let identifiers = identifiers::Identifiers::for_app_data(
        &globals.get::<String>("uniqueDeviceId")?,
        state
            .lock()
            .expect("Skynest state lock poisoned")
            .persistence_path
            .parent()
            .expect("AppData service path"),
    );
    let runtime =
        SkynestAccountRuntime::new(Arc::clone(&state), application_events, identifiers.into());
    let account = lua.create_table()?;
    account.set(
        "native_getServiceName",
        // ICloudService vtable slot +0x18 at sub_1000A7D28.
        lua.create_function(|_, _: MultiValue| Ok("identityLevel2"))?,
    )?;
    let logged_in_state = Arc::clone(&state);
    account.set(
        "native_isLoggedIn",
        lua.create_function(move |_, _: MultiValue| {
            Ok(logged_in_state
                .lock()
                .map_err(|_| runtime_error("Skynest state lock poisoned"))?
                .logged_in)
        })?,
    )?;
    let progress_state = Arc::clone(&state);
    account.set(
        "native_isLoginInProgress",
        lua.create_function(move |_, _: MultiValue| {
            Ok(progress_state
                .lock()
                .map_err(|_| runtime_error("Skynest state lock poisoned"))?
                .login_in_progress)
        })?,
    )?;
    account.set(
        "native_getAccountDetailsUrl",
        lua.create_function(|_, _: MultiValue| Ok("https://account.rovio.com"))?,
    )?;
    let login_runtime = runtime.clone();
    account.set(
        "native_login",
        lua.create_function(move |_, args: MultiValue| {
            // Generated adapter sub_1000A8C1C consumes three strict
            // booleans and ignores the tail.
            let interactive = native_required_boolean(&args, 0, "SkynestAccount.native_login")?;
            let social = native_required_boolean(&args, 1, "SkynestAccount.native_login")?;
            let register = native_required_boolean(&args, 2, "SkynestAccount.native_login")?;
            if interactive {
                login_runtime.begin_interactive(register)
            } else if social {
                login_runtime.begin_unavailable_social()
            } else {
                login_runtime.begin_login()
            }
        })?,
    )?;
    let logout_runtime = runtime.clone();
    account.set(
        "native_logout",
        lua.create_function(move |lua, _: MultiValue| logout_runtime.clear_identity(lua))?,
    )?;
    let social_login_runtime = runtime.clone();
    account.set(
        "native_loginWithSocialNetwork",
        lua.create_function(move |_, _: MultiValue| {
            social_login_runtime.begin_unavailable_social()
        })?,
    )?;
    let unregister_state = Arc::clone(&state);
    account.set(
        "native_unRegister",
        lua.create_function(move |_, _: MultiValue| {
            let mut state = unregister_state
                .lock()
                .map_err(|_| runtime_error("Skynest state lock poisoned"))?;
            state.logged_in = false;
            state.keys.clear();
            state.storage_hashes.clear();
            state.cloud_settings = None;
            state.identity_session.logout().map_err(runtime_error)?;
            state.persist()
        })?,
    )?;

    let nickname_state = Arc::clone(&state);
    account.set(
        "native_hasNickname",
        // Despite its exported name, sub_1000A3D68 returns the identity
        // provider's `profileNickname.empty()`. A signed-out provider has an
        // empty profile regardless of similarly named Skynest Storage keys.
        lua.create_function(move |_, _: MultiValue| {
            // The compatible local identity deliberately starts without a
            // profile nickname, preserving the native inverted query.
            let state = nickname_state
                .lock()
                .map_err(|_| runtime_error("Skynest state lock poisoned"))?;
            Ok(!state.logged_in
                || state
                    .identity_session
                    .profile()
                    .is_none_or(|profile| profile.personal.nickname.is_empty()))
        })?,
    )?;
    let validation_runtime = runtime.clone();
    account.set(
        "native_validateNickname",
        lua.create_function(move |lua, args: MultiValue| {
            // Adapter sub_1000A8998 reads a string and LuaFunction. Purple's
            // success completion calls callback(true, isValid); its transport
            // failure completion calls callback(false). Preserve the success
            // shape while providing a deterministic local validator now that
            // the remote identity service is gone.
            let nickname =
                native_required_string(&args, 0, "SkynestAccount.native_validateNickname")?;
            let callback =
                native_required_function(&args, 1, "SkynestAccount.native_validateNickname")?;
            let trimmed = nickname.trim();
            let is_valid = !trimmed.is_empty() && trimmed.chars().count() <= 32;
            if let Some(config) = validation_runtime.prepared_online_config()? {
                validation_runtime
                    .queue_online_nickname_validation(lua, callback, nickname, config)?;
            } else {
                validation_runtime
                    .queue_nickname_validation(lua.create_registry_value(callback)?, is_valid);
            }
            Ok(())
        })?,
    )?;

    globals.set("SkynestAccount", account)?;
    Ok(runtime)
}

fn native_required_function(
    args: &MultiValue,
    index: usize,
    function: &str,
) -> LuaResult<mlua::Function> {
    match args.iter().nth(index) {
        Some(Value::Function(callback)) => Ok(callback.clone()),
        _ => Err(runtime_error(format!(
            "bad argument #{} to '{function}' (function expected)",
            index + 1
        ))),
    }
}

pub(super) fn complete_initial_login(lua: &Lua) -> LuaResult<()> {
    let environment = game_environment(lua)?;
    if let Value::Table(facade) = environment.get::<Value>("SkynestAccount")? {
        let completed = facade.get::<Value>("initialAutologinDone")?;
        if !matches!(completed, Value::Nil | Value::Boolean(false)) {
            return Ok(());
        }
    }
    let Value::Table(account) = lua.globals().get::<Value>("SkynestAccount")? else {
        return Ok(());
    };
    let Value::Function(login) = account.get::<Value>("native_login")? else {
        return Ok(());
    };
    // sub_1000A3E04 routes (false, false, false) to sub_1000A4AB0, the same
    // automatic-login branch called by the native service constructor. At
    // this point SkynestAccount.lua has installed onLoginFailure, so the
    // retired backend can complete without leaving initialLoadingScreen
    // waiting forever.
    login.call::<()>((false, false, false))
}

/// Deliver retained identity-provider completions at the application frame head.
pub(crate) fn dispatch_local_completion(
    lua: &Lua,
    runtime: &SkynestAccountRuntime,
) -> LuaResult<()> {
    let Some(Queued {
        owner,
        value: completion,
    }) = runtime.pop_pending()
    else {
        return Ok(());
    };
    if !runtime.session.request_owner_is_current(owner) {
        match completion {
            Completion::ValidateNickname { callback, .. } => lua.remove_registry_value(callback)?,
            Completion::LoginUnavailable { login_job }
            | Completion::LoginSucceeded { login_job } => {
                runtime.finish_stale_login_job(login_job);
            }
            Completion::Interactive(completion) => runtime.discard_stale_interactive(*completion),
        }
        return Ok(());
    }
    match completion {
        Completion::LoginUnavailable { login_job } => {
            if runtime.complete_login_job(login_job) {
                notify_login_unavailable(lua, &runtime.state)?;
            }
        }
        Completion::LoginSucceeded { login_job } => {
            if runtime.complete_login_job(login_job) {
                notify_login_success(lua, &runtime.state)?;
            }
        }
        Completion::Interactive(completion) => {
            runtime.dispatch_interactive(lua, owner, *completion)?
        }
        Completion::ValidateNickname { callback, is_valid } => {
            call_retained(lua, callback, (true, is_valid))?;
        }
    }
    Ok(())
}

pub(crate) fn dispatch_online_completion(
    lua: &Lua,
    runtime: &SkynestAccountRuntime,
) -> LuaResult<()> {
    let Some(Queued {
        owner,
        value: completion,
    }) = runtime.pop_online_pending()
    else {
        return Ok(());
    };
    if !runtime.session.request_owner_is_current(owner) {
        match completion {
            OnlineCompletion::ValidateNickname { request_id, .. } => {
                if let Some(callback) = runtime.callbacks.borrow_mut().remove(&request_id) {
                    lua.remove_registry_value(callback)?;
                }
            }
            OnlineCompletion::Login { login_job, .. } => runtime.finish_stale_login_job(login_job),
            OnlineCompletion::Interactive(result) => {
                runtime.discard_stale_interactive_online(result)
            }
        }
        return Ok(());
    }
    match completion {
        OnlineCompletion::Login {
            login_job,
            result: Ok(_profile),
        } => {
            if runtime.complete_login_job(login_job) {
                notify_login_success(lua, &runtime.state)?;
            }
        }
        OnlineCompletion::Login {
            login_job,
            result: Err(message),
        } => {
            if runtime.complete_login_job(login_job) {
                notify_login_failure(lua, &runtime.state, &message)?;
            }
        }
        OnlineCompletion::Interactive(result) => {
            runtime.dispatch_interactive_online(lua, owner, result)?
        }
        OnlineCompletion::ValidateNickname { request_id, result } => {
            if let Some(callback) = runtime.callbacks.borrow_mut().remove(&request_id) {
                match result {
                    Ok(response) => {
                        let _ = response.validation_message;
                        call_retained(lua, callback, (true, response.is_valid))?;
                    }
                    Err(message) => {
                        eprintln!("identity nickname validation failed: {message}");
                        call_retained(lua, callback, false)?;
                    }
                }
            }
        }
    }
    Ok(())
}

fn notify_login_success(lua: &Lua, state: &Arc<Mutex<OfflineState>>) -> LuaResult<()> {
    // 1000A3FB8 synchronously publishes the account-login event before flags
    // and Lua onLoginSuccess. It is distinct from the delayed SDK session event.
    super::social::dispatch_account_login(lua)?;
    let (id, name, connected_to_social_network, is_guest) = {
        let mut state = state
            .lock()
            .map_err(|_| runtime_error("Skynest state lock poisoned"))?;
        state.login_in_progress = false;
        state.logged_in = true;
        state.persist()?;
        let profile = state.identity_session.profile();
        let connected = profile
            .as_ref()
            .is_some_and(|profile| profile.connected_to_social_network);
        (
            profile
                .as_ref()
                .map(|profile| profile.public_account_id.clone())
                .unwrap_or_else(|| "local-player".to_owned()),
            profile
                .as_ref()
                .map(|profile| {
                    if connected {
                        profile.active_social_name.clone()
                    } else {
                        profile.personal.email.clone()
                    }
                })
                .unwrap_or_else(|| "Stella Player".to_owned()),
            connected,
            profile.as_ref().is_none_or(ProfileResponse::is_guest),
        )
    };
    let account = match lua.globals().get::<Value>("SkynestAccount")? {
        Value::Table(account) => account,
        _ => return Ok(()),
    };
    let Value::Function(on_success) = account.get::<Value>("onLoginSuccess")? else {
        return Ok(());
    };
    // sub_1000A3F64 publishes (isGuest, details) with these exact fields.
    let details = lua.create_table()?;
    details.set("isConnectedToSocialNetwork", connected_to_social_network)?;
    details.set("isGuest", is_guest)?;
    details.set("id", id)?;
    details.set("name", name)?;
    on_success.call::<()>((is_guest, details))
}

fn notify_login_unavailable(lua: &Lua, state: &Arc<Mutex<OfflineState>>) -> LuaResult<()> {
    notify_login_failure(
        lua,
        state,
        "Rovio Account is unavailable on this offline host",
    )
}

fn notify_login_failure(
    lua: &Lua,
    state: &Arc<Mutex<OfflineState>>,
    message: &str,
) -> LuaResult<()> {
    notify_account_failure(lua, state, "ERROR_OTHER", message)
}

fn notify_account_failure(
    lua: &Lua,
    state: &Arc<Mutex<OfflineState>>,
    code: &str,
    message: &str,
) -> LuaResult<()> {
    // Both native success and failure completions clear byte +0x41 before
    // entering the corresponding Lua callback (sub_1000A3F64/sub_1000A4578).
    let profile = {
        let mut state = state
            .lock()
            .map_err(|_| runtime_error("Skynest state lock poisoned"))?;
        state.login_in_progress = false;
        state
            .identity_session
            .profile()
            .filter(|profile| !profile.public_account_id.is_empty())
            .map(|profile| (profile.public_account_id, profile.personal.email))
    };
    // SkynestAccount.lua keeps its facade in the game environment but adds
    // the native completions to the original `_G.SkynestAccount` table.
    let account = match lua.globals().get::<Value>("SkynestAccount")? {
        Value::Table(account) => account,
        _ => return Ok(()),
    };
    let Value::Function(on_failure) = account.get::<Value>("onLoginFailure")? else {
        return Ok(());
    };
    // Account manager sub_1000A3BA0 maps backend error 5 to ERROR_OTHER;
    // sub_1000A4578 forwards that code and the provider message verbatim.
    if let Some((id, email)) = profile {
        let details = lua.create_table()?;
        details.set("id", id)?;
        details.set("email", email)?;
        on_failure.call::<()>((code, message, details))
    } else {
        on_failure.call::<()>((code, message))
    }
}

fn call_retained(lua: &Lua, callback: RegistryKey, args: impl IntoLuaMulti) -> LuaResult<()> {
    let function = lua.registry_value::<mlua::Function>(&callback)?;
    let result = function.call::<()>(args);
    lua.remove_registry_value(callback)?;
    result
}
