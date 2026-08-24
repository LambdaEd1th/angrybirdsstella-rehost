//! `removeJointsFromObject` (`sub_1000442D8`) and its synchronous exits.

use crate::*;

pub(super) fn install(
    lua: &Lua,
    globals: &mlua::Table,
    render: Arc<Mutex<RenderBridge>>,
) -> LuaResult<()> {
    globals.set(
        "removeJointsFromObject",
        lua.create_function(move |lua, args: MultiValue| {
            let object = native_required_string(&args, 0, "removeJointsFromObject")?;
            remove_native_object_joints_with_callbacks(lua, &render, &object)?;
            Ok(())
        })?,
    )?;
    Ok(())
}
