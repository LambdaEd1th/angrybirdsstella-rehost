//! `setAudioClipVolume`, registered between play and stop at `0x10002EE8C`.

use crate::resource_manager::require_audio_output;
use crate::*;

pub(super) fn install(
    lua: &Lua,
    globals: &mlua::Table,
    resource_runtime: Arc<Mutex<ResourceRuntime>>,
    audio_runtime: Arc<Mutex<AudioRuntime>>,
) -> LuaResult<()> {
    globals.set(
        "setAudioClipVolume",
        lua.create_function(move |_, args: MultiValue| {
            let handle = AudioRuntime::native_handle(native_required_integer(
                &args,
                0,
                "setAudioClipVolume",
            )?);
            let volume = native_required_number(&args, 1, "setAudioClipVolume")? as f32;
            // sub_10005920C validates both Lua slots first, then follows the
            // live AudioOutputImpl pointer for both the handle query and the
            // volume write. Preserve that ownership/order safely.
            require_audio_output(&resource_runtime, "set audio clip volume")?;
            if let Some(clip) = audio_runtime
                .lock()
                .expect("audio runtime lock poisoned")
                .clips
                .get_mut(&handle)
            {
                clip.volume = volume;
                if std::env::var_os("STELLA_TRACE_AUDIO").is_some() {
                    eprintln!(
                        "audio volume handle={handle} name={:?} volume={volume} looping={} channel={}",
                        clip.name, clip.looping, clip.channel
                    );
                }
            }
            Ok(())
        })?,
    )
}
