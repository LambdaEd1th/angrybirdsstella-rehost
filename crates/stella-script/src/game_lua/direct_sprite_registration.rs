//! Direct sprite/composite registration in `sub_10002C274` order.

mod composite;
mod lookup;
mod plain;
mod shader;

use crate::*;

pub(crate) fn install(
    lua: &Lua,
    globals: &mlua::Table,
    render: Arc<Mutex<RenderBridge>>,
    resource_runtime: Arc<Mutex<ResourceRuntime>>,
    data_root: Arc<PathBuf>,
) -> LuaResult<()> {
    // 0x10002E354..0x10002E40C registers these four native members in this
    // exact order. Each leaf below owns one recovered member boundary.
    composite::install(
        lua,
        globals,
        Arc::clone(&render),
        Arc::clone(&resource_runtime),
        Arc::clone(&data_root),
    )?;
    shader::install(
        lua,
        globals,
        Arc::clone(&render),
        Arc::clone(&resource_runtime),
        Arc::clone(&data_root),
    )?;
    plain::install(
        lua,
        globals,
        render,
        Arc::clone(&resource_runtime),
        data_root,
    )?;
    lookup::install(lua, globals, resource_runtime)
}
