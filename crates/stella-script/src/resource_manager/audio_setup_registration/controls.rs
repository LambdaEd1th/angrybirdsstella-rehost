//! Lua adapters for native audio device start/stop members.

use crate::*;

pub(super) fn install(
    lua: &Lua,
    resource_api: &mlua::Table,
    resource_runtime: Arc<Mutex<ResourceRuntime>>,
) -> LuaResult<()> {
    let start_audio_output_resources = Arc::clone(&resource_runtime);
    resource_api.set(
        "startAudioOutput",
        lua.create_function(move |_, ()| {
            let mut resources = start_audio_output_resources
                .lock()
                .expect("resource runtime lock poisoned");
            if !resources.audio_output_created {
                return Err(runtime_error(
                    "Trying to start audio output but no audio output has been created",
                ));
            }
            // AudioOutputImpl::initializeBuffers queries AL_MAX_GAIN only
            // when the constructor's exact -1.0 sentinel survived unchanged.
            if resources.master_volume == -1.0 {
                resources.master_volume = 1.0;
            }
            resources.audio_output_started = true;
            Ok(true)
        })?,
    )?;

    let stop_audio_output_resources = Arc::clone(&resource_runtime);
    resource_api.set(
        "stopAudioOutput",
        lua.create_function(move |_, ()| {
            let mut resources = stop_audio_output_resources
                .lock()
                .expect("resource runtime lock poisoned");
            if resources.audio_output_created {
                resources.audio_output_started = false;
            }
            Ok(())
        })?,
    )?;

    let start_audio_input_resources = Arc::clone(&resource_runtime);
    resource_api.set(
        "startAudioInput",
        lua.create_function(move |_, ()| {
            let mut resources = start_audio_input_resources
                .lock()
                .expect("resource runtime lock poisoned");
            if !resources.audio_input_created {
                return Err(runtime_error(
                    "Trying to start audio input but no audio input has been created",
                ));
            }
            resources.audio_input_started = true;
            Ok(())
        })?,
    )?;

    let stop_audio_input_resources = Arc::clone(&resource_runtime);
    resource_api.set(
        "stopAudioInput",
        lua.create_function(move |_, ()| {
            let mut resources = stop_audio_input_resources
                .lock()
                .expect("resource runtime lock poisoned");
            if resources.audio_input_created {
                resources.audio_input_started = false;
            }
            Ok(())
        })?,
    )?;
    Ok(())
}
