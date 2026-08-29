//! Fixture restitution/friction and body damping/gravity wrappers.

use crate::*;

pub(super) fn install(
    lua: &Lua,
    globals: &mlua::Table,
    render: &Arc<Mutex<RenderBridge>>,
) -> LuaResult<()> {
    for function_name in [
        "setRestitution",
        "setFriction",
        "setLinearDamping",
        "setAngularDamping",
        "setGravityScale",
    ] {
        let scalar_bridge = Arc::clone(render);
        globals.set(
            function_name,
            lua.create_function(move |_, args: MultiValue| {
                let name = native_required_string(&args, 0, function_name)?;
                let value = f64::from(native_required_number(&args, 1, function_name)? as f32);
                let mut bridge = scalar_bridge.lock().expect("render bridge lock poisoned");
                let object = if function_name == "setGravityScale" {
                    // sub_10004F608 uses throwing getRenderObject before
                    // accessing its body pointer.
                    bridge
                        .game_lua_object_mut(&name)
                        .ok_or_else(|| runtime_error(format!("Missing object: {name}")))?
                } else {
                    let Some(object) = bridge.game_lua_object_mut(&name) else {
                        // The older body members use nullable/manual lookup
                        // and only log an unknown name.
                        return Ok(());
                    };
                    object
                };
                if object.has_physics_body() {
                    match function_name {
                        // The fixture members write only b2Body::m_fixtureList,
                        // i.e. the final entry in our creation-order vector.
                        "setRestitution" => {
                            if let Some(restitution) = object.fixture_restitutions.last_mut() {
                                *restitution = value;
                            }
                        }
                        "setFriction" => {
                            if let Some(friction) = object.fixture_frictions.last_mut() {
                                *friction = value;
                            }
                        }
                        "setLinearDamping" => object.linear_damping = value,
                        "setAngularDamping" => object.angular_damping = value,
                        "setGravityScale" => object.gravity_scale = value,
                        _ => unreachable!(),
                    }
                }
                Ok(())
            })?,
        )?;
    }
    Ok(())
}
