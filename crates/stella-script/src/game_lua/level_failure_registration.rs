//! Late one-string Lua file failure callback registration.

use crate::*;

pub(crate) fn install(lua: &Lua, globals: &mlua::Table) -> LuaResult<()> {
    globals.set(
        "onLoadLuaFileFail",
        lua.create_function(|_, args: MultiValue| {
            // sub_100089E6C strictly consumes one string before dispatching
            // the callback relay at sub_100056950.
            native_required_string(&args, 0, "onLoadLuaFileFail")?;
            Ok(())
        })?,
    )?;
    Ok(())
}
