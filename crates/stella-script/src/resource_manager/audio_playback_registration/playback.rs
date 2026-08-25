//! `game::LuaResources` clip playback members around `sub_100448A94`.

use crate::*;

enum NativeAudioSelector {
    Handle(i64),
    Name(String),
    Other,
}

/// Reproduce the handwritten INTEGER-then-lua_isstring branch shared by
/// `sub_100448C2C` and `sub_100448D68`. Purple checks the selector before the
/// selected resource member checks whether an AudioOutput exists.
fn native_audio_selector(value: Value, function: &str) -> LuaResult<NativeAudioSelector> {
    match value {
        Value::Integer(handle) => Ok(NativeAudioSelector::Handle(handle)),
        Value::Number(handle) => {
            if let Some(handle) = native_integer(&Value::Number(handle)) {
                Ok(NativeAudioSelector::Handle(handle))
            } else {
                // A plain NUMBER satisfies Purple's lua_isstring probe, then
                // fails the following exact STRING extraction.
                Err(runtime_error(format!(
                    "bad argument #1 to '{function}' (string expected)"
                )))
            }
        }
        Value::String(name) => Ok(NativeAudioSelector::Name(name.to_string_lossy())),
        _ => Ok(NativeAudioSelector::Other),
    }
}

pub(crate) fn native_play_audio(
    resource_runtime: &Arc<Mutex<ResourceRuntime>>,
    audio_runtime: &Arc<Mutex<AudioRuntime>>,
    name: String,
    volume: f32,
    looping: bool,
    channel: i32,
) -> LuaResult<i64> {
    let resources = resource_runtime
        .lock()
        .expect("resource runtime lock poisoned");
    if !resources.audio_output_created {
        return Err(runtime_error(
            "Trying to play audio clip but no audio output has been created",
        ));
    }
    // LuaResources::playAudio resolves the named clip before entering the
    // output-owned AudioManager. A missing clip therefore returns the native
    // sentinel without inspecting either manager activity or the channel.
    if !resources.audio_clips.contains(&name) {
        return Ok(-1);
    }
    // AudioManager::play at sub_100572208 checks its active byte at +0xC4
    // before its channel-count helper. Stopping output preserves old
    // instances and returns the sentinel even for an invalid channel.
    if !resources.audio_output_started {
        return Ok(-1);
    }
    // sub_1005724A8 performs an unsigned `channel < 8` check and throws
    // before counting either native instance vector. This is not the later
    // raw channel-limit array access suggested by a shallow decompilation.
    usize::try_from(channel)
        .ok()
        .filter(|channel| *channel < 8)
        .ok_or_else(|| runtime_error(format!("Track {channel} out of bounds! Range [0-7]")))?;
    drop(resources);
    Ok(audio_runtime
        .lock()
        .expect("audio runtime lock poisoned")
        .play(name, volume, looping, channel))
}

pub(crate) fn require_audio_output(
    resource_runtime: &Arc<Mutex<ResourceRuntime>>,
    operation: &str,
) -> LuaResult<()> {
    if resource_runtime
        .lock()
        .expect("resource runtime lock poisoned")
        .audio_output_created
    {
        Ok(())
    } else {
        Err(runtime_error(format!(
            "Trying to {operation} but no audio output has been created"
        )))
    }
}

pub(super) fn install(
    lua: &Lua,
    resource_api: &mlua::Table,
    resource_runtime: Arc<Mutex<ResourceRuntime>>,
    audio_runtime: Arc<Mutex<AudioRuntime>>,
) -> LuaResult<()> {
    let play_resources = Arc::clone(&resource_runtime);
    let play_audio = Arc::clone(&audio_runtime);
    resource_api.set(
        "playAudio",
        lua.create_function(move |_, args: MultiValue| {
            let name = native_required_string(&args, 0, "playAudio")?;
            let volume = if args.len() < 2 {
                1.0
            } else {
                native_required_number(&args, 1, "playAudio")? as f32
            };
            let looping = if args.len() < 3 {
                false
            } else {
                native_required_boolean(&args, 2, "playAudio")?
            };
            let channel = if args.len() < 4 {
                0
            } else {
                native_fcvtzs_f32(native_required_number(&args, 3, "playAudio")? as f32)
            };
            native_play_audio(&play_resources, &play_audio, name, volume, looping, channel)
        })?,
    )?;

    let stop_resources = Arc::clone(&resource_runtime);
    let stop_audio = Arc::clone(&audio_runtime);
    resource_api.set(
        "stopAudio",
        lua.create_function(move |_, value: Value| {
            let selector = native_audio_selector(value, "stopAudio")?;
            if matches!(selector, NativeAudioSelector::Other) {
                return Ok(());
            }
            require_audio_output(&stop_resources, "stop audio clip")?;
            let mut runtime = stop_audio.lock().expect("audio runtime lock poisoned");
            match selector {
                NativeAudioSelector::Handle(handle) => {
                    runtime.clips.remove(&AudioRuntime::native_handle(handle));
                }
                NativeAudioSelector::Name(name) => {
                    runtime.clips.retain(|_, clip| clip.name != name);
                }
                NativeAudioSelector::Other => unreachable!("other selectors returned above"),
            }
            Ok(())
        })?,
    )?;

    let stop_all_resources = Arc::clone(&resource_runtime);
    let stop_all_audio = Arc::clone(&audio_runtime);
    resource_api.set(
        "stopAllAudio",
        lua.create_function(move |_, ()| {
            require_audio_output(&stop_all_resources, "stop all audio clips")?;
            stop_all_audio
                .lock()
                .expect("audio runtime lock poisoned")
                .clips
                .clear();
            Ok(())
        })?,
    )?;

    resource_api.set(
        "isAudioPlaying",
        lua.create_function(move |_, value: Value| {
            let selector = native_audio_selector(value, "isAudioPlaying")?;
            if !resource_runtime
                .lock()
                .expect("resource runtime lock poisoned")
                .audio_output_created
            {
                return Ok(false);
            }
            let runtime = audio_runtime.lock().expect("audio runtime lock poisoned");
            Ok(match selector {
                NativeAudioSelector::Handle(handle) => runtime
                    .clips
                    .contains_key(&AudioRuntime::native_handle(handle)),
                NativeAudioSelector::Name(name) => {
                    runtime.clips.values().any(|clip| clip.name == name)
                }
                NativeAudioSelector::Other => false,
            })
        })?,
    )?;
    Ok(())
}
