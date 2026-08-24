//! Generic script/object loading and persistent-data registrations.

use crate::*;

pub(crate) fn install(lua: &Lua, globals: &mlua::Table, data_root: Arc<PathBuf>) -> LuaResult<()> {
    let root = Arc::clone(&data_root);
    globals.set("loadLuaFile", make_script_loader(lua, root)?)?;

    let object_loader_root = Arc::clone(&data_root);
    globals.set(
        "loadLuaFileToObject",
        lua.create_function(move |lua, args: MultiValue| {
            trace_object_loader("loadLuaFileToObject", &args);
            // sub_10005761C is a hand-written LuaState member: the first
            // three slots are strict STRING/TABLE/STRING. Only an exact
            // four-slot call reads the optional resolve-relative boolean.
            let path = native_required_string(&args, 0, "loadLuaFileToObject")?;
            let parent = required_table(&args, 1, "loadLuaFileToObject")?;
            let child_name = native_required_string(&args, 2, "loadLuaFileToObject")?;
            if args.len() == 4 {
                let _resolve_relative = native_required_boolean(&args, 3, "loadLuaFileToObject")?;
            }
            load_script_to_object(
                lua,
                &object_loader_root,
                &path,
                Some(parent),
                Some(&child_name),
            )?;
            Ok(())
        })?,
    )?;

    let app_object_loader_root = Arc::new(app_data_root(&data_root));
    globals.set(
        "loadLuaFileFromAppDataToObject",
        lua.create_function(move |lua, args: MultiValue| {
            trace_object_loader("loadLuaFileFromAppDataToObject", &args);
            // sub_100057E14 has the same strict prefix, then optional
            // resolve/decrypt/unzip booleans with native defaults 0/1/0.
            let function = "loadLuaFileFromAppDataToObject";
            let path = native_required_string(&args, 0, function)?;
            let parent = required_table(&args, 1, function)?;
            let child_name = native_required_string(&args, 2, function)?;
            let _resolve_relative = optional_boolean(&args, 3, false, function)?;
            let decrypt_persistent = optional_boolean(&args, 4, true, function)?;
            let decompress = optional_boolean(&args, 5, false, function)?;
            load_script_to_object_with_options(
                lua,
                &app_object_loader_root,
                &path,
                Some(parent),
                Some(&child_name),
                decrypt_persistent,
                decompress,
            )?;
            Ok(())
        })?,
    )?;

    game_lua::install_data_imports(lua, globals, &data_root)?;
    game_lua::install_persistent_save(lua, globals, &data_root)?;
    Ok(())
}

fn required_table(args: &MultiValue, index: usize, function: &str) -> LuaResult<mlua::Table> {
    value_table_at(args, index).ok_or_else(|| {
        runtime_error(format!(
            "bad argument #{} to '{function}' (table expected)",
            index + 1
        ))
    })
}

fn value_table_at(args: &MultiValue, index: usize) -> Option<mlua::Table> {
    args.iter().nth(index).and_then(value_table)
}

fn optional_boolean(
    args: &MultiValue,
    index: usize,
    default: bool,
    function: &str,
) -> LuaResult<bool> {
    match args.iter().nth(index) {
        None => Ok(default),
        Some(Value::Boolean(value)) => Ok(*value),
        Some(_) => Err(runtime_error(format!(
            "bad argument #{} to '{function}' (boolean expected)",
            index + 1
        ))),
    }
}
