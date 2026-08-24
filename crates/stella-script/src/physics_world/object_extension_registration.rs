//! Ordered facade for the recovered late GameLua object/platform extensions.

use crate::*;

mod object_state;
mod radius;
mod runtime;

pub(crate) fn install(
    lua: &Lua,
    globals: &mlua::Table,
    render: Arc<Mutex<RenderBridge>>,
) -> LuaResult<()> {
    // Relative publication order in GameLua::GameLua (`sub_10002C274`).
    runtime::install_register_key(lua, globals)?; // 0x10002C900
    object_state::install_sensor_range(lua, globals, Arc::clone(&render))?; // 0x10002CC50
    object_state::install_sprite_rotation(lua, globals, Arc::clone(&render))?; // 0x10002D488
    object_state::install_velocity_multiplier(lua, globals, Arc::clone(&render))?; // 0x10002D7C8
    runtime::install_theme_reset(lua, globals, Arc::clone(&render))?; // 0x10002DEB8
    runtime::install_menu_particle_scale(lua, globals, Arc::clone(&render))?; // 0x10002E008
    radius::install(lua, globals, Arc::clone(&render))?;
    object_state::install_collision_time(lua, globals, Arc::clone(&render))?; // 0x10002EFE0
    object_state::install_revert_gravity(lua, globals, Arc::clone(&render))?; // 0x10002F0B0
    runtime::install_level_limits(lua, globals, Arc::clone(&render))?; // 0x10002F0F4
    runtime::install_rendering_state(lua, globals, Arc::clone(&render))?; // 0x10002F114
    runtime::install_recovery(lua, globals)?; // 0x10002F164
    runtime::install_sensor_force(lua, globals, render) // 0x10002F334
}
