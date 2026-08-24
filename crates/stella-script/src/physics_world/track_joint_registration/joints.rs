//! Native-order facade for the independent joint extension members.

mod limits;
mod parameters;
mod removal;

use crate::*;

pub(super) fn install(
    lua: &Lua,
    globals: &mlua::Table,
    render: Arc<Mutex<RenderBridge>>,
) -> LuaResult<()> {
    parameters::install(lua, globals, Arc::clone(&render))?;
    removal::install(lua, globals, Arc::clone(&render))?;
    limits::install(lua, globals, render)
}
