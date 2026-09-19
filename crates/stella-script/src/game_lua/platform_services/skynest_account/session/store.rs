use serde_json::Value;
use std::{fmt, path::PathBuf, sync::Mutex};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::game_lua::platform_services) enum StoreError {
    Io,
    InvalidCiphertext,
    InvalidDocument,
}

impl fmt::Display for StoreError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Io => "account registry I/O failure",
            Self::InvalidCiphertext => "account registry ciphertext is invalid",
            Self::InvalidDocument => "account registry document is invalid",
        })
    }
}
impl std::error::Error for StoreError {}

/// Injectable local registry boundary. Implementations must only perform short
/// local operations and must not re-enter IdentitySession or make HTTP calls.
/// RegistryStore implements native-format encrypted disk persistence. The
/// memory implementation is for detached/offline sessions and injected tests;
/// it must not be mistaken for process-restart persistence.
pub(in super::super) trait RefreshStore: Send + Sync {
    fn load(&self) -> Result<String, StoreError>;
    fn store(&self, refresh: &str) -> Result<(), StoreError>;
    fn load_profile(&self) -> Result<Option<Value>, StoreError>;
    fn store_profile(&self, profile: Option<&Value>) -> Result<(), StoreError>;

    /// Detached/custom stores have no persistent asset capability. A profile
    /// that actually needs assets must report this instead of claiming a hit.
    fn avatar_directory(&self) -> Option<PathBuf> {
        None
    }
    fn load_avatar_version(&self, _basename: &str) -> Result<String, StoreError> {
        Err(StoreError::InvalidDocument)
    }
    fn store_avatar_version(&self, _basename: &str, _version: &str) -> Result<(), StoreError> {
        Err(StoreError::InvalidDocument)
    }
}

#[derive(Default)]
pub(in super::super) struct MemoryRefreshStore {
    refresh: Mutex<String>,
    profile: Mutex<Option<Value>>,
}

impl RefreshStore for MemoryRefreshStore {
    fn load(&self) -> Result<String, StoreError> {
        Ok(self
            .refresh
            .lock()
            .expect("refresh store lock poisoned")
            .clone())
    }
    fn store(&self, refresh: &str) -> Result<(), StoreError> {
        *self.refresh.lock().expect("refresh store lock poisoned") = refresh.to_owned();
        Ok(())
    }
    fn load_profile(&self) -> Result<Option<Value>, StoreError> {
        Ok(self
            .profile
            .lock()
            .expect("profile store lock poisoned")
            .clone())
    }
    fn store_profile(&self, profile: Option<&Value>) -> Result<(), StoreError> {
        *self.profile.lock().expect("profile store lock poisoned") = profile.cloned();
        Ok(())
    }
}
