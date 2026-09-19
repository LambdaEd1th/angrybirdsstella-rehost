//! Rovio Channel/Toons service lifecycle and retired-content fallback.

use crate::*;
use std::collections::VecDeque;

const SDK_ENABLED_REGISTRY_KEY: &str = "stella.rovio_channel.sdk_enabled";

#[derive(Clone, Debug)]
enum Completion {
    NewContentUpdated(i32),
}

#[derive(Debug, Default)]
struct ChannelState {
    pending_loading_failure: bool,
    new_content_count: i32,
    menu_initialised: bool,
    launch_notification: Option<String>,
    completions: VecDeque<Completion>,
}

/// Retired Channel request, catalog and launch-notification state retained by
/// the native SDK owner.
#[derive(Clone, Debug)]
pub(crate) struct ChannelRuntime {
    state: Arc<Mutex<ChannelState>>,
    application_events: ApplicationEventScheduler,
}

impl ChannelRuntime {
    fn new(application_events: ApplicationEventScheduler) -> Self {
        Self {
            state: Arc::new(Mutex::new(ChannelState::default())),
            application_events,
        }
    }

    fn schedule_loading_failure(&self) {
        let mut state = self.state.lock().expect("channel state lock poisoned");
        if !state.pending_loading_failure {
            state.pending_loading_failure = true;
            self.application_events
                .post(ApplicationEvent::ChannelLoadingFailure);
        }
    }

    fn cancel_loading(&self) {
        self.state
            .lock()
            .expect("channel state lock poisoned")
            .pending_loading_failure = false;
        self.application_events
            .cancel(ApplicationEvent::ChannelLoadingFailure);
    }

    fn take_loading_failure(&self) -> bool {
        std::mem::take(
            &mut self
                .state
                .lock()
                .expect("channel state lock poisoned")
                .pending_loading_failure,
        )
    }

    fn new_content_count(&self) -> i32 {
        self.state
            .lock()
            .expect("channel state lock poisoned")
            .new_content_count
    }

    fn apply_content_count(&self, count: i32) {
        self.state
            .lock()
            .expect("channel state lock poisoned")
            .new_content_count = count;
    }

    fn initialise_menu(&self) {
        self.state
            .lock()
            .expect("channel state lock poisoned")
            .menu_initialised = true;
    }

    fn take_launch_notification(&self) -> Option<String> {
        self.state
            .lock()
            .expect("channel state lock poisoned")
            .launch_notification
            .take()
    }

    fn pop_completion(&self) -> Option<Completion> {
        self.state
            .lock()
            .expect("channel state lock poisoned")
            .completions
            .pop_front()
    }

    pub(crate) fn discard_loading_failure(&self) {
        let _ = self.take_loading_failure();
    }

    pub(crate) fn discard_content_completion(&self) {
        let _ = self.pop_completion();
    }

    pub(crate) fn submit_content_update(&self, count: i32) {
        let mut state = self.state.lock().expect("channel state lock poisoned");
        let count = count.max(0);
        state
            .completions
            .push_back(Completion::NewContentUpdated(count));
        self.application_events
            .post(ApplicationEvent::ChannelContent);
    }

    pub(crate) fn submit_launch_notification(&self, content_path: String) {
        self.state
            .lock()
            .expect("channel state lock poisoned")
            .launch_notification = Some(content_path);
    }
}

// ChannelButton.lua and ChannelIntroPopup.lua name sprites that were delivered
// by the retired island-map promotion service rather than the application
// bundle. These fallbacks preserve each visual role with Stella's bundled
// Toons/button art. ResourceRuntime materialises them only when the exact
// downloadable name is absent, so a recovered original sheet takes priority.
const RETIRED_CHANNEL_SPRITE_FALLBACKS: &[(&str, &str)] = &[
    ("toonsBackgroundButton", "BTN_PLAY_BG"),
    ("BUTTON_TOONS_NORMAL", "ICON_TOONS"),
    ("BUTTON_TOONS_LOOKLEFT", "ICON_TOONS"),
    ("BUTTON_TOONS_LOOKRIGHT", "ICON_TOONS"),
    ("BUTTON_TOONS_BLINK", "ICON_TOONS"),
    ("BUTTON_TOONS_AMOUNT", "BTN_BG_SMALL"),
    ("BUTTON_TOONS_AMOUNT_SMALL", "BTN_BG_SMALL"),
    ("BUTTON_TOONS_AMOUNT_MEDIUM", "BTN_BG_SMALL"),
    ("BUTTON_TOONS_AMOUNT_LARGE", "BTN_BG_SMALL"),
    ("BUTTON_TOONS_AMOUNT_PLUS", "ICON_PUSH"),
    ("toonsBanner", "ICON_TOONS_TV"),
    ("CHANNEL_INTRO_CLOSE", "ICON_X"),
];

pub(super) fn install(
    lua: &Lua,
    globals: &mlua::Table,
    resource_runtime: Arc<Mutex<ResourceRuntime>>,
    application_events: ApplicationEventScheduler,
) -> LuaResult<ChannelRuntime> {
    let runtime = ChannelRuntime::new(application_events);
    lua.set_named_registry_value(SDK_ENABLED_REGISTRY_KEY, false)?;
    resource_runtime
        .lock()
        .expect("resource runtime lock poisoned")
        .register_sprite_aliases(RETIRED_CHANNEL_SPRITE_FALLBACKS);

    let channel = lua.create_table()?;

    let open_runtime = runtime.clone();
    channel.set(
        "openChannelView",
        lua.create_function(move |lua, args: MultiValue| {
            // Generated adapter sub_1000AEFE0 requires exactly the seven
            // values consumed by ChannelManager.openView: game id, mode,
            // locale, viewport width/height, content path and entry point.
            // Like the native adapter, trailing Lua values are ignored.
            native_required_string(&args, 0, "RovioChannel.openChannelView")?;
            native_required_string(&args, 1, "RovioChannel.openChannelView")?;
            native_required_string(&args, 2, "RovioChannel.openChannelView")?;
            native_required_number(&args, 3, "RovioChannel.openChannelView")?;
            native_required_number(&args, 4, "RovioChannel.openChannelView")?;
            native_required_string(&args, 5, "RovioChannel.openChannelView")?;
            native_required_string(&args, 6, "RovioChannel.openChannelView")?;

            if lua
                .named_registry_value::<bool>(SDK_ENABLED_REGISTRY_KEY)
                .unwrap_or(false)
            {
                // sub_1005DF2A0 packages these values into a 0x48-byte Func5
                // and starts it through the shared background scheduler.
                // The retired endpoint's failure continuation is therefore
                // queued rather than invoked on this Lua calling stack.
                open_runtime.schedule_loading_failure();
            }
            Ok(())
        })?,
    )?;

    let cancel_runtime = runtime.clone();
    channel.set(
        "cancelChannelViewLoading",
        lua.create_function(move |_, _: MultiValue| {
            // sub_1005E0C04 succeeds only while SDK state is loading and
            // releases the retained request without emitting failure.
            cancel_runtime.cancel_loading();
            Ok(())
        })?,
    )?;
    channel.set(
        "updateNewContent",
        lua.create_function(|_, _: MultiValue| {
            // sub_1000ADB80 calls Channel::updateNewContent(Normal). The
            // retired endpoint has no successful provider result to publish,
            // so this submission remains a zero-result command. A compatible
            // host provider delivers its eventual result through the retained
            // ChannelRuntime completion queue below.
            Ok(())
        })?,
    )?;
    let menu_runtime = runtime.clone();
    channel.set(
        "onMenuInitialised",
        lua.create_function(move |lua, _: MultiValue| {
            // sub_1000ADBC0 guards only the SDK configuration block with its
            // +0x70 byte. Its launch-notification test runs on every call and
            // synchronously re-enters the retained root Lua object.
            menu_runtime.initialise_menu();
            if !lua
                .named_registry_value::<bool>(SDK_ENABLED_REGISTRY_KEY)
                .unwrap_or(false)
            {
                return Ok(());
            }
            let Some(content_path) = menu_runtime.take_launch_notification() else {
                return Ok(());
            };
            let native_channel = lua.globals().get::<mlua::Table>("RovioChannel")?;
            native_channel
                .get::<mlua::Function>("onRemoteNotificationReceived")?
                .call::<()>(content_path)
        })?,
    )?;
    let count_runtime = runtime.clone();
    channel.set(
        "numOfNewContent",
        // sub_1000ADBAC reads the SDK's persisted `newVideos.num` value. Its
        // generated adapter publishes that int through the native float-number
        // setter, so retain a Lua number rather than an integer.
        lua.create_function(move |lua, _: MultiValue| {
            if !lua
                .named_registry_value::<bool>(SDK_ENABLED_REGISTRY_KEY)
                .unwrap_or(false)
            {
                return Ok(0.0_f64);
            }
            Ok(f64::from(count_runtime.new_content_count()))
        })?,
    )?;
    channel.set(
        "isAvailable",
        // sub_1000AE178 is exactly `this->channel != nullptr`. The pointer is
        // initially null, then sub_1000AE354 allocates it when the native
        // cloud manager enables the service.
        lua.create_function(|lua, _: MultiValue| {
            Ok(lua
                .named_registry_value::<bool>(SDK_ENABLED_REGISTRY_KEY)
                .unwrap_or(false))
        })?,
    )?;
    channel.set(
        "isChannelViewOpened",
        // sub_1000AE188 takes the same null-pointer branch before consulting
        // the native SDK's live view state.
        lua.create_function(|_, _: MultiValue| Ok(false))?,
    )?;

    globals.set("RovioChannel", channel)?;
    Ok(runtime)
}

/// Deliver the retired SDK request result on the application thread.
pub(crate) fn dispatch_loading_failure(lua: &Lua, runtime: &ChannelRuntime) -> LuaResult<()> {
    let native_channel = lua.globals().get::<mlua::Table>("RovioChannel")?;
    if runtime.take_loading_failure() {
        // sub_1000AE41C addresses the retained native LuaObject directly and
        // calls the member with no arguments.
        native_channel
            .get::<mlua::Function>("onChannelLoadingFailed")?
            .call::<()>(())?;
    }
    Ok(())
}

pub(crate) fn dispatch_content_completion(lua: &Lua, runtime: &ChannelRuntime) -> LuaResult<()> {
    let Some(completion) = runtime.pop_completion() else {
        return Ok(());
    };
    let native_channel = lua.globals().get::<mlua::Table>("RovioChannel")?;
    match completion {
        Completion::NewContentUpdated(count) => {
            // Channel's success continuations persist `newVideos.num`, then
            // delegate to sub_1000AE4AC with the same native int.
            runtime.apply_content_count(count);
            native_channel
                .get::<mlua::Function>("onNewChannelContentUpdated")?
                .call::<()>(count)?;
        }
    }
    Ok(())
}

pub(super) fn enable_service(lua: &Lua) -> LuaResult<()> {
    // sub_1000AE354 constructs the 0x110-byte SDK object and stores it at
    // RovioChannel+0x38 before invoking the script-side onEnableService hook.
    lua.set_named_registry_value(SDK_ENABLED_REGISTRY_KEY, true)
}
