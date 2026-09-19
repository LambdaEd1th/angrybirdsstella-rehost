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
    globals.set("cursor", cursor.clone())?;
    retain_native_lua_object(lua, NativeLuaObject::Cursor, Some(&cursor))?;
    // `touches` and `touchcount` are constructed and replaced by the native
    // frame member immediately before each Lua update, not by GameLua's
    // constructor.
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
        let table =
            super::persistence::load_persistent_lua_table(lua, &path, &format!("{name}.lua"))?;
        globals.set(name, table)?;
    }
    // Pointer/button event names are string constants owned by the shipped
    // bytecode. GameLua publishes the three input-state tables above, but it
    // does not mirror those event names into standalone Lua globals.
    // Purple's native constructors do not publish script clock globals. The
    // shipped gamelogic update owns `time` and `playtimeCounter`; the frame
    // delta values remain callback parameters rather than host-created
    // `g_time`, `deltaTime`, or `currentTimeStep` fields.
    let gamelua = lua.create_table()?;
    install_global_fallback(lua, &gamelua)?;
    // lua::LuaObject's constructor (0x100527710) stores _G as an actual
    // field of the independent owner table, not just an inherited lookup.
    gamelua.raw_set("_G", globals.clone())?;
    // These constructor fields belong to GameLua itself. The split Rust
    // registration pipeline has already created them on the engine root;
    // retain the same values on the owner before any script executes. A
    // metatable-only alias is insufficient for ThemeSystem's native raw
    // lookups (0x10009861C and 0x10009AD10 onward).
    for name in ["objects", "particles", "deviceModel"] {
        gamelua.raw_set(name, globals.raw_get::<Value>(name)?)?;
    }
    let ui = lua.create_table()?;
    install_table_fallback(lua, &ui, gamelua.clone())?;
    gamelua.set("ui", ui.clone())?;
    gamelua.set("this", gamelua.clone())?;
    globals.set("gamelua", gamelua.clone())?;
    globals.set("ui", ui)?;
    globals.set("this", gamelua)?;
    globals.set("screenWidth", screen_width)?;
    globals.set("screenHeight", screen_height)?;
    // The common gamelogic bytecode constructs `screen` from the two native
    // dimensions. Purple publishes no provisional native table beforehand.
    globals.set("g_startingResolutionWidth", screen_width)?;
    globals.set("g_startingResolutionHeight", screen_height)?;
    Ok(())
}
