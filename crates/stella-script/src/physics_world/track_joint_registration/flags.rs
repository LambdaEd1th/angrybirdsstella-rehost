//! Strict generated adapters for native-only scene-object flag bytes.

use crate::*;

pub(super) fn install(
    lua: &Lua,
    globals: &mlua::Table,
    render: Arc<Mutex<RenderBridge>>,
) -> LuaResult<()> {
    for function_name in [
        "native_setBlockCollisionEnabled",
        "native_setIgnoresScore",
        "native_setKeepOrientation",
        "setRecordVelocity",
        "setRevertGravity",
    ] {
        let flag_bridge = Arc::clone(&render);
        globals.set(
            function_name,
            lua.create_function(move |_, args: MultiValue| {
                // All five registrations use generated adapter sub_10008598C.
                let name = native_required_string(&args, 0, function_name)?;
                let enabled = native_required_boolean(&args, 1, function_name)?;
                if let Some(object) = flag_bridge
                    .lock()
                    .expect("render bridge lock poisoned")
                    .game_lua_object_mut(&name)
                {
                    match function_name {
                        "native_setBlockCollisionEnabled" => {
                            object.block_collision_enabled = enabled;
                        }
                        "native_setIgnoresScore" => object.ignores_score = enabled,
                        "native_setKeepOrientation" => object.keep_orientation = enabled,
                        "setRecordVelocity" => object.record_velocity = enabled,
                        "setRevertGravity" => object.revert_gravity = enabled,
                        _ => unreachable!("registered native object flag"),
                    }
                }
                // The 36-byte native members only resolve RenderObjectData and
                // write one byte. They do not mirror objects.world fields.
                Ok(())
            })?,
        )?;
    }
    Ok(())
}
