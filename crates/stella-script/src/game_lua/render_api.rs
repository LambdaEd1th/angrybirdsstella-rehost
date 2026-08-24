//! GameLua theme and immediate-render adapters registered by Purple.

use std::sync::{Arc, Mutex};

use mlua::{Lua, Result as LuaResult, Table};

use crate::*;

pub(crate) struct RegistrationContext {
    pub(crate) render: Arc<Mutex<RenderBridge>>,
    pub(crate) resource_runtime: Arc<Mutex<ResourceRuntime>>,
    pub(crate) locale_runtime: Arc<Mutex<LocaleRuntime>>,
    pub(crate) data_root: Arc<PathBuf>,
    pub(crate) libc_random: Arc<Mutex<NativeLibcRandom>>,
}

pub(crate) fn install(lua: &Lua, globals: &Table, context: RegistrationContext) -> LuaResult<()> {
    let RegistrationContext {
        render,
        resource_runtime,
        locale_runtime,
        data_root,
        libc_random,
    } = context;
    install_theme_render_bindings(
        lua,
        globals,
        Arc::clone(&render),
        Arc::clone(&resource_runtime),
        Arc::clone(&data_root),
        libc_random,
    )?;

    install_primitive_render_bindings(lua, globals, Arc::clone(&render))?;
    install_ui_text_bindings(
        lua,
        globals,
        Arc::clone(&render),
        Arc::clone(&resource_runtime),
        Arc::clone(&locale_runtime),
        Arc::clone(&data_root),
    )?;
    install_textured_render_bindings(
        lua,
        globals,
        Arc::clone(&render),
        Arc::clone(&resource_runtime),
        Arc::clone(&locale_runtime),
        Arc::clone(&data_root),
    )?;
    install_direct_sprite_bindings(
        lua,
        globals,
        Arc::clone(&render),
        Arc::clone(&resource_runtime),
        data_root,
    )?;
    Ok(())
}
