//! Ordered native binding installation matching `GameLua::GameLua`.

use crate::*;

mod clip_text;
mod compatibility;
mod notifications;

pub(crate) struct InstalledRuntimes {
    pub(crate) resources: Arc<Mutex<ResourceRuntime>>,
    pub(crate) audio: Arc<Mutex<AudioRuntime>>,
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
    install_platform_service_tables(
        lua,
        &globals,
        Arc::clone(&data_root),
        Arc::clone(&missing),
        Arc::clone(&render),
        Arc::clone(&animation_runtime),
        Arc::clone(&resource_runtime),
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

    install_time_bindings(lua, &globals)?;

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
    )?;

    game_lua::install_object_api(
        lua,
        &globals,
        Arc::clone(&render),
        Arc::clone(&resource_runtime),
        Arc::clone(&data_root),
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
    game_lua::install_platform(lua, &globals, &render)?;

    notifications::install(lua, &globals)?;

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
    )?;
    Ok(InstalledRuntimes {
        resources: resource_runtime,
        audio: audio_runtime,
    })
}
