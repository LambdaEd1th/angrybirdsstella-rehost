//! `game::LuaResources` clip playback members around `sub_100448A94`.

use crate::*;

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
    // AudioManager::play at sub_100572208 checks its active byte at +0xC4
    // before allocating an AudioClipInstance. Stopping output preserves old
    // instances but rejects new playback with the native sentinel.
    if !resources.audio_output_started {
        return Ok(-1);
    }
    if !resources.audio_clips.contains(&name) {
        return Ok(-1);
    }
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
            require_audio_output(&stop_resources, "stop audio clip")?;
            let mut runtime = stop_audio.lock().expect("audio runtime lock poisoned");
            match value {
                Value::Integer(handle) => {
                    runtime.clips.remove(&AudioRuntime::native_handle(handle));
                }
                Value::Number(handle) if native_integer(&Value::Number(handle)).is_some() => {
                    runtime
                        .clips
                        .remove(&AudioRuntime::native_handle(handle as i64));
                }
                Value::String(name) => {
                    let name = name.to_string_lossy();
                    runtime.clips.retain(|_, clip| clip.name != name);
                }
                _ => {}
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
            if !resource_runtime
                .lock()
                .expect("resource runtime lock poisoned")
                .audio_output_created
            {
                return Ok(false);
            }
            let runtime = audio_runtime.lock().expect("audio runtime lock poisoned");
            Ok(match value {
                Value::Integer(handle) => runtime
                    .clips
                    .contains_key(&AudioRuntime::native_handle(handle)),
                Value::Number(handle) if native_integer(&Value::Number(handle)).is_some() => {
                    runtime
                        .clips
                        .contains_key(&AudioRuntime::native_handle(handle as i64))
                }
                Value::String(name) => {
                    let name = name.to_string_lossy();
                    runtime.clips.values().any(|clip| clip.name == name)
                }
                _ => false,
            })
        })?,
    )?;
    Ok(())
}
