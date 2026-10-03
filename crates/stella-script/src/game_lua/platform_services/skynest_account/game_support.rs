//! Opaque Level2 owner for native GameClient requests and encrypted cache.
use super::super::social::game_client::protocol::encrypt;
use super::*;
use session::{PreparedRequest, SessionError};
use ureq::{Body, http::Response};

#[derive(Clone)]
pub(in crate::game_lua::platform_services) struct GameIdentity {
    session: IdentitySession,
    config: IdentityConfig,
    owner: RequestOwner,
    registry_path: PathBuf,
    account: String,
    public_account: String,
}

impl std::fmt::Debug for GameIdentity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("GameIdentity")
    }
}

#[derive(Clone, Copy, Debug)]
pub(in crate::game_lua::platform_services) struct GameError {
    pub(in crate::game_lua::platform_services) status: i32,
}

impl From<SessionError> for GameError {
    fn from(value: SessionError) -> Self {
        Self {
            status: value.status,
        }
    }
}

impl GameIdentity {
    pub(in crate::game_lua::platform_services) fn context_is_current(&self) -> bool {
        self.session
            .request_owner_is_current(self.owner.epoch_only())
    }

    pub(in crate::game_lua::platform_services) fn for_request(&self) -> Option<Self> {
        let (owner, account, public_account) = self.session.current_game_owner(self.owner)?;
        Some(Self {
            owner,
            account,
            public_account,
            ..self.clone()
        })
    }

    pub(in crate::game_lua::platform_services) fn same_account(&self, other: &Self) -> bool {
        self.account == other.account && self.public_account == other.public_account
    }

    pub(in crate::game_lua::platform_services) fn same_request(&self, other: &Self) -> bool {
        self.owner == other.owner
    }

    pub(in crate::game_lua::platform_services) fn is_current(&self) -> bool {
        self.session.request_owner_is_current(self.owner)
    }

    pub(in crate::game_lua::platform_services) fn load_cache(&self) -> Result<String, String> {
        // 100698578 -> identity virtual136 -> UserProfile first string.
        // InitFunc_387 establishes accountId, separately from publicAccountId.
        self.session
            .read_game_cache_for_owner(self.owner, &self.registry_path)
    }

    pub(in crate::game_lua::platform_services) fn store_cache(
        &self,
        text: &str,
    ) -> Result<bool, String> {
        self.session
            .store_game_cache_for_owner(self.owner, &self.registry_path, text)
    }

    pub(in crate::game_lua::platform_services) fn local_player(&self) -> Option<(String, String)> {
        self.session.game_local_player(self.owner)
    }

    pub(in crate::game_lua::platform_services) fn post_scores(
        &mut self,
        plaintext: &str,
        current: &dyn Fn() -> bool,
    ) -> Result<(), GameError> {
        if !current() {
            return Err(GameError { status: -1 });
        }
        let access = self
            .session
            .platform_profile_access(&self.config, &mut self.owner)?;
        self.session.check_request_owner(self.owner)?;
        let body = encrypt(&access, plaintext);
        let url = self.config.endpoint.leaderboard_url("score");
        self.session.execute_game(
            &self.config,
            &mut self.owner,
            &PreparedRequest {
                url: &url,
                body: Some(("application/json", body.as_bytes())),
                headers: &[("EM", "1")],
                timeout: REQUEST_TIMEOUT,
                still_current: Some(current),
            },
        )?;
        Ok(())
    }

    pub(in crate::game_lua::platform_services) fn leaderboard(
        &mut self,
        level: &str,
        nocache: bool,
        current: &dyn Fn() -> bool,
    ) -> Result<Response<Body>, GameError> {
        let mut url = self
            .config
            .endpoint
            .leaderboard_url(&format!("level-{level}/query/friends"));
        if nocache {
            url.push_str("?nocache=1");
        }
        Ok(self.session.execute_game(
            &self.config,
            &mut self.owner,
            &PreparedRequest {
                url: &url,
                body: None,
                headers: &[],
                timeout: REQUEST_TIMEOUT,
                still_current: Some(current),
            },
        )?)
    }
}

impl SkynestAccountRuntime {
    pub(in crate::game_lua::platform_services) fn game_identity(
        &self,
    ) -> LuaResult<Option<GameIdentity>> {
        let Some(config) = self.prepared_online_config()? else {
            return Ok(None);
        };
        let Some((owner, account, public_account)) = self
            .session
            .current_game_owner(self.session.request_owner(ProviderLevel::Level2))
        else {
            return Ok(None);
        };
        Ok(Some(GameIdentity {
            registry_path: registry_path(&self.registry_root, &config),
            owner,
            account,
            public_account,
            config,
            session: self.session.clone(),
        }))
    }

    #[cfg(test)]
    pub(crate) fn read_game_cache_for_test(&self, account: &str) -> String {
        let config = self.online_config().expect("isolated test provider");
        session::FriendsFile::for_game_account(
            &registry_path(&self.registry_root, &config),
            account,
        )
        .unwrap()
        .read()
        .unwrap()
    }

    #[cfg(test)]
    pub(crate) fn seed_game_cache_for_test(&self, account: &str, text: &str) {
        let config = self.online_config().expect("isolated test provider");
        session::FriendsFile::for_game_account(
            &registry_path(&self.registry_root, &config),
            account,
        )
        .unwrap()
        .write(text)
        .unwrap();
    }
}
