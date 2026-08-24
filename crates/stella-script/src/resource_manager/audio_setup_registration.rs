//! Audio input/output and clip creation bindings.
//!
//! Purple publishes these entries from one `LuaResources` constructor, while
//! the called native owners split into device construction, clip construction,
//! format validation, and device control paths. Keep that ownership visible
//! here without changing the recovered publication order.

mod clips;
mod configuration;
mod controls;
mod devices;

use crate::*;

pub(crate) fn install_creation(
    lua: &Lua,
    resource_api: &mlua::Table,
    resource_runtime: Arc<Mutex<ResourceRuntime>>,
    audio_runtime: Arc<Mutex<AudioRuntime>>,
    data_root: Arc<PathBuf>,
) -> LuaResult<()> {
    devices::install(
        lua,
        resource_api,
        Arc::clone(&resource_runtime),
        Arc::clone(&audio_runtime),
    )?;
    clips::install(
        lua,
        resource_api,
        resource_runtime,
        audio_runtime,
        data_root,
    )
}

pub(crate) fn install_controls(
    lua: &Lua,
    resource_api: &mlua::Table,
    resource_runtime: Arc<Mutex<ResourceRuntime>>,
) -> LuaResult<()> {
    controls::install(lua, resource_api, resource_runtime)
}
