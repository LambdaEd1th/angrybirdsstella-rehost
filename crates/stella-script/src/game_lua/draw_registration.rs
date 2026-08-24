//! Ordered facade for native draw registration families.

mod lines;
mod misc;
mod scene;

use crate::*;

pub(crate) fn install(
    lua: &Lua,
    globals: &mlua::Table,
    render: Arc<Mutex<RenderBridge>>,
    draw_callbacks: Rc<RefCell<DrawCallbacks>>,
    animation_runtime: Arc<Mutex<AnimationRuntime>>,
    resource_runtime: Arc<Mutex<ResourceRuntime>>,
    data_root: Arc<PathBuf>,
) -> LuaResult<()> {
    misc::install(
        lua,
        globals,
        Arc::clone(&render),
        Rc::clone(&draw_callbacks),
    )?;
    scene::install(
        lua,
        globals,
        Arc::clone(&render),
        draw_callbacks,
        animation_runtime,
        Arc::clone(&resource_runtime),
        Arc::clone(&data_root),
    )?;
    lines::install(lua, globals, render, resource_runtime, data_root)
}
