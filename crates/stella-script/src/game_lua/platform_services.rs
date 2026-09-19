//! Platform-owned service tables registered by the GameLua constructor.

mod ads;
mod align;
mod analytics;
mod app_store_launcher;
mod apprater;
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
    pub(crate) apprater: AppraterRuntime,
    pub(crate) assets: AssetsRuntime,
    pub(crate) channel: ChannelRuntime,
    pub(crate) game_server: GameServerRuntime,
    pub(crate) gamer_services: GamerServicesRuntime,
    pub(crate) iap: IapRuntime,
    pub(crate) qr_scanner: QrScannerRuntime,
    pub(crate) skynest_account: SkynestAccountRuntime,
    pub(crate) skynest_storage: SkynestStorageRuntime,
    pub(crate) social: SocialRuntime,
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
    locale_runtime: Arc<Mutex<LocaleRuntime>>,
    application_events: ApplicationEventScheduler,
) -> LuaResult<InstalledPlatformServices> {
    let apprater = apprater::install(
        lua,
        globals,
        &data_root,
        Arc::clone(&render),
        Arc::clone(&resource_runtime),
        locale_runtime,
    )?;
    ads::install(lua, globals)?;
    app_store_launcher::install(lua, globals, Arc::clone(&data_root), Arc::clone(&render))?;
    force_update::install(lua, globals, Arc::clone(&data_root), Arc::clone(&render))?;
    let game_server = game_server::install(lua, globals, application_events.clone())?;
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
    analytics::install(lua, globals, Arc::clone(&render))?;
    let gamer_services_path =
        app_data_path(&data_root, "stella-gamer-services.json").map_err(runtime_error)?;
    let gamer_services = gamer_services::install(
        lua,
        globals,
        gamer_services_path,
        Arc::clone(&render),
        application_events.clone(),
    )?;
    let iap = iap::install(
        lua,
        globals,
        Arc::clone(&data_root),
        application_events.clone(),
    )?;
    let qr_scanner = qr_scanner::install(lua, globals, application_events.clone())?;
    zappar::install(lua, globals)?;
    let skynest_state_path =
        app_data_path(&data_root, "stella-services.json").map_err(runtime_error)?;
    let skynest_state = Arc::new(Mutex::new(skynest_account::OfflineState::new(
        skynest_state_path,
    )));
    let skynest_account = skynest_account::install(
        lua,
        globals,
        Arc::clone(&skynest_state),
        application_events.clone(),
    )?;
    let skynest_storage = skynest_storage::install(
        lua,
        globals,
        skynest_state,
        skynest_account.clone(),
        application_events.clone(),
    )?;
    let social_path = app_data_path(&data_root, "stella-social.json").map_err(runtime_error)?;
    let social = social::install(
        lua,
        globals,
        social_path,
        Arc::clone(&data_root),
        Arc::clone(&resource_runtime),
        application_events.clone(),
        social::SkynestServices {
            account: skynest_account.clone(),
            storage: skynest_storage.clone(),
        },
    )?;
    let assets = assets::install(
        lua,
        globals,
        Arc::clone(&data_root),
        Arc::clone(&resource_runtime),
        application_events.clone(),
    )?;
    let channel = channel::install(lua, globals, resource_runtime, application_events)?;
    game_lua::install_simple_random(lua, globals)?;
    align::install(lua, globals)?;
    Ok(InstalledPlatformServices {
        apprater,
        assets,
        channel,
        game_server,
        gamer_services,
        iap,
        qr_scanner,
        skynest_account,
        skynest_storage,
        social,
    })
}

pub(crate) use apprater::AppraterRuntime;

pub(crate) use assets::{AssetsRuntime, dispatch_completion as dispatch_assets_completion};
pub(crate) use channel::{
    ChannelRuntime, dispatch_content_completion as dispatch_channel_content_completion,
    dispatch_loading_failure as dispatch_channel_loading_failure,
};
pub(crate) use cloud_service::announce_registrations as announce_cloud_service_registrations;
pub(crate) use game_server::enable_shipped_facade as enable_shipped_game_server_facade;
pub(crate) use game_server::install_offline_facade as install_offline_game_server_facade;
pub(crate) use game_server::{
    GameServerRuntime, dispatch_completion as dispatch_game_server_completion,
    load_shipped_facade as load_shipped_game_server_facade,
};
pub(crate) use gamer_services::{
    GamerServicesRuntime,
    dispatch_authentication_completion as dispatch_gamer_services_authentication_completion,
    dispatch_platform_completions as dispatch_gamer_services_platform_completions,
};
pub(crate) use iap::{
    IapRuntime, complete_initialization as complete_iap_initialization,
    dispatch_completion as dispatch_iap_completion,
};
pub(crate) use qr_scanner::{QrScannerRuntime, dispatch_completion as dispatch_qr_completion};
pub(crate) use skynest_account::{
    SkynestAccountRuntime, dispatch_local_completion as dispatch_skynest_account_local_completion,
    dispatch_online_completion as dispatch_skynest_account_online_completion,
};
pub(crate) use skynest_storage::{
    SkynestStorageRuntime, dispatch_local_completion as dispatch_skynest_storage_local_completion,
    dispatch_online_completion as dispatch_skynest_storage_online_completion,
};
pub(crate) use social::{
    SocialRuntime, dispatch_local_completion as dispatch_social_local_completion,
    dispatch_online_completion as dispatch_social_online_completion,
};
