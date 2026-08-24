//! Four particle render passes registered at `0x10002DF48..0x10002DFD8`.

use crate::*;

pub(super) fn install(
    lua: &Lua,
    globals: &mlua::Table,
    render: Arc<Mutex<RenderBridge>>,
) -> LuaResult<()> {
    for (function_name, mode, requires_in_game_particles) in [
        ("native_drawBackgroundParticles", 2, true),
        ("native_drawForegroundParticles", 1, true),
        ("drawMenuParticlesNative", 3, false),
        ("native_drawNotificationParticles", 4, false),
    ] {
        let draw_bridge = Arc::clone(&render);
        globals.set(
            function_name,
            lua.create_function(move |_, _: MultiValue| {
                let mut bridge = draw_bridge.lock().expect("render bridge lock poisoned");
                // GameLua+0x199 gates only the two in-game passes.
                if !requires_in_game_particles || bridge.particles_enabled {
                    bridge.draw_particles(mode);
                }
                Ok(())
            })?,
        )?;
    }
    Ok(())
}
