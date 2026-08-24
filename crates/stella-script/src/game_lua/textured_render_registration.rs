//! Native textured, selected, masked, 3D-text and nine-slice registration order.

mod box_draw;
mod masked;
mod selected;
mod text_3d;
mod textured;

use crate::*;

pub(crate) fn install(
    lua: &Lua,
    globals: &mlua::Table,
    render: Arc<Mutex<RenderBridge>>,
    resource_runtime: Arc<Mutex<ResourceRuntime>>,
    locale_runtime: Arc<Mutex<LocaleRuntime>>,
    data_root: Arc<PathBuf>,
) -> LuaResult<()> {
    textured::install(lua, globals, Arc::clone(&render))?;
    selected::install(
        lua,
        globals,
        Arc::clone(&render),
        Arc::clone(&resource_runtime),
        Arc::clone(&data_root),
    )?;
    masked::install(
        lua,
        globals,
        Arc::clone(&render),
        Arc::clone(&resource_runtime),
        Arc::clone(&data_root),
    )?;
    text_3d::install(
        lua,
        globals,
        Arc::clone(&render),
        Arc::clone(&resource_runtime),
        locale_runtime,
        Arc::clone(&data_root),
    )?;
    box_draw::install(lua, globals, render, resource_runtime, data_root)
}
