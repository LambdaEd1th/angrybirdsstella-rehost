//! AimStream population, time query, and active-flag control.

use crate::*;

pub(in crate::game_lua::trajectory_registration) fn install_populate(
    lua: &Lua,
    globals: &mlua::Table,
    render: Arc<Mutex<RenderBridge>>,
) -> LuaResult<()> {
    globals.set(
        "populateAimingAid",
        lua.create_function(move |lua, _: MultiValue| {
            let (spawn_time, speed) = native_aim_stream_settings(lua)?;
            render
                .lock()
                .expect("render bridge lock poisoned")
                .populate_native_aim_stream(spawn_time, speed);
            Ok(())
        })?,
    )
}

pub(in crate::game_lua::trajectory_registration) fn install_time(
    lua: &Lua,
    globals: &mlua::Table,
) -> LuaResult<()> {
    globals.set(
        "getAimingTime",
        lua.create_function(|lua, _: MultiValue| {
            // sub_10004B8EC excludes the trajectory time-step multiplier.
            let (current_time_step, iterations, _, _) = native_trajectory_settings(lua)?;
            Ok(f64::from(current_time_step * iterations as f32))
        })?,
    )
}

pub(in crate::game_lua::trajectory_registration) fn install_clear(
    lua: &Lua,
    globals: &mlua::Table,
    render: Arc<Mutex<RenderBridge>>,
) -> LuaResult<()> {
    globals.set(
        "clearAimingAid",
        lua.create_function(move |_, args: MultiValue| {
            let amount = value_number_at(&args, 0).ok_or_else(|| {
                LuaError::RuntimeError(
                    "bad argument #1 to 'clearAimingAid' (number expected)".to_owned(),
                )
            })? as f32;
            let mut bridge = render.lock().expect("render bridge lock poisoned");
            // sub_1000082CC performs an upper-bound search on each particle's
            // normalized path parameter, compacts the suffix in place, and
            // discards everything at or behind `amount`. sub_10004BA70 then
            // deactivates the stream without changing its control points.
            let segment_count = bridge.aim_stream_control_points.len() as i32 - 3;
            let segment_count = segment_count as f32;
            let first_retained = bridge
                .aim_stream_particles
                .partition_point(|particle| particle.path_parameter / segment_count <= amount);
            bridge.aim_stream_particles.drain(..first_retained);
            bridge.aim_stream_active = false;
            Ok(())
        })?,
    )
}
