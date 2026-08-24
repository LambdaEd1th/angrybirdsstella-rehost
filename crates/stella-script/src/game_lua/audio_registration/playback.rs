//! Unique-handle play and stop members, separated around the volume member.

use super::{optional_boolean, optional_number};
use crate::resource_manager::{native_play_audio, require_audio_output};
use crate::*;

pub(super) fn install_play(
    lua: &Lua,
    globals: &mlua::Table,
    resource_runtime: Arc<Mutex<ResourceRuntime>>,
    audio_runtime: Arc<Mutex<AudioRuntime>>,
) -> LuaResult<()> {
    globals.set(
        "playAudioReturnUniqueHandle",
        lua.create_function(move |_, args: MultiValue| {
            let name = native_required_string(&args, 0, "playAudioReturnUniqueHandle")?;
            let volume = optional_number(&args, 1, 1.0, "playAudioReturnUniqueHandle")?;
            let looping = optional_boolean(&args, 2, false, "playAudioReturnUniqueHandle")?;
            let channel = native_fcvtzs_f32(optional_number(
                &args,
                3,
                0.0,
                "playAudioReturnUniqueHandle",
            )?);
            let handle = native_play_audio(
                &resource_runtime,
                &audio_runtime,
                name.clone(),
                volume,
                looping,
                channel,
            )?;
            if std::env::var_os("STELLA_TRACE_AUDIO").is_some() {
                eprintln!(
                    "audio play handle={handle} name={name:?} volume={volume} looping={looping} channel={channel}"
                );
            }
            Ok(handle)
        })?,
    )
}

pub(super) fn install_stop(
    lua: &Lua,
    globals: &mlua::Table,
    resource_runtime: Arc<Mutex<ResourceRuntime>>,
    audio_runtime: Arc<Mutex<AudioRuntime>>,
) -> LuaResult<()> {
    globals.set(
        "stopAudioWithHandle",
        lua.create_function(move |_, args: MultiValue| {
            let handle = AudioRuntime::native_handle(native_required_integer(
                &args,
                0,
                "stopAudioWithHandle",
            )?);
            require_audio_output(&resource_runtime, "stop audio clip")?;
            if let Some(clip) = audio_runtime
                .lock()
                .expect("audio runtime lock poisoned")
                .clips
                .remove(&handle)
                && std::env::var_os("STELLA_TRACE_AUDIO").is_some()
            {
                eprintln!("audio stop handle={handle} name={:?}", clip.name);
            }
            Ok(())
        })?,
    )
}
