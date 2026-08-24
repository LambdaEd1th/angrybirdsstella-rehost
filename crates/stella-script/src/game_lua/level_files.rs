//! Ordered coordinator for Purple's native level-file registration cluster.

use super::{
    level_editor_registration, level_failure_registration, level_load_registration,
    level_save_registration,
};
use crate::*;

pub(crate) fn install(
    lua: &Lua,
    globals: &mlua::Table,
    data_root: Arc<PathBuf>,
    render: Arc<Mutex<RenderBridge>>,
    draw_callbacks: Rc<RefCell<DrawCallbacks>>,
) -> LuaResult<()> {
    level_editor_registration::install(lua, globals, Arc::clone(&data_root))?;
    level_load_registration::install(lua, globals, Arc::clone(&data_root), render, draw_callbacks)?;
    level_save_registration::install(lua, globals, data_root)?;
    level_failure_registration::install(lua, globals)
}
