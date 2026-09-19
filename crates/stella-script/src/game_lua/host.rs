//! Public Lua host facade and core VM/filesystem ownership.

use crate::*;

/// Lua VM plus the original filesystem lookup behavior needed by the scripts.
pub struct StellaLua {
    pub(crate) apprater: AppraterRuntime,
    pub(crate) lua: Lua,
    pub(crate) data_root: Arc<PathBuf>,
    pub(crate) missing_globals: Arc<Mutex<BTreeSet<String>>>,
    pub(crate) fallback_calls: Arc<Mutex<BTreeSet<String>>>,
    pub(crate) compatibility_bindings: Arc<Mutex<BTreeSet<String>>>,
    pub(crate) _libc_random: Arc<Mutex<NativeLibcRandom>>,
    pub(crate) render: Arc<Mutex<RenderBridge>>,
    pub(crate) resource_runtime: Arc<Mutex<ResourceRuntime>>,
    pub(crate) _audio_runtime: Arc<Mutex<AudioRuntime>>,
    pub(crate) _animation_runtime: Arc<Mutex<AnimationRuntime>>,
    pub(crate) application_event_dispatcher: ApplicationEventDispatcher,
    pub(crate) installed_apps: InstalledAppsRuntime,
    pub(crate) assets: AssetsRuntime,
    pub(crate) channel: ChannelRuntime,
    pub(crate) game_server: GameServerRuntime,
    pub(crate) gamer_services: GamerServicesRuntime,
    pub(crate) iap: IapRuntime,
    pub(crate) qr_scanner: QrScannerRuntime,
    pub(crate) skynest_account: SkynestAccountRuntime,
    pub(crate) skynest_storage: SkynestStorageRuntime,
    pub(crate) social: SocialRuntime,
    pub(crate) server_time: ServerTimeRuntime,
    pub(crate) notifications: NotificationRuntime,
    #[cfg(test)]
    pub(crate) draw_callbacks: Rc<RefCell<DrawCallbacks>>,
    pub(crate) touches: Arc<Mutex<Vec<(u64, i32, i32)>>>,
    /// GameApp's fixed platform hold/press/release byte arrays. The native
    /// frame loop publishes them to GameLua and consumes only the two edges.
    pub(crate) native_keys: Mutex<NativeKeyBuffers>,
    /// GameLua+0x513. The gamelogic loader sets this only after executing the
    /// decoded chunk and successfully invoking its `updateValues` callback.
    pub(crate) gamelogic_loaded: Cell<bool>,
    /// GameApp+0x520. The audio-activation virtual stores this byte before
    /// touching either native audio device; GameLua rereads it at the head of
    /// every `sub_10005E898` frame.
    pub(crate) application_audio_active: Cell<bool>,
}

impl StellaLua {
    pub fn new(data_root: impl Into<PathBuf>) -> Result<Self, ScriptError> {
        Self::new_with_resolution(data_root, 1024, 768)
    }

    /// Route the recovered GameServerConnection facade to a compatible
    /// replacement for the discontinued Stella endpoint. This must be called
    /// before [`Self::boot`] so the shipped high-level facade is selected.
    pub fn set_game_server_url(&self, base_url: &str) -> Result<(), ScriptError> {
        self.game_server.set_compatible_base_url(base_url)?;
        Ok(())
    }

    /// Route `ServerTime` synchronization to an explicitly selected
    /// replacement for the retired identity/2.0/time provider.
    pub fn set_server_time_url(&self, url: &str) -> Result<(), ScriptError> {
        self.server_time.set_compatible_url(url)?;
        Ok(())
    }

    /// Route downloadable `Assets` requests to a compatible replacement for
    /// the retired `apdrive/1/apps/<app>/assets` provider. The URL is the full
    /// manifest endpoint; Purple's repeated `name` query parameters and
    /// `assets`/`failedAssets` response protocol remain intact.
    pub fn set_assets_url(&self, url: &str) -> Result<(), ScriptError> {
        self.assets.set_compatible_url(url)?;
        Ok(())
    }

    /// Route the recovered Identity Level 2 access, own-profile and nickname
    /// validation operations to an explicitly selected compatible service.
    /// `url` is the `identity/2.0` service root.
    pub fn set_identity_url(&self, url: &str) -> Result<(), ScriptError> {
        self.skynest_account.set_compatible_url(url)?;
        self.social.synchronize_native_context()?;
        self.iap.use_session_provider();
        Ok(())
    }

    /// Supply compatible-service client metadata. Historical Stella secrets
    /// are neither embedded nor inferred; a replacement service may accept an
    /// application-specific id, signature and salt supplied by its operator.
    pub fn set_identity_client(
        &self,
        client_id: Option<&str>,
        client_signature: Option<&str>,
        client_salt: Option<&str>,
    ) -> Result<(), ScriptError> {
        self.skynest_account
            .set_compatible_client(client_id, client_signature, client_salt)?;
        self.social.synchronize_native_context()?;
        Ok(())
    }

    /// Generate each access signature/salt using an explicit replacement
    /// provider key and fresh randomness. Key bytes are preserved exactly.
    /// This switches away from literal signature/salt mode; calling
    /// set_identity_client switches back. Neither mode retrieves a native key.
    pub fn set_identity_signing_key(&self, key: &[u8]) -> Result<(), ScriptError> {
        self.skynest_account.set_compatible_signing_key(key)?;
        self.social.synchronize_native_context()?;
        Ok(())
    }

    /// Route Skynest key/value and cloud-settings operations to an explicitly
    /// selected replacement for the retired `storage/1.0` provider. `url` is
    /// the service root; the recovered `state` and `states/query` routes are
    /// appended by the client.
    pub fn set_storage_url(&self, url: &str) -> Result<(), ScriptError> {
        self.skynest_storage.set_compatible_url(url)?;
        Ok(())
    }

    /// Supply optional credentials for a compatible storage service using
    /// Purple's recovered `X-Access-Token` and `Rovio-Sgs` header names. No
    /// historical account credentials are embedded or inferred.
    pub fn set_storage_credentials(
        &self,
        access_token: Option<&str>,
        signature: Option<&str>,
    ) -> Result<(), ScriptError> {
        self.skynest_storage
            .set_compatible_credentials(access_token, signature)?;
        Ok(())
    }

    /// Route the native SocialManager operations to an explicitly selected
    /// compatible provider. This is a rehost JSON-RPC boundary over the
    /// recovered native method/callback ABI, not a Facebook credential shim.
    pub fn set_social_url(&self, url: &str) -> Result<(), ScriptError> {
        self.social.set_compatible_url(url)?;
        Ok(())
    }

    /// Install an explicitly authenticated Facebook platform session. This is
    /// independent of both Skynest identity and the compatible social endpoint.
    /// Replacing/removing the provider retires its pending deliveries.
    pub fn set_facebook_session(
        &self,
        provider: Option<Arc<dyn SocialPlatformProvider>>,
    ) -> Result<(), ScriptError> {
        self.social.set_facebook_session(self.lua(), provider)?;
        Ok(())
    }

    /// Deliver a platform authorization URL on the application thread, before
    /// posting the subsequent application-resumed event. Returns whether the
    /// current provider handled it; unrelated application links return false.
    pub fn handle_platform_open_url(&self, url: &str) -> Result<bool, ScriptError> {
        Ok(self.social.handle_platform_open_url(self.lua(), url)?)
    }

    /// Deliver an embedded authorization view's actual callback on the
    /// application thread. This also schedules the native service profile
    /// request and resolves any pending Friends login consumer.
    pub fn handle_platform_login_dialog_event(
        &self,
        event: &FacebookLoginDialogEvent,
    ) -> Result<bool, ScriptError> {
        Ok(self
            .social
            .handle_platform_login_dialog_event(self.lua(), event)?)
    }

    /// Enable persistent local replacements for retired identity, cloud
    /// storage, achievement and social providers. The original Lua callbacks
    /// and asynchronous frame boundaries remain in use; only unavailable
    /// external providers are replaced. Call this before [`Self::boot`].
    pub fn enable_local_services(&self) -> Result<(), ScriptError> {
        self.skynest_account.enable_local_provider()?;
        self.gamer_services.enable_local_provider()?;
        self.social.enable_local_provider()?;
        self.iap.enable_local_provider();
        // Also support opting into local services after script boot: a
        // retired account-provider timer cannot bootstrap the new store.
        complete_iap_initialization(&self.lua, &self.iap)?;
        Ok(())
    }

    /// Publish URL schemes that the host can open. Purple routes both
    /// `checkInstalledApps*` and `AppStoreLauncher.updateGameData` through
    /// UIApplication's `canOpenURL`; this explicit registry is the portable
    /// equivalent. Values may be bare schemes or complete scheme URLs.
    pub fn set_installed_url_schemes<'a>(
        &self,
        schemes: impl IntoIterator<Item = &'a str>,
    ) -> Result<(), ScriptError> {
        let normalized = schemes
            .into_iter()
            .map(|scheme| {
                game_lua::platform::normalized_url_scheme(scheme).ok_or_else(|| {
                    runtime_error(format!("invalid installed URL scheme '{scheme}'"))
                })
            })
            .collect::<LuaResult<BTreeSet<_>>>()?;
        self.render
            .lock()
            .expect("render bridge lock poisoned")
            .installed_url_schemes = normalized;
        Ok(())
    }

    /// Construct a host that records absent global reads for explicit
    /// compatibility audits. Normal hosts intentionally leave `_G` without a
    /// metatable, matching Purple's stock Lua 5.1 lookup path.
    pub fn new_with_missing_global_diagnostics(
        data_root: impl Into<PathBuf>,
    ) -> Result<Self, ScriptError> {
        Self::new_with_resolution_and_missing_global_diagnostics(data_root, 1024, 768)
    }

    /// Construct the native script host with the drawable size that Purple
    /// publishes before loading `gamelogic.lua`.
    pub fn new_with_resolution(
        data_root: impl Into<PathBuf>,
        screen_width: u32,
        screen_height: u32,
    ) -> Result<Self, ScriptError> {
        Self::new_with_resolution_options(data_root, screen_width, screen_height, false)
    }

    /// Resolution-aware variant of [`Self::new_with_missing_global_diagnostics`].
    pub fn new_with_resolution_and_missing_global_diagnostics(
        data_root: impl Into<PathBuf>,
        screen_width: u32,
        screen_height: u32,
    ) -> Result<Self, ScriptError> {
        Self::new_with_resolution_options(data_root, screen_width, screen_height, true)
    }

    fn new_with_resolution_options(
        data_root: impl Into<PathBuf>,
        screen_width: u32,
        screen_height: u32,
        track_missing_globals: bool,
    ) -> Result<Self, ScriptError> {
        if screen_width == 0 || screen_height == 0 {
            return Err(ScriptError::Lua(LuaError::RuntimeError(
                "screen resolution must be non-zero".to_owned(),
            )));
        }
        let data_root = Arc::new(data_root.into());
        let missing_globals = Arc::new(Mutex::new(BTreeSet::new()));
        let fallback_calls = Arc::new(Mutex::new(BTreeSet::new()));
        let compatibility_bindings = Arc::new(Mutex::new(BTreeSet::new()));
        let libc_random = Arc::new(Mutex::new(NativeLibcRandom::default()));
        let render_bridge = RenderBridge {
            screen_width,
            screen_height,
            device_orientation_index: canonical_device_orientation(screen_width, screen_height),
            ..RenderBridge::default()
        };
        let render = Arc::new(Mutex::new(render_bridge));
        let animation_runtime = Arc::new(Mutex::new(AnimationRuntime::default()));
        let draw_callbacks = Rc::new(RefCell::new(DrawCallbacks::default()));
        let touches = Arc::new(Mutex::new(Vec::new()));
        let lua = Lua::new();
        let installed = install_base_globals(
            &lua,
            screen_width,
            screen_height,
            Arc::clone(&data_root),
            Arc::clone(&missing_globals),
            Arc::clone(&fallback_calls),
            Arc::clone(&compatibility_bindings),
            Arc::clone(&libc_random),
            Arc::clone(&render),
            Arc::clone(&animation_runtime),
            Rc::clone(&draw_callbacks),
            track_missing_globals,
        )?;
        Ok(Self {
            apprater: installed.apprater,
            lua,
            data_root,
            missing_globals,
            fallback_calls,
            compatibility_bindings,
            _libc_random: libc_random,
            render,
            resource_runtime: installed.resources,
            _audio_runtime: installed.audio,
            _animation_runtime: animation_runtime,
            application_event_dispatcher: installed.application_event_dispatcher,
            installed_apps: installed.installed_apps,
            assets: installed.assets,
            channel: installed.channel,
            game_server: installed.game_server,
            gamer_services: installed.gamer_services,
            iap: installed.iap,
            qr_scanner: installed.qr_scanner,
            skynest_account: installed.skynest_account,
            skynest_storage: installed.skynest_storage,
            social: installed.social,
            server_time: installed.server_time,
            notifications: installed.notifications,
            #[cfg(test)]
            draw_callbacks,
            touches,
            native_keys: Mutex::new(NativeKeyBuffers::default()),
            gamelogic_loaded: Cell::new(false),
            application_audio_active: Cell::new(false),
        })
    }

    /// Mirror GameApp slot 7 (`sub_10002A25C -> sub_10006E1F4`): after the
    /// renderer has installed its new extent, snapshot the old corrected
    /// camera scale, replace the renderer-reported globals, and invoke the
    /// required script callback. `g_startingResolution*` deliberately remains
    /// the construction-time size.
    pub fn set_screen_resolution(
        &self,
        screen_width: u32,
        screen_height: u32,
    ) -> Result<bool, ScriptError> {
        if screen_width == 0 || screen_height == 0 {
            return Err(ScriptError::Lua(LuaError::RuntimeError(
                "screen resolution must be non-zero".to_owned(),
            )));
        }
        let globals = self.lua.globals();
        let old_width = globals.get::<u32>("screenWidth")?;
        let old_height = globals.get::<u32>("screenHeight")?;
        if old_width == screen_width && old_height == screen_height {
            return Ok(false);
        }
        // GameApp slot +0x38 calls sub_10009961C before publishing the new
        // screen globals and before Lua recalculates its corrected cameras.
        self.capture_resolution_camera_scale()?;
        {
            let mut render = self.render.lock().expect("render bridge lock poisoned");
            render.screen_width = screen_width;
            render.screen_height = screen_height;
            // A desktop drawable has no UIKit device-orientation sensor. Use
            // Purple's first supported orientation for the current aspect;
            // the Lua-facing enum-to-degree mapping remains byte-for-byte.
            render.device_orientation_index =
                canonical_device_orientation(screen_width, screen_height);
        }
        globals.set("screenWidth", screen_width)?;
        globals.set("screenHeight", screen_height)?;

        // LuaResources constructs its default scissor from the renderer's
        // drawable extent. Preserve an explicit script clip, but carry the
        // old full-screen rectangle across a drawable resize just like the
        // native GL context's viewport/scissor state.
        let old_full = [0, 0, old_width as i32, old_height as i32];
        let new_full = [0, 0, screen_width as i32, screen_height as i32];
        {
            let mut resources = self
                .resource_runtime
                .lock()
                .expect("resource runtime lock poisoned");
            if resources.clip_rect == old_full {
                resources.clip_rect = new_full;
            }
        }
        {
            let mut render = self.render.lock().expect("render bridge lock poisoned");
            if render.state.clip_rect == Some(old_full) {
                render.state.clip_rect = Some(new_full);
            }
        }

        // sub_1005278E8 performs an unconditional zero-argument Lua call. A
        // missing/non-function resolutionChanged is an error after the new
        // context extent and globals have already become observable; Purple
        // has no host-side fallback that edits the script-owned screen table.
        game_environment(&self.lua)?
            .get::<mlua::Function>("resolutionChanged")?
            .call::<()>(())?;
        Ok(true)
    }

    pub(super) fn capture_resolution_camera_scale(&self) -> Result<bool, ScriptError> {
        let environment = game_environment(&self.lua)?;
        let Value::Table(camera) = environment.raw_get::<Value>("gameCamera")? else {
            return Ok(false);
        };
        let Value::Table(cameras) = camera.get::<Value>("resolutionCorrectedCameras")? else {
            return Ok(false);
        };
        let Some(end_index) = camera
            .get::<Value>("endCameraIndex")
            .ok()
            .as_ref()
            .and_then(value_number)
        else {
            return Ok(false);
        };
        // The native path narrows the Lua number to float32 before FCVTZS.
        let end_index = native_fcvtzs_f32(end_index as f32);
        let Value::Table(end_camera) = cameras.raw_get::<Value>(end_index)? else {
            return Ok(false);
        };
        let Some(scale) = end_camera
            .get::<Value>("sx")
            .ok()
            .as_ref()
            .and_then(value_number)
        else {
            return Ok(false);
        };
        self.render
            .lock()
            .expect("render bridge lock poisoned")
            .resolution_camera_scale = scale as f32;
        Ok(true)
    }

    pub fn lua(&self) -> &Lua {
        &self.lua
    }

    pub fn data_root(&self) -> &Path {
        &self.data_root
    }

    /// Resolve a platform-owned media request through the same safe bundle
    /// lookup used by native resource streams.
    pub fn resolve_bundle_resource(&self, requested: &str) -> Result<PathBuf, ScriptError> {
        resolve_bundle_file(&self.data_root, requested)
    }

    pub fn execute(&self, path: &str) -> Result<(), ScriptError> {
        let environment = game_environment(&self.lua)?;
        execute_script_in(&self.lua, &self.data_root, path, environment).map_err(ScriptError::Lua)
    }

    /// Execute diagnostic or host-integration Lua in the same environment as
    /// the game. This is also useful for platform callbacks with arguments.
    pub fn execute_source(&self, source: &str) -> Result<(), ScriptError> {
        let environment = game_environment(&self.lua)?;
        self.lua
            .load(source)
            .set_name("[stella-host]")
            .set_environment(environment)
            .exec()?;
        Ok(())
    }

    /// Snapshot GameLua's retained persistent-load diagnostics. Native GameApp
    /// delivers and clears them after gamelogic initialization. Later loads
    /// append to the same queue; ordinary frames do not drain it.
    pub fn pending_persistent_load_messages(&self) -> Vec<String> {
        super::persistence::pending_persistent_load_messages(&self.lua)
    }

    /// Advertise a host-provided QR source to the shipped Telepods menus.
    /// Desktop builds leave it disabled until a platform integration or a
    /// deterministic command-line code explicitly enables it.
    pub fn set_qr_scanner_available(&self, available: bool) -> Result<(), ScriptError> {
        self.qr_scanner.set_host_available(available);
        Ok(())
    }

    /// Submit one decoded payload to the virtual QR capture source. Returns
    /// true if an available, active, non-busy scanner accepted it and posted an
    /// application event; the Lua callback is never invoked inline. Otherwise
    /// the latest input waits for `start` or a later host frame. Already posted
    /// results survive `stop`; delivery uses the callback then installed.
    pub fn submit_qr_code(&self, code: &str) -> Result<bool, ScriptError> {
        Ok(self.qr_scanner.submit_host_code(code))
    }

    /// Deliver one platform local-notification event through the callback
    /// installed by `setNotificationCallback`. This follows UIKit's delegate
    /// boundary and therefore re-enters Lua synchronously.
    pub fn submit_local_notification(&self, event_name: &str) {
        if let Err(error) = self.try_submit_local_notification(event_name) {
            eprintln!("local notification callback failed: {error}");
        }
    }

    /// Checked variant of [`Self::submit_local_notification`] for hosts that
    /// want Lua callback failures to stop their runtime explicitly.
    pub fn try_submit_local_notification(&self, event_name: &str) -> Result<(), ScriptError> {
        dispatch_notification_callback(&self.lua, &self.render, event_name)?;
        Ok(())
    }

    /// Queue a compatible Rovio Channel catalog result. The native SDK stores
    /// this as `newVideos.num` and reports it to Lua on the application thread.
    pub fn submit_channel_content_update(&self, count: i32) {
        self.channel.submit_content_update(count);
    }

    /// Supply the Channel content path carried by a platform launch
    /// notification. `RovioChannel.onMenuInitialised` consumes it only after
    /// the Channel service exists and synchronously forwards it to Lua.
    pub fn submit_channel_launch_notification(&self, content_path: &str) {
        self.channel
            .submit_launch_notification(content_path.to_owned());
    }
}

fn canonical_device_orientation(screen_width: u32, screen_height: u32) -> u32 {
    if screen_width >= screen_height { 1 } else { 0 }
}
