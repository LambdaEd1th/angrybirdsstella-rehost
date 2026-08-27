//! Lua-side descriptor publication and custom-joint dispatch.

use mlua::{Lua, Result as LuaResult, Value};

use crate::{NativeLuaObject, game_environment, native_lua_object, retain_native_lua_object};

pub(crate) fn mirror_lua_joint_descriptor(lua: &Lua, descriptor: &mlua::Table) -> LuaResult<()> {
    let name = descriptor.get::<String>("name").unwrap_or_default();
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
    // createJoint constructs a new native-owned LuaTable and copies resolved
    // fields into it. The caller's descriptor is never retained by identity.
    let published = lua.create_table()?;
    for pair in descriptor.clone().pairs::<Value, Value>() {
        let (key, value) = pair?;
        published.raw_set(key, value)?;
    }
    joints.raw_set(name, published)?;
    Ok(())
}

pub(crate) fn dispatch_custom_joint(lua: &Lua, descriptor: mlua::Table) -> LuaResult<()> {
    let environment = game_environment(lua)?;
    if let Value::Function(create_custom_joint) = environment.get::<Value>("createCustomJoint")? {
        create_custom_joint.call::<()>(descriptor)?;
    }
    Ok(())
}
