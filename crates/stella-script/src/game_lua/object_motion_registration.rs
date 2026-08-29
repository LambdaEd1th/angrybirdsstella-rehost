//! Box2D body velocity, impulse, and force adapters recovered from GameLua.

use crate::*;

pub(crate) fn install(
    lua: &Lua,
    globals: &mlua::Table,
    render: Arc<Mutex<RenderBridge>>,
) -> LuaResult<()> {
    let velocity_bridge = Arc::clone(&render);
    globals.set(
        "setVelocity",
        lua.create_function(move |_, args: MultiValue| {
            let name = native_required_string(&args, 0, "setVelocity")?;
            let native_velocity_x = native_required_number(&args, 1, "setVelocity")? as f32;
            let native_velocity_y = native_required_number(&args, 2, "setVelocity")? as f32;
            let velocity_x = f64::from(native_velocity_x);
            let velocity_y = f64::from(native_velocity_y);
            if let Some(object) = velocity_bridge
                .lock()
                .expect("render bridge lock poisoned")
                .game_lua_object_mut(&name)
            {
                // sub_100040FA4 ignores static bodies and changes only the
                // native b2Body float fields. Lua velocity/sleeping values
                // remain untouched until the fixed-step write-back.
                if object.dynamic_body || object.kinematic_body {
                    object.velocity_x = velocity_x;
                    object.velocity_y = velocity_y;
                    object
                        .reset_display_interpolation_velocity(native_velocity_x, native_velocity_y);
                    // sub_100040FA4 squares both lanes, reduces with FADDP and
                    // wakes only when the float32 sum is strictly positive.
                    if native_velocity_x * native_velocity_x + native_velocity_y * native_velocity_y
                        > 0.0
                    {
                        object.wake();
                        object.motion_started = true;
                    }
                }
            }
            if std::env::var_os("STELLA_TRACE_NATIVE").is_some() {
                eprintln!("native setVelocity({name:?}, {velocity_x}, {velocity_y})");
            }
            Ok(())
        })?,
    )?;

    let angular_velocity_bridge = Arc::clone(&render);
    globals.set(
        "setAngularVelocity",
        lua.create_function(move |_, args: MultiValue| {
            let name = native_required_string(&args, 0, "setAngularVelocity")?;
            let native_angular_velocity =
                native_required_number(&args, 1, "setAngularVelocity")? as f32;
            let angular_velocity = f64::from(native_angular_velocity);
            if let Some(object) = angular_velocity_bridge
                .lock()
                .expect("render bridge lock poisoned")
                .game_lua_object_mut(&name)
                && (object.dynamic_body || object.kinematic_body)
            {
                object.angular_velocity = angular_velocity;
                if native_angular_velocity * native_angular_velocity > 0.0 {
                    object.wake();
                    object.motion_started = true;
                }
            }
            Ok(())
        })?,
    )?;

    for (function_name, is_impulse) in [("applyImpulse", true), ("applyForceNative", false)] {
        let force_bridge = Arc::clone(&render);
        globals.set(
            function_name,
            lua.create_function(move |_, args: MultiValue| {
                let name = native_required_string(&args, 0, function_name)?;
                let force_x = native_required_number(&args, 1, function_name)? as f32;
                let force_y = native_required_number(&args, 2, function_name)? as f32;
                let point_x = native_required_number(&args, 3, function_name)? as f32;
                let point_y = native_required_number(&args, 4, function_name)? as f32;
                if std::env::var_os("STELLA_TRACE_NATIVE").is_some() {
                    eprintln!(
                        "native {function_name}({name:?}, {force_x}, {force_y}, {point_x}, {point_y})"
                    );
                }
                let mut bridge = force_bridge.lock().expect("render bridge lock poisoned");
                if let Some(object) = bridge.game_lua_object_mut(&name) {
                    if !object.dynamic_body {
                        return Ok(());
                    }
                    object.wake();
                    object.motion_started = true;
                    // Both GameLua wrappers forward float arguments directly
                    // into Box2D. Keep each multiply/add at float32 precision,
                    // including the exact cross-product ordering seen at
                    // sub_10003F930/sub_10003F9CC.
                    let (center_x, center_y) = object.native_world_center();
                    let torque =
                        (center_y - point_y) * force_x + (point_x - center_x) * force_y;
                    if is_impulse {
                        // b2Body::ApplyLinearImpulse immediately changes the
                        // body's linear and angular velocities using the body's
                        // independently aggregated inverse mass and inertia.
                        let inverse_mass = object.inverse_mass as f32;
                        object.velocity_x =
                            f64::from((object.velocity_x as f32) + inverse_mass * force_x);
                        object.velocity_y =
                            f64::from((object.velocity_y as f32) + inverse_mass * force_y);
                        object.reset_display_interpolation_velocity(
                            object.velocity_x as f32,
                            object.velocity_y as f32,
                        );
                        object.angular_velocity = f64::from(
                            (object.angular_velocity as f32)
                                + (object.inverse_inertia() as f32) * torque,
                        );
                    } else {
                        object.force_x = f64::from((object.force_x as f32) + force_x);
                        object.force_y = f64::from((object.force_y as f32) + force_y);
                        // b2Body::ApplyForce accumulates torque even while the
                        // fixed-rotation flag makes inverse inertia zero.
                        object.torque = f64::from((object.torque as f32) + torque);
                    }
                }
                Ok(())
            })?,
        )?;
    }

    Ok(())
}
