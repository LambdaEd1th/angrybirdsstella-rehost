//! Missing-method audit metatable and global table publication.

use std::{
    collections::BTreeSet,
    sync::{Arc, Mutex},
};

use mlua::{Lua, Result as LuaResult, Table, Value};

pub(super) fn publish(
    lua: &Lua,
    globals: &Table,
    animation_native: Table,
    missing: Arc<Mutex<BTreeSet<String>>>,
) -> LuaResult<()> {
    let animation_metatable = lua.create_table()?;
    let missing_animation_methods = Arc::clone(&missing);
    animation_metatable.set(
        "__index",
        lua.create_function(move |_, (_table, key): (mlua::Table, String)| {
            missing_animation_methods
                .lock()
                .expect("missing-global lock poisoned")
                .insert(format!("AnimationWrapperNative.{key}"));
            Ok(Value::Nil)
        })?,
    )?;
    animation_native.set_metatable(Some(animation_metatable))?;
    globals.set("AnimationWrapperNative", animation_native)?;
    Ok(())
}
