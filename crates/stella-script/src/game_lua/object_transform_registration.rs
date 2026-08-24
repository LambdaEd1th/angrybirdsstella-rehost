//! Ordered coordinator for RenderObject pose and Box2D fixture scaling.

use super::{object_physics_scale_registration, object_pose_registration};
use crate::*;

pub(crate) fn install(
    lua: &Lua,
    globals: &mlua::Table,
    render: Arc<Mutex<RenderBridge>>,
) -> LuaResult<()> {
    object_pose_registration::install(lua, globals, Arc::clone(&render))?;
    object_physics_scale_registration::install(lua, globals, render)
}
