//! Playback callback registration and callback-table ownership.

use mlua::{Function, Lua, Result as LuaResult, Table};

pub(in super::super) fn install_callback(
    lua: &Lua,
    animation_native: &Table,
    animation_callbacks: Table,
) -> LuaResult<()> {
    let callbacks = animation_callbacks.clone();
    animation_native.set(
        "setPlaybackEvent",
        lua.create_function(move |_, (tag, callback): (String, Function)| {
            callbacks.raw_set(tag, callback)?;
            Ok(())
        })?,
    )?;
    Ok(())
}
