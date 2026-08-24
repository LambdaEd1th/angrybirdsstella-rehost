//! `GameLua::setPhysicsScale` registration and native member facade.

mod arguments;
mod fixture_rebuild;
mod member;

use crate::*;

pub(crate) fn install(
    lua: &Lua,
    globals: &mlua::Table,
    render: Arc<Mutex<RenderBridge>>,
) -> LuaResult<()> {
    globals.set(
        "setPhysicsScale",
        lua.create_function(move |lua, args: MultiValue| {
            // Constructor slot 0x10002D598 uses strict STRING/FLOAT/FLOAT
            // adapter sub_10008897C before entering sub_10004050C.
            let name = native_required_string(&args, 0, "setPhysicsScale")?;
            let scale_x = f64::from(native_required_number(&args, 1, "setPhysicsScale")? as f32);
            let scale_y = f64::from(native_required_number(&args, 2, "setPhysicsScale")? as f32);
            member::apply(lua, &render, &name, scale_x, scale_y)
        })?,
    )
}
