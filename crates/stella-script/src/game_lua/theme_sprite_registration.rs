//! Contiguous ThemeSprite registration cluster.

mod create;
mod modify;
mod remove;
mod rotate;

use crate::*;

pub(crate) fn install(
    lua: &Lua,
    globals: &mlua::Table,
    render: Arc<Mutex<RenderBridge>>,
) -> LuaResult<()> {
    // Exact constructor order at 0x10002D1A0..0x10002D244.
    create::install(lua, globals, Arc::clone(&render))?;
    remove::install(lua, globals, Arc::clone(&render))?;
    modify::install(lua, globals, Arc::clone(&render))?;
    rotate::install(lua, globals, render)
}
