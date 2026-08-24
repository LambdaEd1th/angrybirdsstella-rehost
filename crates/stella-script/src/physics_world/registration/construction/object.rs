//! Shared constructor tail: publish the Lua mirror, then install the body/node.

mod lua_mirror;
mod scene;

use std::sync::{Arc, Mutex};

use mlua::{Lua, Result as LuaResult};

use crate::{RenderBridge, object_world};

use super::PreparedConstruction;

pub(super) fn commit(
    lua: &Lua,
    render: &Arc<Mutex<RenderBridge>>,
    prepared: PreparedConstruction,
) -> LuaResult<()> {
    let world_identity = object_world(lua)?.to_pointer() as usize;
    render
        .lock()
        .expect("render bridge lock poisoned")
        .synchronize_object_world_owner(world_identity);
    lua_mirror::replace(lua, &prepared)?;
    scene::insert(render, prepared);
    Ok(())
}
