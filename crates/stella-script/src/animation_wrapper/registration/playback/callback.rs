//! Playback callback registration and callback-table ownership.

use mlua::{Lua, MultiValue, Result as LuaResult, Table, Value};

use crate::{native_required_string, runtime_error};

pub(in super::super) fn install_callback(
    lua: &Lua,
    animation_native: &Table,
    animation_callbacks: Table,
) -> LuaResult<()> {
    let callbacks = animation_callbacks.clone();
    animation_native.set(
        "setPlaybackEvent",
        lua.create_function(move |_, args: MultiValue| {
            let tag = native_required_string(&args, 0, "setPlaybackEvent")?;
            let callback = match args.iter().nth(1) {
                Some(Value::Function(callback)) => callback.clone(),
                _ => {
                    return Err(runtime_error(
                        "bad argument #2 to 'setPlaybackEvent' (function expected)",
                    ));
                }
            };
            callbacks.raw_set(tag, callback)?;
            Ok(())
        })?,
    )?;
    Ok(())
}
