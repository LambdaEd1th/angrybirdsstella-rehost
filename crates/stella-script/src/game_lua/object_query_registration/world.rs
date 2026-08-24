//! GameLua-wide physics-lock query (`sub_1000421F8`).

use crate::*;

pub(super) fn install(
    lua: &Lua,
    globals: &mlua::Table,
    render: &Arc<Mutex<RenderBridge>>,
) -> LuaResult<()> {
    let physics_enabled_bridge = Arc::clone(render);
    globals.set(
        "isPhysicsEnabled",
        lua.create_function(move |_, _: MultiValue| {
            Ok(physics_enabled_bridge
                .lock()
                .expect("render bridge lock poisoned")
                .physics_enabled)
        })?,
    )
}
