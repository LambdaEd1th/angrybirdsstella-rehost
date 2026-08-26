//! Bundle/AppData level wrappers around native `sub_100065D3C`.

use crate::*;

pub(crate) fn install(
    lua: &Lua,
    globals: &mlua::Table,
    data_root: Arc<PathBuf>,
    render: Arc<Mutex<RenderBridge>>,
    draw_callbacks: Rc<RefCell<DrawCallbacks>>,
) -> LuaResult<()> {
    // sub_10004715C forwards to sub_100065D3C with selector zero.
    let bundle_root = Arc::clone(&data_root);
    let bundle_bridge = Arc::clone(&render);
    let bundle_callbacks = Rc::clone(&draw_callbacks);
    globals.set(
        "loadLevel",
        lua.create_function(move |lua, args: MultiValue| {
            let mut requested = native_required_string(&args, 0, "loadLevel")?;
            clear_native_level_owner(&bundle_bridge, &bundle_callbacks);
            // sub_100065D3C appends this suffix unconditionally. Passing an
            // already suffixed name therefore probes `.lua.lua` and fails.
            requested.push_str(".lua");
            let environment = lua.create_table()?;
            install_table_fallback(lua, &environment, game_environment(lua)?)?;
            execute_script_in(lua, &bundle_root, &requested, environment.clone())?;
            if std::env::var_os("STELLA_TRACE_EMPTY_LEVEL_WORLD").is_some() {
                environment.set("world", lua.create_table()?)?;
            }

            let requested_filename = Path::new(&requested)
                .file_name()
                .and_then(|value| value.to_str())
                .ok_or_else(|| runtime_error("Filename is missing from level file"))?;
            let loaded_filename = environment
                .get::<Value>("filename")
                .ok()
                .as_ref()
                .and_then(value_string)
                .ok_or_else(|| runtime_error("Filename is missing from level file"))?;
            if loaded_filename != requested_filename {
                return Err(runtime_error(format!(
                    "level filename mismatch: expected {requested_filename}, got {loaded_filename}"
                )));
            }
            publish_loaded_level(lua, environment, &bundle_bridge)
        })?,
    )?;

    // sub_100047234 calls the same implementation with selector one. The
    // unsuffixed fallback remains solely for migration of early rehost saves.
    let app_root = data_root;
    let app_callbacks = draw_callbacks;
    globals.set(
        "loadLevelFromAppData",
        lua.create_function(move |lua, args: MultiValue| {
            let requested = native_required_string(&args, 0, "loadLevelFromAppData")?;
            clear_native_level_owner(&render, &app_callbacks);
            let native_name = with_lua_extension(requested.clone());
            let native_path = app_data_path(&app_root, &native_name).map_err(runtime_error)?;
            let path = if native_path.is_file() {
                native_path
            } else {
                app_data_path(&app_root, &requested)
                    .ok()
                    .filter(|path| path.is_file())
                    .ok_or_else(|| {
                        runtime_error(format!("AppData level not found: {native_name}"))
                    })?
            };
            let environment = load_saved_lua_table(lua, &path)?;
            install_table_fallback(lua, &environment, game_environment(lua)?)?;
            publish_loaded_level(lua, environment, &render)
        })?,
    )?;
    Ok(())
}

fn clear_native_level_owner(
    render: &Arc<Mutex<RenderBridge>>,
    draw_callbacks: &Rc<RefCell<DrawCallbacks>>,
) {
    let mut bridge = render.lock().expect("render bridge lock poisoned");
    bridge.clear_native_level_scene();
    // loadLevelImpl calls sub_1000675C0 at 0x100066228, before executing the
    // requested level chunk. Retaining these records leaked the previous
    // attempt's flight dots through retry and into the next level.
    bridge.reset_native_flight_trails_for_level_load();
    drop(bridge);
    let mut callbacks = draw_callbacks.borrow_mut();
    callbacks.clear_records();
    callbacks.object_world_identity = None;
}

fn publish_loaded_level(
    lua: &Lua,
    environment: mlua::Table,
    render: &Arc<Mutex<RenderBridge>>,
) -> LuaResult<()> {
    let game = game_environment(lua)?;
    game.set("loadedObjects", environment.clone())?;
    lua.globals().set("loadedObjects", environment.clone())?;

    let dead_blocks = match game.get::<Value>("deadBlocks")? {
        Value::Table(table) => Some(table),
        _ => None,
    };
    // loadLevelImpl resolves `deadBlocks` by name and copies that LuaObject
    // into GameLua+0x4A8 at 0x100065DEC..0x100065E14. Native collision and
    // delayed-destruction paths subsequently write through the retained
    // object even if Lua replaces the same-name global.
    retain_native_lua_object(lua, NativeLuaObject::DeadBlocks, dead_blocks.as_ref())?;
    let world_attributes = match game.get::<Value>("worldAttributes")? {
        Value::Table(table) => Some(table),
        _ => None,
    };
    // loadLevelImpl resolves and replaces GameLua+0x4D0's LuaObject before
    // reading the five simulation/AimStream attributes.
    retain_native_lua_object(
        lua,
        NativeLuaObject::WorldAttributes,
        world_attributes.as_ref(),
    )?;
    let (iterations, time_step_multiplier, point_sampler, aim_spawn_time, aim_speed) =
        native_trajectory_level_settings(lua)?;
    let level_number = |field: &str, default_field: &str| -> Option<f64> {
        environment
            .get::<Value>(field)
            .ok()
            .as_ref()
            .and_then(native_lua51_number)
            .or_else(|| {
                world_attributes
                    .as_ref()
                    .and_then(|table| table.get::<Value>(default_field).ok())
                    .as_ref()
                    .and_then(native_lua51_number)
            })
            .map(|value| f64::from(value as f32))
    };
    let mut bridge = render.lock().expect("render bridge lock poisoned");
    if let Some(value) = level_number("gravityForceMultiplier", "defaultGravityForceMultiplier") {
        bridge.gravity_force_multiplier = value;
    }
    if let Some(value) = level_number("waterForceMultiplier", "defaultWaterForceMultiplier") {
        bridge.water_force_multiplier = value;
    }
    // loadLevelImpl copies the three BirdSimulation scalars to GameLua and
    // both AimStream scalars to the stream before resetting/deactivating it.
    // Later Lua mutations must not reconfigure the already loaded level.
    bridge.load_native_simulation_settings(iterations, time_step_multiplier, point_sampler);
    bridge.load_native_aim_stream_settings(aim_spawn_time, aim_speed);
    Ok(())
}
