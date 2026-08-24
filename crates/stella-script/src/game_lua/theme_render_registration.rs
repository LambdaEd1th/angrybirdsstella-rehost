//! Theme member registration in `sub_10002C274` relative order.

mod lifecycle;
mod offsets;
mod passes;
mod selection;
mod sprite;

use crate::*;

pub(crate) fn install(
    lua: &Lua,
    globals: &mlua::Table,
    render: Arc<Mutex<RenderBridge>>,
    resource_runtime: Arc<Mutex<ResourceRuntime>>,
    data_root: Arc<PathBuf>,
    libc_random: Arc<Mutex<NativeLibcRandom>>,
) -> LuaResult<()> {
    // The six members are interleaved with unrelated GameLua registrations in
    // the constructor. Preserve their relative native order here.
    lifecycle::install(
        lua,
        globals,
        Arc::clone(&render),
        Arc::clone(&resource_runtime),
        Arc::clone(&data_root),
    )?;
    sprite::install(lua, globals, Arc::clone(&render))?;
    offsets::install(lua, globals, Arc::clone(&render))?;
    passes::install_background(
        lua,
        globals,
        Arc::clone(&render),
        Arc::clone(&resource_runtime),
        Arc::clone(&data_root),
    )?;
    selection::install(
        lua,
        globals,
        Arc::clone(&render),
        resource_runtime.clone(),
        libc_random,
    )?;
    passes::install_foreground(lua, globals, render, resource_runtime, data_root)
}
