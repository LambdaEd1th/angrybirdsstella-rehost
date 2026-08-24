//! Ordered coordinator for native ThemeSystem, ThemeSprite and ThemeAnimation bindings.

use super::{theme_animation_registration, theme_sprite_registration, theme_system_registration};
use crate::*;

pub(crate) fn install(
    lua: &Lua,
    globals: &mlua::Table,
    render: Arc<Mutex<RenderBridge>>,
    resources: Arc<Mutex<ResourceRuntime>>,
    data_root: Arc<PathBuf>,
) -> LuaResult<()> {
    theme_system_registration::install(lua, globals, Arc::clone(&render), resources, data_root)?;
    theme_sprite_registration::install(lua, globals, Arc::clone(&render))?;
    theme_animation_registration::install(lua, globals, render)
}
