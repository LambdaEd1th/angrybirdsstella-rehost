//! 731C68 connection admission; SDK readiness and Lua success are distinct.
use super::*;

impl SocialRuntime {
    pub(in crate::game_lua::platform_services::social) fn connect_native_platform(
        &self,
        lua: &Lua,
    ) -> LuaResult<()> {
        self.synchronize_native_context()?;
        // 0C3748 has no Friends wrapper before the account-login constructor.
        if self
            .state
            .lock()
            .map_err(|_| runtime_error("social state lock poisoned"))?
            .friends_store
            .is_none()
        {
            return Ok(());
        }
        if let Some(client) = self.account.friends_client()? {
            self.begin_platform_connect(lua, client, ConnectionConsumer::Lua)?;
        }
        Ok(())
    }

    pub(super) fn begin_platform_connect(
        &self,
        lua: &Lua,
        client: FriendsClient,
        consumer: ConnectionConsumer,
    ) -> LuaResult<()> {
        if !client.is_current() {
            return Ok(());
        }
        let mut state = self
            .platform
            .lock()
            .map_err(|_| runtime_error("platform state lock poisoned"))?;
        let lease = state.session.clone();
        if state.connecting || lease.is_none() {
            // 731C68 rejects a pending connection without replacing callbacks.
            // The application caller's failure callback is deliberately empty.
            if matches!(consumer, ConnectionConsumer::Readiness) {
                state.readiness = state.readiness.saturating_sub(1);
            }
            return Ok(());
        }
        let lease = lease.expect("checked platform lease");
        state.connecting = true;
        state.connection_owner = Some(client.clone());
        drop(state);
        if lease.provider.is_logged_in() {
            return self.request_connect_profile(lua, lease, client, consumer);
        }
        // Native platform login completion739098 either starts731EF0 or
        // reports failure through734794. No identity/Graph request precedes it.
        let request = match lease.provider.clone().prepare_login() {
            SocialLoginRequest::Ready(result) => {
                self.start_login_service_profile(lua, &lease.provider)?;
                return self.finish_platform(
                    lua,
                    PlatformCompletion::Login {
                        lease,
                        client,
                        consumer,
                        result,
                    },
                );
            }
            SocialLoginRequest::Pending(request) => request,
            SocialLoginRequest::AwaitingCallback => {
                self.platform
                    .lock()
                    .map_err(|_| runtime_error("platform state lock poisoned"))?
                    .callback_login = Some((lease.clone(), client, consumer));
                return self.finish_callback_login(lua, &lease.provider);
            }
        };
        self.spawn_online("platform-login", move || {
            OnlineCompletion::platform(PlatformCompletion::Login {
                lease,
                client,
                consumer,
                result: request(),
            })
        })
    }

    pub(super) fn current_platform_provider(
        &self,
    ) -> LuaResult<Option<Arc<dyn SocialPlatformProvider>>> {
        Ok(self
            .platform
            .lock()
            .map_err(|_| runtime_error("platform state lock poisoned"))?
            .session
            .as_ref()
            .map(|lease| lease.provider.clone()))
    }

    pub(crate) fn handle_platform_open_url(&self, lua: &Lua, url: &str) -> LuaResult<bool> {
        let Some(provider) = self.current_platform_provider()? else {
            return Ok(false);
        };
        let handled = provider.handle_open_url(url).map_err(runtime_error)?;
        self.finish_callback_login(lua, &provider)?;
        Ok(handled)
    }

    pub(crate) fn handle_platform_login_dialog_event(
        &self,
        lua: &Lua,
        event: &crate::FacebookLoginDialogEvent,
    ) -> LuaResult<bool> {
        let Some(provider) = self.current_platform_provider()? else {
            return Ok(false);
        };
        let handled = provider
            .handle_login_dialog_event(event)
            .map_err(runtime_error)?;
        self.finish_callback_login(lua, &provider)?;
        Ok(handled)
    }

    pub(super) fn finish_callback_login(
        &self,
        lua: &Lua,
        provider: &Arc<dyn SocialPlatformProvider>,
    ) -> LuaResult<()> {
        self.start_login_service_profile(lua, provider)?;
        let mut state = self
            .platform
            .lock()
            .map_err(|_| runtime_error("platform state lock poisoned"))?;
        let Some((lease, _, _)) = state.callback_login.as_ref() else {
            return Ok(());
        };
        if !Arc::ptr_eq(&lease.provider, provider) {
            return Ok(());
        }
        let Some(result) = provider.take_login_completion() else {
            return Ok(());
        };
        let (lease, client, consumer) =
            state.callback_login.take().expect("checked callback owner");
        drop(state);
        self.finish_platform(
            lua,
            PlatformCompletion::Login {
                lease,
                client,
                consumer,
                result,
            },
        )
    }

    pub(super) fn start_login_service_profile(
        &self,
        lua: &Lua,
        provider: &Arc<dyn SocialPlatformProvider>,
    ) -> LuaResult<()> {
        let lease = self
            .platform
            .lock()
            .map_err(|_| runtime_error("platform state lock poisoned"))?
            .session
            .clone();
        let Some(lease) = lease.filter(|lease| Arc::ptr_eq(&lease.provider, provider)) else {
            return Ok(());
        };
        let Some(request) = provider.clone().take_login_profile_request() else {
            return Ok(());
        };
        match request {
            SocialProfileRequest::Ready(result) => {
                self.finish_platform(lua, PlatformCompletion::ServiceProfile { lease, result })
            }
            SocialProfileRequest::Pending(request) => {
                self.spawn_online("platform-service-profile", move || {
                    OnlineCompletion::platform(PlatformCompletion::ServiceProfile {
                        lease,
                        result: request(),
                    })
                })
            }
        }
    }

    pub(super) fn request_connect_profile(
        &self,
        lua: &Lua,
        lease: PlatformLease,
        client: FriendsClient,
        consumer: ConnectionConsumer,
    ) -> LuaResult<()> {
        let request = match lease.provider.clone().prepare_user_profile() {
            SocialProfileRequest::Ready(result) => {
                return self.finish_platform(
                    lua,
                    PlatformCompletion::ConnectProfile {
                        lease,
                        client,
                        consumer,
                        result,
                    },
                );
            }
            SocialProfileRequest::Pending(request) => request,
        };
        self.spawn_online("platform-connect-profile", move || {
            OnlineCompletion::platform(PlatformCompletion::ConnectProfile {
                lease,
                client,
                consumer,
                result: request(),
            })
        })
    }

    pub(super) fn publish_platform_connection(
        &self,
        lua: &Lua,
        client: &FriendsClient,
    ) -> LuaResult<()> {
        let Some((account_id, profile)) = client.current_social_user() else {
            return Ok(());
        };
        let name = profile["socialNetworks"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|p| p["socialAttributes"]["name"].as_str())
            .find(|name| !name.is_empty())
            .unwrap_or_default()
            .to_owned();
        {
            let mut state = self
                .state
                .lock()
                .map_err(|_| runtime_error("social state lock poisoned"))?;
            state.local_account_id = account_id;
            state.local_player_name = name;
            state.local_profile = profile;
            state.connected = true;
        }
        // 0C50D0 /0C5378: current local profile -> set insertion -> Lua.
        lua.globals()
            .get::<mlua::Table>("SocialManager")?
            .get::<mlua::Function>("onSocialNetworkConnected")?
            .call::<()>("facebook")
    }
}
