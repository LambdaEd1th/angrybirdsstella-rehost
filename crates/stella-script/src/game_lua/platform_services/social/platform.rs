//! Facebook platform provider and SkynestFriends SDK session consumer.
//! Native ordering/address index: docs/native-social-platform.md.
use super::super::skynest_account::{IdentityLifetime, friends_support::FriendsClient};
use super::*;
mod completion;
mod connection;

#[derive(Clone, Copy, Debug)]
pub(super) enum ConnectionConsumer {
    Readiness,
    Lua,
}

#[derive(Clone)]
pub(super) struct PlatformLease {
    provider: Arc<dyn SocialPlatformProvider>,
    current: Arc<Mutex<bool>>,
}
impl std::fmt::Debug for PlatformLease {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("PlatformLease")
    }
}
impl PlatformLease {
    fn is_current(&self) -> bool {
        *self.current.lock().expect("platform lease lock poisoned")
    }
}

#[derive(Default)]
pub(super) struct PlatformState {
    session: Option<PlatformLease>,
    sdk_current: Option<Arc<Mutex<bool>>>,
    readiness: u32,
    connecting: bool,
    connection_owner: Option<FriendsClient>,
    callback_login: Option<(PlatformLease, FriendsClient, ConnectionConsumer)>,
    connected: bool,
}
impl Drop for PlatformState {
    fn drop(&mut self) {
        if let Some(current) = &self.sdk_current {
            *current.lock().expect("platform SDK lifetime lock poisoned") = false;
        }
        if let Some(session) = &self.session {
            *session
                .current
                .lock()
                .expect("platform lease lock poisoned") = false;
        }
    }
}

#[derive(Clone, Debug)]
pub(super) enum PlatformCompletion {
    ServiceProfile {
        lease: PlatformLease,
        result: Result<SocialPlatformProfile, SocialPlatformError>,
    },
    Login {
        lease: PlatformLease,
        client: FriendsClient,
        consumer: ConnectionConsumer,
        result: Result<(), SocialPlatformError>,
    },
    ConnectProfile {
        lease: PlatformLease,
        client: FriendsClient,
        consumer: ConnectionConsumer,
        result: Result<SocialPlatformProfile, SocialPlatformError>,
    },
    Available {
        lease: PlatformLease,
        client: FriendsClient,
        account_id: String,
        linked_id: String,
        result: Result<SocialPlatformProfile, SocialPlatformError>,
    },
    Refresh {
        lease: PlatformLease,
        client: FriendsClient,
    },
    Checked {
        lease: PlatformLease,
        client: FriendsClient,
        linked_id: String,
        result: Result<SocialPlatformProfile, SocialPlatformError>,
    },
    Connected {
        lease: PlatformLease,
        client: FriendsClient,
        consumer: ConnectionConsumer,
        result: Result<(), String>,
    },
    Friends {
        lease: PlatformLease,
        client: FriendsClient,
        result: Result<SocialPlatformFriends, SocialPlatformError>,
    },
}

impl SocialRuntime {
    #[cfg(test)]
    pub(crate) fn platform_state_for_test(&self) -> (u32, bool, bool) {
        let state = self.platform.lock().unwrap();
        (state.readiness, state.connecting, state.connected)
    }
    pub(super) fn retire_platform_jobs(&self) -> LuaResult<()> {
        let mut state = self
            .platform
            .lock()
            .map_err(|_| runtime_error("platform state lock poisoned"))?;
        if let Some(old) = state.session.take() {
            *old.current.lock().expect("platform lease lock poisoned") = false;
            state.session = Some(PlatformLease {
                provider: old.provider,
                current: Arc::new(Mutex::new(true)),
            });
        }
        state.connecting = false;
        state.connection_owner = None;
        state.callback_login = None;
        state.connected = false;
        state.readiness = 0;
        Ok(())
    }

    pub(super) fn logout_account_platform(&self, network: i32) -> LuaResult<()> {
        // Retire outstanding consumers even when another external provider is
        // selected. Logout itself addresses only the active platform service.
        self.retire_platform_jobs()?;
        let provider = self
            .platform
            .lock()
            .map_err(|_| runtime_error("platform state lock poisoned"))?
            .session
            .as_ref()
            .map(|lease| lease.provider.clone());
        if network == SocialNetwork::Facebook as i32
            && let Some(provider) = provider
        {
            provider.logout().map_err(runtime_error)?;
        }
        Ok(())
    }

    pub(crate) fn set_facebook_session(
        &self,
        lua: &Lua,
        provider: Option<Arc<dyn SocialPlatformProvider>>,
    ) -> LuaResult<()> {
        let installed_provider = provider.clone();
        let sdk_current = Arc::new(Mutex::new(true));
        {
            let mut state = self
                .platform
                .lock()
                .map_err(|_| runtime_error("platform state lock poisoned"))?;
            if let Some(old) = state.session.take() {
                *old.current.lock().expect("platform lease lock poisoned") = false;
            }
            if let Some(old) = state.sdk_current.take() {
                *old.lock().expect("platform SDK lifetime lock poisoned") = false;
            }
            if provider.is_some() {
                state.sdk_current = Some(sdk_current.clone());
            }
            state.session = provider.map(|provider| PlatformLease {
                provider,
                current: Arc::new(Mutex::new(true)),
            });
            state.connecting = false;
            state.connection_owner = None;
            state.callback_login = None;
            state.connected = false;
        }
        let native_transport = self.compatible_url().is_none();
        let initialized = {
            let mut state = self
                .state
                .lock()
                .map_err(|_| runtime_error("social state lock poisoned"))?;
            if native_transport && !state.local_provider {
                state.connected = false;
            }
            if let Some(store) = &mut state.friends_store {
                store.platform_pending = 0;
            }
            state.friends_store.is_some()
        };
        if let Some(provider) = installed_provider {
            let queue = Arc::downgrade(&self.online_completions);
            let events = self.application_events.clone();
            provider.set_application_dispatcher(Arc::new(move |task| {
                if let Some(queue) = queue.upgrade() {
                    let mut pending = queue
                        .lock()
                        .expect("social online completion lock poisoned");
                    pending.push_back((
                        0,
                        OnlineCompletion::PlatformSdk {
                            current: sdk_current.clone(),
                            task,
                        },
                    ));
                    events.post(ApplicationEvent::SocialOnline);
                }
            }));
            // FacebookService.init opens a valid cached session without UI
            // and primes /me before the separate Friends consumer7792B8.
            self.start_login_service_profile(lua, &provider)?;
        }
        if initialized && let Some(client) = self.account.friends_client()? {
            self.check_platform_connection(lua, client.clone())?;
            self.refresh_platform_availability(lua, client)?;
        }
        Ok(())
    }

    pub(crate) fn post_application_resumed(&self) {
        self.application_events
            .post(ApplicationEvent::ApplicationResumed);
    }

    pub(crate) fn application_resumed(&self, lua: &Lua) -> LuaResult<()> {
        // The platform manager is configured independently of account login.
        // 7854E4 ->779690 ->319F70 cancels pending external authorization
        // before the account-gated Friends subscriber does its recheck.
        if let Some(provider) = self.current_platform_provider()? {
            provider.application_resumed();
            self.finish_callback_login(lua, &provider)?;
        }
        self.synchronize_native_context()?;
        // 738FBC runs at event delivery, uses the current identity's plain
        // nonempty access test66F850, and calls734B10 only for a live store.
        // It neither captures the identity at posting nor consults Lua login.
        let initialized = self
            .state
            .lock()
            .map_err(|_| runtime_error("social state lock poisoned"))?
            .friends_store
            .is_some();
        if initialized
            && let Some(client) = self.account.friends_client()?
            && client.has_access_token()
        {
            self.check_platform_connection(lua, client)?;
        }
        Ok(())
    }

    pub(crate) fn session_succeeded(&self, lua: &Lua, lifetime: IdentityLifetime) -> LuaResult<()> {
        if !lifetime.is_current() {
            return Ok(());
        }
        self.synchronize_native_context()?;
        // The native Friends instance is created by account-login. A session
        // published before that constructor is handled by its initial check.
        let initialized = self
            .state
            .lock()
            .map_err(|_| runtime_error("social state lock poisoned"))?
            .friends_store
            .is_some();
        if initialized && let Some(client) = self.account.friends_client()? {
            self.check_platform_connection(lua, client)?;
        }
        Ok(())
    }

    pub(super) fn initialize_platform_readiness(
        &self,
        lua: &Lua,
        client: FriendsClient,
    ) -> LuaResult<()> {
        self.platform
            .lock()
            .map_err(|_| runtime_error("platform state lock poisoned"))?
            .readiness = 1;
        self.check_platform_connection(lua, client.clone())?;
        if self
            .platform
            .lock()
            .map_err(|_| runtime_error("platform state lock poisoned"))?
            .readiness
            == 0
        {
            self.refresh_native_friends(client.clone(), None)?;
        }
        self.refresh_platform_availability(lua, client)?;
        Ok(())
    }

    pub(super) fn refresh_platform_availability(
        &self,
        lua: &Lua,
        client: FriendsClient,
    ) -> LuaResult<()> {
        // SocialManager's separate1000C33D4 check follows both constructors.
        // +160 is the connected-network SET SIZE, not a service pointer.
        self.state
            .lock()
            .map_err(|_| runtime_error("social state lock poisoned"))?
            .connected = false;
        let Some(linked_id) = client.linked_platform_id(SocialNetwork::Facebook) else {
            return Ok(());
        };
        let Some((account_id, _)) = client.current_social_user() else {
            return Ok(());
        };
        let lease = self
            .platform
            .lock()
            .map_err(|_| runtime_error("platform state lock poisoned"))?
            .session
            .clone();
        let Some(lease) = lease else {
            return Ok(());
        };
        if !lease.provider.is_logged_in() {
            // 730C98 error3 ->0C52C8 ->0C3748 is CONNECT, not disconnect.
            return self.begin_platform_connect(lua, client, ConnectionConsumer::Lua);
        }
        let request = match lease.provider.clone().prepare_user_profile() {
            SocialProfileRequest::Ready(result) => {
                return self.finish_platform(
                    lua,
                    PlatformCompletion::Available {
                        lease,
                        client,
                        account_id,
                        linked_id,
                        result,
                    },
                );
            }
            SocialProfileRequest::Pending(request) => request,
        };
        self.spawn_online("platform-availability", move || {
            let result = request();
            OnlineCompletion::platform(PlatformCompletion::Available {
                lease,
                client,
                account_id,
                linked_id,
                result,
            })
        })
    }

    fn check_platform_connection(&self, lua: &Lua, client: FriendsClient) -> LuaResult<()> {
        let linked = client.linked_platform_id(SocialNetwork::Facebook);
        let mut state = self
            .platform
            .lock()
            .map_err(|_| runtime_error("platform state lock poisoned"))?;
        if state.connecting
            && state
                .connection_owner
                .as_ref()
                .is_some_and(|owner| !owner.is_current())
        {
            // An SDK event from a newer identity must not inherit an old
            // connection's pending flag. Retire that job's disk/callback permit
            // before admitting the new account under this same platform session.
            if let Some(old) = state.session.take() {
                *old.current.lock().expect("platform lease lock poisoned") = false;
                state.session = Some(PlatformLease {
                    provider: old.provider,
                    current: Arc::new(Mutex::new(true)),
                });
            }
            state.connecting = false;
            state.connection_owner = None;
            state.connected = false;
        }
        let lease = state
            .session
            .clone()
            .filter(|lease| lease.provider.is_logged_in());
        let (Some(linked_id), Some(lease)) = (linked, lease) else {
            // 730C98's absent-link/closed-session callbacks are synchronous.
            state.readiness = state.readiness.saturating_sub(1);
            return Ok(());
        };
        drop(state);
        let request = match lease.provider.clone().prepare_user_profile() {
            SocialProfileRequest::Ready(result) => {
                return self.finish_platform(
                    lua,
                    PlatformCompletion::Checked {
                        lease,
                        client,
                        linked_id,
                        result,
                    },
                );
            }
            SocialProfileRequest::Pending(request) => request,
        };
        self.spawn_online("platform-profile-check", move || {
            let result = request();
            OnlineCompletion::platform(PlatformCompletion::Checked {
                lease,
                client,
                linked_id,
                result,
            })
        })
    }

    fn refresh_native_friends(
        &self,
        client: FriendsClient,
        network: Option<SocialNetwork>,
    ) -> LuaResult<()> {
        self.spawn_online("native-friends", move || {
            let (client, result) = client.fetch();
            OnlineCompletion::NativeFriends {
                client,
                result,
                network,
            }
        })
    }

    pub(super) fn request_platform_friends(
        &self,
        lua: &Lua,
        client: FriendsClient,
    ) -> LuaResult<()> {
        let lease = self
            .platform
            .lock()
            .map_err(|_| runtime_error("platform state lock poisoned"))?
            .session
            .clone()
            .filter(|lease| lease.provider.is_logged_in());
        let Some(lease) = lease else {
            return self.finish_platform_friends(
                lua,
                &client,
                Err(SocialPlatformError::Unavailable),
            );
        };
        self.spawn_online("platform-friends", move || {
            let result = lease.provider.friends(SocialFriendDetails::Profiles);
            OnlineCompletion::platform(PlatformCompletion::Friends {
                lease,
                client,
                result,
            })
        })
    }
}

impl OnlineCompletion {
    fn platform(completion: PlatformCompletion) -> Self {
        Self::Platform(Box::new(completion))
    }
}
