//! Capture member plus the final `openURL`/`res` publication tail.

use crate::*;

pub(crate) fn install_capture(
    lua: &Lua,
    resource_api: &mlua::Table,
    render: Arc<Mutex<RenderBridge>>,
) -> LuaResult<()> {
    resource_api.set(
        "captureSprite",
        lua.create_function(move |_, args: MultiValue| {
            let name = native_required_string(&args, 0, "captureSprite")?;
            render
                .lock()
                .expect("render bridge lock poisoned")
                .push_capture_command(name);
            Ok(())
        })?,
    )
}

pub(crate) fn install_open_url_and_publish(
    lua: &Lua,
    globals: &mlua::Table,
    resource_api: &mlua::Table,
) -> LuaResult<()> {
    resource_api.set(
        "openURL",
        lua.create_function(|_, args: MultiValue| {
            let _ = native_required_string(&args, 0, "openURL")?;
            Ok(false)
        })?,
    )?;
    globals.set("res", resource_api)
}
