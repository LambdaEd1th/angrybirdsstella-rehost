//! `setChannelCountLimit`, the first audio member at `0x10002EE48`.

use crate::resource_manager::require_audio_output;
use crate::*;

pub(super) fn install(
    lua: &Lua,
    globals: &mlua::Table,
    resource_runtime: Arc<Mutex<ResourceRuntime>>,
    audio_runtime: Arc<Mutex<AudioRuntime>>,
) -> LuaResult<()> {
    globals.set(
        "setChannelCountLimit",
        lua.create_function(move |_, args: MultiValue| {
            let channel =
                native_fcvtzs_f32(native_required_number(&args, 0, "setChannelCountLimit")? as f32);
            let limit =
                native_fcvtzs_f32(native_required_number(&args, 1, "setChannelCountLimit")? as f32);
            // sub_100058FFC obtains LuaResources' AudioOutputImpl pointer,
            // then sub_1005796C8 dereferences it before AudioManager checks
            // the converted channel index. No independent pre-output limit
            // table exists in Purple.
            require_audio_output(&resource_runtime, "set channel count limit")?;
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
