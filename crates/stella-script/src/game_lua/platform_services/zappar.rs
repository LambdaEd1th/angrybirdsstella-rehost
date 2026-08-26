//! Zappar augmented-reality bridge and unsupported-host close behavior.

use crate::*;

pub(super) fn install(lua: &Lua, globals: &mlua::Table) -> LuaResult<()> {
    let zappar = lua.create_table()?;
    zappar.set(
        "native_isZapparSupported",
        lua.create_function(|_, _: MultiValue| {
            // LuaZappar::lua_isZapparSupported ultimately calls
            // +[ZapparEmbed isDeviceCompatible]. The desktop rehost has no
            // Zappar camera SDK, so it is not a compatible device.
            Ok(false)
        })?,
    )?;
    zappar.set(
        "native_launchZappar",
        lua.create_function(|_, args: MultiValue| {
            // Adapter sub_1000E38CC reads a required LuaFunction from slot
            // one. The native iOS component invokes it from its `onClosed`
            // block after the presented Zappar view is dismissed. There is
            // no AR view to present on the desktop host, so complete that
            // lifecycle immediately. ZapparHandler's callback restores the
            // audio state it paused before entering this member.
            let callback = match args.front() {
                Some(Value::Function(callback)) => callback.clone(),
                _ => {
                    return Err(runtime_error(
                        "bad argument #1 to 'native_launchZappar' (function expected)".to_owned(),
                    ));
                }
            };
            callback.call::<()>(())
        })?,
    )?;
    globals.set("Zappar", zappar)?;
    Ok(())
}
