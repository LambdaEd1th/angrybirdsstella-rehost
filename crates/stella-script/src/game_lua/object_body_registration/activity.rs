//! `setActive` and collision-enabled proxy/contact lifecycle members.

use crate::*;

pub(super) fn install(
    lua: &Lua,
    globals: &mlua::Table,
    render: &Arc<Mutex<RenderBridge>>,
) -> LuaResult<()> {
    let active_bridge = Arc::clone(render);
    globals.set(
        "setActive",
        lua.create_function(move |lua, args: MultiValue| {
            let name = native_required_string(&args, 0, "setActive")?;
            let enabled = native_required_boolean(&args, 1, "setActive")?;
            if std::env::var_os("STELLA_TRACE_NATIVE").is_some() {
                eprintln!("native setActive({name:?}, {enabled})");
            }
            let exits = {
                let mut bridge = active_bridge.lock().expect("render bridge lock poisoned");
                let Some(was_active) = bridge
                    .scene
                    .get(&name)
                    .filter(|object| object.has_physics_body())
                    .map(|object| object.active)
                else {
                    return Ok(());
                };
                bridge.set_object_active_state(&name, enabled);
                if was_active && !enabled {
                    bridge
                        .drain_contacts_for_invalidated_objects(std::slice::from_ref(&name), false)
                } else {
                    Vec::new()
                }
            };
            // SetActive does not mirror an `active` field into objects.world.
            dispatch_native_contact_exits(lua, &active_bridge, &exits)
        })?,
    )?;

    let collision_enabled_bridge = Arc::clone(render);
    globals.set(
        "setCollisionEnabled",
        lua.create_function(move |lua, args: MultiValue| {
            let name = native_required_string(&args, 0, "setCollisionEnabled")?;
            let enabled = native_required_boolean(&args, 1, "setCollisionEnabled")?;
            if std::env::var_os("STELLA_TRACE_NATIVE").is_some() {
                eprintln!("native setCollisionEnabled({name:?}, {enabled})");
            }
            // sub_10004F3D8 deactivates first. EndContact therefore observes
            // both the native byte and Lua collisionEnabled field unchanged.
            let exits = {
                let mut bridge = collision_enabled_bridge
                    .lock()
                    .expect("render bridge lock poisoned");
                let object = bridge
                    .scene
                    .get(&name)
                    .ok_or_else(|| runtime_error(format!("Missing object: {name}")))?;
                if !object.has_physics_body() {
                    return Ok(());
                }
                let was_active = object.active;
                bridge.set_object_active_state(&name, false);
                if was_active {
                    bridge
                        .drain_contacts_for_invalidated_objects(std::slice::from_ref(&name), false)
                } else {
                    Vec::new()
                }
            };
            dispatch_native_contact_exits(lua, &collision_enabled_bridge, &exits)?;

            let mut bridge = collision_enabled_bridge
                .lock()
                .expect("render bridge lock poisoned");
            bridge.set_object_active_state(&name, true);
            if let Some(object) = bridge.scene.get_mut(&name) {
                object.collision_enabled = enabled;
            } else {
                return Ok(());
            }
            drop(bridge);
            if let Value::Table(entry) = object_world(lua)?.raw_get::<Value>(name.as_str())? {
                entry.set("collisionEnabled", enabled)?;
            }
            Ok(())
        })?,
    )?;

    Ok(())
}
