//! Native-order body/fixture coefficient, flag, and activation facade.

use crate::*;

mod activity;
mod density;
mod flags;
mod scalars;

pub(crate) fn install(
    lua: &Lua,
    globals: &mlua::Table,
    render: Arc<Mutex<RenderBridge>>,
) -> LuaResult<()> {
    density::install(lua, globals, &render)?;
    scalars::install(lua, globals, &render)?;
    flags::install(lua, globals, &render)?;
    activity::install(lua, globals, &render)
}
