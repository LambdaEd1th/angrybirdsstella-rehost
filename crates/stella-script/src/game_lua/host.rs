//! Public Lua host facade and core VM/filesystem ownership.

use crate::*;

/// Lua VM plus the original filesystem lookup behavior needed by the scripts.
pub struct StellaLua {
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
    pub(crate) url_requests: UrlRequestRuntime,
    pub(crate) installed_apps: InstalledAppsRuntime,
    pub(crate) game_server: GameServerRuntime,
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
            url_requests: installed.url_requests,
            installed_apps: installed.installed_apps,
            game_server: installed.game_server,
            #[cfg(test)]
            draw_callbacks,
            touches,
            native_keys: Mutex::new(NativeKeyBuffers::default()),
            gamelogic_loaded: Cell::new(false),
            application_audio_active: Cell::new(false),
        })
    }

    /// Mirror Purple's resolution callback at `sub_10006E990`: replace the
    /// renderer-reported globals, then invoke the script callback if startup
    /// has installed it. `g_startingResolution*` deliberately remains the
    /// construction-time size.
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

        let environment = game_environment(&self.lua)?;
        if let Value::Function(callback) = environment.get::<Value>("resolutionChanged")? {
            callback.call::<()>(())?;
        } else if let Value::Table(screen) = environment.get::<Value>("screen")? {
            screen.set("right", f64::from(screen_width))?;
            screen.set("bottom", f64::from(screen_height))?;
            screen.set("width", f64::from(screen_width))?;
            screen.set("height", f64::from(screen_height))?;
        }
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

    /// Advertise a host-provided QR source to the shipped Telepods menus.
    /// Desktop builds leave it disabled until a platform integration or a
    /// deterministic command-line code explicitly enables it.
    pub fn set_qr_scanner_available(&self, available: bool) -> Result<(), ScriptError> {
        super::platform_services::set_qr_scanner_available(&self.lua, available)?;
        Ok(())
    }

    /// Queue or deliver one recognized QR payload through QrScanner's retained
    /// native callback. Returns true when an active scanner consumed it now;
    /// otherwise it remains queued until `start` and callback registration.
    pub fn submit_qr_code(&self, code: &str) -> Result<bool, ScriptError> {
        Ok(super::platform_services::submit_host_code(&self.lua, code)?)
    }
}

fn canonical_device_orientation(screen_width: u32, screen_height: u32) -> u32 {
    if screen_width >= screen_height { 1 } else { 0 }
}
