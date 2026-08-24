//! Lua collision callback, sensor-overlap and joint-descriptor dispatch.

use std::collections::BTreeSet;
use std::sync::{Arc, Mutex};

use mlua::{Lua, Result as LuaResult, Value};

use crate::*;

pub(crate) fn remove_lua_joint_descriptors(lua: &Lua, names: &BTreeSet<String>) -> LuaResult<()> {
    if names.is_empty() {
        return Ok(());
    }
    let environment = game_environment(lua)?;
    let Value::Table(objects) = environment.get::<Value>("objects")? else {
        return Ok(());
    };
    let Value::Table(joints) = objects.get::<Value>("joints")? else {
        return Ok(());
    };
    for name in names {
        joints.raw_set(name.as_str(), Value::Nil)?;
    }

    // Native sub_10007BF50 erases the descriptor rather than leaving a hole
    // in the serialized level list.
    let length = joints.raw_len();
    let mut retained = Vec::with_capacity(length);
    for index in 1..=length {
        let value = joints.raw_get::<Value>(index)?;
        let removed = matches!(&value, Value::Table(descriptor)
            if descriptor.get::<String>("name").ok().is_some_and(|name| names.contains(&name)));
        if !removed && !matches!(&value, Value::Nil) {
            retained.push(value);
        }
    }
    for index in 1..=length {
        joints.raw_set(index, Value::Nil)?;
    }
    for (index, value) in retained.into_iter().enumerate() {
        joints.raw_set(index + 1, value)?;
    }
    Ok(())
}

/// `sub_10006800C`: notify both block endpoints while the joint descriptor is
/// still live, then re-read `isDrawn` and optionally queue its break particles.
pub(crate) fn dispatch_native_joint_removal_callbacks(lua: &Lua, name: &str) -> LuaResult<()> {
    let environment = game_environment(lua)?;
    if let Value::Function(function) = environment.get::<Value>("lua_onBeforeJointRemove")? {
        function.call::<()>(name)?;
    }

    // Purple performs this lookup after lua_onBeforeJointRemove returns, so a
    // callback mutation of isDrawn is immediately observable.
    let is_drawn = match environment.get::<Value>("objects")? {
        Value::Table(objects) => match objects.get::<Value>("joints")? {
            Value::Table(joints) => match joints.raw_get::<Value>(name)? {
                Value::Table(descriptor) => !matches!(
                    descriptor.raw_get::<Value>("isDrawn")?,
                    Value::Nil | Value::Boolean(false)
                ),
                _ => false,
            },
            _ => false,
        },
        _ => false,
    };
    if is_drawn
        && let Value::Function(function) = environment.get::<Value>("lua_addParticlesToJoint")?
    {
        function.call::<()>(name)?;
    }
    Ok(())
}

pub(crate) fn dispatch_and_remove_lua_joint(lua: &Lua, name: &str) -> LuaResult<()> {
    dispatch_native_joint_removal_callbacks(lua, name)?;
    remove_lua_joint_descriptors(lua, &BTreeSet::from([name.to_owned()]))
}

/// Callback-aware `removeObject` traversal. Each object's attached descriptors
/// are notified and erased before its native body tears down the corresponding
/// Box2D joints. DestroyBody dispatches EndContact while the RenderObjectData
/// and body are still lookup-visible, then `removeObject` erases the native
/// record. Type-five links only arm the target's native destruction timer; even
/// a zero timer is published through `deadBlocks` by the later per-frame expiry
/// pass.
pub(crate) fn remove_native_objects_with_joint_callbacks(
    lua: &Lua,
    render: &Arc<Mutex<RenderBridge>>,
    initial: Vec<String>,
) -> LuaResult<Vec<String>> {
    let mut removed = Vec::new();
    for name in initial {
        let attached = render
            .lock()
            .expect("render bridge lock poisoned")
            .attached_joint_names(&name);
        for joint_name in attached {
            dispatch_and_remove_lua_joint(lua, &joint_name)?;
        }
        let exists = render
            .lock()
            .expect("render bridge lock poisoned")
            .scene
            .contains_key(&name);
        if !exists {
            continue;
        }
        let exits = render
            .lock()
            .expect("render bridge lock poisoned")
            .drain_contacts_for_removed_objects(std::slice::from_ref(&name));
        dispatch_native_contact_exits(lua, render, &exits)?;
        let did_remove = render
            .lock()
            .expect("render bridge lock poisoned")
            .remove_one_object_with_destroy_links(&name);
        if did_remove {
            removed.push(name);
        }
    }
    Ok(removed)
}

/// `removeJointsFromObject` has a distinct order from body destruction: for
/// each attached joint it calls the Lua hook, destroys the native joint, then
/// removes the Lua descriptor.
pub(crate) fn remove_native_object_joints_with_callbacks(
    lua: &Lua,
    render: &Arc<Mutex<RenderBridge>>,
    object: &str,
) -> LuaResult<()> {
    let attached = render
        .lock()
        .expect("render bridge lock poisoned")
        .attached_joint_names(object);
    for name in attached {
        dispatch_native_joint_removal_callbacks(lua, &name)?;
        let destroyed = render
            .lock()
            .expect("render bridge lock poisoned")
            .destroy_attached_joint(object, &name);
        if destroyed {
            remove_lua_joint_descriptors(lua, &BTreeSet::from([name]))?;
        }
    }
    Ok(())
}

pub(crate) fn set_inside_gravity_fields(
    lua: &Lua,
    names: &[String],
    inside: bool,
) -> LuaResult<()> {
    let world = object_world(lua)?;
    for name in names {
        if let Value::Table(object) = world.raw_get::<Value>(name.as_str())? {
            if inside {
                object.raw_set("insideGravity", true)?;
            } else {
                object.raw_set("insideGravity", Value::Nil)?;
            }
        }
    }
    Ok(())
}

fn dispatch_native_contact_enter(
    lua: &Lua,
    render: &Arc<Mutex<RenderBridge>>,
    first: &str,
    second: &str,
) -> LuaResult<()> {
    let environment = game_environment(lua)?;
    if let Value::Function(function) = environment.get::<Value>("enterCollision")? {
        function.call::<()>((first, second))?;
    }
    let set_inside = render
        .lock()
        .expect("render bridge lock poisoned")
        .begin_native_sensor_overlap(first, second);
    set_inside_gravity_fields(lua, &set_inside, true)
}

pub(crate) fn dispatch_native_contact_exits(
    lua: &Lua,
    render: &Arc<Mutex<RenderBridge>>,
    exits: &[(String, String, bool)],
) -> LuaResult<()> {
    if exits.is_empty() {
        return Ok(());
    }
    let environment = game_environment(lua)?;
    for (first, second, sensor) in exits {
        if *sensor
            && let Value::Function(function) = environment.get::<Value>("exitTriggerCollision")?
        {
            function.call::<()>((first.as_str(), second.as_str()))?;
        }
        if *sensor {
            let clear_inside = render
                .lock()
                .expect("render bridge lock poisoned")
                .end_native_sensor_overlap(first, second);
            set_inside_gravity_fields(lua, &clear_inside, false)?;
        }
        if let Value::Function(function) = environment.get::<Value>("exitCollision")? {
            function.call::<()>((first.as_str(), second.as_str()))?;
        }
    }
    Ok(())
}

pub(crate) fn dispatch_native_contact_callbacks(
    lua: &Lua,
    render: &Arc<Mutex<RenderBridge>>,
    callbacks: Vec<NativeContactCallback>,
) -> LuaResult<()> {
    let environment = game_environment(lua)?;
    for callback in callbacks {
        if std::env::var_os("STELLA_TRACE_DAMAGE").is_some() {
            eprintln!("native-contact-callback {callback:?}");
        }
        match callback {
            NativeContactCallback::Enter { first, second } => {
                dispatch_native_contact_enter(lua, render, first.as_str(), second.as_str())?;
            }
            NativeContactCallback::Exit {
                first,
                second,
                sensor,
            } => {
                dispatch_native_contact_exits(lua, render, &[(first, second, sensor)])?;
            }
            NativeContactCallback::Bird {
                first,
                second,
                force,
                damage,
                point_x,
                point_y,
                normal_x,
                normal_y,
            } => {
                if let Value::Function(function) = environment.get::<Value>("birdCollision")? {
                    // The native template supplies eight values;
                    // birdCollision's optional ninth argument is nil.
                    function.call::<()>((
                        first.as_str(),
                        second.as_str(),
                        force,
                        damage,
                        point_x,
                        point_y,
                        normal_x,
                        normal_y,
                    ))?;
                }
            }
            NativeContactCallback::Block {
                first,
                second,
                force,
                damaged,
                second_damage,
                point_x,
                point_y,
                normal_x,
                normal_y,
                score_damage,
            } => {
                if let Value::Function(function) = environment.get::<Value>("blockCollision")? {
                    function.call::<()>((
                        first.as_str(),
                        second.as_str(),
                        force,
                        damaged,
                        false,
                        second_damage,
                        point_x,
                        point_y,
                        normal_x,
                        normal_y,
                    ))?;
                }
                native_add_block_collision_score(lua, score_damage)?;
            }
        }
    }
    Ok(())
}
