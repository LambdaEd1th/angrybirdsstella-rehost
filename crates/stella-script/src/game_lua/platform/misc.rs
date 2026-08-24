//! Literal platform stubs and strict filesystem adapters.

use mlua::{Lua, MultiValue, Result as LuaResult, Table};

use crate::native_required_string;

pub(super) fn install(lua: &Lua, globals: &Table) -> LuaResult<()> {
    globals.set(
        "checkDirectory",
        lua.create_function(|_, args: MultiValue| {
            native_required_string(&args, 0, "checkDirectory")?;
            Ok(false)
        })?,
    )?;
    let directory_file_list_globals = globals.clone();
    globals.set(
        "getDirectoryFileList",
        lua.create_function(move |_, args: MultiValue| {
            native_required_string(&args, 0, "getDirectoryFileList")?;
            // The shipped member thunk at sub_10005A298 ignores the path and
            // constructs a lua::LuaTable from GameLua+0x18. Its constructor
            // sub_100529C84 retains stack index -10000 (LUA_GLOBALSINDEX), so
            // this literal platform stub returns the global table itself.
            Ok(directory_file_list_globals.clone())
        })?,
    )?;
    for exact_noop in ["goToTaskSwitcherLua", "printGlobals"] {
        globals.set(
            exact_noop,
            lua.create_function(|_, _: MultiValue| Ok(MultiValue::new()))?,
        )?;
    }
    for one_string_noop in ["createDirectory", "linkSensor", "print"] {
        globals.set(
            one_string_noop,
            lua.create_function(move |_, args: MultiValue| {
                native_required_string(&args, 0, one_string_noop)?;
                Ok(())
            })?,
        )?;
    }
    globals.set(
        "printWithTag",
        lua.create_function(|_, args: MultiValue| {
            native_required_string(&args, 0, "printWithTag")?;
            native_required_string(&args, 1, "printWithTag")?;
            Ok(())
        })?,
    )?;
    globals.set(
        "sendTweet",
        lua.create_function(|_, args: MultiValue| {
            for index in 0..4 {
                native_required_string(&args, index, "sendTweet")?;
            }
            Ok(())
        })?,
    )?;
    globals.set(
        "isTwitterSupported",
        lua.create_function(|_, _: MultiValue| Ok(false))?,
    )?;
    globals.set(
        "isInFullScreenMode",
        // GameLua member sub_1000310E4 dispatches through IOSOSInterface's
        // vtable slot +0x58. Purple's concrete iOS member sub_100405434 is
        // literally `MOV W0, #1; RET`.
        lua.create_function(|_, _: MultiValue| Ok(true))?,
    )?;
    Ok(())
}
