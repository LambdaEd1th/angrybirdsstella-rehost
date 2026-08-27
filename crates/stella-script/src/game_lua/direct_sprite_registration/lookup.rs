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
            // Generated adapter sub_100088FD0 reads exact STRING slot one
            // and ignores every trailing stack value.
            let name = native_required_borrowed_string(&args, 0, "isCompoSprite")?;
            Ok(resource_runtime
                .lock()
                .expect("resource runtime lock poisoned")
                .active_composite_parts(&name)
                .is_some())
        })?,
    )
}
