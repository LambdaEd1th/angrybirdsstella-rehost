//! Social avatar cache and graphics-memory state recovered from Purple.

use super::*;
use stella_assets::ka3d::SpriteRegion;

const PREFERRED_LOCAL_AVATAR: &str = "TORUNAMENT_AVATAR_STELLA";
const FALLBACK_LOCAL_AVATAR: &str = "skynestdata/images/socialnetwork/facebook@2x.png";

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) enum AvatarStage {
    #[default]
    New,
    Downloading,
    Cached,
    Loaded,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum LoadResult {
    None,
    DownloadQueued,
    Loaded,
}

impl SocialRuntime {
    pub(super) fn load_avatar(&self, account_id: &str) -> LuaResult<LoadResult> {
        self.synchronize_native_context()?;
        if self.compatible_url().is_some()
            || self
                .state
                .lock()
                .map_err(|_| runtime_error("social state lock poisoned"))?
                .friends_store
                .is_some()
        {
            return self.load_online_avatar(account_id);
        }
        let action = {
            let state = self
                .state
                .lock()
                .map_err(|_| runtime_error("social state lock poisoned"))?;
            if (!state.local_provider && self.compatible_url().is_none()) || !state.connected {
                return Ok(LoadResult::None);
            }
            drop(state);
            if !self.is_known_account(account_id) {
                return Ok(LoadResult::None);
            }
            let mut state = self
                .state
                .lock()
                .map_err(|_| runtime_error("social state lock poisoned"))?;
            match state.avatars.get(account_id).copied().unwrap_or_default() {
                AvatarStage::New => {
                    state
                        .avatars
                        .insert(account_id.to_owned(), AvatarStage::Downloading);
                    state.completions.push_back(Completion::AvatarDownloaded {
                        account_id: account_id.to_owned(),
                    });
                    self.application_events.post(ApplicationEvent::SocialLocal);
                    LoadResult::DownloadQueued
                }
                AvatarStage::Cached => LoadResult::Loaded,
                AvatarStage::Downloading | AvatarStage::Loaded => LoadResult::None,
            }
        };

        if action == LoadResult::Loaded {
            self.publish_avatar(account_id)?;
            self.state
                .lock()
                .map_err(|_| runtime_error("social state lock poisoned"))?
                .avatars
                .insert(account_id.to_owned(), AvatarStage::Loaded);
        }
        Ok(action)
    }

    fn load_online_avatar(&self, account_id: &str) -> LuaResult<LoadResult> {
        use super::super::skynest_account::avatar_support::avatar_url;
        let provider = self.compatible_url();
        let mut state = self
            .state
            .lock()
            .map_err(|_| runtime_error("social state lock poisoned"))?;
        if state.friends_store.is_none() && !state.connected {
            return Ok(LoadResult::None);
        }
        let native_profile = state.friends_store.as_ref().and_then(|store| {
            self.account.avatar_profile(account_id).or_else(|| {
                store
                    .friends
                    .get(account_id)
                    .map(|friend| friend.profile.clone())
            })
        });
        let profile = if let Some(profile) = native_profile {
            profile
        } else if state.friends_store.is_some() {
            return Ok(LoadResult::None);
        } else if account_id == state.local_account_id {
            self.account
                .avatar_profile(account_id)
                .unwrap_or_else(|| state.local_profile.clone())
        } else if let Some(friend) = state
            .document
            .friends
            .iter()
            .find(|friend| friend.account_id == account_id)
        {
            friend.profile.clone()
        } else {
            return Ok(LoadResult::None);
        };
        match state.avatars.get(account_id).copied().unwrap_or_default() {
            AvatarStage::Downloading | AvatarStage::Loaded => return Ok(LoadResult::None),
            AvatarStage::Cached => {
                drop(state);
                self.publish_avatar(account_id)?;
                self.state
                    .lock()
                    .map_err(|_| runtime_error("social state lock poisoned"))?
                    .avatars
                    .insert(account_id.to_owned(), AvatarStage::Loaded);
                return Ok(LoadResult::Loaded);
            }
            AvatarStage::New => {}
        }
        let url = avatar_url(&profile, 1, 64).map_err(runtime_error)?;
        if state.avatar_cache.is_none() {
            let root = if let Some(store) = &state.friends_store {
                store
                    .cache_path
                    .parent()
                    .ok_or_else(|| runtime_error("social cache root unavailable"))?
                    .to_owned()
            } else {
                let Some(provider) = provider else {
                    return Ok(LoadResult::None);
                };
                let scope = crate::game_lua::platform::upper_hex(
                    &crate::game_lua::platform::sha1_digest(provider.as_bytes()),
                );
                state
                    .persistence_path
                    .parent()
                    .ok_or_else(|| runtime_error("social cache root unavailable"))?
                    .join("social-providers")
                    .join(scope)
            };
            state.avatar_cache = Some(
                super::avatar_cache::AvatarCache::new(root, self.account.sdk_log_sink())
                    .map_err(runtime_error)?,
            );
        }
        state
            .avatars
            .insert(account_id.to_owned(), AvatarStage::Downloading);
        let pending = state.pending_avatars.entry(url.clone()).or_default();
        pending.push(account_id.to_owned());
        if pending.len() == 1 {
            let generation = state.provider_generation;
            let queue = self.online_completions.clone();
            let scheduler = self.application_events.clone();
            let completed_url = url.clone();
            let result = state
                .avatar_cache
                .as_ref()
                .expect("constructed avatar cache")
                .request(url.clone(), move |result| {
                    queue
                        .lock()
                        .expect("social online completion lock poisoned")
                        .push_back((
                            generation,
                            OnlineCompletion::AvatarFetched {
                                url: completed_url,
                                result,
                            },
                        ));
                    scheduler.post(ApplicationEvent::SocialOnline);
                });
            if let Err(error) = result {
                for account in state.pending_avatars.remove(&url).unwrap_or_default() {
                    state.avatars.insert(account, AvatarStage::New);
                }
                return Err(runtime_error(error));
            }
        }
        Ok(LoadResult::DownloadQueued)
    }

    pub(super) fn finish_avatar_download(&self, account_id: &str) -> LuaResult<bool> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| runtime_error("social state lock poisoned"))?;
        let stage = state.avatars.get(account_id).copied().unwrap_or_default();
        if stage != AvatarStage::Downloading {
            return Ok(false);
        }
        state
            .avatars
            .insert(account_id.to_owned(), AvatarStage::Cached);
        Ok(true)
    }

    pub(super) fn unload_avatar(&self, account_id: &str) -> LuaResult<()> {
        let was_loaded = {
            let mut state = self
                .state
                .lock()
                .map_err(|_| runtime_error("social state lock poisoned"))?;
            if state.avatars.get(account_id) != Some(&AvatarStage::Loaded) {
                false
            } else {
                state
                    .avatars
                    .insert(account_id.to_owned(), AvatarStage::Cached);
                true
            }
        };
        if was_loaded {
            self.remove_avatar_resource(account_id)?;
        }
        Ok(())
    }

    pub(super) fn unload_all_avatars(&self) -> LuaResult<()> {
        let loaded = {
            let mut state = self
                .state
                .lock()
                .map_err(|_| runtime_error("social state lock poisoned"))?;
            let loaded = state
                .avatars
                .iter()
                .filter_map(|(account_id, stage)| {
                    (*stage == AvatarStage::Loaded).then_some(account_id.clone())
                })
                .collect::<Vec<_>>();
            for account_id in &loaded {
                state
                    .avatars
                    .insert(account_id.clone(), AvatarStage::Cached);
            }
            loaded
        };
        for account_id in loaded {
            self.remove_avatar_resource(&account_id)?;
        }
        Ok(())
    }

    fn publish_avatar(&self, account_id: &str) -> LuaResult<()> {
        let resource_name = avatar_resource_name(account_id);
        let path = self
            .state
            .lock()
            .map_err(|_| runtime_error("social state lock poisoned"))?
            .avatar_paths
            .get(account_id)
            .cloned();
        if let Some(path) = path {
            // 1000C1CD8 constructs the Image on the second native load. Decode
            // before graphics publication/success, and retain pixels through
            // later file replacement and deferred command execution.
            let bytes = std::fs::read(&path).map_err(runtime_error)?;
            let image = Arc::new(
                stella_assets::native_image::decode_native_image(
                    &bytes,
                    path.extension().and_then(|s| s.to_str()),
                )
                .map_err(runtime_error)?,
            );
            let width = i16::try_from(image.width)
                .map_err(|_| runtime_error("avatar image width exceeds host sprite extent"))?;
            let height = i16::try_from(image.height)
                .map_err(|_| runtime_error("avatar image height exceeds host sprite extent"))?;
            let sprite = SpriteRegion {
                name: resource_name.clone(),
                x: 0,
                y: 0,
                width,
                height,
                pivot_x: width / 2,
                pivot_y: height / 2,
                atlas_rotation: 0,
            };
            self.resource_runtime
                .lock()
                .map_err(|_| runtime_error("resource runtime lock poisoned"))?
                .replace_downloaded_avatar_sprite(
                    &resource_name,
                    sprite,
                    path.to_string_lossy().into_owned(),
                    image,
                    &self.data_root,
                );
            return Ok(());
        }
        if self.compatible_url().is_some() {
            return Err(runtime_error("online avatar cache path is missing"));
        }
        let mut resources = self
            .resource_runtime
            .lock()
            .map_err(|_| runtime_error("resource runtime lock poisoned"))?;
        let (mut sprite, texture_source) = resources
            .active_atlas_catalog_region(PREFERRED_LOCAL_AVATAR, &self.data_root)
            .map(|region| (region.sprite.clone(), region.texture_source.clone()))
            .unwrap_or_else(|| {
                (
                    SpriteRegion {
                        name: resource_name.clone(),
                        x: 0,
                        y: 0,
                        width: 64,
                        height: 64,
                        pivot_x: 32,
                        pivot_y: 32,
                        atlas_rotation: 0,
                    },
                    self.data_root
                        .join(FALLBACK_LOCAL_AVATAR)
                        .to_string_lossy()
                        .into_owned(),
                )
            });
        sprite.name = resource_name.clone();
        resources.replace_dynamic_atlas_sprite(
            &resource_name,
            sprite,
            texture_source,
            &self.data_root,
        );
        Ok(())
    }

    fn remove_avatar_resource(&self, account_id: &str) -> LuaResult<()> {
        self.resource_runtime
            .lock()
            .map_err(|_| runtime_error("resource runtime lock poisoned"))?
            .remove_dynamic_atlas_sprite(&avatar_resource_name(account_id));
        Ok(())
    }
}

fn avatar_resource_name(account_id: &str) -> String {
    format!("AVATAR_{account_id}")
}
