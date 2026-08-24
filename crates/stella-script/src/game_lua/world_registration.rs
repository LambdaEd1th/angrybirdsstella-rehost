//! Ordered façade for GameLua's world/control registration constructor.
//!
//! Purple's `sub_10002C274` is a 15,028-byte generated registration routine.
//! Keep its observable registration order here while the member-function
//! families live in focused modules matching the recovered native ownership.

use crate::*;

pub(crate) fn install(
    lua: &Lua,
    globals: &mlua::Table,
    render: Arc<Mutex<RenderBridge>>,
    resources: Arc<Mutex<ResourceRuntime>>,
    data_root: Arc<PathBuf>,
) -> LuaResult<()> {
    // The conversion helpers are script-facing aliases over shared state;
    // native members below follow their exact relative sites in sub_10002C274.
    super::world_transform_registration::install_coordinates(lua, globals, Arc::clone(&render))?;
    super::world_environment_registration::install_early(lua, globals, Arc::clone(&render))?;
    super::world_physics_camera_registration::install_aiming_aid(
        lua,
        globals,
        Arc::clone(&render),
    )?; // 0x10002CF20
    super::world_physics_camera_registration::install_physics_cluster(
        lua,
        globals,
        Arc::clone(&render),
    )?; // 0x10002D7F8..0x10002D8D8
    super::world_environment_registration::install_editing(lua, globals, Arc::clone(&render))?; // 0x10002D938
    super::world_transform_registration::install_world_scale(lua, globals, Arc::clone(&render))?; // 0x10002D968
    super::world_environment_registration::install_mouse_wheel(lua, globals, Arc::clone(&render))?; // 0x10002D998
    super::world_transform_registration::install_render_state(lua, globals, Arc::clone(&render))?; // 0x10002DAF8
    super::world_transform_registration::install_alpha(lua, globals, Arc::clone(&render))?; // 0x10002DB28
    super::world_physics_camera_registration::install_clear_screen(
        lua,
        globals,
        Arc::clone(&render),
    )?; // 0x10002DB88
    super::world_environment_registration::install_theme_and_sensor(
        lua,
        globals,
        Arc::clone(&render),
    )?; // 0x10002DE88, 0x10002DF18
    super::world_physics_camera_registration::install_level_limits(
        lua,
        globals,
        Arc::clone(&render),
    )?; // 0x10002E6E0
    super::world_environment_registration::install_game_on(lua, globals, Arc::clone(&render))?; // 0x10002E748
    super::world_physics_camera_registration::install_starting_and_limits(
        lua,
        globals,
        Arc::clone(&render),
    )?; // 0x10002E7B0..0x10002E7E4
    super::world_physics_camera_registration::install_refresh_current_locale(
        lua, globals, resources, data_root,
    )?; // 0x10002E880
    super::world_environment_registration::install_orientation(lua, globals, Arc::clone(&render))?; // 0x10002E91C
    super::world_environment_registration::install_parameters(lua, globals, Arc::clone(&render))?; // 0x10002EB64
    super::world_environment_registration::install_os_name(lua, globals) // 0x10002F314
}
