//! Ordered facade for `LuaResources` draw, capture and platform-tail members.

mod capture;
mod sprite;
mod text;

use crate::*;

pub(crate) use capture::{install_capture, install_open_url_and_publish};

pub(crate) fn install_draw(
    lua: &Lua,
    resource_api: &mlua::Table,
    render: Arc<Mutex<RenderBridge>>,
    resource_runtime: Arc<Mutex<ResourceRuntime>>,
    locale_runtime: Arc<Mutex<LocaleRuntime>>,
    data_root: Arc<PathBuf>,
) -> LuaResult<()> {
    sprite::install(
        lua,
        resource_api,
        Arc::clone(&render),
        Arc::clone(&resource_runtime),
        Arc::clone(&data_root),
    )?;
    text::install(
        lua,
        resource_api,
        render,
        resource_runtime,
        locale_runtime,
        data_root,
    )
}
