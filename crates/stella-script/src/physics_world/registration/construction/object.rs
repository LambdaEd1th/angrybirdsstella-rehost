//! Shared constructor tail: publish the Lua mirror, then install the body/node.

mod lua_mirror;
mod scene;

use std::{
    cell::RefCell,
    rc::Rc,
    sync::{Arc, Mutex},
};

use mlua::{Lua, Result as LuaResult};

use crate::{DrawCallbackRecord, DrawCallbacks, RenderBridge, object_world, runtime_error};

use super::PreparedConstruction;

pub(super) fn commit(
    lua: &Lua,
    render: &Arc<Mutex<RenderBridge>>,
    draw_callbacks: &Rc<RefCell<DrawCallbacks>>,
    prepared: PreparedConstruction,
) -> LuaResult<()> {
    if prepared.request.kind.has_body()
        && render
            .lock()
            .expect("render bridge lock poisoned")
            .physics_world_locked
    {
        // b2World::CreateBody (sub_10086DF90) returns nullptr when e_locked
        // is set.  Box/circle immediately pass that pointer to CreateFixture;
        // polygon/line either do the same or later read body+0x98 for mass.
        // No native physics constructor checks null, so invoking one from
        // BeginContact terminates Purple through a null dereference.
        // Preserve the failed construction and absence of a usable record,
        // but contain the process crash as a catchable Lua runtime error.
        return Err(runtime_error(format!(
            "{} cannot create a body while the physics world is locked",
            prepared.request.kind.script_name()
        )));
    }
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
