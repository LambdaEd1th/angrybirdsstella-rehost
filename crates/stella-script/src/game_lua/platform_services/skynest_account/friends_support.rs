//! Native SkynestFriends worker, using the retained Level2 request owner.
use super::super::social::{LocalSocialFriend, friends_store::protocol};
use super::*;

mod platform_connection;

pub(in crate::game_lua::platform_services) fn profile_integer(
    value: &serde_json::Value,
) -> LuaResult<i64> {
    session::profile_integer(value).map_err(runtime_error)
}

#[derive(Clone)]
pub(in crate::game_lua::platform_services) struct FriendsClient {
    session: IdentitySession,
    config: IdentityConfig,
    owner: RequestOwner,
    registry_path: PathBuf,
}

impl std::fmt::Debug for FriendsClient {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("FriendsClient")
    }
}

#[derive(Clone, Debug)]
pub(in crate::game_lua::platform_services) struct FriendsError {
    pub(in crate::game_lua::platform_services) code: u8,
    pub(in crate::game_lua::platform_services) detail: String,
}

impl FriendsError {
    fn exception(detail: impl ToString) -> Self {
        Self {
            code: 2,
            detail: detail.to_string(),
        }
    }
}

impl FriendsClient {
    pub(in crate::game_lua::platform_services) fn context_is_current(&self) -> bool {
        self.session
            .request_owner_is_current(self.owner.epoch_only())
    }

    pub(in crate::game_lua::platform_services) fn is_current(&self) -> bool {
        self.session.request_owner_is_current(self.owner)
    }

    pub(in crate::game_lua::platform_services) fn fetch(
        mut self,
    ) -> (Self, Result<Vec<LocalSocialFriend>, FriendsError>) {
        let result = self.fetch_inner();
        (self, result)
    }

    fn fetch_inner(&mut self) -> Result<Vec<LocalSocialFriend>, FriendsError> {
        let response = self
            .session
            .execute_get_for_owner(
                &self.config,
                &mut self.owner,
                ProviderLevel::Level2,
                "friends",
            )
            .map_err(FriendsError::exception)?;
        if response.status().as_u16() != 200 {
            return Err(FriendsError {
                code: 1,
                detail: format!("friends returned HTTP {}", response.status().as_u16()),
            });
        }
        let mut friends = protocol::parse_relations(&response_text(response)?)
            .map_err(FriendsError::exception)?;
        if !friends.is_empty() {
            // 10068BF5C is ordered vector FormDataBody, not a map: repeats,
            // empty IDs, and input order all survive to the wire.
            let ids = protocol::account_ids(&friends);
            let body = ids
                .iter()
                .map(|id| format!("publicAccountId={}", form_component(id)))
                .collect::<Vec<_>>()
                .join("&");
            let response = self
                .session
                .execute_form_for_owner(
                    &self.config,
                    &mut self.owner,
                    ProviderLevel::Level2,
                    "profile/search",
                    &body,
                )
                .map_err(FriendsError::exception)?;
            if response.status().as_u16() != 200 {
                return Err(FriendsError::exception(format!(
                    "profile/search returned HTTP {}",
                    response.status().as_u16()
                )));
            }
            protocol::merge_avatar_profiles(&mut friends, &response_text(response)?)
                .map_err(FriendsError::exception)?;
        }
        if !self.is_current() {
            return Err(FriendsError::exception("identity request cancelled"));
        }
        Ok(friends)
    }

    pub(in crate::game_lua::platform_services) fn store_update(
        &self,
        update: impl FnOnce() -> serde_json::Value,
    ) -> LuaResult<bool> {
        self.session
            .store_friends_cache_for_owner(self.owner, &self.registry_path, update)
            .map_err(runtime_error)
    }
}

fn response_text(mut response: ureq::http::Response<ureq::Body>) -> Result<String, FriendsError> {
    const MAX: u64 = 8 * 1024 * 1024;
    let mut bytes = Vec::new();
    response
        .body_mut()
        .as_reader()
        .take(MAX + 1)
        .read_to_end(&mut bytes)
        .map_err(FriendsError::exception)?;
    if bytes.len() as u64 > MAX {
        return Err(FriendsError::exception(
            "friends response exceeds host byte limit",
        ));
    }
    String::from_utf8(bytes).map_err(FriendsError::exception)
}

impl SkynestAccountRuntime {
    #[cfg(test)]
    pub(crate) fn friends_profile_for_test(&self) -> Option<serde_json::Value> {
        self.session.profile().map(|profile| profile.raw)
    }
    #[cfg(test)]
    pub(crate) fn read_friends_cache_for_test(&self, account: &str) -> String {
        let config = self.online_config().expect("isolated test provider");
        session::FriendsFile::for_account(&registry_path(&self.registry_root, &config), account)
            .unwrap()
            .read()
            .unwrap()
    }

    #[cfg(test)]
    pub(crate) fn seed_friends_cache_for_test(&self, account: &str, text: &str) {
        let config = self.online_config().expect("isolated test provider");
        session::FriendsFile::for_account(&registry_path(&self.registry_root, &config), account)
            .unwrap()
            .write(text)
            .unwrap();
    }

    pub(in crate::game_lua::platform_services) fn native_friends_cache(
        &self,
    ) -> LuaResult<Option<(String, String, PathBuf)>> {
        let Some(config) = self.prepared_online_config()? else {
            return Ok(None);
        };
        let Some(profile) = self.session.profile() else {
            return Ok(None);
        };
        let path = registry_path(&self.registry_root, &config);
        let file = session::FriendsFile::for_account(&path, &profile.public_account_id)
            .map_err(runtime_error)?;
        let cache = file.read().map_err(runtime_error)?;
        Ok(Some((
            profile.public_account_id,
            cache,
            file.path().to_owned(),
        )))
    }

    pub(in crate::game_lua::platform_services) fn friends_client(
        &self,
    ) -> LuaResult<Option<FriendsClient>> {
        Ok(self.prepared_online_config()?.map(|config| FriendsClient {
            registry_path: registry_path(&self.registry_root, &config),
            owner: self.session.request_owner(ProviderLevel::Level2),
            config,
            session: self.session.clone(),
        }))
    }
}
