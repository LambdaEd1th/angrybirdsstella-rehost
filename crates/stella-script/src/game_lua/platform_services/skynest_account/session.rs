//! The active Skynest identity's two token layers and bounded 401 replay.
//!
//! `docs/native-account-session-renewal.md` records the native call sites.
//! Session/Level1 HTTP happens under the independent renewal mutex, never an
//! OfflineState or session-state lock. The host epoch prevents an abandoned
//! worker from restoring a logged-out identity; it is not a native wire field.

use super::{AccessResponse, IdentityConfig, ProfileResponse, access_metadata, agent, form_body};
use std::{
    collections::BTreeMap,
    fmt,
    sync::{Arc, Mutex},
};

mod avatar_assets;
mod events;
pub(super) use avatar_assets::{AvatarAsset, parse_avatar_assets};
pub(super) use protocol::parse_profile_value;
mod execution;
mod friends_file;
pub(super) use friends_file::FriendsFile;
mod protocol;
mod registry_codec;
mod registry_store;
mod request;
mod store;
pub(super) use protocol::signed_number as profile_integer;
pub(super) use protocol::{parse_access_response, parse_profile_response};
use protocol::{parse_flat_tokens, parse_session, unix_seconds};
pub(in crate::game_lua::platform_services) use registry_store::RegistryNamespace;
pub(super) use registry_store::RegistryStore;
pub(super) use request::PreparedRequest;
pub(in crate::game_lua::platform_services) use store::StoreError;
pub(super) use store::{MemoryRefreshStore, RefreshStore};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum ProviderLevel {
    Level1,
    Level2,
}

/// Never derive Debug/Serialize: neither logs nor a general snapshot may
/// disclose access/refresh credentials. The registry adapter stores refresh
/// alone through the explicit RefreshStore boundary.
#[derive(Clone, Default)]
pub(super) struct Tokens {
    pub(super) access_token: String,
    pub(super) refresh_token: String,
    pub(super) segment: String,
    pub(super) absolute_expiry: i64,
}

impl Tokens {
    fn from_flat(access: &AccessResponse) -> Self {
        Self {
            access_token: access.access_token.clone(),
            refresh_token: access.refresh_token.clone(),
            segment: access.segment.clone().unwrap_or_default(),
            absolute_expiry: access.absolute_expiry,
        }
    }

    fn needs_acquire(&self, level: ProviderLevel) -> bool {
        // 100689B04's 600-second predicate is effective for Level1. Active
        // Level2's 100744E24 override returns early whenever access is nonempty,
        // even when called by the base getter after that predicate fires.
        self.access_token.is_empty()
            || (level == ProviderLevel::Level1
                && self.absolute_expiry != 0
                && unix_seconds() >= self.absolute_expiry.saturating_sub(600))
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) struct SessionError {
    pub(super) status: i32,
    cause: SessionFailure,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum SessionFailure {
    Other,
    Stale,
    SaltGeneration,
    TimezoneQuery,
    OsVersionQuery,
}

impl SessionError {
    fn transport() -> Self {
        Self {
            status: -1,
            cause: SessionFailure::Other,
        }
    }

    fn http(status: u16) -> Self {
        Self {
            status: i32::from(status),
            cause: SessionFailure::Other,
        }
    }

    fn cancelled() -> Self {
        Self {
            status: -1,
            cause: SessionFailure::Stale,
        }
    }

    pub(super) fn is_stale(self) -> bool {
        self.cause == SessionFailure::Stale
    }
}

impl fmt::Display for SessionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.cause == SessionFailure::SaltGeneration {
            write!(f, "{}", super::signing::SignatureError)
        } else if self.cause == SessionFailure::TimezoneQuery {
            f.write_str("identity timezone query failed")
        } else if self.cause == SessionFailure::OsVersionQuery {
            f.write_str("identity OS version query failed")
        } else {
            write!(f, "identity status {}", self.status)
        }
    }
}
impl fmt::Debug for SessionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(self, f)
    }
}
impl std::error::Error for SessionError {}

impl From<StoreError> for SessionError {
    fn from(_: StoreError) -> Self {
        Self::transport()
    }
}

impl From<super::os_version::OsVersionError> for SessionError {
    fn from(_: super::os_version::OsVersionError) -> Self {
        Self {
            status: -1,
            cause: SessionFailure::OsVersionQuery,
        }
    }
}

impl From<super::timezone::TimezoneError> for SessionError {
    fn from(_: super::timezone::TimezoneError) -> Self {
        Self {
            status: -1,
            cause: SessionFailure::TimezoneQuery,
        }
    }
}

impl From<super::signing::SignatureError> for SessionError {
    fn from(_: super::signing::SignatureError) -> Self {
        Self {
            status: -1,
            cause: SessionFailure::SaltGeneration,
        }
    }
}

#[derive(Default)]
struct SessionState {
    epoch: u64,
    // Separate from the UI request epoch: explicit credential replacement can
    // keep its own completion alive, but must retire already queued storage work.
    identity_generation: u64,
    level1: Tokens,
    level2: Tokens,
    profile: Option<ProfileResponse>,
    config: Option<BTreeMap<String, String>>,
}

/// Own-profile work retains the admitted identity, not merely a UI epoch.
/// Publication advances this owner for the remaining deferred callbacks.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct OwnProfileOwner {
    epoch: u64,
    generation: u64,
}

impl OwnProfileOwner {
    pub(super) fn request_owner(self) -> RequestOwner {
        RequestOwner {
            epoch: self.epoch,
            generation: Some(self.generation),
        }
    }
}

/// Native credential continuations snapshot these values before profile/own.
/// Keep the owner with the snapshot so a different login cannot reuse it.
pub(super) struct LoginProfileIdentity {
    owner: OwnProfileOwner,
    public_account_id: String,
    was_guest: bool,
}

pub(super) struct PreparedLoginProfile {
    owner: OwnProfileOwner,
    before: LoginProfileIdentity,
}

/// Captured on the admitting thread and retained through worker/UI queues.
/// Level1 is app-scoped, so changing only the Level2 identity does not retire it.
/// A request may adopt an owner returned by its own initial session publication,
/// never an owner sampled after an unrelated credential installation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct RequestOwner {
    epoch: u64,
    generation: Option<u64>,
}

impl RequestOwner {
    #[cfg(test)]
    pub(super) fn epoch(self) -> u64 {
        self.epoch
    }

    pub(super) fn epoch_only(self) -> Self {
        Self {
            generation: None,
            ..self
        }
    }
}

impl SessionState {
    fn tokens(&self, level: ProviderLevel) -> &Tokens {
        match level {
            ProviderLevel::Level1 => &self.level1,
            ProviderLevel::Level2 => &self.level2,
        }
    }
    fn tokens_mut(&mut self, level: ProviderLevel) -> &mut Tokens {
        match level {
            ProviderLevel::Level1 => &mut self.level1,
            ProviderLevel::Level2 => &mut self.level2,
        }
    }
    fn check_epoch(&self, expected: u64) -> Result<(), SessionError> {
        if self.epoch == expected {
            Ok(())
        } else {
            Err(SessionError::cancelled())
        }
    }

    fn check_owner(&self, epoch: u64, generation: Option<u64>) -> Result<(), SessionError> {
        self.check_epoch(epoch)?;
        if generation.is_some_and(|generation| generation != self.identity_generation) {
            return Err(SessionError::cancelled());
        }
        Ok(())
    }

    fn replace_profile(&mut self, profile: ProfileResponse) {
        if self.profile.as_ref().is_some_and(|old| {
            !old.public_account_id.is_empty() && old.public_account_id != profile.public_account_id
        }) {
            self.identity_generation = self.identity_generation.wrapping_add(1);
        }
        self.profile = Some(profile);
    }
}

#[derive(Clone)]
pub(super) struct IdentitySession {
    pub(super) sdk_logger: Arc<super::sdk_logger::SdkLogger>,
    state: Arc<Mutex<SessionState>>,
    level1_renewal: Arc<Mutex<()>>,
    level2_renewal: Arc<Mutex<()>>,
    refresh_store: Arc<Mutex<Arc<dyn RefreshStore>>>,
    success_events: Arc<Mutex<events::SessionEvents>>,
}

impl Default for IdentitySession {
    fn default() -> Self {
        Self::with_refresh_store(Arc::new(MemoryRefreshStore::default()))
    }
}

impl IdentitySession {
    pub(super) fn store_friends_cache_for_owner(
        &self,
        owner: RequestOwner,
        path: &std::path::Path,
        update: impl FnOnce() -> serde_json::Value,
    ) -> Result<bool, String> {
        let state = self
            .state
            .lock()
            .map_err(|_| "identity session lock poisoned".to_owned())?;
        if state.check_owner(owner.epoch, owner.generation).is_err() {
            return Ok(false);
        }
        let profile = state
            .profile
            .as_ref()
            .ok_or_else(|| "identity profile unavailable".to_owned())?;
        let text = serde_json::to_string(&update()).map_err(|e| e.to_string())?;
        FriendsFile::for_account(path, &profile.public_account_id)?.write(&text)?;
        Ok(true)
    }

    pub(super) fn with_refresh_store(refresh_store: Arc<dyn RefreshStore>) -> Self {
        Self {
            sdk_logger: Arc::default(),
            state: Arc::new(Mutex::new(SessionState::default())),
            level1_renewal: Arc::new(Mutex::new(())),
            level2_renewal: Arc::new(Mutex::new(())),
            refresh_store: Arc::new(Mutex::new(refresh_store)),
            success_events: Arc::default(),
        }
    }

    #[cfg(test)]
    pub(super) fn epoch(&self) -> u64 {
        self.state
            .lock()
            .expect("identity session lock poisoned")
            .epoch
    }

    pub(super) fn storage_lifetime(&self) -> (u64, u64) {
        let state = self.state.lock().expect("identity session lock poisoned");
        (state.epoch, state.identity_generation)
    }

    pub(super) fn request_owner(&self, level: ProviderLevel) -> RequestOwner {
        let state = self.state.lock().expect("identity session lock poisoned");
        RequestOwner {
            epoch: state.epoch,
            generation: (level == ProviderLevel::Level2).then_some(state.identity_generation),
        }
    }

    pub(super) fn request_owner_is_current(&self, owner: RequestOwner) -> bool {
        self.check_owner(owner.epoch, owner.generation).is_ok()
    }

    pub(super) fn storage_lifetime_is_current(&self, epoch: u64, generation: u64) -> bool {
        self.state
            .lock()
            .expect("identity session lock poisoned")
            .check_owner(epoch, Some(generation))
            .is_ok()
    }

    pub(super) fn logout(&self) -> Result<u64, StoreError> {
        // This deliberately does not acquire renewal: a stalled HTTP response
        // must not prevent local logout or re-install its credentials later.
        let mut state = self.state.lock().expect("identity session lock poisoned");
        state.level1 = Tokens::default();
        state.level2 = Tokens::default();
        state.profile = None;
        state.epoch = state.epoch.wrapping_add(1);
        let store = self.store();
        // Always clear both persisted fields, even when the first write fails;
        // local epoch/state invalidation must never depend on I/O succeeding.
        let refresh = store.store("");
        let profile = store.store_profile(None);
        refresh.and(profile)?;
        Ok(state.epoch)
    }

    fn store(&self) -> Arc<dyn RefreshStore> {
        self.refresh_store
            .lock()
            .expect("account store lock poisoned")
            .clone()
    }

    pub(super) fn bind_store(&self, store: Arc<dyn RefreshStore>) -> Result<(), StoreError> {
        let profile = store
            .load_profile()?
            .as_ref()
            .map(protocol::parse_profile_value);
        let mut state = self.state.lock().expect("identity session lock poisoned");
        let epoch = state.epoch.wrapping_add(1);
        *state = SessionState {
            epoch,
            profile,
            ..SessionState::default()
        };
        *self
            .refresh_store
            .lock()
            .expect("account store lock poisoned") = store;
        Ok(())
    }

    /// Changing an explicitly configured provider is not native account logout:
    /// detach its cache without deleting its on-disk refresh/profile.
    pub(super) fn detach_store(&self) {
        let mut state = self.state.lock().expect("identity session lock poisoned");
        let epoch = state.epoch.wrapping_add(1);
        *state = SessionState {
            epoch,
            ..SessionState::default()
        };
        self.sdk_logger.reset_provider();
        *self
            .refresh_store
            .lock()
            .expect("account store lock poisoned") = Arc::new(MemoryRefreshStore::default());
    }

    #[cfg(test)]
    pub(super) fn install_flat(&self, access: &AccessResponse) {
        let epoch = self.epoch();
        self.install_flat_if_epoch(epoch, access)
            .expect("test memory store");
    }

    #[cfg(test)]
    pub(super) fn install_flat_if_epoch(
        &self,
        epoch: u64,
        access: &AccessResponse,
    ) -> Result<bool, StoreError> {
        self.install_flat_for_profile(epoch, access)
            .map(|owner| owner.is_some())
    }

    #[cfg(test)]
    pub(super) fn install_flat_for_profile(
        &self,
        epoch: u64,
        access: &AccessResponse,
    ) -> Result<Option<OwnProfileOwner>, StoreError> {
        self.install_flat_guarded(
            RequestOwner {
                epoch,
                generation: None,
            },
            access,
        )
    }

    #[cfg(test)]
    pub(super) fn install_flat_for_request_owner(
        &self,
        owner: RequestOwner,
        access: &AccessResponse,
    ) -> Result<Option<OwnProfileOwner>, StoreError> {
        // Level1 can return registration credentials, but installing them still
        // requires the independent Level2 permission captured on submission.
        if owner.generation.is_none() {
            return Ok(None);
        }
        self.install_flat_guarded(owner, access)
    }

    #[cfg(test)]
    fn install_flat_guarded(
        &self,
        owner: RequestOwner,
        access: &AccessResponse,
    ) -> Result<Option<OwnProfileOwner>, StoreError> {
        let mut state = self.state.lock().expect("identity session lock poisoned");
        if state.check_owner(owner.epoch, owner.generation).is_err() {
            return Ok(None);
        }
        state.level2 = Tokens::from_flat(access);
        state.identity_generation = state.identity_generation.wrapping_add(1);
        if let Err(error) = self.store().store(&state.level2.refresh_token) {
            // Native installs tokens before saving. The host reports storage
            // failure rather than swallowing it; do not let the next acquire
            // falsely short-circuit that failed operation with this access.
            state.level2.access_token.clear();
            return Err(error);
        }
        Ok(Some(OwnProfileOwner {
            epoch: state.epoch,
            generation: state.identity_generation,
        }))
    }

    #[cfg(test)]
    pub(super) fn install_level1_flat(&self, access: &AccessResponse) {
        self.state
            .lock()
            .expect("identity session lock poisoned")
            .level1 = Tokens::from_flat(access);
    }

    pub(super) fn level2_tokens(&self) -> Tokens {
        self.state
            .lock()
            .expect("identity session lock poisoned")
            .level2
            .clone()
    }

    pub(super) fn profile(&self) -> Option<ProfileResponse> {
        self.state
            .lock()
            .expect("identity session lock poisoned")
            .profile
            .clone()
    }

    // Retained for the next native global-configuration consumer slice. Session
    // acquisition already validates and atomically replaces this complete map.
    #[allow(dead_code)]
    pub(super) fn config(&self) -> Option<BTreeMap<String, String>> {
        self.state
            .lock()
            .expect("identity session lock poisoned")
            .config
            .clone()
    }

    /// Explicit host-provider reconfiguration boundary, not native logout.
    /// Call after invalidating the previous provider's epoch with logout.
    #[cfg(test)]
    pub(super) fn reset_config(&self) {
        self.state
            .lock()
            .expect("identity session lock poisoned")
            .config = None;
    }

    #[cfg(test)]
    pub(super) fn install_profile_if_epoch(
        &self,
        epoch: u64,
        profile: &ProfileResponse,
    ) -> Result<bool, StoreError> {
        self.install_profile(epoch, None, profile, false)
            .map(|owner| owner.is_some())
    }

    pub(super) fn own_profile_owner_is_current(&self, owner: OwnProfileOwner) -> bool {
        self.storage_lifetime_is_current(owner.epoch, owner.generation)
    }

    pub(super) fn own_profile_owner_for_request(
        &self,
        owner: RequestOwner,
    ) -> Option<OwnProfileOwner> {
        let generation = owner.generation?;
        let state = self.state.lock().expect("identity session lock poisoned");
        state.check_owner(owner.epoch, Some(generation)).ok()?;
        Some(OwnProfileOwner {
            epoch: owner.epoch,
            generation,
        })
    }

    pub(super) fn login_profile_identity(
        &self,
        owner: OwnProfileOwner,
    ) -> Option<LoginProfileIdentity> {
        let state = self.state.lock().expect("identity session lock poisoned");
        state
            .check_owner(owner.epoch, Some(owner.generation))
            .ok()?;
        Some(LoginProfileIdentity {
            owner,
            public_account_id: state
                .profile
                .as_ref()
                .map(|profile| profile.public_account_id.clone())
                .unwrap_or_default(),
            was_guest: state
                .profile
                .as_ref()
                .is_some_and(ProfileResponse::is_guest),
        })
    }

    /// Test convenience for profiles with no transport step. Production uses
    /// the same two publication stages around the native avatar fetch.
    #[cfg(test)]
    pub(super) fn publish_login_profile(
        &self,
        owner: &mut OwnProfileOwner,
        access: &AccessResponse,
        profile: &ProfileResponse,
        identifiers: &super::identifiers::Identifiers,
        before: LoginProfileIdentity,
    ) -> Result<bool, StoreError> {
        let Some(prepared) = self.prepare_login_profile(owner, profile, before)? else {
            return Ok(false);
        };
        self.finish_login_profile(owner, access, identifiers, prepared)
    }

    pub(super) fn prepare_login_profile(
        &self,
        owner: &mut OwnProfileOwner,
        profile: &ProfileResponse,
        before: LoginProfileIdentity,
    ) -> Result<Option<PreparedLoginProfile>, StoreError> {
        let mut state = self.state.lock().expect("identity session lock poisoned");
        if before.owner != *owner
            || state
                .check_owner(owner.epoch, Some(owner.generation))
                .is_err()
        {
            return Ok(None);
        }
        self.store().store_profile(Some(&profile.raw))?;
        state.replace_profile(profile.clone());
        // Retire previous host work as soon as this new profile is visible.
        // Carry this exact owner across the HTTP gap; never sample a later one.
        state.identity_generation = state.identity_generation.wrapping_add(1);
        owner.generation = state.identity_generation;
        Ok(Some(PreparedLoginProfile {
            owner: *owner,
            before,
        }))
    }

    pub(super) fn finish_login_profile(
        &self,
        owner: &mut OwnProfileOwner,
        access: &AccessResponse,
        identifiers: &super::identifiers::Identifiers,
        prepared: PreparedLoginProfile,
    ) -> Result<bool, StoreError> {
        let mut state = self.state.lock().expect("identity session lock poisoned");
        if prepared.owner != *owner
            || state
                .check_owner(owner.epoch, Some(owner.generation))
                .is_err()
        {
            return Ok(false);
        }
        // Native 10067222C fetches assets after profile installation and before
        // the flat-token continuation at 10074F67C. Failure preserves that new
        // profile and old tokens, with no UUID rotation/session-success event.
        state.level2 = Tokens::from_flat(access);
        if let Err(error) = self.store().store(&state.level2.refresh_token) {
            state.level2.access_token.clear();
            return Err(error);
        }
        let profile = state.profile.as_ref().expect("prepared profile owner");
        if prepared.before.was_guest
            && prepared.before.public_account_id == profile.public_account_id
            && !profile.is_guest()
        {
            identifiers.regenerate_account_id()?;
        }
        self.publish_success_locked(&state);
        Ok(true)
    }

    #[cfg(test)]
    fn install_profile(
        &self,
        epoch: u64,
        generation: Option<u64>,
        profile: &ProfileResponse,
        publish_success: bool,
    ) -> Result<Option<OwnProfileOwner>, StoreError> {
        let mut state = self.state.lock().expect("identity session lock poisoned");
        if state.check_owner(epoch, generation).is_err() {
            return Ok(None);
        }
        if let Err(error) = self.store().store_profile(Some(&profile.raw)) {
            state.level2.access_token.clear();
            return Err(error);
        }
        state.replace_profile(profile.clone());
        if publish_success {
            self.publish_success_locked(&state);
        }
        Ok(Some(OwnProfileOwner {
            epoch: state.epoch,
            generation: state.identity_generation,
        }))
    }

    fn acquire_level1(
        &self,
        config: &IdentityConfig,
        epoch: u64,
        generation: Option<u64>,
    ) -> Result<(), SessionError> {
        let body = form_body(&access_metadata(config)?);
        self.check_owner(epoch, generation)?;
        let response = agent()
            .post(config.endpoint.request_url("access"))
            .header("Content-Type", "application/x-www-form-urlencoded")
            .send(&body)
            .map_err(|_| SessionError::transport())?;
        self.check_owner(epoch, generation)?;
        let tokens = parse_flat_tokens(response)?;
        let mut state = self.state.lock().expect("identity session lock poisoned");
        state.check_owner(epoch, generation)?;
        state.level1 = tokens;
        Ok(())
    }

    fn acquire_level2(
        &self,
        config: &IdentityConfig,
        epoch: u64,
        generation: Option<u64>,
    ) -> Result<u64, SessionError> {
        let mut refresh = {
            let mut state = self.state.lock().expect("identity session lock poisoned");
            state.check_owner(epoch, generation)?;
            let refresh = self.store().load()?;
            state.level2 = Tokens {
                refresh_token: refresh.clone(),
                ..Tokens::default()
            };
            refresh
        };
        // Native 1007457B8 recurses once after rejecting a stored refresh.
        // A rejection of the null-refresh request does not recurse again.
        loop {
            let fields = access_metadata(config)?;
            let access: serde_json::Map<String, serde_json::Value> = fields
                .into_iter()
                .map(|(key, value)| (key.to_owned(), value.into()))
                .collect();
            let refresh_value = if refresh.is_empty() {
                serde_json::Value::Null
            } else {
                serde_json::json!({"token": refresh})
            };
            let body = serde_json::json!({"access": access, "refresh": refresh_value}).to_string();
            self.check_owner(epoch, generation)?;
            let response = agent()
                .post(config.endpoint.session_url(&config.client_id))
                .header("Content-Type", "application/json")
                .send(&body)
                .map_err(|_| SessionError::transport())?;
            self.check_owner(epoch, generation)?;
            if response.status() == 401 {
                let had_refresh = !refresh.is_empty();
                {
                    let mut state = self.state.lock().expect("identity session lock poisoned");
                    state.check_owner(epoch, generation)?;
                    self.store().store("")?;
                    state.level2.refresh_token.clear();
                }
                if had_refresh {
                    refresh.clear();
                    continue;
                }
                return Err(SessionError::http(401));
            }
            let parsed = parse_session(response)?;
            let mut state = self.state.lock().expect("identity session lock poisoned");
            state.check_owner(epoch, generation)?;
            state.config = Some(parsed.config);
            state.level2 = parsed.tokens;
            let stored = self
                .store()
                .store(&state.level2.refresh_token)
                .and_then(|()| self.store().store_profile(Some(&parsed.profile.raw)));
            if let Err(error) = stored {
                // Keep the recovered publication order, but a host I/O error
                // must be retryable: nonempty access would otherwise bypass
                // acquisition and expose an older cached profile as success.
                state.level2.access_token.clear();
                return Err(error.into());
            }
            state.replace_profile(parsed.profile);
            self.publish_success_locked(&state);
            self.sdk_logger.configure(
                config,
                state
                    .config
                    .as_ref()
                    .and_then(|map| map.get("device.logLevel"))
                    .map_or("", String::as_str),
            );
            // Return the publication's owner while still holding its lock. A
            // later read of current generation could adopt a different login.
            return Ok(state.identity_generation);
        }
    }
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod ownership_tests;
