//! Lua-side descriptor publication and custom-joint dispatch.

use mlua::{Lua, Result as LuaResult, Value};

use crate::{NativeLuaObject, game_environment, native_lua_object, retain_native_lua_object};

use super::physics::CreatedPhysicsJoint;

pub(crate) fn mirror_lua_joint_descriptor(
    lua: &Lua,
    descriptor: &mlua::Table,
    created: &CreatedPhysicsJoint,
) -> LuaResult<()> {
    let joint = &created.joint;
    let name = joint.name.clone();
    if name.is_empty() {
        return Ok(());
    }
    let environment = game_environment(lua)?;
    let objects = match native_lua_object(lua, NativeLuaObject::Objects)? {
        Some(objects) => objects,
        None => {
            let objects = lua.create_table()?;
            environment.set("objects", objects.clone())?;
            retain_native_lua_object(lua, NativeLuaObject::Objects, Some(&objects))?;
            objects
        }
    };
    let joints = match objects.get::<Value>("joints")? {
        Value::Table(joints) => joints,
        _ => {
            let joints = lua.create_table()?;
            objects.set("joints", joints.clone())?;
            joints
        }
    };
    // createJoint constructs a new native-owned LuaTable and writes a
    // per-class canonical schema. It does not clone arbitrary editor fields.
    let published = lua.create_table()?;
    published.raw_set("name", name.clone())?;
    published.raw_set("end1", joint.first.clone())?;
    published.raw_set("end2", joint.second.clone())?;
    published.raw_set("type", native_number(descriptor, "type")?.unwrap_or(0.0))?;
    published.raw_set("coordType", f64::from(joint.coord_type as f32))?;

    copy_optional_boolean(descriptor, &published, "breakable")?;
    copy_optional_number(descriptor, &published, "breakForce")?;
    if copy_optional_boolean(descriptor, &published, "isDrawn")? {
        copy_optional_string(descriptor, &published, "sprite")?;
    }

    let (x1, y1, x2, y2) = created.descriptor_anchors;
    let publish_anchors = || -> LuaResult<()> {
        published.raw_set("x1", f32_number(x1))?;
        published.raw_set("x2", f32_number(x2))?;
        published.raw_set("y1", f32_number(y1))?;
        published.raw_set("y2", f32_number(y2))?;
        Ok(())
    };
    match joint.joint_type {
        1 => {
            publish_anchors()?;
            published.raw_set("frequency", f32_number(joint.frequency))?;
            published.raw_set("dampingRatio", f32_number(joint.damping_ratio))?;
            published.raw_set("collideConnected", joint.collide_connected)?;
            published.raw_set("length", f32_number(joint.rest_length))?;
        }
        2 => {
            publish_anchors()?;
            published.raw_set("collideConnected", joint.collide_connected)?;
        }
        3 => {
            published.raw_set("motor", joint.motor_enabled)?;
            published.raw_set("motorSpeed", f32_number(joint.motor_speed.unwrap_or(0.0)))?;
            published.raw_set("maxTorque", f32_number(joint.max_torque))?;
            copy_optional_number(descriptor, &published, "angleTarget")?;
            published.raw_set("limit", joint.limits_enabled)?;
            published.raw_set("lowerLimit", f32_number(joint.lower_limit))?;
            published.raw_set("upperLimit", f32_number(joint.upper_limit))?;
            published.raw_set(
                "backAndForth",
                native_boolean(descriptor, "backAndForth")?.unwrap_or(false),
            )?;
            publish_anchors()?;
            published.raw_set("collideConnected", joint.collide_connected)?;
            if native_boolean(descriptor, "isMenuJoint")? == Some(true) {
                published.raw_set("isMenuJoint", true)?;
            }
        }
        4 | 5 if joint.is_physical => {
            published.raw_set("limit", joint.limits_enabled)?;
            published.raw_set("lowerLimit", f32_number(joint.lower_limit))?;
            published.raw_set("upperLimit", f32_number(joint.upper_limit))?;
            published.raw_set("motor", joint.motor_enabled)?;
            published.raw_set("motorSpeed", f32_number(joint.motor_speed.unwrap_or(0.0)))?;
            published.raw_set("maxTorque", f32_number(joint.max_torque))?;
            publish_anchors()?;
            published.raw_set(
                "worldAxisX",
                native_number(descriptor, "worldAxisX")?.unwrap_or(0.0),
            )?;
            published.raw_set(
                "worldAxisY",
                native_number(descriptor, "worldAxisY")?.unwrap_or(0.0),
            )?;
            published.raw_set("collideConnected", joint.collide_connected)?;
            published.raw_set(
                "backAndForth",
                native_boolean(descriptor, "backAndForth")?.unwrap_or(true),
            )?;
        }
        5 => {
            publish_anchors()?;
            published.raw_set("destroyTimer", f32_number(joint.destroy_timer))?;
            copy_optional_boolean(descriptor, &published, "oneWayDestroy")?;
        }
        6 => {
            publish_anchors()?;
            published.raw_set("collideConnected", joint.collide_connected)?;
        }
        _ => {}
    }
    joints.raw_set(name, published)?;
    Ok(())
}

fn f32_number(value: f64) -> f64 {
    f64::from(value as f32)
}

fn native_number(table: &mlua::Table, field: &str) -> LuaResult<Option<f64>> {
    Ok(match table.raw_get::<Value>(field)? {
        Value::Integer(value) => Some(f64::from(value as f32)),
        Value::Number(value) => Some(f64::from(value as f32)),
        _ => None,
    })
}

fn native_boolean(table: &mlua::Table, field: &str) -> LuaResult<Option<bool>> {
    Ok(match table.raw_get::<Value>(field)? {
        Value::Boolean(value) => Some(value),
        _ => None,
    })
}

fn copy_optional_number(
    source: &mlua::Table,
    destination: &mlua::Table,
    field: &str,
) -> LuaResult<bool> {
    let Some(value) = native_number(source, field)? else {
        return Ok(false);
    };
    destination.raw_set(field, value)?;
    Ok(true)
}

fn copy_optional_boolean(
    source: &mlua::Table,
    destination: &mlua::Table,
    field: &str,
) -> LuaResult<bool> {
    let Some(value) = native_boolean(source, field)? else {
        return Ok(false);
    };
    destination.raw_set(field, value)?;
    Ok(true)
}

fn copy_optional_string(
    source: &mlua::Table,
    destination: &mlua::Table,
    field: &str,
) -> LuaResult<bool> {
    let Value::String(value) = source.raw_get::<Value>(field)? else {
        return Ok(false);
    };
    destination.raw_set(field, value)?;
    Ok(true)
}

pub(crate) fn dispatch_custom_joint(lua: &Lua, descriptor: mlua::Table) -> LuaResult<()> {
    let environment = game_environment(lua)?;
    if let Value::Function(create_custom_joint) = environment.get::<Value>("createCustomJoint")? {
        create_custom_joint.call::<()>(descriptor)?;
    }
    Ok(())
}
