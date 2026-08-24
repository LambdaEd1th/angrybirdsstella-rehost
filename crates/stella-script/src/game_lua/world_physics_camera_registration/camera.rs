//! Camera/world-limit members recovered from GameLua's constructor.

use crate::*;

mod level_limits;

pub(super) fn install_origin_and_max_scale(
    lua: &Lua,
    globals: &mlua::Table,
    render: Arc<Mutex<RenderBridge>>,
) -> LuaResult<()> {
    let top_left_bridge = Arc::clone(&render);
    globals.set(
        "setTopLeft",
        lua.create_function(move |_, args: MultiValue| {
            let top_left_x = f64::from(native_required_number(&args, 0, "setTopLeft")? as f32);
            let top_left_y = f64::from(native_required_number(&args, 1, "setTopLeft")? as f32);
            let mut bridge = top_left_bridge.lock().expect("render bridge lock poisoned");
            bridge.top_left_x = top_left_x;
            bridge.top_left_y = top_left_y;
            if std::env::var_os("STELLA_TRACE_NATIVE").is_some() {
                eprintln!(
                    "native setTopLeft({}, {})",
                    bridge.top_left_x, bridge.top_left_y
                );
            }
            Ok(())
        })?,
    )?;

    globals.set(
        "setMaxWorldScale",
        lua.create_function(move |_, args: MultiValue| {
            let scale = f64::from(native_required_number(&args, 0, "setMaxWorldScale")? as f32);
            render
                .lock()
                .expect("render bridge lock poisoned")
                .max_world_scale = scale;
            Ok(())
        })?,
    )
}

pub(super) fn install_level_limits(
    lua: &Lua,
    globals: &mlua::Table,
    render: Arc<Mutex<RenderBridge>>,
) -> LuaResult<()> {
    globals.set(
        "setLevelLimits",
        lua.create_function(move |lua, args: MultiValue| {
            let environment = game_environment(lua)?;
            level_limits::apply(&args, &environment, &render)
        })?,
    )
}

pub(super) fn install_starting_and_limits(
    lua: &Lua,
    globals: &mlua::Table,
    render: Arc<Mutex<RenderBridge>>,
) -> LuaResult<()> {
    let starting_bridge = Arc::clone(&render);
    globals.set(
        "setStartingCameraValue",
        lua.create_function(move |_, args: MultiValue| {
            let value = native_required_boolean(&args, 0, "setStartingCameraValue")?;
            starting_bridge
                .lock()
                .expect("render bridge lock poisoned")
                .starting_camera_value = value;
            Ok(())
        })?,
    )?;

    globals.set(
        "setCameraLimits",
        lua.create_function(move |_, args: MultiValue| {
            let limit = f64::from(native_required_number(&args, 0, "setCameraLimits")? as f32);
            render
                .lock()
                .expect("render bridge lock poisoned")
                .camera_limit = limit;
            Ok(())
        })?,
    )
}
