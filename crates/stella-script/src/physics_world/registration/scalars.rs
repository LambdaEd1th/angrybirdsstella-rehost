//! PhysicsWorld time-step and force-multiplier Lua bindings.

use std::sync::{Arc, Mutex};

use mlua::{Lua, MultiValue, Result as LuaResult, Table};

use crate::*;

pub(super) fn install(
    lua: &Lua,
    globals: &Table,
    render: Arc<Mutex<RenderBridge>>,
) -> LuaResult<()> {
    // These values live in the native engine in the iOS build and are read
    // back by Lua during every gameplay update. Returning no values here is
    // observably different from the original ABI: Lua immediately performs
    // arithmetic on the result.
    let set_delta_time_multiplier = Arc::clone(&render);
    globals.set(
        "setDeltaTimeMultiplier",
        lua.create_function(move |_, args: MultiValue| {
            let value = native_required_number(&args, 0, "setDeltaTimeMultiplier")? as f32;
            set_delta_time_multiplier
                .lock()
                .expect("render bridge lock poisoned")
                .delta_time_multiplier = value;
            Ok(())
        })?,
    )?;
    let get_delta_time_multiplier = Arc::clone(&render);
    globals.set(
        "getDeltaTimeMultiplier",
        lua.create_function(move |_, _: MultiValue| {
            Ok(f64::from(
                get_delta_time_multiplier
                    .lock()
                    .expect("render bridge lock poisoned")
                    .delta_time_multiplier,
            ))
        })?,
    )?;

    for (setter, getter, water) in [
        (
            "setGravityForceMultiplier",
            "getGravityForceMultiplier",
            false,
        ),
        ("setWaterForceMultiplier", "getWaterForceMultiplier", true),
    ] {
        let setter_bridge = Arc::clone(&render);
        globals.set(
            setter,
            lua.create_function(move |_, args: MultiValue| {
                let value = f64::from(native_required_number(&args, 0, setter)? as f32);
                let mut bridge = setter_bridge.lock().expect("render bridge lock poisoned");
                if water {
                    bridge.water_force_multiplier = value;
                } else {
                    bridge.gravity_force_multiplier = value;
                }
                Ok(())
            })?,
        )?;
        let getter_bridge = Arc::clone(&render);
        globals.set(
            getter,
            lua.create_function(move |_, _: MultiValue| {
                let bridge = getter_bridge.lock().expect("render bridge lock poisoned");
                Ok(if water {
                    bridge.water_force_multiplier
                } else {
                    bridge.gravity_force_multiplier
                })
            })?,
        )?;
    }

    Ok(())
}
