//! Ordered lifecycle phases of `game::LuaResources::LuaResources`.

mod creation;
mod loading;
mod release;

use crate::*;

pub(crate) fn install_creation(
    lua: &Lua,
    resource_api: &mlua::Table,
    resource_runtime: Arc<Mutex<ResourceRuntime>>,
    locale_runtime: Arc<Mutex<LocaleRuntime>>,
    data_root: Arc<PathBuf>,
) -> LuaResult<()> {
    creation::install(
        lua,
        resource_api,
        resource_runtime,
        locale_runtime,
        data_root,
    )
}

pub(crate) fn install_release(
    lua: &Lua,
    resource_api: &mlua::Table,
    resource_runtime: Arc<Mutex<ResourceRuntime>>,
    audio_runtime: Arc<Mutex<AudioRuntime>>,
    locale_runtime: Arc<Mutex<LocaleRuntime>>,
) -> LuaResult<()> {
    release::install(
        lua,
        resource_api,
        resource_runtime,
        audio_runtime,
        locale_runtime,
    )
}

pub(crate) use creation::create_sprite_sheet;
pub(crate) use loading::load_sprite_sheet_path;
pub(crate) use release::release_sprite_sheet;
