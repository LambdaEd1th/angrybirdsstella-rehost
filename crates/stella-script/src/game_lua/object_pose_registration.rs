//! RenderObject position, angle and visual-scale members from the transform cluster.

use super::object_scale_member;
use crate::*;

pub(crate) fn install(
    lua: &Lua,
    globals: &mlua::Table,
    render: Arc<Mutex<RenderBridge>>,
) -> LuaResult<()> {
    let position_bridge = Arc::clone(&render);
    globals.set(
        "setPosition",
        lua.create_function(move |lua, args: MultiValue| {
            // sub_1000889E4 consumes the complete STRING/NUMBER/NUMBER tuple
            // and narrows both coordinates before sub_10003FA60 is entered.
            let name = native_required_string(&args, 0, "setPosition")?;
            let x = f64::from(native_required_number(&args, 1, "setPosition")? as f32);
            let y = f64::from(native_required_number(&args, 2, "setPosition")? as f32);
            {
                let mut bridge = position_bridge.lock().expect("render bridge lock poisoned");
                let object = bridge
                    .scene
                    .get_mut(&name)
                    .ok_or_else(|| runtime_error(format!("Missing object: {name}")))?;
                object.x = x;
                object.y = y;
                object.sync_native_sweep_from_transform();
                // b2Body::SetTransform synchronizes this body's fixture
                // proxies and then immediately drains the move buffer.
                bridge.sync_native_body_broad_phase(&name);
            }
            if let Value::Table(entry) = object_world(lua)?.raw_get::<Value>(name.as_str())? {
                entry.set("x", x)?;
                entry.set("y", y)?;
            }
            Ok(())
        })?,
    )?;

    for function_name in ["setRotation", "setAngle"] {
        let angle_bridge = Arc::clone(&render);
        globals.set(
            function_name,
            lua.create_function(move |lua, args: MultiValue| {
                let name = native_required_string(&args, 0, function_name)?;
                // sub_10003FB78 receives one float from sub_1000866F8, builds
                // 2π with one float add, calls fmodf and adds it only for a
                // strictly negative remainder (therefore preserving -0).
                let input_angle = native_required_number(&args, 1, function_name)? as f32;
                let tau = std::f32::consts::PI + std::f32::consts::PI;
                let mut native_angle = input_angle % tau;
                if native_angle < 0.0 {
                    native_angle += tau;
                }
                let angle = f64::from(native_angle);
                if std::env::var_os("STELLA_TRACE_NATIVE").is_some() {
                    eprintln!("native {function_name}({name:?}, {angle})");
                }
                {
                    let mut bridge = angle_bridge.lock().expect("render bridge lock poisoned");
                    let object = bridge
                        .scene
                        .get_mut(&name)
                        .ok_or_else(|| runtime_error(format!("Missing object: {name}")))?;
                    object.angle = angle;
                    object.sync_native_sweep_from_transform();
                    bridge.sync_native_body_broad_phase(&name);
                }
                if let Value::Table(entry) = object_world(lua)?.raw_get::<Value>(name.as_str())? {
                    entry.set("angle", angle)?;
                }
                Ok(())
            })?,
        )?;
    }

    globals.set(
        "setScale",
        lua.create_function(move |lua, args: MultiValue| {
            let name = native_required_string(&args, 0, "setScale")?;
            let scale_x = f64::from(native_required_number(&args, 1, "setScale")? as f32);
            let scale_y = f64::from(native_required_number(&args, 2, "setScale")? as f32);
            if std::env::var_os("STELLA_TRACE_NATIVE").is_some() {
                eprintln!("native setScale({name:?}, {scale_x}, {scale_y})");
            }
            object_scale_member::apply(lua, &render, &name, scale_x, scale_y)
        })?,
    )?;
    Ok(())
}
