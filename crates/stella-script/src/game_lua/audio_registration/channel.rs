//! `setChannelCountLimit`, the first audio member at `0x10002EE48`.

use crate::*;

pub(super) fn install(
    lua: &Lua,
    globals: &mlua::Table,
    audio_runtime: Arc<Mutex<AudioRuntime>>,
) -> LuaResult<()> {
    globals.set(
        "setChannelCountLimit",
        lua.create_function(move |_, args: MultiValue| {
            let channel =
                native_fcvtzs_f32(native_required_number(&args, 0, "setChannelCountLimit")? as f32);
            let limit =
                native_fcvtzs_f32(native_required_number(&args, 1, "setChannelCountLimit")? as f32);
            let Some(channel) = usize::try_from(channel).ok().filter(|channel| *channel < 8) else {
                return Err(runtime_error(format!(
                    "Track {channel} out of bounds! Range [0-7]"
                )));
            };
            audio_runtime
                .lock()
                .expect("audio runtime lock poisoned")
                .channel_limits[channel] = limit;
            Ok(())
        })?,
    )
}
