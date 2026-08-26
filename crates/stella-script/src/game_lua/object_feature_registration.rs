//! Ordered coordinator for the RenderObject feature members registered by GameLua.

use super::{
    object_decoration_registration, object_joint_registration, object_lifecycle_registration,
    object_parameter_registration,
};
use crate::*;

pub(crate) fn install(
    lua: &Lua,
    globals: &mlua::Table,
    render: Arc<Mutex<RenderBridge>>,
    draw_callbacks: Rc<RefCell<DrawCallbacks>>,
) -> LuaResult<()> {
    object_parameter_registration::install_gravity(lua, globals, Arc::clone(&render))?; // 0x10002CC80..CCB0
    object_decoration_registration::install_decoration(lua, globals, Arc::clone(&render))?; // 0x10002D4B8
    object_decoration_registration::install_pivot(lua, globals, Arc::clone(&render))?; // 0x10002D4E8
    object_lifecycle_registration::install_remove(
        lua,
        globals,
        Arc::clone(&render),
        draw_callbacks,
    )?; // 0x10002D908
    object_joint_registration::install(lua, globals, Arc::clone(&render))?; // 0x10002DAA8
    object_parameter_registration::install_parameter(lua, globals, Arc::clone(&render))?; // 0x10002E4E8
    object_lifecycle_registration::install_flash(lua, globals, render) // 0x10002E578..E5A8
}
