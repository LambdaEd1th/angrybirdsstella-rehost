//! Ordered façade for Purple's immediate primitive members.

use crate::*;

mod lines;
mod polygon;
mod rectangle;

pub(crate) fn install(
    lua: &Lua,
    globals: &mlua::Table,
    render: Arc<Mutex<RenderBridge>>,
) -> LuaResult<()> {
    rectangle::install(lua, globals, Arc::clone(&render))?; // 0x10002D9F8
    polygon::install(lua, globals, Arc::clone(&render))?; // 0x10002DA48
    lines::install(lua, globals, render) // 0x10002E308..0x10002E338
}
