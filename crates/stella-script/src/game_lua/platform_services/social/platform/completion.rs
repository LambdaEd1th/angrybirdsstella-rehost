//! Application-thread platform publication, callbacks and persisted merge.
use super::*;

impl SocialRuntime {
    pub(in crate::game_lua::platform_services::social) fn finish_platform(
        &self,
        lua: &Lua,
        mut completion: PlatformCompletion,
    ) -> LuaResult<()> {
        // 779928 ->77998C dispatches cache assignment onto the main queue.
        // Both initial consumers are admitted before either completion caches.
        match &mut completion {
            PlatformCompletion::ServiceProfile { lease, result }
            | PlatformCompletion::Available { lease, result, .. }
            | PlatformCompletion::Checked { lease, result, .. }
            | PlatformCompletion::ConnectProfile { lease, result, .. }
                if lease.is_current() =>
            {
                if let Ok(profile) = result {
                    *result = lease.provider.publish_completed_profile(profile);
                }
            }
            _ => (),
        }
        match completion {
            // 77A228 only installs the preferred account name on success.
            // It does not own the separate C++ login callback or report a
            // connection failure; that consumer performs its own profile read.
            PlatformCompletion::ServiceProfile { .. } => {}
            PlatformCompletion::Available {
                lease,
                client,
                account_id,
                linked_id,
                result,
            } => {
                if !lease.is_current() || !result.is_ok_and(|p| p.user.id == linked_id) {
                    return Ok(());
                }
                let Some((_, profile)) = client
                    .current_social_user()
                    .filter(|(id, _)| *id == account_id)
                else {
                    return Ok(());
                };
                if !profile["socialNetworks"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .find(|p| p["provider"].as_str() == Some("facebook"))
                    .is_some_and(|p| p["id"].as_str() == Some(linked_id.as_str()))
                {
                    return Ok(());
                }
                self.publish_platform_connection(lua, &client)?;
            }
            PlatformCompletion::Refresh { lease, client } => {
                if lease.is_current() && client.is_current() {
                    self.refresh_native_friends(client, Some(SocialNetwork::Facebook))?;
                }
            }
            PlatformCompletion::Checked {
                lease,
                client,
                linked_id,
                result,
            } => {
                if !lease.is_current() || !client.is_current() {
                    return Ok(());
                }
                if !result
                    .as_ref()
                    .is_ok_and(|profile| profile.user.id == linked_id)
                {
                    let mut state = self
                        .platform
                        .lock()
                        .map_err(|_| runtime_error("platform state lock poisoned"))?;
                    state.readiness = state.readiness.saturating_sub(1);
                    return Ok(());
                }
                self.begin_platform_connect(lua, client, ConnectionConsumer::Readiness)?;
            }
            PlatformCompletion::Login {
                lease,
                client,
                consumer,
                result,
            } => {
                if !lease.is_current() || !client.is_current() {
                    return Ok(());
                }
                match result {
                    Ok(()) => self.request_connect_profile(lua, lease, client, consumer)?,
                    Err(error) => self.finish_platform(
                        lua,
                        PlatformCompletion::Connected {
                            lease,
                            client,
                            consumer,
                            result: Err(error.to_string()),
                        },
                    )?,
                }
            }
            PlatformCompletion::ConnectProfile {
                lease,
                client,
                consumer,
                result,
            } => {
                if !lease.is_current() || !client.is_current() {
                    return Ok(());
                }
                let profile = match result {
                    Ok(profile) => profile,
                    Err(error) => {
                        return self.finish_platform(
                            lua,
                            PlatformCompletion::Connected {
                                lease,
                                client,
                                consumer,
                                result: Err(error.to_string()),
                            },
                        );
                    }
                };
                self.spawn_online("platform-connect", move || {
                    let mut client = client;
                    let result = (|| {
                        if !lease.is_current() {
                            return Err(SocialPlatformError::Cancelled.to_string());
                        }
                        let friends = lease
                            .provider
                            .friends(SocialFriendDetails::Identifiers)
                            .map_err(|e| e.to_string())?;
                        let ids = friends
                            .users
                            .into_iter()
                            .map(|user| user.id)
                            .collect::<Vec<_>>();
                        client.connect_platform(
                            SocialNetwork::Facebook,
                            &profile,
                            &ids,
                            &lease.current,
                        )
                    })();
                    OnlineCompletion::platform(PlatformCompletion::Connected {
                        lease,
                        client,
                        consumer,
                        result,
                    })
                })?;
            }
            PlatformCompletion::Connected {
                lease,
                client,
                consumer,
                result,
            } => {
                if !lease.is_current() || !client.is_current() {
                    return Ok(());
                }
                let mut state = self
                    .platform
                    .lock()
                    .map_err(|_| runtime_error("platform state lock poisoned"))?;
                state.connecting = false;
                state.connection_owner = None;
                state.connected = result.is_ok();
                // 734794 sets state, invokes retained readiness callback, then
                // ONLY on success posts the store's C2B950(network) refresh.
                if matches!(consumer, ConnectionConsumer::Readiness) {
                    state.readiness = state.readiness.saturating_sub(1);
                }
                let ready = state.readiness == 0;
                drop(state);
                if result.is_ok() && matches!(consumer, ConnectionConsumer::Lua) {
                    self.publish_platform_connection(lua, &client)?;
                }
                match result {
                    Ok(()) if ready => {
                        let generation = self
                            .state
                            .lock()
                            .map_err(|_| runtime_error("social state lock poisoned"))?
                            .provider_generation;
                        self.online_completions
                            .lock()
                            .map_err(|_| runtime_error("social queue lock poisoned"))?
                            .push_back((
                                generation,
                                OnlineCompletion::platform(PlatformCompletion::Refresh {
                                    lease,
                                    client,
                                }),
                            ));
                        self.application_events.post(ApplicationEvent::SocialOnline);
                    }
                    Err(error) => eprintln!("native platform connection failed: {error}"),
                    _ => (),
                }
            }
            PlatformCompletion::Friends {
                lease,
                client,
                result,
            } => {
                if !lease.is_current() || !client.is_current() {
                    return Ok(());
                }
                self.finish_platform_friends(lua, &client, result)?;
            }
        }
        Ok(())
    }

    pub(super) fn finish_platform_friends(
        &self,
        lua: &Lua,
        client: &FriendsClient,
        result: Result<SocialPlatformFriends, SocialPlatformError>,
    ) -> LuaResult<()> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| runtime_error("social state lock poisoned"))?;
        let Some(store) = &mut state.friends_store else {
            return Ok(());
        };
        // 740374 decrements pending BEFORE merging and saving. Failed platform
        // requests do not overwrite the just-persisted game relation snapshot.
        store.platform_pending = store.platform_pending.saturating_sub(1);
        match result {
            Ok(friends) => {
                if !client.store_update(|| {
                    store.merge_platform_friends(SocialNetwork::Facebook, friends.users);
                    store.cache_value()
                })? {
                    return Ok(());
                }
            }
            Err(error) => eprintln!("native platform friends failed: {error}"),
        }
        let ready = store.platform_pending == 0;
        drop(state);
        if ready {
            self.get_native_friends_progress(lua)?;
        }
        Ok(())
    }
}
