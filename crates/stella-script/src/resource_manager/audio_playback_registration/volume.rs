//! `LuaResources` master/track volume adapters and AudioManager state.

use crate::*;

fn checked_track(track: f32) -> LuaResult<usize> {
    let track = native_fcvtzs_f32(track);
    usize::try_from(track)
        .ok()
        .filter(|track| *track < 8)
        .ok_or_else(|| runtime_error(format!("Track {track} out of bounds! Range [0-7]")))
}

pub(super) fn install(
    lua: &Lua,
    resource_api: &mlua::Table,
    resource_runtime: Arc<Mutex<ResourceRuntime>>,
    audio_runtime: Arc<Mutex<AudioRuntime>>,
) -> LuaResult<()> {
    resource_api.set(
        "setMasterVolume",
        lua.create_function(move |_, args: MultiValue| {
            let volume = native_required_number(&args, 0, "setMasterVolume")? as f32;
            let mut resources = resource_runtime
                .lock()
                .expect("resource runtime lock poisoned");
            // LuaResources::setMasterVolume checks its output pointer and is
            // a void no-op when no AudioOutput has been constructed.
            if resources.audio_output_created {
                resources.master_volume = volume;
            }
            Ok(())
        })?,
    )?;

    let set_audio = Arc::clone(&audio_runtime);
    resource_api.set(
        "setTrackVolume",
        lua.create_function(move |_, args: MultiValue| {
            let volume = native_required_number(&args, 0, "setTrackVolume")? as f32;
            let track = checked_track(native_required_number(&args, 1, "setTrackVolume")? as f32)?;
            set_audio
                .lock()
                .expect("audio runtime lock poisoned")
                .track_volumes[track] = volume.clamp(0.0, 1.0);
            Ok(())
        })?,
    )?;

    resource_api.set(
        "getTrackVolume",
        lua.create_function(move |_, args: MultiValue| {
            let track = checked_track(native_required_number(&args, 0, "getTrackVolume")? as f32)?;
            Ok(audio_runtime
                .lock()
                .expect("audio runtime lock poisoned")
                .track_volumes[track])
        })?,
    )?;
    Ok(())
}
