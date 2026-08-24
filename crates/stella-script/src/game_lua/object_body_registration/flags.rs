//! Fixed-rotation, fixture-sensor, and sleeping body members.

use crate::*;

pub(super) fn install(
    lua: &Lua,
    globals: &mlua::Table,
    render: &Arc<Mutex<RenderBridge>>,
) -> LuaResult<()> {
    for function_name in ["setFixedRotation", "setAsSensor", "setSleeping"] {
        let boolean_bridge = Arc::clone(render);
        globals.set(
            function_name,
            lua.create_function(move |_, args: MultiValue| {
                let name = native_required_string(&args, 0, function_name)?;
                let enabled = native_required_boolean(&args, 1, function_name)?;
                if std::env::var_os("STELLA_TRACE_NATIVE").is_some() {
                    eprintln!("native {function_name}({name:?}, {enabled})");
                }
                let mut bridge = boolean_bridge.lock().expect("render bridge lock poisoned");
                if let Some(object) = bridge
                    .scene
                    .get_mut(&name)
                    .filter(|object| object.has_physics_body())
                {
                    match function_name {
                        "setFixedRotation" => {
                            // sub_1000411F0 updates b2Body's flag and then
                            // unconditionally calls ResetMassData. This also
                            // discards a prior SetMassData inertia override.
                            let old_center = object.world_center();
                            object.fixed_rotation = enabled;
                            object.reset_native_mass_data(old_center);
                        }
                        "setAsSensor" => {
                            // b2Fixture::SetSensor wakes the body only when the
                            // fixture byte actually flips.
                            if object.sensor != enabled {
                                object.wake();
                                object.sensor = enabled;
                            }
                        }
                        "setSleeping" => {
                            if enabled {
                                object.sleeping = true;
                                object.sleep_time = 0.0;
                                object.velocity_x = 0.0;
                                object.velocity_y = 0.0;
                                object.angular_velocity = 0.0;
                                object.force_x = 0.0;
                                object.force_y = 0.0;
                                object.torque = 0.0;
                            } else if object.sleeping {
                                object.wake();
                            }
                        }
                        _ => unreachable!(),
                    }
                }
                Ok(())
            })?,
        )?;
    }
    Ok(())
}
