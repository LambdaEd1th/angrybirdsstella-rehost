//! Capture member plus the final `openURL`/`res` publication tail.

use crate::*;

pub(crate) fn install_capture(
    lua: &Lua,
    resource_api: &mlua::Table,
    render: Arc<Mutex<RenderBridge>>,
    resources: Arc<Mutex<ResourceRuntime>>,
    data_root: Arc<PathBuf>,
) -> LuaResult<()> {
    resource_api.set(
        "captureSprite",
        lua.create_function(move |_, args: MultiValue| {
            let name = native_required_string(&args, 0, "captureSprite")?;
            let mut bridge = render.lock().expect("render bridge lock poisoned");
            let dimensions = [bridge.screen_width, bridge.screen_height];
            let (texture_source, temporary) = resources
                .lock()
                .expect("resource runtime lock poisoned")
                .capture_sprite(&name, dimensions, &data_root)?;
            bridge.push_capture_command(name, texture_source, temporary);
            Ok(())
        })?,
    )
}

pub(crate) fn install_open_url_and_publish(
    lua: &Lua,
    globals: &mlua::Table,
    resource_api: &mlua::Table,
    render: Arc<Mutex<RenderBridge>>,
) -> LuaResult<()> {
    resource_api.set(
        "openURL",
        lua.create_function(move |_, args: MultiValue| {
            let url = native_required_string(&args, 0, "openURL")?;
            // LuaResources::openURL (`sub_10044AAF8`) constructs the iOS
            // platform adapter, calls UIApplication openURL:, and returns its
            // boolean. The desktop host accepts the request synchronously and
            // performs the platform call at the next application frame.
            render
                .lock()
                .expect("render bridge lock poisoned")
                .platform_action_requests
                .push(PlatformActionRequest::OpenUrl { url });
            Ok(true)
        })?,
    )?;
    globals.set("res", resource_api)
}
