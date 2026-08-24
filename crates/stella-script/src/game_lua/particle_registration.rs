//! Ordered façade for GameLua particle publication and native members.

use crate::*;

mod clear;
mod draw;
mod spawn;

pub(crate) fn install(
    lua: &Lua,
    globals: &mlua::Table,
    render: Arc<Mutex<RenderBridge>>,
    resources: Arc<Mutex<ResourceRuntime>>,
    data_root: Arc<PathBuf>,
) -> LuaResult<()> {
    // Publish the Lua-owned containers before installing their methods.
    for name in ["objects", "particles"] {
        globals.set(name, lua.create_table()?)?;
    }
    draw::install(lua, globals, Arc::clone(&render))?; // 0x10002DF48..0x10002DFD8
    clear::install(lua, globals, Arc::clone(&render))?; // 0x10002E038..0x10002E098
    spawn::install(lua, globals, render, resources, data_root) // native add at 0x10002F6D8
}
