//! Native identity projections over the explicit desktop installation boundary.

use super::session::{RegistryStore, StoreError};
use std::path::{Path, PathBuf};

#[derive(Clone)]
pub(super) struct Identifiers {
    pub(super) persistent_guid: String,
    installation: Installation,
}

#[cfg(test)]
mod tests {
    use super::super::{ClientSigning, IdentityConfig, IdentityEndpoint, access_metadata};
    use super::*;
    use std::{
        fs,
        sync::atomic::{AtomicU64, Ordering},
    };

    struct Root(PathBuf);
    impl Root {
        fn new() -> Self {
            static NEXT: AtomicU64 = AtomicU64::new(0);
            let path = std::env::temp_dir().join(format!(
                "stella-wire-identifiers-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            fs::create_dir(&path).unwrap();
            Self(path)
        }
    }
    impl Drop for Root {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn identity_metadata_preserves_device_projection_and_installation_across_provider_changes() {
        let root = Root::new();
        let identities =
            Identifiers::for_app_data("A9993E364706816ABA3E25717850C26C9CD0D89D", &root.0);
        // Already projected GUID is copied exactly; access must not hash twice.
        assert_eq!(
            identities.persistent_guid,
            "A9993E364706816ABA3E25717850C26C9CD0D89D"
        );
        assert!(!root.0.join("stella-installation.registry").exists());
        let mut config = IdentityConfig {
            endpoint: IdentityEndpoint::parse("http://127.0.0.1:9/identity/3.0").unwrap(),
            client_id: "first-client".to_owned(),
            signing: ClientSigning::default(),
            identifiers: identities.into(),
        };
        let fields: std::collections::BTreeMap<_, _> =
            access_metadata(&config).unwrap().into_iter().collect();
        let first = fields["installationId"].clone();
        assert_ne!(first, fields["persistentGuid"]);
        config.endpoint = IdentityEndpoint::parse("http://127.0.0.1:8/identity/3.0").unwrap();
        config.client_id = "other-client".to_owned();
        config.identifiers =
            Identifiers::for_app_data("A9993E364706816ABA3E25717850C26C9CD0D89D", &root.0).into();
        let fields: std::collections::BTreeMap<_, _> =
            access_metadata(&config).unwrap().into_iter().collect();
        assert_eq!(fields["installationId"], first);
        let other = Root::new();
        assert_ne!(
            Identifiers::for_app_data("A9993E364706816ABA3E25717850C26C9CD0D89D", &other.0)
                .installation_id()
                .unwrap(),
            first
        );
    }

    #[test]
    fn identity_metadata_storage_error_stops_access_and_does_not_reset_corruption() {
        let root = Root::new();
        let path = root.0.join("stella-installation.registry");
        fs::write(&path, b"damaged registry").unwrap();
        let config = IdentityConfig {
            endpoint: IdentityEndpoint::parse("http://127.0.0.1:9/identity/3.0").unwrap(),
            client_id: "private-client-id".to_owned(),
            signing: ClientSigning::default(),
            identifiers: Identifiers::for_app_data("private-device-id", &root.0).into(),
        };
        let error = access_metadata(&config).unwrap_err();
        assert_eq!(error.status, -1);
        assert_eq!(error.to_string(), "identity status -1");
        assert_eq!(fs::read(&path).unwrap(), b"damaged registry");
    }
}

#[derive(Clone)]
enum Installation {
    Registry(PathBuf),
    #[cfg(test)]
    Synthetic(std::sync::Arc<std::sync::Mutex<String>>),
}

impl Identifiers {
    pub(super) fn for_app_data(device_guid: &str, app_data: &Path) -> Self {
        Self {
            // Lua and default access GUID both call native 100539D84. Capture
            // the already projected value before game Lua can replace globals.
            persistent_guid: device_guid.to_owned(),
            // Installation identity is app-wide, unlike scoped credentials.
            // Never discover/import a historical player's fusion.registry.
            installation: Installation::Registry(app_data.join("stella-installation.registry")),
        }
    }

    pub(super) fn installation_id(&self) -> Result<String, StoreError> {
        match &self.installation {
            Installation::Registry(path) => RegistryStore::open(path.clone())?.installation_id(),
            #[cfg(test)]
            Installation::Synthetic(value) => value
                .lock()
                .map(|value| value.clone())
                .map_err(|_| StoreError::Io),
        }
    }

    pub(super) fn regenerate_account_id(&self) -> Result<(), StoreError> {
        match &self.installation {
            Installation::Registry(path) => {
                RegistryStore::open(path.clone())?.regenerate_account_id()?;
            }
            #[cfg(test)]
            Installation::Synthetic(value) => {
                let mut value = value.lock().map_err(|_| StoreError::Io)?;
                *value =
                    crate::game_lua::platform::generate_uuid_v4().map_err(|_| StoreError::Io)?;
            }
        }
        Ok(())
    }

    #[cfg(test)]
    pub(super) fn synthetic() -> Self {
        Self {
            persistent_guid: "fixture-device-sha1".to_owned(),
            installation: Installation::Synthetic(std::sync::Arc::new(std::sync::Mutex::new(
                "fixture-installation-id".to_owned(),
            ))),
        }
    }
}
