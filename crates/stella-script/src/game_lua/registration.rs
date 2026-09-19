//! Ordered native binding installation matching `GameLua::GameLua`.

use crate::*;

mod clip_text;
mod compatibility;
mod device_identity;
mod notifications;

pub(crate) struct InstalledRuntimes {
    pub(crate) apprater: AppraterRuntime,
    pub(crate) application_event_dispatcher: ApplicationEventDispatcher,
    pub(crate) resources: Arc<Mutex<ResourceRuntime>>,
    pub(crate) audio: Arc<Mutex<AudioRuntime>>,
    pub(crate) installed_apps: InstalledAppsRuntime,
    pub(crate) assets: AssetsRuntime,
    pub(crate) channel: ChannelRuntime,
    pub(crate) game_server: GameServerRuntime,
    pub(crate) gamer_services: GamerServicesRuntime,
    pub(crate) iap: IapRuntime,
    pub(crate) qr_scanner: QrScannerRuntime,
    pub(crate) skynest_account: SkynestAccountRuntime,
    pub(crate) skynest_storage: SkynestStorageRuntime,
    pub(crate) social: SocialRuntime,
    pub(crate) server_time: ServerTimeRuntime,
    pub(crate) notifications: notifications::NotificationRuntime,
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn install_base_globals(
    lua: &Lua,
    screen_width: u32,
    screen_height: u32,
    data_root: Arc<PathBuf>,
    missing: Arc<Mutex<BTreeSet<String>>>,
    fallback_calls: Arc<Mutex<BTreeSet<String>>>,
    compatibility_bindings: Arc<Mutex<BTreeSet<String>>>,
    libc_random: Arc<Mutex<NativeLibcRandom>>,
    render: Arc<Mutex<RenderBridge>>,
    animation_runtime: Arc<Mutex<AnimationRuntime>>,
    draw_callbacks: Rc<RefCell<DrawCallbacks>>,
    track_missing_globals: bool,
) -> LuaResult<InstalledRuntimes> {
    let globals = lua.globals();
    let bitmap_font_assets = Arc::new(load_bitmap_fonts(&data_root));
    let localized_strings = load_localized_strings(&data_root, "en_EN");
    let locale_runtime = Arc::new(Mutex::new(LocaleRuntime {
        current: "en_EN".to_owned(),
        loaded: BTreeMap::from([(
            "en_EN".to_owned(),
            BTreeMap::from([("TEXTS_BASIC".to_owned(), localized_strings)]),
        )]),
    }));
    let resource_runtime = Arc::new(Mutex::new(ResourceRuntime::new(
        screen_width,
        screen_height,
    )));
    let audio_runtime = Arc::new(Mutex::new(AudioRuntime::default()));
    let application_events = ApplicationEventScheduler::default();
    // Lua's stock library was built for the host's `lua_Number` and libc.
    // Purple replaces these two members with float32 wrappers around iOS
    // `rand`/`srand` before any game bytecode is evaluated.
    install_math_random(lua, &globals, Arc::clone(&libc_random))?;
    // These exact relative paths are assigned by the platform constructor at
    // 0x10002A278 and injected into Lua by 0x100026D2C.
    globals.set("imagePath", "images")?;
    globals.set("fontPath", "fonts")?;
    globals.set("audioPath", "audio")?;
    globals.set("localizationPath", "localization")?;
    globals.set("levelPath", "levels")?;
    globals.set("scriptPath", "scripts")?;
    globals.set("commonScriptPath", "scripts_common")?;
    globals.set("configPath", "config")?;
    // Purple does not publish physicsScale/worldScale from its native
    // constructor. The shipped common gamelogic chunk owns both globals and
    // initializes them in the retained GameLua environment.
    // The recovered 1.1.6 data set contains the `ios` camera profile. Keep
    // script-facing platform selection faithful even when the Rust host runs
    // on Windows, macOS, or Linux.
    globals.set("deviceModel", "ios")?;
    globals.set("deviceInfoModel", native_device_info_model())?;
    device_identity::install(&globals, &data_root)?;
    // GameApp virtual slot +0xC8 publishes platform mouse availability. The
    // iOS 1.1.6 target has touch input but no platform mouse.
    globals.set("g_mouseAvailable", false)?;
    resource_manager::install(
        lua,
        &globals,
        resource_manager::RegistrationContext {
            missing: Arc::clone(&missing),
            render: Arc::clone(&render),
            resource_runtime: Arc::clone(&resource_runtime),
            locale_runtime: Arc::clone(&locale_runtime),
            audio_runtime: Arc::clone(&audio_runtime),
            data_root: Arc::clone(&data_root),
            bitmap_font_assets: Arc::clone(&bitmap_font_assets),
        },
    )?;
    let platform_services = install_platform_service_tables(
        lua,
        &globals,
        Arc::clone(&data_root),
        Arc::clone(&missing),
        Arc::clone(&render),
        Arc::clone(&animation_runtime),
        Arc::clone(&resource_runtime),
        Arc::clone(&locale_runtime),
        application_events.clone(),
    )?;
    clip_text::install(
        lua,
        &globals,
        Arc::clone(&resource_runtime),
        Arc::clone(&bitmap_font_assets),
        Arc::clone(&locale_runtime),
    )?;
    install_particle_bindings(
        lua,
        &globals,
        Arc::clone(&render),
        Arc::clone(&resource_runtime),
        Arc::clone(&data_root),
    )?;
    install_bootstrap_globals(lua, &globals, screen_width, screen_height, &data_root)?;

    install_loader_bindings(lua, &globals, Arc::clone(&data_root))?;
    // GameServerConnection's C++ constructor loads this common facade
    // immediately after publishing its two native members. The Rust host must
    // first construct the retained GameLua environment and loader, so this is
    // the earliest equivalent point in its split registration pipeline.
    load_shipped_game_server_facade(lua, &data_root)?;

    let server_time = install_time_bindings(lua, &globals, application_events.clone())?;

    install_world_bindings(
        lua,
        &globals,
        Arc::clone(&render),
        Arc::clone(&resource_runtime),
        Arc::clone(&data_root),
    )?;
    install_draw_bindings(
        lua,
        &globals,
        Arc::clone(&render),
        Rc::clone(&draw_callbacks),
        Arc::clone(&animation_runtime),
        Arc::clone(&resource_runtime),
        Arc::clone(&data_root),
    )?;

    install_trajectory_bindings(
        lua,
        &globals,
        Arc::clone(&render),
        Arc::clone(&resource_runtime),
        Arc::clone(&data_root),
    )?;

    game_lua::install_audio_bindings(
        lua,
        &globals,
        Arc::clone(&resource_runtime),
        Arc::clone(&audio_runtime),
    )?;

    physics_world::install_bindings(
        lua,
        &globals,
        Arc::clone(&render),
        Arc::clone(&resource_runtime),
        Arc::clone(&data_root),
        Rc::clone(&draw_callbacks),
    )?;

    game_lua::install_object_api(
        lua,
        &globals,
        Arc::clone(&render),
        Arc::clone(&resource_runtime),
        Arc::clone(&data_root),
        Rc::clone(&draw_callbacks),
    )?;

    game_lua::install_render_api(
        lua,
        &globals,
        game_lua::RenderRegistrationContext {
            render: Arc::clone(&render),
            resource_runtime: Arc::clone(&resource_runtime),
            locale_runtime: Arc::clone(&locale_runtime),
            data_root: Arc::clone(&data_root),
            libc_random: Arc::clone(&libc_random),
        },
    )?;
    let platform = game_lua::install_platform(lua, &globals, &render, application_events.clone())?;

    let notifications = notifications::install(lua, &globals)?;

    game_lua::install_string_loader(lua, &globals, &data_root)?;
    game_lua::install_table_files(lua, &globals, &data_root)?;
    game_lua::install_level_files(
        lua,
        &globals,
        Arc::clone(&data_root),
        Arc::clone(&render),
        Rc::clone(&draw_callbacks),
    )?;

    physics_world::install_extensions(
        lua,
        &globals,
        Arc::clone(&render),
        Arc::clone(&resource_runtime),
        Arc::clone(&data_root),
    )?;

    game_lua::install_theme_objects(
        lua,
        &globals,
        Arc::clone(&render),
        Arc::clone(&resource_runtime),
        Arc::clone(&data_root),
    )?;

    compatibility::install(
        lua,
        &globals,
        data_root,
        missing,
        fallback_calls,
        compatibility_bindings,
        track_missing_globals,
    )?;
    let application_event_dispatcher = ApplicationEventDispatcher::new(
        application_events,
        platform.url_requests.clone(),
        platform_services.game_server.clone(),
        server_time.clone(),
        platform_services.gamer_services.clone(),
        platform_services.social.clone(),
        platform_services.assets.clone(),
        platform_services.channel.clone(),
        platform_services.iap.clone(),
        platform_services.qr_scanner.clone(),
        platform_services.skynest_account.clone(),
        platform_services.skynest_storage.clone(),
    );
    install_application_event_dispatcher(lua, application_event_dispatcher.clone())?;
    Ok(InstalledRuntimes {
        apprater: platform_services.apprater,
        application_event_dispatcher,
        resources: resource_runtime,
        audio: audio_runtime,
        installed_apps: platform.installed_apps,
        assets: platform_services.assets,
        channel: platform_services.channel,
        game_server: platform_services.game_server,
        gamer_services: platform_services.gamer_services,
        iap: platform_services.iap,
        qr_scanner: platform_services.qr_scanner,
        skynest_account: platform_services.skynest_account,
        skynest_storage: platform_services.skynest_storage,
        social: platform_services.social,
        server_time,
        notifications,
    })
}

pub(crate) use notifications::{
    NotificationRuntime, dispatch_callback as dispatch_notification_callback,
    dispatch_due_callbacks as dispatch_due_notification_callbacks,
};
