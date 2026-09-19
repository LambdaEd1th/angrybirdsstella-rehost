//! Skynest storage boundary, scoped online cache and native deferred callbacks.

use super::skynest_account::{
    IdentityLifetime, OfflineState, SkynestAccountRuntime, StorageIdentity,
};
use crate::*;
use mlua::{IntoLuaMulti, RegistryKey};
use serde::Deserialize;
use std::{
    cell::{Cell, RefCell},
    collections::VecDeque,
    rc::Rc,
    sync::atomic::{AtomicU64, Ordering},
    time::Duration,
};

mod cloud_payload;
mod codec;
mod delivery;
mod lifecycle;
mod registration;
mod transport;
pub(crate) use delivery::{dispatch_local_completion, dispatch_online_completion};
pub(super) use registration::install;
use transport::{request_batch, request_get, request_set};

const DEFAULT_REQUEST_TIMEOUT: Duration = Duration::from_secs(30);
const CLOUD_SETTINGS_KEY: &str = "PurpleState";
const STORAGE_PREFIX: &str = "[my]/[client]/";

enum Completion {
    LoadCloudSettings,
    SaveCloudSettings(serde_json::Value),
    SetKey {
        key: String,
        value: String,
        callback: RegistryKey,
    },
    GetKey {
        key: String,
        callback: RegistryKey,
    },
    GetKeyForAccountIds {
        key: String,
        account_ids: Vec<String>,
        callback: RegistryKey,
    },
}

#[derive(Clone)]
struct OnlineConfig {
    base_url: String,
    auth: StorageAuth,
    timeout: Duration,
    owner: RequestOwner,
}

#[derive(Clone)]
enum StorageAuth {
    Managed(StorageIdentity),
    Snapshot {
        access_token: Option<String>,
        segment: Option<String>,
    },
}

#[derive(Clone, Debug, Deserialize)]
struct StoredValue {
    hash: String,
    value: String,
    encoding: String,
}

#[derive(Clone, Debug, Deserialize)]
struct StoredHash {
    hash: String,
}

#[derive(Clone, Debug, Deserialize)]
struct AccountStates {
    #[serde(rename = "accountId")]
    account_id: String,
    states: Vec<AccountValue>,
}

#[derive(Clone, Debug, Deserialize)]
struct AccountValue {
    value: String,
    encoding: String,
}

#[derive(Clone, Debug, Deserialize)]
struct AccountStatesResponse {
    result: Vec<AccountStates>,
}

#[derive(Clone, Debug)]
struct ServiceError {
    status: Option<u16>,
    message: &'static str,
}

enum OnlineCompletion {
    LoadCloudSettings(Result<StoredValue, ServiceError>),
    SaveCloudSettings {
        config: OnlineConfig,
        result: Result<StoredHash, ServiceError>,
    },
    SaveCloudSettingsConflict(Result<StoredValue, ServiceError>),
    SetKey {
        request_id: u64,
        key: String,
        value: String,
        config: OnlineConfig,
        result: Result<StoredHash, ServiceError>,
    },
    SetKeyConflict {
        request_id: u64,
        key: String,
        result: Result<StoredValue, ServiceError>,
    },
    GetKey {
        request_id: u64,
        key: String,
        result: Result<StoredValue, ServiceError>,
    },
    GetKeyForAccountIds {
        request_id: u64,
        result: Result<BTreeMap<String, String>, ServiceError>,
    },
}

#[derive(Clone)]
struct RequestOwner {
    identity: IdentityLifetime,
    storage_generation: Arc<AtomicU64>,
    generation: u64,
    sequence: u64,
}

impl RequestOwner {
    fn is_current(&self) -> bool {
        self.identity.is_current()
            && self.storage_generation.load(Ordering::Acquire) == self.generation
    }
}

struct Queued<T> {
    owner: RequestOwner,
    completion: T,
}

// Online results never overwrite the independent local-provider document.
#[derive(Default)]
struct OnlineCache {
    keys: BTreeMap<String, String>,
    hashes: BTreeMap<String, String>,
}

#[derive(Clone)]
pub(crate) struct SkynestStorageRuntime {
    state: Arc<Mutex<OfflineState>>,
    account: SkynestAccountRuntime,
    completions: Rc<RefCell<VecDeque<Queued<Completion>>>>,
    compatible_url: Arc<Mutex<Option<String>>>,
    access_token: Arc<Mutex<Option<String>>>,
    signature: Arc<Mutex<Option<String>>>,
    request_timeout: Arc<Mutex<Duration>>,
    generation: Arc<AtomicU64>,
    observed_owner: Rc<RefCell<Option<RequestOwner>>>,
    transaction_owner: Rc<Cell<Option<u64>>>,
    next_sequence: Rc<Cell<u64>>,
    online_cache: Rc<RefCell<OnlineCache>>,
    online_completions: Arc<Mutex<VecDeque<Queued<OnlineCompletion>>>>,
    callbacks: Rc<RefCell<BTreeMap<u64, RegistryKey>>>,
    next_request_id: Rc<Cell<u64>>,
    application_events: ApplicationEventScheduler,
}

impl SkynestStorageRuntime {
    pub(in crate::game_lua::platform_services) fn request_social_progress(
        &self,
        lua: &Lua,
        ids: Vec<String>,
        callback: mlua::Function,
    ) -> LuaResult<bool> {
        let (config, owner) = self.online_config()?;
        let Some(config) = config else {
            return Ok(false);
        };
        let request_id = self.retain_callback(lua, callback)?;
        if let Err(error) =
            self.spawn_online(owner, move || OnlineCompletion::GetKeyForAccountIds {
                request_id,
                result: request_batch(&config, "progress", &ids),
            })
        {
            self.remove_callback(lua, request_id)?;
            return Err(error);
        }
        Ok(true)
    }

    fn new(
        state: Arc<Mutex<OfflineState>>,
        account: SkynestAccountRuntime,
        application_events: ApplicationEventScheduler,
    ) -> Self {
        Self {
            state,
            account,
            completions: Rc::new(RefCell::new(VecDeque::new())),
            compatible_url: Arc::new(Mutex::new(None)),
            access_token: Arc::new(Mutex::new(None)),
            signature: Arc::new(Mutex::new(None)),
            request_timeout: Arc::new(Mutex::new(DEFAULT_REQUEST_TIMEOUT)),
            generation: Arc::new(AtomicU64::new(0)),
            observed_owner: Rc::new(RefCell::new(None)),
            transaction_owner: Rc::new(Cell::new(None)),
            next_sequence: Rc::new(Cell::new(1)),
            online_cache: Rc::new(RefCell::new(OnlineCache::default())),
            online_completions: Arc::new(Mutex::new(VecDeque::new())),
            callbacks: Rc::new(RefCell::new(BTreeMap::new())),
            next_request_id: Rc::new(Cell::new(1)),
            application_events,
        }
    }

    pub(crate) fn set_compatible_url(&self, url: &str) -> LuaResult<()> {
        let url = transport::validate_base_url(url).map_err(runtime_error)?;
        let mut current = self
            .compatible_url
            .lock()
            .map_err(|_| runtime_error("storage URL lock poisoned"))?;
        if current.as_ref() == Some(&url) {
            return Ok(());
        }
        *current = Some(url);
        drop(current);
        self.generation.fetch_add(1, Ordering::AcqRel);
        self.synchronize_owner()
    }

    pub(crate) fn set_compatible_credentials(
        &self,
        access_token: Option<&str>,
        signature: Option<&str>,
    ) -> LuaResult<()> {
        let access_token = nonempty_option(access_token);
        let signature = nonempty_option(signature);
        let mut current_access = self
            .access_token
            .lock()
            .map_err(|_| runtime_error("storage access-token lock poisoned"))?;
        let mut current_segment = self
            .signature
            .lock()
            .map_err(|_| runtime_error("storage signature lock poisoned"))?;
        if *current_access == access_token && *current_segment == signature {
            return Ok(());
        }
        *current_access = access_token;
        *current_segment = signature;
        drop(current_access);
        drop(current_segment);
        self.generation.fetch_add(1, Ordering::AcqRel);
        self.synchronize_owner()
    }

    fn online_config(&self) -> LuaResult<(Option<OnlineConfig>, RequestOwner)> {
        let base_url = self
            .compatible_url
            .lock()
            .map_err(|_| runtime_error("storage URL lock poisoned"))?
            .clone();
        let config = if let Some(base_url) = base_url {
            let access = self
                .access_token
                .lock()
                .map_err(|_| runtime_error("storage access-token lock poisoned"))?
                .clone();
            let segment = self
                .signature
                .lock()
                .map_err(|_| runtime_error("storage signature lock poisoned"))?
                .clone();
            let auth = if access.is_none()
                && segment.is_none()
                && let Some(identity) = self.account.prepare_storage_identity()?
            {
                StorageAuth::Managed(identity)
            } else {
                // Explicit host overrides preserve the old per-field fallback.
                // They are frozen credentials, not an implicit identity login.
                let inherited = self
                    .state
                    .lock()
                    .map_err(|_| runtime_error("Skynest state lock poisoned"))?
                    .identity_headers();
                StorageAuth::Snapshot {
                    access_token: access.or(inherited.0),
                    segment: segment.or(inherited.1),
                }
            };
            Some((base_url, auth))
        } else {
            None
        };
        // prepare_storage_identity may bind a saved provider and change its
        // lifetime. Capture only afterward, never around the lazy bind.
        self.synchronize_owner()?;
        let owner = self.capture_owner();
        let config = config.map(|(base_url, auth)| OnlineConfig {
            base_url,
            auth,
            timeout: *self
                .request_timeout
                .lock()
                .expect("storage timeout lock poisoned"),
            owner: owner.clone(),
        });
        Ok((config, owner))
    }

    fn push(&self, owner: RequestOwner, completion: Completion) {
        self.completions
            .borrow_mut()
            .push_back(Queued { owner, completion });
        self.application_events
            .post(ApplicationEvent::SkynestStorageLocal);
    }

    fn pop_pending(&self) -> Option<Queued<Completion>> {
        self.completions.borrow_mut().pop_front()
    }

    fn pop_online_pending(&self) -> Option<Queued<OnlineCompletion>> {
        self.online_completions
            .lock()
            .expect("storage completion lock poisoned")
            .pop_front()
    }

    fn spawn_online(
        &self,
        owner: RequestOwner,
        task: impl FnOnce() -> OnlineCompletion + Send + 'static,
    ) -> LuaResult<()> {
        let queue = Arc::clone(&self.online_completions);
        let application_events = self.application_events.clone();
        std::thread::Builder::new()
            .name("stella-storage".to_owned())
            .spawn(move || {
                let completion = task();
                let mut completions = queue.lock().expect("storage completion lock poisoned");
                completions.push_back(Queued { owner, completion });
                // Keep the FIFO/event pair even when the owner was invalidated.
                // Dispatch consumes the tombstone instead of stealing a new result.
                application_events.post(ApplicationEvent::SkynestStorageOnline);
            })
            .map_err(|_| runtime_error("Creating thread failed"))?;
        Ok(())
    }

    fn retain_callback(&self, lua: &Lua, callback: mlua::Function) -> LuaResult<u64> {
        let request_id = self.next_request_id.get();
        self.next_request_id.set(request_id.wrapping_add(1).max(1));
        self.callbacks
            .borrow_mut()
            .insert(request_id, lua.create_registry_value(callback)?);
        Ok(request_id)
    }

    fn take_callback(&self, request_id: u64) -> Option<RegistryKey> {
        self.callbacks.borrow_mut().remove(&request_id)
    }

    fn remove_callback(&self, lua: &Lua, request_id: u64) -> LuaResult<()> {
        if let Some(callback) = self.take_callback(request_id) {
            lua.remove_registry_value(callback)?;
        }
        Ok(())
    }

    #[cfg(test)]
    pub(crate) fn pending_online_count_for_test(&self) -> usize {
        self.online_completions
            .lock()
            .expect("storage completion lock poisoned")
            .len()
    }
}

fn nonempty_option(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
}
