//! Opaque, Send account lifetimes and storage transport; no credential export.

use super::{
    IdentityConfig, SkynestAccountRuntime,
    session::{IdentitySession, PreparedRequest},
};
use mlua::Result as LuaResult;
use std::time::Duration;
use ureq::{Body, http::Response};

#[derive(Clone)]
pub(crate) struct IdentityLifetime {
    session: IdentitySession,
    epoch: u64,
    generation: u64,
}

impl IdentityLifetime {
    pub(crate) fn is_current(&self) -> bool {
        self.session
            .storage_lifetime_is_current(self.epoch, self.generation)
    }
}

impl std::fmt::Debug for IdentityLifetime {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("IdentityLifetime")
            .field("epoch", &self.epoch)
            .field("generation", &self.generation)
            .finish_non_exhaustive()
    }
}

#[derive(Clone)]
pub(in super::super) struct StorageIdentity {
    config: IdentityConfig,
    lifetime: IdentityLifetime,
}

impl StorageIdentity {
    pub(in super::super) fn request_if_current(
        &self,
        url: &str,
        body: Option<(&str, &[u8])>,
        timeout: Duration,
        still_current: &dyn Fn() -> bool,
    ) -> Result<Response<Body>, i32> {
        // StorageImpl+8 retains active Level2. GET/state, POST/state and
        // POST/states/query all select mode zero; no parent fallback is used.
        // Storage retains its own exact-200 parser after common 2xx checking.
        self.lifetime
            .session
            .execute_storage(
                &self.config,
                self.lifetime.epoch,
                self.lifetime.generation,
                &PreparedRequest {
                    url,
                    body,
                    timeout,
                    still_current: Some(still_current),
                },
            )
            .map_err(|error| error.status)
    }
}

impl SkynestAccountRuntime {
    pub(crate) fn pop_session_success(&self) -> Option<IdentityLifetime> {
        let (epoch, generation) = self.session.pop_success_owner()?;
        let lifetime = IdentityLifetime {
            session: self.session.clone(),
            epoch,
            generation,
        };
        lifetime.is_current().then_some(lifetime)
    }

    pub(in super::super) fn identity_lifetime(&self) -> IdentityLifetime {
        let (epoch, generation) = self.session.storage_lifetime();
        IdentityLifetime {
            session: self.session.clone(),
            epoch,
            generation,
        }
    }

    pub(in super::super) fn prepare_storage_identity(&self) -> LuaResult<Option<StorageIdentity>> {
        Ok(self
            .prepared_online_config()?
            .map(|config| StorageIdentity {
                config,
                lifetime: self.identity_lifetime(),
            }))
    }
}
