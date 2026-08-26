//! Shared constructor tail: publish the Lua mirror, then install the body/node.

mod lua_mirror;
mod scene;

use std::{
    cell::RefCell,
    rc::Rc,
    sync::{Arc, Mutex},
};

use mlua::{Lua, Result as LuaResult};

use crate::{DrawCallbackRecord, DrawCallbacks, RenderBridge, object_world};

use super::PreparedConstruction;

pub(super) fn commit(
    lua: &Lua,
    render: &Arc<Mutex<RenderBridge>>,
    draw_callbacks: &Rc<RefCell<DrawCallbacks>>,
    prepared: PreparedConstruction,
) -> LuaResult<()> {
    let world_identity = object_world(lua)?.to_pointer() as usize;
    render
        .lock()
        .expect("render bridge lock poisoned")
        .synchronize_object_world_owner(world_identity);
    let object = lua_mirror::replace(lua, &prepared)?;
    let callback_slot = {
        let mut callbacks = draw_callbacks.borrow_mut();
        if callbacks.object_world_identity != Some(world_identity) {
            callbacks.clear_records();
            callbacks.object_world_identity = Some(world_identity);
        }
        callbacks.insert_record(
            prepared.request.name.clone(),
            DrawCallbackRecord {
                object,
                pre: None,
                post: None,
            },
        )
    };
    scene::insert(render, prepared, callback_slot);
    Ok(())
}
