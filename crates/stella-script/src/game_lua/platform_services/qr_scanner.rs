//! Native QR/Telepods scanner boundary and host-code injection bridge.

use crate::*;

const CALLBACK_REGISTRY_KEY: &str = "stella.qr_scanner.callback";

#[derive(Debug, Default)]
struct QrScannerState {
    available: bool,
    active: bool,
    /// Desktop source extension: the latest frame/code not yet admitted by
    /// the scanner. This is not a decoded native event and survives stop.
    pending_code: Option<String>,
    /// One admitted decode, matching the +0x80 in-flight gate in 0x1000DE634.
    /// Its event has already been posted and survives stop independently.
    completion: Option<String>,
}

#[derive(Clone, Debug)]
pub(crate) struct QrScannerRuntime {
    state: Arc<Mutex<QrScannerState>>,
    application_events: ApplicationEventScheduler,
}

impl QrScannerRuntime {
    fn new(application_events: ApplicationEventScheduler) -> Self {
        Self {
            state: Arc::new(Mutex::new(QrScannerState::default())),
            application_events,
        }
    }

    fn admit_pending(&self, state: &mut QrScannerState) -> bool {
        if !state.available || !state.active || state.completion.is_some() {
            return false;
        }
        let Some(code) = state.pending_code.take() else {
            return false;
        };
        // Decoder 0x1000DE7B8 posts even an unsuccessful (empty) result via
        // 0x1000DEB20 -> 0x10057C1BC, never directly re-entering Lua. The
        // host supplies decoded text instead of camera pixels/worker time.
        state.completion = Some(code);
        self.application_events
            .post(ApplicationEvent::QrRecognition);
        true
    }

    fn start(&self) {
        let mut state = self.state.lock().expect("QR scanner state lock poisoned");
        // 0x1000DE0D0 is an immediate no-op for an existing session. Failed
        // camera discovery leaves +0x50 null, so later capability discovery
        // alone cannot activate capture without another start command.
        if state.active || !state.available {
            return;
        }
        state.active = true;
        self.admit_pending(&mut state);
    }

    fn stop(&self) {
        // 0x1000DE1A8 releases only the camera at +0x50, not the decode
        // count at +0x80, retained callback at +0x88, or already posted event.
        self.state
            .lock()
            .expect("QR scanner state lock poisoned")
            .active = false;
    }

    fn available(&self) -> bool {
        self.state
            .lock()
            .expect("QR scanner state lock poisoned")
            .available
    }

    pub(crate) fn set_host_available(&self, available: bool) {
        self.state
            .lock()
            .expect("QR scanner state lock poisoned")
            .available = available;
    }

    pub(crate) fn submit_host_code(&self, code: &str) -> bool {
        let mut state = self.state.lock().expect("QR scanner state lock poisoned");
        state.pending_code = Some(code.to_owned());
        self.admit_pending(&mut state)
    }

    /// Poll only the virtual capture source at the host frame boundary. A
    /// code held while a decode was in flight must wait for another capture,
    /// not be consumed by callback replacement or by a nested queue drain.
    pub(crate) fn poll_host_frame(&self) {
        let mut state = self.state.lock().expect("QR scanner state lock poisoned");
        self.admit_pending(&mut state);
    }

    fn take_completion(&self) -> Option<String> {
        // 0x1000DDFF0 decrements in-flight before reading success/callback.
        // Release the lock and the slot even if Lua subsequently raises.
        self.state
            .lock()
            .expect("QR scanner state lock poisoned")
            .completion
            .take()
    }

    pub(crate) fn discard_completion(&self) {
        let _ = self.take_completion();
    }
}

pub(super) fn install(
    lua: &Lua,
    globals: &mlua::Table,
    application_events: ApplicationEventScheduler,
) -> LuaResult<QrScannerRuntime> {
    let scanner = lua.create_table()?;
    let runtime = QrScannerRuntime::new(application_events);
    lua.set_named_registry_value(CALLBACK_REGISTRY_KEY, Value::Nil)?;

    let camera_runtime = runtime.clone();
    scanner.set(
        "isCameraSupported",
        lua.create_function(move |_, _: MultiValue| {
            // 0x1000DE050 accepts either camera side. The explicit desktop
            // injection source is a virtual camera; it is off by default.
            Ok(camera_runtime.available())
        })?,
    )?;
    scanner.set(
        "isFrontCameraSupported",
        lua.create_function(|_, _: MultiValue| {
            // 0x1000DE094 requires camera side 2. The virtual source never
            // claims that it is a physical front-facing camera.
            Ok(false)
        })?,
    )?;

    let start_runtime = runtime.clone();
    scanner.set(
        "start",
        lua.create_function(move |_, _: MultiValue| {
            // 0x1000DE0C0 opens capture; callback registration is independent.
            start_runtime.start();
            Ok(())
        })?,
    )?;
    let stop_runtime = runtime.clone();
    scanner.set(
        "stop",
        lua.create_function(move |_, _: MultiValue| {
            stop_runtime.stop();
            Ok(())
        })?,
    )?;

    scanner.set(
        "setQrRecognizedCallback",
        lua.create_function(|lua, args: MultiValue| {
            // 0x1000DE1F4 retains a function or clears on any other Lua tag.
            // It neither captures a frame nor invokes the callback. Delivery
            // reads the live retained function, not the one present at post.
            let callback = match args.front() {
                Some(Value::Function(callback)) => Value::Function(callback.clone()),
                _ => Value::Nil,
            };
            lua.set_named_registry_value(CALLBACK_REGISTRY_KEY, callback)
        })?,
    )?;

    globals.set("QrScanner", scanner)?;
    Ok(runtime)
}

pub(crate) fn dispatch_completion(lua: &Lua, runtime: &QrScannerRuntime) -> LuaResult<()> {
    let Some(code) = runtime.take_completion() else {
        return Ok(());
    };
    // 0x1000DE7B8 tests the complete std::string for success. 0x1000DDFF0
    // deliberately does NOT test availability or the current camera session.
    if code.is_empty() {
        return Ok(());
    }
    let Value::Function(callback) = lua.named_registry_value(CALLBACK_REGISTRY_KEY)? else {
        return Ok(());
    };
    // 0x100528804 -> 0x1005096E4 pushes a C string: a NUL truncates the Lua
    // parameter, but does not change the earlier full-string success test.
    let code = code.split('\0').next().unwrap_or_default();
    callback.call::<()>(code)
}
