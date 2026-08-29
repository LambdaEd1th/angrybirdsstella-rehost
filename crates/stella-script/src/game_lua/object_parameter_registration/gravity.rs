//! Gravity mask/category stores registered at `0x10002CC80..0x10002CCB0`.

use crate::*;

pub(super) fn install(
    lua: &Lua,
    globals: &mlua::Table,
    render: Arc<Mutex<RenderBridge>>,
) -> LuaResult<()> {
    let mask_bridge = Arc::clone(&render);
    globals.set(
        "setSensorGravityMask",
        lua.create_function(move |_, args: MultiValue| {
            let name = native_required_string(&args, 0, "setSensorGravityMask")?;
            let mask =
                native_fcvtzs_f32(native_required_number(&args, 1, "setSensorGravityMask")? as f32);
            let mut bridge = mask_bridge.lock().expect("render bridge lock poisoned");
            let Some(object) = bridge.game_lua_object_mut(&name) else {
                return Err(runtime_error(format!("Missing object: {name}")));
            };
            // sub_1000313B4 stores at RenderObjectData+0x104.
            object.sensor_gravity_mask = mask;
            Ok(())
        })?,
    )?;

    globals.set(
        "setObjectGravityCategory",
        lua.create_function(move |_, args: MultiValue| {
            let name = native_required_string(&args, 0, "setObjectGravityCategory")?;
            let category =
                native_fcvtzs_f32(
                    native_required_number(&args, 1, "setObjectGravityCategory")? as f32,
                );
            let mut bridge = render.lock().expect("render bridge lock poisoned");
            let Some(object) = bridge.game_lua_object_mut(&name) else {
                return Err(runtime_error(format!("Missing object: {name}")));
            };
            // sub_1000313D8 stores at RenderObjectData+0x124.
            object.gravity_category = category;
            Ok(())
        })?,
    )
}
