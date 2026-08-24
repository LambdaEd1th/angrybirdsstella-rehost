//! Lua adapters for native audio device construction.

use super::configuration::audio_io_configuration;
use crate::*;

pub(super) fn install(
    lua: &Lua,
    resource_api: &mlua::Table,
    resource_runtime: Arc<Mutex<ResourceRuntime>>,
    audio_runtime: Arc<Mutex<AudioRuntime>>,
) -> LuaResult<()> {
    let create_audio_output_resources = Arc::clone(&resource_runtime);
    let replace_audio_output = Arc::clone(&audio_runtime);
    resource_api.set(
        "createAudioOutput",
        lua.create_function(move |_, args: MultiValue| {
            // The native signature is void(float, float, float). Only one
            // output pointer exists at LuaResources +0x38; a second call
            // replaces it rather than creating an integer-addressed pool.
            let mut parameters = [0_i32; 3];
            for (index, target) in parameters.iter_mut().enumerate() {
                *target = native_fcvtzs_f32(value_number_at(&args, index).ok_or_else(|| {
                    runtime_error(format!(
                        "createAudioOutput argument {} must be number",
                        index + 1
                    ))
                })? as f32);
            }
            // sub_100459F40 releases the old output before constructing its
            // replacement. A supported Lua number tuple that fails native
            // configuration validation therefore leaves no output behind.
            {
                let mut resources = create_audio_output_resources
                    .lock()
                    .expect("resource runtime lock poisoned");
                resources.audio_output_created = false;
                resources.audio_output_started = false;
                resources.audio_output_configuration = None;
                resources.audio_output_generation =
                    resources.audio_output_generation.wrapping_add(1);
                // AudioOutputImpl owns master gain at +0x130. Its constructor
                // reinstates -1.0; initializeBuffers replaces that sentinel
                // with the OpenAL source's maximum gain on first start.
                resources.master_volume = -1.0;
            }
            replace_audio_output
                .lock()
                .expect("audio runtime lock poisoned")
                .reset_output_manager();
            let configuration = audio_io_configuration(parameters, "AudioOutput")?;
            let mut resources = create_audio_output_resources
                .lock()
                .expect("resource runtime lock poisoned");
            resources.audio_output_created = true;
            resources.audio_output_configuration = Some(configuration);
            Ok(())
        })?,
    )?;

    let create_audio_input_resources = Arc::clone(&resource_runtime);
    resource_api.set(
        "createAudioInput",
        lua.create_function(move |_, args: MultiValue| {
            let mut parameters = [0_i32; 3];
            for (index, target) in parameters.iter_mut().enumerate() {
                *target = native_fcvtzs_f32(value_number_at(&args, index).ok_or_else(|| {
                    runtime_error(format!(
                        "createAudioInput argument {} must be number",
                        index + 1
                    ))
                })? as f32);
            }
            {
                let mut resources = create_audio_input_resources
                    .lock()
                    .expect("resource runtime lock poisoned");
                resources.audio_input_created = false;
                resources.audio_input_started = false;
                resources.audio_input_configuration = None;
            }
            let configuration = audio_io_configuration(parameters, "AudioInput")?;
            let mut resources = create_audio_input_resources
                .lock()
                .expect("resource runtime lock poisoned");
            resources.audio_input_created = true;
            resources.audio_input_configuration = Some(configuration);
            Ok(())
        })?,
    )?;
    Ok(())
}
