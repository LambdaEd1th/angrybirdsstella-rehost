//! Raw simulation-vector accessors at GameLua `+0x590`.

use crate::*;

pub(in crate::game_lua::trajectory_registration) fn install_clear(
    lua: &Lua,
    globals: &mlua::Table,
    render: Arc<Mutex<RenderBridge>>,
) -> LuaResult<()> {
    globals.set(
        "ClearSimulationTrajectory",
        lua.create_function(move |_, _: MultiValue| {
            let mut bridge = render.lock().expect("render bridge lock poisoned");
            // sub_1000311B4 only assigns begin to end. It does not reset
            // either flight-trail record or the prepared AimStream.
            bridge.trajectory_points.clear();
            Ok(())
        })?,
    )
}

pub(in crate::game_lua::trajectory_registration) fn install_get(
    lua: &Lua,
    globals: &mlua::Table,
    render: Arc<Mutex<RenderBridge>>,
) -> LuaResult<()> {
    globals.set(
        "getSimulationTrajectoryPoints",
        lua.create_function(move |lua, _: MultiValue| {
            let bridge = render.lock().expect("render bridge lock poisoned");
            if bridge.trajectory_points.is_empty() {
                return Ok(Value::Nil);
            }
            let result = lua.create_table()?;
            for (index, (x, y)) in bridge.trajectory_points.iter().copied().enumerate() {
                let point = lua.create_table()?;
                point.set("x", x)?;
                point.set("y", y)?;
                result.raw_set(index + 1, point)?;
            }
            Ok(Value::Table(result))
        })?,
    )
}
