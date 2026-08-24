//! Ordered façade for Purple's environment/control members.
//!
//! The relative order follows the string registration sites in
//! `GameLua::GameLua` (`sub_10002C274`).  Each leaf owns one recovered member
//! family instead of collecting every environment concern in this file.

use crate::*;

mod device;
mod gameplay;
mod water;
mod world_controls;

pub(super) fn install_early(
    lua: &Lua,
    globals: &mlua::Table,
    render: Arc<Mutex<RenderBridge>>,
) -> LuaResult<()> {
    gameplay::install_request_exit(lua, globals, Arc::clone(&render))?; // 0x10002C770
    world_controls::install_smooth_zoom(lua, globals, Arc::clone(&render))?; // 0x10002C8D0
    water::install(lua, globals, Arc::clone(&render))?; // 0x10002CAE0..0x10002CB70
    device::install_notification(lua, globals, Arc::clone(&render))?; // 0x10002CEC0
    world_controls::install_gravity(lua, globals, render) // 0x10002CEF0
}

pub(super) fn install_editing(
    lua: &Lua,
    globals: &mlua::Table,
    render: Arc<Mutex<RenderBridge>>,
) -> LuaResult<()> {
    world_controls::install_editing(lua, globals, render) // 0x10002D938
}

pub(super) fn install_mouse_wheel(
    lua: &Lua,
    globals: &mlua::Table,
    render: Arc<Mutex<RenderBridge>>,
) -> LuaResult<()> {
    world_controls::install_mouse_wheel(lua, globals, render) // 0x10002D998
}

pub(super) fn install_theme_and_sensor(
    lua: &Lua,
    globals: &mlua::Table,
    render: Arc<Mutex<RenderBridge>>,
) -> LuaResult<()> {
    device::install_theme_refresh(lua, globals, Arc::clone(&render))?; // 0x10002DE88
    device::install_accelerometer(lua, globals, render) // 0x10002DF18
}

pub(super) fn install_game_on(
    lua: &Lua,
    globals: &mlua::Table,
    render: Arc<Mutex<RenderBridge>>,
) -> LuaResult<()> {
    gameplay::install_game_on(lua, globals, render) // 0x10002E748
}

pub(super) fn install_orientation(
    lua: &Lua,
    globals: &mlua::Table,
    render: Arc<Mutex<RenderBridge>>,
) -> LuaResult<()> {
    device::install_orientation(lua, globals, render) // 0x10002E91C
}

pub(super) fn install_parameters(
    lua: &Lua,
    globals: &mlua::Table,
    render: Arc<Mutex<RenderBridge>>,
) -> LuaResult<()> {
    gameplay::install_parameters(lua, globals, render) // 0x10002EB64
}

pub(super) fn install_os_name(lua: &Lua, globals: &mlua::Table) -> LuaResult<()> {
    device::install_os_name(lua, globals) // 0x10002F314
}
