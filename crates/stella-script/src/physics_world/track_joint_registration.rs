//! GameLua bindings split along Purple's native track/joint registration clusters.

use crate::*;

mod flags;
mod joints;
mod track;
mod vertices;

pub(crate) fn install(
    lua: &Lua,
    globals: &mlua::Table,
    render: Arc<Mutex<RenderBridge>>,
) -> LuaResult<()> {
    // Preserve the order of the recovered sub_10002C274 registration groups.
    track::install(lua, globals, Arc::clone(&render))?;
    joints::install(lua, globals, Arc::clone(&render))?;
    flags::install(lua, globals, Arc::clone(&render))?;
    vertices::install(lua, globals, render)
}
