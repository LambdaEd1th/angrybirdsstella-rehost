//! Nullable-body velocity and awake-state queries.

use crate::*;

pub(super) fn install(
    lua: &Lua,
    globals: &mlua::Table,
    render: &Arc<Mutex<RenderBridge>>,
) -> LuaResult<()> {
    let angular_velocity_bridge = Arc::clone(render);
    globals.set(
        "getAngularVelocity",
        lua.create_function(move |_, args: MultiValue| {
            let name = native_required_string(&args, 0, "getAngularVelocity")?;
            let bridge = angular_velocity_bridge
                .lock()
                .expect("render bridge lock poisoned");
            Ok(bridge.scene.get(&name).map_or(0.0, |object| {
                if object.has_physics_body() {
                    f64::from(object.angular_velocity as f32)
                } else {
                    0.0
                }
            }))
        })?,
    )?;

    let velocity_bridge = Arc::clone(render);
    globals.set(
        "getVelocity",
        lua.create_function(move |_, args: MultiValue| {
            let name = native_required_string(&args, 0, "getVelocity")?;
            let bridge = velocity_bridge.lock().expect("render bridge lock poisoned");
            Ok(bridge.scene.get(&name).map_or(0.0, |object| {
                if !object.has_physics_body() {
                    return 0.0;
                }
                // sub_1000410D8 rounds y*y before one x*x+y² FMA and FSQRT.
                let velocity_x = object.velocity_x as f32;
                let velocity_y = object.velocity_y as f32;
                f64::from(
                    velocity_x
                        .mul_add(velocity_x, velocity_y * velocity_y)
                        .sqrt(),
                )
            }))
        })?,
    )?;

    let linear_velocity_bridge = Arc::clone(render);
    globals.set(
        "getLinearVelocity",
        lua.create_function(move |_, args: MultiValue| {
            let name = native_required_string(&args, 0, "getLinearVelocity")?;
            let bridge = linear_velocity_bridge
                .lock()
                .expect("render bridge lock poisoned");
            Ok(bridge.scene.get(&name).map_or((0.0, 0.0), |object| {
                if object.has_physics_body() {
                    (
                        f64::from(object.velocity_x as f32),
                        f64::from(object.velocity_y as f32),
                    )
                } else {
                    (0.0, 0.0)
                }
            }))
        })?,
    )?;

    let sleeping_bridge = Arc::clone(render);
    globals.set(
        "isSleeping",
        lua.create_function(move |_, args: MultiValue| {
            let name = native_required_string(&args, 0, "isSleeping")?;
            let bridge = sleeping_bridge.lock().expect("render bridge lock poisoned");
            // sub_10004DB60 returns true for null body; otherwise !awake.
            Ok(bridge
                .scene
                .get(&name)
                .is_none_or(|object| !object.has_physics_body() || object.sleeping))
        })?,
    )?;

    Ok(())
}
