//! Audio bindings split at Purple's `LuaResources` and `GameLua` owners.

mod playback;
mod volume;

use crate::*;

pub(crate) use playback::{native_play_audio, require_audio_output};

pub(crate) fn install_resource_playback(
    lua: &Lua,
    resource_api: &mlua::Table,
    resource_runtime: Arc<Mutex<ResourceRuntime>>,
    audio_runtime: Arc<Mutex<AudioRuntime>>,
) -> LuaResult<()> {
    playback::install(lua, resource_api, resource_runtime, audio_runtime)
}

pub(crate) fn install_volume(
    lua: &Lua,
    resource_api: &mlua::Table,
    resource_runtime: Arc<Mutex<ResourceRuntime>>,
    audio_runtime: Arc<Mutex<AudioRuntime>>,
) -> LuaResult<()> {
    volume::install(lua, resource_api, resource_runtime, audio_runtime)
}
