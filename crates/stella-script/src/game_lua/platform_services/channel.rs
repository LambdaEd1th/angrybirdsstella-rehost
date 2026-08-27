//! Rovio Channel/Toons service lifecycle and retired-content fallback.

use crate::*;

const SDK_ENABLED_REGISTRY_KEY: &str = "stella.rovio_channel.sdk_enabled";

/// Retired Channel request state retained by the native SDK owner.
#[derive(Clone, Debug, Default)]
pub(crate) struct ChannelRuntime {
    pending_loading_failure: Arc<Mutex<bool>>,
}

impl ChannelRuntime {
    fn schedule_loading_failure(&self) {
        *self
            .pending_loading_failure
            .lock()
            .expect("channel completion lock poisoned") = true;
    }

    fn cancel_loading(&self) {
        *self
            .pending_loading_failure
            .lock()
            .expect("channel completion lock poisoned") = false;
    }

    fn take_loading_failure(&self) -> bool {
        std::mem::take(
            &mut *self
                .pending_loading_failure
                .lock()
                .expect("channel completion lock poisoned"),
        )
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
) -> LuaResult<ChannelRuntime> {
    let runtime = ChannelRuntime::default();
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
    for method in ["updateNewContent", "onMenuInitialised"] {
        channel.set(method, lua.create_function(|_, _: MultiValue| Ok(()))?)?;
    }
    channel.set(
        "numOfNewContent",
        // sub_1000ADBAC returns zero when the native Channel pointer is null.
        // Its generated adapter then publishes that int through the native
        // float-number setter, so retain a Lua number rather than an integer.
        lua.create_function(|_, _: MultiValue| Ok(0.0_f64))?,
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
pub(crate) fn dispatch_completions(lua: &Lua, runtime: &ChannelRuntime) -> LuaResult<()> {
    if !runtime.take_loading_failure() {
        return Ok(());
    }
    let native_channel = lua.globals().get::<mlua::Table>("RovioChannel")?;
    // sub_1000AE41C addresses the retained native LuaObject directly and
    // calls the member with no arguments.
    native_channel
        .get::<mlua::Function>("onChannelLoadingFailed")?
        .call::<()>(())
}

pub(super) fn enable_service(lua: &Lua) -> LuaResult<()> {
    // sub_1000AE354 constructs the 0x110-byte SDK object and stores it at
    // RovioChannel+0x38 before invoking the script-side onEnableService hook.
    lua.set_named_registry_value(SDK_ENABLED_REGISTRY_KEY, true)
}
