//! Fixed-schema editor level export (`sub_10004730C`).

use super::level_save_schema::native_saved_level_table;
use crate::*;

pub(crate) fn install(lua: &Lua, globals: &mlua::Table, data_root: Arc<PathBuf>) -> LuaResult<()> {
    globals.set(
        "saveLevel",
        lua.create_function(move |lua, args: MultiValue| {
            let file_name = native_required_string(&args, 0, "saveLevel")?;
            let file_name = with_lua_extension(file_name);
            let destination = app_data_path(&data_root, &file_name).map_err(runtime_error)?;
            let objects = native_lua_object(lua, NativeLuaObject::Objects)?
                .ok_or_else(|| runtime_error("objects is not a table"))?;
            let level = native_saved_level_table(lua, &objects)?;
            write_saved_lua_table(&destination, Value::Table(level), false)?;
            Ok(())
        })?,
    )?;
    Ok(())
}
