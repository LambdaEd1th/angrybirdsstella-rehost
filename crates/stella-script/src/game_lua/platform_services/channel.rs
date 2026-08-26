//! Rovio Channel/Toons service lifecycle and retired-content fallback.

use crate::*;

const SDK_ENABLED_REGISTRY_KEY: &str = "stella.rovio_channel.sdk_enabled";

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
) -> LuaResult<()> {
    lua.set_named_registry_value(SDK_ENABLED_REGISTRY_KEY, false)?;
    resource_runtime
        .lock()
        .expect("resource runtime lock poisoned")
        .register_sprite_aliases(RETIRED_CHANNEL_SPRITE_FALLBACKS);

    let channel = lua.create_table()?;

    channel.set(
        "openChannelView",
        lua.create_function(|lua, args: MultiValue| {
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
                // Purple receives this callback asynchronously after its
                // Channel 1.2 request fails. The endpoint is retired, so the
                // desktop equivalent completes the failure continuation at
                // the native boundary instead of leaving ConnectionScreen up
                // forever. The callback is installed by ChannelManager.lua.
                let native_channel: mlua::Table = lua.globals().get("RovioChannel")?;
                if let Value::Function(callback) =
                    native_channel.get::<Value>("onChannelLoadingFailed")?
                {
                    callback.call::<()>(())?;
                }
            }
            Ok(())
        })?,
    )?;

    for method in [
        "cancelChannelViewLoading",
        "updateNewContent",
        "onMenuInitialised",
    ] {
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
    Ok(())
}

pub(super) fn enable_service(lua: &Lua) -> LuaResult<()> {
    // sub_1000AE354 constructs the 0x110-byte SDK object and stores it at
    // RovioChannel+0x38 before invoking the script-side onEnableService hook.
    lua.set_named_registry_value(SDK_ENABLED_REGISTRY_KEY, true)
}
