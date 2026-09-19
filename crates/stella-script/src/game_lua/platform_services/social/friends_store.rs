//! Account-login construction and persisted SkynestFriendsStoreImpl records.
//! Native addresses and the still-missing online refresh phases are indexed in
//! docs/native-social-lifecycle.md. This cache does not establish a connection.

use super::*;
use serde_json::Value;
pub(in crate::game_lua::platform_services) mod protocol;

pub(super) const LOGOUT_REGISTRY_KEY: &str = "stella.social.account-logout";
pub(super) const LOGIN_REGISTRY_KEY: &str = "stella.social.account-login";

#[derive(Clone, Debug, Default)]
struct NetworkProfile {
    network: i32,
    uid: String,
    avatar_url: String,
    name: String,
}

impl NetworkProfile {
    // 10072BFB8: independently typed members, with fresh defaults per entry.
    fn from_cache(value: &Value) -> LuaResult<Self> {
        let number = value.get("socialNetwork").filter(|v| v.is_number());
        let network = number
            .map(super::super::skynest_account::friends_support::profile_integer)
            .transpose()?
            .unwrap_or(0) as i32;
        Ok(Self {
            network,
            uid: string(value, "uid"),
            avatar_url: string(value, "avatarUrl"),
            name: string(value, "name"),
        })
    }

    fn profile_value(&self) -> Value {
        let provider = match self.network {
            1 => "facebook",
            2 => "sinaweibo",
            3 => "gamecenter",
            4 => "kakaotalk",
            _ => "",
        };
        serde_json::json!({"provider":provider, "id":self.uid, "nativeSocialNetwork":self.network,
            "socialAttributes":{"avatarUrl":self.avatar_url,"name":self.name}})
    }
}

fn string(value: &Value, key: &str) -> String {
    value
        .get(key)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_owned()
}

#[derive(Debug)]
pub(super) struct FriendsStore {
    _initial_account_id: String,
    pub(super) context: Option<super::super::skynest_account::friends_support::FriendsClient>,
    pub(super) cache_path: PathBuf,
    pub(super) friends: BTreeMap<String, LocalSocialFriend>,
    // Native owns this second map independently; loading does NOT merge it
    // into game-friend records (that happens after an online platform result).
    platform_profiles: BTreeMap<(i32, String), NetworkProfile>,
    pub(super) platform_pending: u32,
}

impl FriendsStore {
    /// 10073EA4C replaces platform entries by (network, platform ID), then
    /// 10073E7A0 fills only empty fields in already-known game relations.
    pub(super) fn merge_platform_friends(
        &mut self,
        network: SocialNetwork,
        users: Vec<SocialPlatformUser>,
    ) {
        let network = network as i32;
        for user in users {
            let avatar_url = if user.avatar_url.is_empty() {
                protocol::default_avatar_url(network, &user.id)
            } else {
                user.avatar_url
            };
            let name = if user.name.is_empty() {
                user.username
            } else {
                user.name
            };
            self.platform_profiles.insert(
                (network, user.id.clone()),
                NetworkProfile {
                    network,
                    uid: user.id,
                    name,
                    avatar_url,
                },
            );
        }
        for friend in self.friends.values_mut() {
            if let Some(profiles) = friend
                .profile
                .get_mut("socialNetworks")
                .and_then(Value::as_array_mut)
            {
                for profile in profiles.iter_mut() {
                    let number = profile
                        .get("nativeSocialNetwork")
                        .and_then(Value::as_i64)
                        .unwrap_or(0) as i32;
                    let uid = string(profile, "id");
                    if let Some(platform) = self.platform_profiles.get(&(number, uid)) {
                        let attrs = &mut profile["socialAttributes"];
                        if string(attrs, "name").is_empty() {
                            attrs["name"] = Value::String(platform.name.clone());
                        }
                        if string(attrs, "avatarUrl").is_empty() {
                            attrs["avatarUrl"] = Value::String(platform.avatar_url.clone());
                        }
                    }
                }
                friend.name = profiles
                    .iter()
                    .map(|p| string(&p["socialAttributes"], "name"))
                    .find(|n| !n.is_empty())
                    .unwrap_or_default();
            }
        }
    }

    pub(super) fn cache_value(&self) -> Value {
        let friends: Vec<_> = self.friends.values().map(|friend| {
            let mut value = serde_json::json!({"accountId":friend.account_id,"nickName":friend.nickname});
            let profiles: Vec<_> = friend.profile.get("socialNetworks").and_then(Value::as_array)
                .into_iter().flatten().map(|p| {
                    let network = p.get("nativeSocialNetwork").and_then(Value::as_i64).map(|n|n as i32).unwrap_or_else(|| match p["provider"].as_str().unwrap_or_default() {
                        "facebook"=>1,"sinaweibo"=>2,"gamecenter"=>3,"kakaotalk"=>4,_=>0,
                    });
                    serde_json::json!({"socialNetwork":network,"uid":string(p,"id"),
                        "avatarUrl":string(&p["socialAttributes"],"avatarUrl"),"name":string(&p["socialAttributes"],"name")})
                }).collect();
            if !profiles.is_empty() { value["socialNetworkProfiles"] = Value::Array(profiles); }
            value
        }).collect();
        let social: Vec<_> = self
            .platform_profiles
            .values()
            .map(|p| {
                serde_json::json!({
            "socialNetwork":p.network,"uid":p.uid,"avatarUrl":p.avatar_url,"name":p.name})
            })
            .collect();
        serde_json::json!({"friends":friends,"socialNetworkFriends":social})
    }

    pub(super) fn load(account_id: String, text: &str, cache_path: PathBuf) -> LuaResult<Self> {
        // 10055A768 accepts exactly empty input as null. Whitespace/malformed
        // JSON throws; neither is a reason to overwrite a player's cache.
        let root: Value = if text.is_empty() {
            Value::Null
        } else {
            serde_json::from_str(text)
                .map_err(|e| runtime_error(format!("Parsing friends cache JSON failed: {e}")))?
        };
        let mut friends = BTreeMap::new();
        if let Some(entries) = root.get("friends").and_then(Value::as_array) {
            for entry in entries {
                let account_id = string(entry, "accountId");
                let nickname = string(entry, "nickName");
                let profiles: Vec<_> = entry
                    .get("socialNetworkProfiles")
                    .and_then(Value::as_array)
                    .into_iter()
                    .flatten()
                    .map(NetworkProfile::from_cache)
                    .collect::<LuaResult<_>>()?;
                // 100684B9C preference1: first nonempty social name, then nickName.
                let name = profiles
                    .iter()
                    .find(|p| !p.name.is_empty())
                    .map(|p| p.name.clone())
                    .unwrap_or_else(|| nickname.clone());
                let profile = serde_json::json!({"publicAccountId":account_id,
                    "socialNetworks":profiles.iter().map(NetworkProfile::profile_value).collect::<Vec<_>>()});
                // 10072B880 does NOT load avatarId or imageAssets from disk.
                // 10073F580 assigns by ID, retaining the last duplicate, and
                // 10073E160 traverses the resulting ordered map.
                friends.insert(
                    account_id.clone(),
                    LocalSocialFriend {
                        account_id,
                        name,
                        nickname,
                        profile,
                        ..Default::default()
                    },
                );
            }
        }
        let mut platform_profiles = BTreeMap::new();
        if let Some(entries) = root.get("socialNetworkFriends").and_then(Value::as_array) {
            for entry in entries {
                let profile = NetworkProfile::from_cache(entry)?;
                platform_profiles.insert((profile.network, profile.uid.clone()), profile);
            }
        }
        Ok(Self {
            _initial_account_id: account_id,
            context: None,
            cache_path,
            friends,
            platform_profiles,
            platform_pending: 0,
        })
    }
}

impl SocialRuntime {
    pub(crate) fn synchronize_native_context(&self) -> LuaResult<()> {
        let retired = self
            .state
            .lock()
            .map_err(|_| runtime_error("social state lock poisoned"))?
            .friends_store
            .as_ref()
            .and_then(|store| store.context.as_ref())
            .is_some_and(|context| !context.context_is_current());
        if !retired {
            return Ok(());
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
        state.friends_store = None;
        state.connected = false;
        state.local_profile = Value::Null;
        state.avatars.clear();
        state.avatar_paths.clear();
        state.pending_avatars.clear();
        state.completions.clear();
        Ok(())
    }

    pub(super) fn finish_native_friends(
        &self,
        lua: &Lua,
        client: &super::super::skynest_account::friends_support::FriendsClient,
        friends: Vec<LocalSocialFriend>,
        network: Option<SocialNetwork>,
    ) -> LuaResult<()> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| runtime_error("social state lock poisoned"))?;
        let Some(store) = &mut state.friends_store else {
            return Ok(());
        };
        // 100740548 persists game relations BEFORE clearing/fetching platform
        // profiles. A failure of that later platform request does not persist
        // its cleared map and does not undo successful game-friend relations.
        if !client.store_update(|| {
            store.friends = friends
                .into_iter()
                .map(|f| (f.account_id.clone(), f))
                .collect();
            store.cache_value()
        })? {
            return Ok(());
        }
        if let Some(network) = network {
            store
                .platform_profiles
                .retain(|(n, _), _| *n != network as i32);
        } else {
            store.platform_profiles.clear();
        }
        store.platform_pending = 1;
        drop(state);
        self.request_platform_friends(lua, client.clone())
    }

    pub(super) fn get_native_friends_progress(&self, lua: &Lua) -> LuaResult<()> {
        let state = self
            .state
            .lock()
            .map_err(|_| runtime_error("social state lock poisoned"))?;
        let Some(store) = &state.friends_store else {
            return Ok(());
        };
        let ids = store.friends.keys().take(50).cloned().collect::<Vec<_>>();
        let generation = state.provider_generation;
        drop(state);
        if ids.is_empty() {
            return Ok(());
        }
        let runtime = self.clone();
        let callback = lua.create_function(move |lua, values: Option<mlua::Table>| {
            let state = runtime
                .state
                .lock()
                .map_err(|_| runtime_error("social state lock poisoned"))?;
            if state.provider_generation != generation {
                return Ok(());
            }
            let Some(store) = &state.friends_store else {
                return Ok(());
            };
            let result = lua.create_table()?;
            let success = values.is_some();
            if let Some(values) = values {
                let values = values
                    .pairs::<String, String>()
                    .collect::<LuaResult<BTreeMap<_, _>>>()?;
                let mut index = 1;
                for (id, progress) in values {
                    if let Some(friend) = store.friends.get(&id)
                        && !friend.display_name().is_empty()
                    {
                        let entry = lua.create_table()?;
                        entry.set("accountId", id)?;
                        entry.set("nickname", friend.display_name())?;
                        entry.set("progress", progress)?;
                        result.raw_set(index, entry)?;
                        index += 1;
                    }
                }
            }
            drop(state);
            lua.globals()
                .get::<mlua::Table>("SocialManager")?
                .get::<mlua::Function>("onFriendsProgressUpdated")?
                .call::<()>((success, result))
        })?;
        if !self.storage.request_social_progress(lua, ids, callback)? {
            eprintln!("SocialManager: Error getting progress: SkynestStorage not available.");
        }
        Ok(())
    }

    pub(super) fn initialize_friends_store(&self, lua: &Lua) -> LuaResult<()> {
        self.synchronize_native_context()?;
        // 1000C0198 initializes once. Explicit local/compatible providers keep
        // their separate transport contracts and do not imply SDK readiness.
        if self.compatible_url().is_some() {
            return Ok(());
        }
        let mut state = self
            .state
            .lock()
            .map_err(|_| runtime_error("social state lock poisoned"))?;
        if state.local_provider {
            return Ok(());
        }
        if state.friends_store.is_some() {
            drop(state);
            if let Some(client) = self.account.friends_client()? {
                self.refresh_platform_availability(lua, client)?;
            }
            return Ok(());
        }
        let Some((account, text, path)) = self.account.native_friends_cache()? else {
            return Ok(());
        };
        let mut store = FriendsStore::load(account, &text, path)?;
        let Some(client) = self.account.friends_client()? else {
            return Ok(());
        };
        store.context = Some(client.clone());
        state.friends_store = Some(store);
        drop(state);
        self.initialize_platform_readiness(lua, client)
    }
}

#[cfg(test)]
mod tests;
