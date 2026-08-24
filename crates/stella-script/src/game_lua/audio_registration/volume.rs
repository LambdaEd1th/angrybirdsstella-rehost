//! `setAudioClipVolume`, registered between play and stop at `0x10002EE8C`.

use crate::*;

pub(super) fn install(
    lua: &Lua,
    globals: &mlua::Table,
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
