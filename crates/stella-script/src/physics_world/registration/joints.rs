//! PhysicsWorld joint creation and destruction Lua bindings.

use std::sync::{Arc, Mutex};

use mlua::{Lua, MultiValue, Result as LuaResult, Table, Value};

use crate::*;

pub(super) fn install_creation(
    lua: &Lua,
    globals: &Table,
    render: Arc<Mutex<RenderBridge>>,
) -> LuaResult<()> {
    let joint_bridge = Arc::clone(&render);
    globals.set(
        "createJoint",
        lua.create_function(move |lua, args: MultiValue| {
            // Direct member sub_100037374 wraps stack slot -1 and immediately
            // performs a strict numeric read of descriptor.type.
            let index = args
                .len()
                .checked_sub(1)
                .ok_or_else(|| runtime_error("createJoint expects a table"))?;
            let descriptor = native_required_table(&args, index, "createJoint")?;
            let joint_type = table_required_number(&descriptor, "type", "createJoint")? as f32;
            if joint_type >= 7.0 {
                dispatch_custom_joint(lua, descriptor)?;
            } else if let Some(created) = insert_physics_joint(
                lua,
                &mut joint_bridge.lock().expect("render bridge lock poisoned"),
                &descriptor,
                joint_type,
            )? {
                // Purple publishes a fresh resolved descriptor only after
                // the ordinary native joint has been constructed.
                mirror_lua_joint_descriptor(lua, &descriptor, &created)?;
            }
            Ok(())
        })?,
    )?;

    // The batch entry point is a thin native loop over joint descriptor
    // tables. Keep it explicit so the descriptor contract can be mirrored by
    // the Rust physics bridge instead of being swallowed by the generic stub.
    let joints_bridge = Arc::clone(&render);
    globals.set(
        "createJoints",
        lua.create_function(move |lua, args: MultiValue| {
            let index = args
                .len()
                .checked_sub(1)
                .ok_or_else(|| runtime_error("createJoints expects a table"))?;
            let descriptors = native_required_table(&args, index, "createJoints")?;
            for pair in descriptors.pairs::<Value, Value>() {
                let (_, value) = pair?;
                if std::env::var_os("STELLA_TRACE_JOINTS").is_some()
                    && let Ok(json) = lua.from_value::<serde_json::Value>(value.clone())
                {
                    eprintln!(
                        "native createJoints({})",
                        serde_json::to_string(&json).unwrap_or_else(|_| "null".to_owned())
                    );
                }
                let Value::Table(descriptor) = value else {
                    return Err(runtime_error("createJoints descriptor must be table"));
                };
                let joint_type = table_required_number(&descriptor, "type", "createJoints")? as f32;
                if joint_type >= 7.0 {
                    // sub_10003CC64 invokes createJoint inside its lua_next
                    // loop. A custom handler therefore completes before the
                    // next descriptor and may create an endpoint it uses.
                    dispatch_custom_joint(lua, descriptor)?;
                } else if let Some(created) = insert_physics_joint(
                    lua,
                    &mut joints_bridge.lock().expect("render bridge lock poisoned"),
                    &descriptor,
                    joint_type,
                )? {
                    mirror_lua_joint_descriptor(lua, &descriptor, &created)?;
                }
            }
            Ok(())
        })?,
    )?;
    Ok(())
}

pub(super) fn install_destruction(
    lua: &Lua,
    globals: &Table,
    render: Arc<Mutex<RenderBridge>>,
) -> LuaResult<()> {
    let destroy_joint_bridge = Arc::clone(&render);
    globals.set(
        "destroyJoint",
        lua.create_function(move |lua, args: MultiValue| {
            let name = native_required_string(&args, 0, "destroyJoint")?;
            // sub_10003E668 erases the Lua descriptor before it dispatches
            // b2World::DestroyJoint and removes the native vector record.
            remove_lua_joint_descriptors(lua, &BTreeSet::from([name.clone()]))?;
            let mut bridge = destroy_joint_bridge
                .lock()
                .expect("render bridge lock poisoned");
            // A collision-broken record has already left GameLua+0x3C0 and
            // therefore cannot be found by this explicit name lookup. Its
            // queued native joint remains owned by the frame-tail drain.
            if !bridge.joint_pending_native_destruction(&name) {
                bridge.destroy_native_joint(&name);
            }
            Ok(())
        })?,
    )?;

    Ok(())
}
