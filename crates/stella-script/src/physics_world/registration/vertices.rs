//! Shared polygon and line-shape vertex-buffer Lua bindings.

use std::sync::{Arc, Mutex};

use mlua::{Lua, MultiValue, Result as LuaResult, Table};

use crate::*;

pub(super) fn install(
    lua: &Lua,
    globals: &Table,
    render: Arc<Mutex<RenderBridge>>,
) -> LuaResult<()> {
    let clear_vertices_bridge = Arc::clone(&render);
    globals.set(
        "clearVertices",
        lua.create_function(move |_, _: MultiValue| {
            clear_vertices_bridge
                .lock()
                .expect("render bridge lock poisoned")
                .vertex_buffer
                .clear();
            Ok(())
        })?,
    )?;
    let add_vertex_bridge = Arc::clone(&render);
    globals.set(
        "addVertex",
        lua.create_function(move |_, args: MultiValue| {
            // Adapter `sub_100088294` requires two numbers and narrows each
            // through a single-precision register before appending the pair.
            let x = f64::from(native_required_number(&args, 0, "addVertex")? as f32);
            let y = f64::from(native_required_number(&args, 1, "addVertex")? as f32);
            add_vertex_bridge
                .lock()
                .expect("render bridge lock poisoned")
                .vertex_buffer
                .push((x, y));
            Ok(())
        })?,
    )?;

    Ok(())
}
