//! Late runtime, renderer-recovery, theme and platform no-op members.

use crate::*;

pub(super) fn install_sensor_force(
    lua: &Lua,
    globals: &mlua::Table,
    render: Arc<Mutex<RenderBridge>>,
) -> LuaResult<()> {
    globals.set(
        "native_applySensorForces",
        lua.create_function(move |_, args: MultiValue| {
            // Hand-written member sub_10005B570 calls the exact STRING-tag
            // accessor sub_1005285CC for slots one and two. Numeric values
            // must not receive mlua's normal string coercion; later slots are
            // left untouched and therefore ignored.
            let sensor_name = native_required_string(&args, 0, "native_applySensorForces")?;
            let object_name = native_required_string(&args, 1, "native_applySensorForces")?;
            let mut bridge = render.lock().expect("render bridge lock poisoned");
            apply_native_sensor_forces(&mut bridge, &sensor_name, &object_name);
            Ok(())
        })?,
    )?;
    Ok(())
}

pub(super) fn install_level_limits(
    lua: &Lua,
    globals: &mlua::Table,
    render: Arc<Mutex<RenderBridge>>,
) -> LuaResult<()> {
    globals.set(
        "native_getLevelLimits",
        lua.create_function(move |_, _: MultiValue| {
            let limits = render
                .lock()
                .expect("render bridge lock poisoned")
                .level_limits;
            Ok((limits[0], limits[1], limits[2], limits[3]))
        })?,
    )?;
    Ok(())
}

pub(super) fn install_rendering_state(
    lua: &Lua,
    globals: &mlua::Table,
    render: Arc<Mutex<RenderBridge>>,
) -> LuaResult<()> {
    let rendering_state_bridge = Arc::clone(&render);
    globals.set(
        "setGameRenderingDisabled",
        lua.create_function(move |_, args: MultiValue| {
            // Direct member sub_100059D58 reads stack slot -1 through the
            // strict Boolean accessor. With extra arguments the topmost value
            // wins; an empty or non-Boolean stack is an error.
            let index = args
                .len()
                .checked_sub(1)
                .ok_or_else(|| runtime_error("setGameRenderingDisabled expects boolean"))?;
            let disabled = native_required_boolean(&args, index, "setGameRenderingDisabled")?;
            rendering_state_bridge
                .lock()
                .expect("render bridge lock poisoned")
                .game_rendering_disabled = disabled;
            Ok(())
        })?,
    )?;
    globals.set(
        "isGameRenderingDisabled",
        lua.create_function(move |_, _: MultiValue| {
            Ok(render
                .lock()
                .expect("render bridge lock poisoned")
                .game_rendering_disabled)
        })?,
    )?;
    Ok(())
}

pub(super) fn install_recovery(lua: &Lua, globals: &mlua::Table) -> LuaResult<()> {
    globals.set(
        "recoverRenderObjects",
        lua.create_function(|_, _: MultiValue| {
            // sub_100059DB0 reacquires native sprite, texture and sheet
            // pointers after an OpenGL context/resource recovery. Rehost
            // commands retain resource names and wgpu resolves those names
            // through its live atlas cache at submission time, so no stale
            // raw pointer exists to repair. Keep the native zero-result ABI.
            Ok(())
        })?,
    )?;
    Ok(())
}

pub(super) fn install_theme_reset(
    lua: &Lua,
    globals: &mlua::Table,
    render: Arc<Mutex<RenderBridge>>,
) -> LuaResult<()> {
    globals.set(
        "native_resetThemeSystem",
        lua.create_function(move |_, _: MultiValue| {
            // sub_1000984C0 only clears ThemeSystem+0x30 and the two cached
            // floats at +0x54/+0x58. It does not destroy layer arrays, theme
            // sprites, particle maps, or authored offsets.
            let mut bridge = render.lock().expect("render bridge lock poisoned");
            bridge.theme_camera.valid = false;
            bridge.theme_camera.effect_x = 0.0;
            bridge.theme_camera.effect_y = 0.0;
            Ok(())
        })?,
    )?;
    Ok(())
}

pub(super) fn install_menu_particle_scale(
    lua: &Lua,
    globals: &mlua::Table,
    render: Arc<Mutex<RenderBridge>>,
) -> LuaResult<()> {
    globals.set(
        "setMenuParticlesScale",
        lua.create_function(move |_, args: MultiValue| {
            // Generated adapter sub_100088D24 reads stack slot one through
            // sub_10052859C: the value must have the exact Lua NUMBER tag,
            // extra slots are ignored, and the call narrows to float32 before
            // the Particles virtual setter stores it at +0x38.
            let scale = native_required_number(&args, 0, "setMenuParticlesScale")? as f32;
            render
                .lock()
                .expect("render bridge lock poisoned")
                .particle_system
                .scale = scale;
            Ok(())
        })?,
    )?;
    Ok(())
}

pub(super) fn install_register_key(lua: &Lua, globals: &mlua::Table) -> LuaResult<()> {
    globals.set(
        "registerKey",
        lua.create_function(|_, args: MultiValue| {
            // Generated adapter sub_100089188 reads exactly three strings
            // before dispatching to sub_100031104. The member stores the
            // little-endian halfword 0x0100 at GameLua+0x98, restoring the
            // same supported/pending pair exposed by checkRegistrationResult.
            // The offline host has no other registration transition, so the
            // observable state is unchanged while the strict Lua ABI remains.
            native_required_string(&args, 0, "registerKey")?;
            native_required_string(&args, 1, "registerKey")?;
            native_required_string(&args, 2, "registerKey")?;
            Ok(())
        })?,
    )?;
    Ok(())
}
