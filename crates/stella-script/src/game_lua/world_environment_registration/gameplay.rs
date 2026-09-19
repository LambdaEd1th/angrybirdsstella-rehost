//! Game lifecycle and parameter-table members.

use crate::*;

pub(super) fn install_request_exit(
    lua: &Lua,
    globals: &mlua::Table,
    render: Arc<Mutex<RenderBridge>>,
) -> LuaResult<()> {
    globals.set(
        "requestExit",
        lua.create_function(move |_, _: MultiValue| {
            render
                .lock()
                .expect("render bridge lock poisoned")
                .exit_requested = true;
            Ok(())
        })?,
    )
}

pub(super) fn install_game_on(
    lua: &Lua,
    globals: &mlua::Table,
    render: Arc<Mutex<RenderBridge>>,
) -> LuaResult<()> {
    globals.set(
        "setGameOn",
        lua.create_function(move |_, args: MultiValue| {
            let enabled = native_required_boolean(&args, 0, "setGameOn")?;
            if std::env::var_os("STELLA_TRACE_NATIVE").is_some() {
                eprintln!("native setGameOn({enabled})");
            }
            render.lock().expect("render bridge lock poisoned").game_on = enabled;
            // sub_1000504A8 forwards `enabled ^ 1` to world pause.
            Ok(())
        })?,
    )
}

pub(super) fn install_parameters(
    lua: &Lua,
    globals: &mlua::Table,
    render: Arc<Mutex<RenderBridge>>,
) -> LuaResult<()> {
    globals.set(
        "setGameParameters",
        lua.create_function(move |_, args: MultiValue| {
            // sub_100055438 probes Lua stack index -1.
            let Some(Value::Table(parameters)) = args.iter().last() else {
                return Ok(());
            };
            let mut bridge = render.lock().expect("render bridge lock poisoned");
            // 0x1000554A0/4F8/550/5A8 use lua_rawget. Only an actual
            // BOOLEAN changes +0x190; truthy numbers and __index values do
            // not select deterministic physics. Scale uses lua_isnumber /
            // lua_tonumber instead, so numeric strings remain valid.
            if let Value::Boolean(enabled) = parameters.raw_get("deterministicPhysics")? {
                bridge.deterministic_physics = enabled;
            }
            bridge.game_world_scale =
                native_lua51_number(&parameters.raw_get::<Value>("gameWorldScale")?)
                    .map(|value| f64::from(value as f32))
                    .unwrap_or(1.0);
            Ok(())
        })?,
    )
}
