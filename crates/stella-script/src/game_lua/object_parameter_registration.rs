//! RenderObject parameter members split at their distant constructor sites.

use crate::*;

mod gravity;
mod parameter;

pub(super) fn install_gravity(
    lua: &Lua,
    globals: &mlua::Table,
    render: Arc<Mutex<RenderBridge>>,
) -> LuaResult<()> {
    gravity::install(lua, globals, render)
}

pub(super) fn install_parameter(
    lua: &Lua,
    globals: &mlua::Table,
    render: Arc<Mutex<RenderBridge>>,
) -> LuaResult<()> {
    parameter::install(lua, globals, render)
}
