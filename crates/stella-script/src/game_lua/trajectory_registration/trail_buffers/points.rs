//! Flight-trail record creation and point/puff insertion.

use crate::*;

pub(in crate::game_lua::trajectory_registration) fn install_points(
    lua: &Lua,
    globals: &mlua::Table,
    render: Arc<Mutex<RenderBridge>>,
) -> LuaResult<()> {
    let start_bridge = Arc::clone(&render);
    globals.set(
        "startNewTrajectory",
        lua.create_function(move |_, _: MultiValue| {
            let mut bridge = start_bridge.lock().expect("render bridge lock poisoned");
            // sub_10004FD3C advances the signed index modulo two, then
            // assigns a default 0x38-byte record to that slot.
            bridge.trajectory_stream_index = (bridge.trajectory_stream_index + 1) % 2;
            let index = bridge.trajectory_stream_index;
            bridge.trajectory_streams[index] = NativeTrajectoryBuffer::default();
            Ok(())
        })?,
    )?;
    install_point(lua, globals, Arc::clone(&render), "addToTrajectory", false)?;
    install_point(lua, globals, render, "addPuffToTrajectory", true)
}

fn install_point(
    lua: &Lua,
    globals: &mlua::Table,
    render: Arc<Mutex<RenderBridge>>,
    function_name: &'static str,
    is_puff: bool,
) -> LuaResult<()> {
    globals.set(
        function_name,
        lua.create_function(move |_, args: MultiValue| {
            // Generated adapter sub_100089A44 validates three fixed number
            // slots. The native members ignore s0 and narrow s1/s2 to f32.
            for index in 0..3 {
                if value_number_at(&args, index).is_none() {
                    return Err(LuaError::RuntimeError(format!(
                        "bad argument #{} to '{function_name}' (number expected)",
                        index + 1
                    )));
                }
            }
            let x = value_number_at(&args, 1).expect("validated number") as f32;
            let y = value_number_at(&args, 2).expect("validated number") as f32;
            let point = (f64::from(x), f64::from(y));
            let mut bridge = render.lock().expect("render bridge lock poisoned");
            let index = bridge.trajectory_stream_index;
            if is_puff {
                bridge.trajectory_streams[index].puff = Some(point);
            } else {
                bridge.trajectory_streams[index].points.push(point);
            }
            Ok(())
        })?,
    )
}
