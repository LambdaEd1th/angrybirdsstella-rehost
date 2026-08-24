//! Script-facing conversions sharing the same world/camera transform state.

use crate::*;

pub(super) fn install(
    lua: &Lua,
    globals: &mlua::Table,
    render: Arc<Mutex<RenderBridge>>,
) -> LuaResult<()> {
    let world_to_screen_bridge = Arc::clone(&render);
    globals.set(
        "worldToScreenTransform",
        lua.create_function(move |_, args: MultiValue| {
            let bridge = world_to_screen_bridge
                .lock()
                .expect("render bridge lock poisoned");
            Ok((
                (value_number_at(&args, 0).unwrap_or(0.0) - bridge.top_left_x) * bridge.world_scale,
                (value_number_at(&args, 1).unwrap_or(0.0) - bridge.top_left_y) * bridge.world_scale,
            ))
        })?,
    )?;

    let screen_to_world_bridge = Arc::clone(&render);
    globals.set(
        "screenToWorldTransform",
        lua.create_function(move |_, args: MultiValue| {
            let bridge = screen_to_world_bridge
                .lock()
                .expect("render bridge lock poisoned");
            let scale = bridge.world_scale.max(f64::EPSILON);
            Ok((
                value_number_at(&args, 0).unwrap_or(0.0) / scale + bridge.top_left_x,
                value_number_at(&args, 1).unwrap_or(0.0) / scale + bridge.top_left_y,
            ))
        })?,
    )?;

    globals.set(
        "physicsToWorldTransform",
        lua.create_function(|_, args: MultiValue| {
            Ok((
                value_number_at(&args, 0).unwrap_or(0.0) * 20.0,
                value_number_at(&args, 1).unwrap_or(0.0) * 20.0,
            ))
        })?,
    )?;
    globals.set(
        "worldToPhysicsTransform",
        lua.create_function(|_, args: MultiValue| {
            Ok((
                value_number_at(&args, 0).unwrap_or(0.0) / 20.0,
                value_number_at(&args, 1).unwrap_or(0.0) / 20.0,
            ))
        })?,
    )?;

    let physics_to_screen_bridge = Arc::clone(&render);
    globals.set(
        "physicsToScreenTransform",
        lua.create_function(move |_, args: MultiValue| {
            let bridge = physics_to_screen_bridge
                .lock()
                .expect("render bridge lock poisoned");
            Ok((
                (value_number_at(&args, 0).unwrap_or(0.0) * 20.0 - bridge.top_left_x)
                    * bridge.world_scale,
                (value_number_at(&args, 1).unwrap_or(0.0) * 20.0 - bridge.top_left_y)
                    * bridge.world_scale,
            ))
        })?,
    )?;
    globals.set(
        "screenToPhysicsTransform",
        lua.create_function(move |_, args: MultiValue| {
            let bridge = render.lock().expect("render bridge lock poisoned");
            let scale = bridge.world_scale.max(f64::EPSILON);
            Ok((
                (value_number_at(&args, 0).unwrap_or(0.0) / scale + bridge.top_left_x) / 20.0,
                (value_number_at(&args, 1).unwrap_or(0.0) / scale + bridge.top_left_y) / 20.0,
            ))
        })?,
    )
}
