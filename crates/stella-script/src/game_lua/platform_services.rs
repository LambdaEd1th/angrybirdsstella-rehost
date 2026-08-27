//! Platform-owned service tables registered by the GameLua constructor.

mod ads;
mod align;
mod analytics;
mod app_store_launcher;
mod assets;
mod channel;
mod cloud_service;
mod force_update;
mod game_server;
mod gamer_services;
mod iap;
mod qr_scanner;
mod skynest_account;
mod skynest_storage;
mod social;
mod zappar;

use crate::*;

pub(crate) struct InstalledPlatformServices {
    pub(crate) assets: AssetsRuntime,
    pub(crate) game_server: GameServerRuntime,
    pub(crate) gamer_services: GamerServicesRuntime,
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn install(
    lua: &Lua,
    globals: &mlua::Table,
    data_root: Arc<PathBuf>,
    missing: Arc<Mutex<BTreeSet<String>>>,
    render: Arc<Mutex<RenderBridge>>,
    animation_runtime: Arc<Mutex<AnimationRuntime>>,
    resource_runtime: Arc<Mutex<ResourceRuntime>>,
) -> LuaResult<InstalledPlatformServices> {
    ads::install(lua, globals)?;
    app_store_launcher::install(lua, globals, Arc::clone(&data_root), Arc::clone(&render))?;
    force_update::install(lua, globals, Arc::clone(&render))?;
    let game_server = game_server::install(lua, globals)?;
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
    let gamer_services = gamer_services::install(lua, globals)?;
    iap::install(lua, globals, Arc::clone(&data_root))?;
    qr_scanner::install(lua, globals)?;
    zappar::install(lua, globals)?;
    let skynest_state = Arc::new(Mutex::new(skynest_account::OfflineState::default()));
    skynest_account::install(lua, globals, Arc::clone(&skynest_state))?;
    skynest_storage::install(lua, globals, skynest_state)?;
    social::install(lua, globals)?;
    let assets = assets::install(
        lua,
        globals,
        Arc::clone(&data_root),
        Arc::clone(&resource_runtime),
    )?;
    channel::install(lua, globals, resource_runtime)?;
    game_lua::install_simple_random(lua, globals)?;
    align::install(lua, globals)?;
    Ok(InstalledPlatformServices {
        assets,
        game_server,
        gamer_services,
    })
}

pub(crate) use assets::{AssetsRuntime, dispatch_completions as dispatch_assets_completions};
pub(crate) use cloud_service::announce_registrations as announce_cloud_service_registrations;
pub(crate) use game_server::install_offline_facade as install_offline_game_server_facade;
pub(crate) use game_server::{
    GameServerRuntime, dispatch_completions as dispatch_game_server_completions,
    load_shipped_facade as load_shipped_game_server_facade,
};
pub(crate) use gamer_services::{
    GamerServicesRuntime, dispatch_completions as dispatch_gamer_services_completions,
};
pub(crate) use iap::complete_initialization as complete_iap_initialization;
pub(crate) use qr_scanner::{set_host_available as set_qr_scanner_available, submit_host_code};
