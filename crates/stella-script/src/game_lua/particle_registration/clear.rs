//! Clear-by-stream and in-game enable members at `0x10002E038..0x10002E098`.

use crate::*;

pub(super) fn install(
    lua: &Lua,
    globals: &mlua::Table,
    render: Arc<Mutex<RenderBridge>>,
) -> LuaResult<()> {
    for function_name in ["clearParticlesNative", "clearParticlesWithTagNative"] {
        let clear_bridge = Arc::clone(&render);
        globals.set(
            function_name,
            lua.create_function(move |_, args: MultiValue| {
                let tag = if function_name == "clearParticlesWithTagNative" {
                    native_required_string(&args, 0, function_name)?
                } else {
                    "ALL".to_owned()
                };
                let mut bridge = clear_bridge.lock().expect("render bridge lock poisoned");
                match tag.as_str() {
                    "INGAME_BACKGROUND" => bridge
                        .particle_system
                        .particles
                        .retain(|particle| particle.mode != 2),
                    "INGAME_FOREGROUND" => bridge
                        .particle_system
                        .particles
                        .retain(|particle| particle.mode != 1),
                    "MENU" => bridge
                        .particle_system
                        .particles
                        .retain(|particle| particle.mode != 3),
                    "ALL" => bridge.particle_system.particles.clear(),
                    _ => {}
                }
                Ok(())
            })?,
        )?;
    }

    globals.set(
        "enableInGameParticlesNative",
        lua.create_function(move |_, args: MultiValue| {
            let enabled = native_required_boolean(&args, 0, "enableInGameParticlesNative")?;
            render
                .lock()
                .expect("render bridge lock poisoned")
                .particles_enabled = enabled;
            Ok(())
        })?,
    )
}
