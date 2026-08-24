//! Strict `createNativeBlockExtension` adapter (`sub_10005A9F0`).

use super::{PendingDirtCollisions, collision, queries, rebuild};
use crate::*;

pub(super) fn create(
    lua: &Lua,
    render: &Arc<Mutex<RenderBridge>>,
    resources: &Arc<Mutex<ResourceRuntime>>,
    data_root: &Arc<PathBuf>,
    args: MultiValue,
) -> LuaResult<Value> {
    // Dirt.lua calls this as ("dirt", block.name). The native adapter checks
    // exact strings in fixed slots 1 and 2 before consulting its registry.
    let extension_tag = args
        .front()
        .and_then(value_string)
        .ok_or_else(|| runtime_error("createNativeBlockExtension argument 1 must be string"))?;
    let object_name = args
        .iter()
        .nth(1)
        .and_then(value_string)
        .ok_or_else(|| runtime_error("createNativeBlockExtension argument 2 must be string"))?;
    if extension_tag != "dirt" {
        return Ok(Value::Nil);
    }
    if !render
        .lock()
        .expect("render bridge lock poisoned")
        .scene
        .contains_key(&object_name)
    {
        return Ok(Value::Nil);
    }

    // The level loader can publish the definition one statement later, so
    // render and collision processing repeat this idempotent binding.
    ensure_dirt_component(lua, render, resources, data_root, &object_name)?;
    let pending = PendingDirtCollisions::default();
    let extension = lua.create_table()?;
    collision::install(lua, &extension, Arc::clone(render), Rc::clone(&pending))?;
    rebuild::install(
        lua,
        &extension,
        Arc::clone(render),
        Arc::clone(resources),
        Arc::clone(data_root),
        object_name.clone(),
        pending,
    )?;
    queries::install(
        lua,
        &extension,
        Arc::clone(render),
        Arc::clone(resources),
        Arc::clone(data_root),
        object_name,
    )?;
    Ok(Value::Table(extension))
}
