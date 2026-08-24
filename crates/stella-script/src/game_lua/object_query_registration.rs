//! Native-order RenderObject and Box2D query facade.

use crate::*;

mod appearance;
mod motion;
mod points;
mod world;

pub(crate) fn install(
    lua: &Lua,
    globals: &mlua::Table,
    render: Arc<Mutex<RenderBridge>>,
) -> LuaResult<()> {
    appearance::install(lua, globals, &render)?;
    motion::install(lua, globals, &render)?;
    world::install(lua, globals, &render)?;
    points::install(lua, globals, &render)
}
