//! Current-font selection member published at `0x1004468B4`.

use crate::*;

pub(crate) fn install(
    lua: &Lua,
    resource_api: &mlua::Table,
    resource_runtime: Arc<Mutex<ResourceRuntime>>,
) -> LuaResult<()> {
    resource_api.set(
        "useFont",
        lua.create_function(move |_, args: MultiValue| {
            let name = native_required_string(&args, 0, "useFont")?;
            let mut resources = resource_runtime
                .lock()
                .expect("resource runtime lock poisoned");
            if resources.bitmap_fonts.contains(&name) || resources.system_fonts.contains_key(&name)
            {
                resources.current_font = Some(name);
            }
            Ok(())
        })?,
    )
}
