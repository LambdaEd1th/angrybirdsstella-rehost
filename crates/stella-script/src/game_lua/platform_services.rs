//! Platform-owned service tables registered by the GameLua constructor.

mod align;
mod analytics;
mod assets;
mod cloud_service;
mod force_update;
mod game_server;
mod gamer_services;
mod social;

use crate::*;

#[allow(clippy::too_many_arguments)]
pub(crate) fn install(
    lua: &Lua,
    globals: &mlua::Table,
    data_root: Arc<PathBuf>,
    missing: Arc<Mutex<BTreeSet<String>>>,
    render: Arc<Mutex<RenderBridge>>,
    animation_runtime: Arc<Mutex<AnimationRuntime>>,
    resource_runtime: Arc<Mutex<ResourceRuntime>>,
) -> LuaResult<()> {
    force_update::install(lua, globals, Arc::clone(&render))?;
    animation_wrapper::install(
        lua,
        globals,
        animation_wrapper::RegistrationContext {
            animation_runtime: Arc::clone(&animation_runtime),
            data_root: Arc::clone(&data_root),
            render: Arc::clone(&render),
            resource_runtime: Arc::clone(&resource_runtime),
            missing: Arc::clone(&missing),
        },
    )?;
    analytics::install(lua, globals)?;
    gamer_services::install(lua, globals)?;
    social::install(lua, globals)?;
    assets::install(lua, globals, Arc::clone(&data_root), resource_runtime)?;
    game_lua::install_simple_random(lua, globals)?;
    align::install(lua, globals)?;
    Ok(())
}

pub(crate) use cloud_service::announce_registrations as announce_cloud_service_registrations;
pub(crate) use game_server::install_offline_facade as install_offline_game_server_facade;
