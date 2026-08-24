//! Discontinued cloud-version service boundary.

use crate::*;

pub(super) fn install(
    lua: &Lua,
    globals: &mlua::Table,
    render: Arc<Mutex<RenderBridge>>,
) -> LuaResult<()> {
    let force_update = lua.create_table()?;
    force_update.set(
        "native_checkForcedUpdate",
        lua.create_function(|_, args: MultiValue| {
            // Adapter sub_1000268E8 reads a required configuration string and
            // a required Lua callback before sub_1000256B4 performs the
            // version checks. It always returns zero Lua results. The direct
            // member only invokes the forced-update continuation when the
            // downloaded cloud configuration requires a newer build; there is
            // no remote configuration in the offline rehost.
            native_required_string(&args, 0, "native_checkForcedUpdate")?;
            if !matches!(args.iter().nth(1), Some(Value::Function(_))) {
                return Err(runtime_error(
                    "bad argument #2 to 'native_checkForcedUpdate' (function expected)".to_owned(),
                ));
            }
            Ok(())
        })?,
    )?;
    force_update.set(
        "native_launchAppStore",
        lua.create_function(move |_, (): ()| {
            // Zero-argument adapter sub_100026808 dispatches to
            // sub_100026238, which opens the iOS App Store product whose
            // literal identifier is 875251011. Retain the request rather than
            // attempting a platform side effect during deterministic runs.
            render
                .lock()
                .expect("render bridge lock poisoned")
                .requested_app_store_product = Some(("875251011".to_owned(), 3));
            Ok(())
        })?,
    )?;
    globals.set("ForceUpdate", force_update)?;
    Ok(())
}
