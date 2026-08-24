//! Adjacent sprite-name adapters at `0x10002EDAC..0x10002EE14`.

use crate::*;

pub(in crate::game_lua::trajectory_registration) fn install_sprites(
    lua: &Lua,
    globals: &mlua::Table,
    render: Arc<Mutex<RenderBridge>>,
) -> LuaResult<()> {
    // sub_10002C274 registers normal, special, then AimStream sprite.
    for (function_name, sprite_slot) in [
        ("setNormalTrailSprite", 1_u8),
        ("setSpecialTrailSprite", 2_u8),
        ("setAimingAidSprite", 0_u8),
    ] {
        let sprite_bridge = Arc::clone(&render);
        globals.set(
            function_name,
            lua.create_function(move |_, args: MultiValue| {
                let sprite = args.iter().next().and_then(value_string).ok_or_else(|| {
                    LuaError::RuntimeError(format!(
                        "bad argument #1 to '{function_name}' (string expected)"
                    ))
                })?;
                let mut bridge = sprite_bridge.lock().expect("render bridge lock poisoned");
                match sprite_slot {
                    0 => bridge.aiming_aid_sprite = sprite,
                    1 => {
                        let index = bridge.trajectory_stream_index;
                        bridge.trajectory_streams[index].normal_sprite = sprite;
                    }
                    2 => {
                        let index = bridge.trajectory_stream_index;
                        bridge.trajectory_streams[index].special_sprite = sprite;
                    }
                    _ => unreachable!(),
                }
                Ok(())
            })?,
        )?;
    }
    Ok(())
}
