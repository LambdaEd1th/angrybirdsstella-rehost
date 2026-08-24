//! Native physics/camera members split by the ownership visible in Purple.

use crate::*;

mod camera;
mod framebuffer;
mod locale;
mod physics;

pub(super) fn install_aiming_aid(
    lua: &Lua,
    globals: &mlua::Table,
    render: Arc<Mutex<RenderBridge>>,
) -> LuaResult<()> {
    physics::install_aiming_aid(lua, globals, render)
}

pub(super) fn install_physics_cluster(
    lua: &Lua,
    globals: &mlua::Table,
    render: Arc<Mutex<RenderBridge>>,
) -> LuaResult<()> {
    physics::install_core(lua, globals, Arc::clone(&render))?;
    camera::install_origin_and_max_scale(lua, globals, render)
}

pub(super) fn install_level_limits(
    lua: &Lua,
    globals: &mlua::Table,
    render: Arc<Mutex<RenderBridge>>,
) -> LuaResult<()> {
    camera::install_level_limits(lua, globals, render)
}

pub(super) fn install_starting_and_limits(
    lua: &Lua,
    globals: &mlua::Table,
    render: Arc<Mutex<RenderBridge>>,
) -> LuaResult<()> {
    camera::install_starting_and_limits(lua, globals, render)
}

pub(super) fn install_clear_screen(
    lua: &Lua,
    globals: &mlua::Table,
    render: Arc<Mutex<RenderBridge>>,
) -> LuaResult<()> {
    framebuffer::install_clear_screen(lua, globals, render)
}

pub(super) fn install_refresh_current_locale(
    lua: &Lua,
    globals: &mlua::Table,
    resources: Arc<Mutex<ResourceRuntime>>,
    data_root: Arc<PathBuf>,
) -> LuaResult<()> {
    locale::install_refresh_current_locale(lua, globals, resources, data_root)
}
