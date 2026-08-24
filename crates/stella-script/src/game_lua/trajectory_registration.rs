//! Native-order facade for the three independent trajectory stores.

mod aim_stream;
mod simulation;
mod trail_buffers;

use crate::*;

pub(crate) fn install(
    lua: &Lua,
    globals: &mlua::Table,
    render: Arc<Mutex<RenderBridge>>,
    resources: Arc<Mutex<ResourceRuntime>>,
    data_root: Arc<PathBuf>,
) -> LuaResult<()> {
    // sub_10002C274 registers these members among unrelated GameLua APIs.
    // Preserve their exact relative order instead of grouping by Rust store.
    // 0x10002CBA0
    simulation::install_clear(lua, globals, Arc::clone(&render))?;
    // 0x10002CBC0
    simulation::install_get(lua, globals, Arc::clone(&render))?;
    // 0x10002CD50
    simulation::install_update(lua, globals, Arc::clone(&render))?;
    // 0x10002CD80
    aim_stream::install_populate(lua, globals, Arc::clone(&render))?;
    // 0x10002CDB0
    simulation::install_selected(lua, globals, Arc::clone(&render))?;
    // 0x10002DD08, then 0x10002DD38
    aim_stream::install_time(lua, globals)?;
    aim_stream::install_clear(lua, globals, Arc::clone(&render))?;
    // 0x10002DEE8
    aim_stream::install_draw(lua, globals, Arc::clone(&render), resources, data_root)?;
    // 0x10002E644, 0x10002E678, 0x10002E6AC
    trail_buffers::install_points(lua, globals, Arc::clone(&render))?;
    // 0x10002EDAC, 0x10002EDE0, 0x10002EE14
    trail_buffers::install_sprites(lua, globals, render)
}
