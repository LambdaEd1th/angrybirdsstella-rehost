//! `isCompoSprite` (`sub_10004E3A0`).

use crate::*;

pub(super) fn install(
    lua: &Lua,
    globals: &mlua::Table,
    resource_runtime: Arc<Mutex<ResourceRuntime>>,
) -> LuaResult<()> {
    globals.set(
        "isCompoSprite",
        lua.create_function(move |_, args: MultiValue| {
            let name = args.iter().filter_map(value_string).next_back();
            Ok(name.is_some_and(|name| {
                resource_runtime
                    .lock()
                    .expect("resource runtime lock poisoned")
                    .active_composite_parts(&name)
                    .is_some()
            }))
        })?,
    )
}
