//! Core Lua tables, input state, clocks, and screen constants.

use crate::*;

pub(crate) fn install(
    lua: &Lua,
    globals: &mlua::Table,
    screen_width: u32,
    screen_height: u32,
    data_root: &Path,
) -> LuaResult<()> {
    let cursor = lua.create_table()?;
    cursor.set("x", 0.0)?;
    cursor.set("y", 0.0)?;
    cursor.set("down", false)?;
    globals.set("cursor", cursor.clone())?;
    retain_native_lua_object(lua, NativeLuaObject::Cursor, Some(&cursor))?;
    globals.set("touches", lua.create_table()?)?;
    globals.set("touchcount", 0)?;
    for (name, object) in [
        ("keyPressed", NativeLuaObject::KeyPressed),
        ("keyReleased", NativeLuaObject::KeyReleased),
        ("keyHold", NativeLuaObject::KeyHold),
    ] {
        let table = lua.create_table()?;
        globals.set(name, table.clone())?;
        retain_native_lua_object(lua, object, Some(&table))?;
    }
    let multitouch_sweep = lua.create_table()?;
    globals.set("multitouchSweep", multitouch_sweep.clone())?;
    retain_native_lua_object(
        lua,
        NativeLuaObject::MultitouchSweep,
        Some(&multitouch_sweep),
    )?;
    let multitouch_zoom = lua.create_table()?;
    multitouch_zoom.set("zoomCoolingTime", -1.0_f64)?;
    globals.set("multitouchZoom", multitouch_zoom.clone())?;
    retain_native_lua_object(lua, NativeLuaObject::MultitouchZoom, Some(&multitouch_zoom))?;
    // GameLua owns this table for its whole lifetime (`this + 0x430`).
    // `clipText` mutates the two result fields instead of replacing the table.
    let clipped_text = lua.create_table()?;
    globals.set("clippedText", clipped_text.clone())?;
    // GameLua constructs this LuaObject directly at +0x430 before loading
    // gamelogic. Native clipText keeps that identity for the host lifetime.
    retain_native_lua_object(lua, NativeLuaObject::ClippedText, Some(&clipped_text))?;
    for name in ["highscores", "settings", "bi_data"] {
        let path = app_data_path(data_root, &format!("{name}.lua")).map_err(runtime_error)?;
        let table = if path.is_file() {
            load_saved_lua_table(lua, &path)?
        } else {
            lua.create_table()?
        };
        globals.set(name, table)?;
    }
    for input_constant in [
        "LBUTTON", "RBUTTON", "LPRESS", "LHOLD", "LRELEASE", "RPRESS", "RHOLD", "RRELEASE",
        "HOVER", "PRESS", "RELEASE", "WHEEL",
    ] {
        globals.set(input_constant, input_constant)?;
    }
    globals.set("time", 0.0)?;
    globals.set("g_time", 0.0)?;
    globals.set("deltaTime", 0.0)?;
    globals.set("currentTimeStep", 0.0)?;
    globals.set("playtimeCounter", 0.0)?;
    let gamelua = lua.create_table()?;
    install_global_fallback(lua, &gamelua)?;
    let ui = lua.create_table()?;
    install_table_fallback(lua, &ui, gamelua.clone())?;
    gamelua.set("ui", ui.clone())?;
    gamelua.set("this", gamelua.clone())?;
    globals.set("gamelua", gamelua.clone())?;
    globals.set("ui", ui)?;
    globals.set("this", gamelua)?;
    globals.set("screenWidth", screen_width)?;
    globals.set("screenHeight", screen_height)?;
    let screen = lua.create_table()?;
    screen.set("left", 0.0_f64)?;
    screen.set("top", 0.0_f64)?;
    screen.set("right", f64::from(screen_width))?;
    screen.set("bottom", f64::from(screen_height))?;
    screen.set("width", f64::from(screen_width))?;
    screen.set("height", f64::from(screen_height))?;
    globals.set("screen", screen)?;
    globals.set("g_startingResolutionWidth", screen_width)?;
    globals.set("g_startingResolutionHeight", screen_height)?;
    Ok(())
}
