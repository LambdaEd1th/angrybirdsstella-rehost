//! Ordered facade for independent `LuaResources` query member families.

mod clip_rect;
mod font;
mod geometry;

use crate::*;

pub(crate) use clip_rect::install as install_clip_rect;
pub(crate) use font::install as install_use_font;

pub(crate) fn install_geometry(
    lua: &Lua,
    resource_api: &mlua::Table,
    render: Arc<Mutex<RenderBridge>>,
    resource_runtime: Arc<Mutex<ResourceRuntime>>,
) -> LuaResult<()> {
    geometry::install(lua, resource_api, render, resource_runtime)
}
