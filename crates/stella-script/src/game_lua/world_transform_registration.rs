//! Coordinate helpers and renderer-state members split at native boundaries.

use crate::*;

mod coordinates;
mod render_state;

pub(super) fn install_coordinates(
    lua: &Lua,
    globals: &mlua::Table,
    render: Arc<Mutex<RenderBridge>>,
) -> LuaResult<()> {
    coordinates::install(lua, globals, render)
}

pub(super) fn install_world_scale(
    lua: &Lua,
    globals: &mlua::Table,
    render: Arc<Mutex<RenderBridge>>,
) -> LuaResult<()> {
    render_state::install_world_scale(lua, globals, render)
}

pub(super) fn install_render_state(
    lua: &Lua,
    globals: &mlua::Table,
    render: Arc<Mutex<RenderBridge>>,
) -> LuaResult<()> {
    render_state::install_render_state(lua, globals, render)
}

pub(super) fn install_alpha(
    lua: &Lua,
    globals: &mlua::Table,
    render: Arc<Mutex<RenderBridge>>,
) -> LuaResult<()> {
    render_state::install_alpha(lua, globals, render)
}
