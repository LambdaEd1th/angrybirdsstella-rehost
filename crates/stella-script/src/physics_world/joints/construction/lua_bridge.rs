//! Lua-side descriptor publication and custom-joint dispatch.

use mlua::{Lua, Result as LuaResult, Value};

use crate::game_environment;

pub(crate) fn mirror_lua_joint_descriptor(lua: &Lua, descriptor: &mlua::Table) -> LuaResult<()> {
    let name = descriptor.get::<String>("name").unwrap_or_default();
    if name.is_empty() {
        return Ok(());
    }
    let environment = game_environment(lua)?;
    let objects = match environment.get::<Value>("objects")? {
        Value::Table(objects) => objects,
        _ => {
            let objects = lua.create_table()?;
            environment.set("objects", objects.clone())?;
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
    joints.raw_set(name, descriptor.clone())?;
    Ok(())
}

pub(crate) fn dispatch_custom_joint(lua: &Lua, descriptor: mlua::Table) -> LuaResult<()> {
    let environment = game_environment(lua)?;
    if let Value::Function(create_custom_joint) = environment.get::<Value>("createCustomJoint")? {
        create_custom_joint.call::<()>(descriptor)?;
    }
    Ok(())
}
