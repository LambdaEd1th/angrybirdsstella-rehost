//! Ordered GameLua installation for the Purple PhysicsWorld surface.

mod construction;
mod joints;
mod queries;
mod scalars;
mod tracks;
mod vertices;

use std::sync::{Arc, Mutex};

use mlua::{Lua, Result as LuaResult, Table};

use crate::{RenderBridge, ResourceRuntime};

pub(crate) fn install_bindings(
    lua: &Lua,
    globals: &Table,
    render: Arc<Mutex<RenderBridge>>,
    resources: Arc<Mutex<ResourceRuntime>>,
    data_root: Arc<std::path::PathBuf>,
) -> LuaResult<()> {
    // Keep the subsystem order visible here. Hopper and IDA both place these
    // registrations in the large native installer sub_10002C274, followed by
    // dedicated query callbacks sub_10005411C and sub_10005464C.
    scalars::install(lua, globals, Arc::clone(&render))?;
    construction::install(lua, globals, Arc::clone(&render), resources, data_root)?;
    joints::install_creation(lua, globals, Arc::clone(&render))?;
    tracks::install(lua, globals, Arc::clone(&render))?;
    joints::install_destruction(lua, globals, Arc::clone(&render))?;
    vertices::install(lua, globals, Arc::clone(&render))?;
    queries::install(lua, globals, render)
}
