use super::*;

#[test]
fn definition_indexing_adds_native_index_and_group_metadata() {
    let lua = Lua::new();
    let definitions = lua.create_table().unwrap();
    let birds = lua.create_table().unwrap();
    let stella = lua.create_table().unwrap();
    stella.set("definition", "Stella").unwrap();
    stella.set("marker", 17).unwrap();
    birds.raw_set(3, stella.clone()).unwrap();
    let large_index = lua.create_table().unwrap();
    large_index.set("definition", "LargeIndex").unwrap();
    birds.raw_set(16_777_217_i64, large_index.clone()).unwrap();
    birds
        .set("named_entry", lua.create_table().unwrap())
        .unwrap();
    definitions.set("birds", birds).unwrap();

    let block_table = lua.create_table().unwrap();
    let existing = lua.create_table().unwrap();
    let retained = lua.create_table().unwrap();
    retained.set("marker", 41).unwrap();
    existing.set("Retained", retained).unwrap();
    block_table.set("blocks", existing).unwrap();

    // The native outer iterator only accepts string group keys.
    let numeric_group = lua.create_table().unwrap();
    let ignored = lua.create_table().unwrap();
    ignored.set("definition", "Ignored").unwrap();
    numeric_group.raw_set(1, ignored).unwrap();
    definitions.raw_set(9, numeric_group).unwrap();

    index_definition_lists(&lua, &block_table, &definitions).unwrap();

    assert_eq!(stella.get::<f64>("index").unwrap(), 3.0);
    assert_eq!(stella.get::<String>("group").unwrap(), "birds");
    assert_eq!(large_index.get::<f64>("index").unwrap(), 16_777_216.0);
    assert_eq!(large_index.get::<String>("group").unwrap(), "birds");
    let blocks = block_table.get::<mlua::Table>("blocks").unwrap();
    assert_eq!(
        blocks
            .get::<mlua::Table>("Stella")
            .unwrap()
            .get::<i64>("marker")
            .unwrap(),
        17
    );
    assert_eq!(
        blocks
            .get::<mlua::Table>("Retained")
            .unwrap()
            .get::<i64>("marker")
            .unwrap(),
        41
    );
    assert!(matches!(
        blocks.raw_get::<Value>("Ignored").unwrap(),
        Value::Nil
    ));
    assert!(matches!(
        block_table.raw_get::<Value>("birds").unwrap(),
        Value::Nil
    ));
}

#[test]
fn script_loader_routes_definition_packs_into_native_block_table_shape() {
    let unique = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let root = std::env::temp_dir().join(format!(
        "stella-definition-loader-{}-{unique}",
        std::process::id()
    ));
    fs::create_dir_all(&root).unwrap();
    fs::write(root.join("themes-one.lua"), b"old = 1; shared = 10").unwrap();
    fs::write(root.join("themes-two.lua"), b"fresh = 2; shared = 20").unwrap();
    fs::write(
        root.join("birds.lua"),
        b"birds = {{ definition = 'TestBird', marker = 33 }}",
    )
    .unwrap();
    fs::write(
            root.join("inherited.lua"),
            b"variants = { inheritsBlock('TestBird', IGNORE_COMPONENTS)({ definition = 'DerivedBird' }) }",
        )
        .unwrap();

    let runtime = StellaLua::new(&root).unwrap();
    runtime
        .execute_source(
            r#"
                assert(select('#', loadLuaFile(
                    "themes-one.lua", "themes", true, false
                )) == 0)
                assert(type(blockTable.themes) == "table")
                assert(rawget(gamelua, "themes") == nil)
                assert(blockTable.themes.old == 1)

                nativeBlockTable = blockTable
                blockTable = { replacement = true }

                assert(select('#', loadLuaFile(
                    "themes-two.lua", "themes", true, false
                )) == 0)
                assert(nativeBlockTable.themes.fresh == 2)
                assert(nativeBlockTable.themes.shared == 20)
                assert(rawget(nativeBlockTable.themes, "old") == nil)
                assert(blockTable.replacement == true)
                assert(rawget(blockTable, "themes") == nil)

                assert(select('#', loadLuaFile(
                    "birds.lua", "blockTable", true, true
                )) == 0)
                assert(nativeBlockTable.blocks.TestBird.marker == 33)
                assert(nativeBlockTable.blocks.TestBird.index == 1)
                assert(nativeBlockTable.blocks.TestBird.group == "birds")
                assert(rawget(nativeBlockTable, "birds") == nil)

                inheritsBlock = function(base, ignore_components)
                    return function(definition)
                        definition.__inheritedFrom = base
                        definition.__ignoreComponents = ignore_components
                        return definition
                    end
                end
                assert(select('#', loadLuaFile(
                    "inherited.lua", "blockTable", true, true
                )) == 0)
                assert(nativeBlockTable.blocks.DerivedBird.__inheritedFrom == "TestBird")
                assert(nativeBlockTable.blocks.DerivedBird.__ignoreComponents == true)
                assert(rawget(gamelua, "IGNORE_COMPONENTS") == nil)
                "#,
        )
        .unwrap();

    drop(runtime);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn object_script_loader_injects_raw_gamelua_only_into_named_child() {
    let unique = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let root = std::env::temp_dir().join(format!(
        "stella-object-loader-{}-{unique}",
        std::process::id()
    ));
    fs::create_dir_all(&root).unwrap();
    fs::write(root.join("named.lua"), b"marker = gamelua").unwrap();
    fs::write(root.join("direct.lua"), b"direct_marker = true").unwrap();

    let runtime = StellaLua::new(&root).unwrap();
    runtime
        .execute_source(
            r#"
                holder = {}
                assert(select('#', loadLuaFileToObject(
                    "named.lua", holder, "child"
                )) == 0)
                assert(rawget(holder.child, "gamelua") == gamelua)
                assert(holder.child.marker == gamelua)

                direct = {}
                assert(select('#', loadLuaFileToObject(
                    "direct.lua", direct, ""
                )) == 0)
                assert(direct.direct_marker == true)
                assert(rawget(direct, "gamelua") == nil)

                assert(not pcall(loadLuaFileToObject, "named.lua"))
                assert(not pcall(loadLuaFileToObject,
                    "named.lua", holder, 7
                ))
                assert(not pcall(loadLuaFileToObject,
                    "named.lua", holder, "bad", "true"
                ))
                -- sub_10005761C only reads slot 4 when the stack top is
                -- exactly four; later extras leave the native default set.
                assert(select('#', loadLuaFileToObject(
                    "named.lua", holder, "extra", "ignored", 1
                )) == 0)
                "#,
        )
        .unwrap();

    drop(runtime);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn json_import_populates_the_existing_named_table_with_void_abi() {
    let unique = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let root = std::env::temp_dir().join(format!(
        "stella-json-import-{}-{unique}",
        std::process::id()
    ));
    fs::create_dir_all(&root).unwrap();

    let runtime = StellaLua::new(&root).unwrap();
    runtime
        .execute_source(
            r#"
                imported = { retained = 9 }
                local identity = imported
                assert(select('#', importJSONToLuaTable(
                    "{\"name\":\"Stella\",\"items\":[1,true,{\"x\":2}]}",
                    "imported"
                )) == 0)
                assert(imported == identity)
                assert(imported.retained == 9)
                assert(imported.name == "Stella")
                assert(imported.items[1] == 1)
                assert(imported.items[2] == true)
                assert(imported.items[3].x == 2)

                assert(not pcall(importJSONToLuaTable, {}, "imported"))
                assert(not pcall(importJSONToLuaTable, "{}", "missing"))
                assert(not pcall(importJSONToLuaTable, "not-json", "imported"))
                "#,
        )
        .unwrap();

    drop(runtime);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn level_loader_snapshots_native_force_and_aim_stream_world_attributes() {
    let unique = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let root = std::env::temp_dir().join(format!(
        "stella-level-parameters-{}-{unique}",
        std::process::id()
    ));
    fs::create_dir_all(&root).unwrap();
    fs::write(
        root.join("explicit.lua"),
        b"filename = 'explicit.lua'; gravityForceMultiplier = '3.25'; waterForceMultiplier = '0xA'",
    )
    .unwrap();
    fs::write(root.join("defaults.lua"), b"filename = 'defaults.lua'").unwrap();
    fs::write(root.join("mismatch.lua"), b"filename = 'other.lua'").unwrap();

    let runtime = StellaLua::new(&root).unwrap();
    runtime
        .execute_source(
            r#"
                worldAttributes = {
                    defaultGravityForceMultiplier = "2.5",
                    defaultWaterForceMultiplier = "0x2",
                    simulationIterations = "4",
                    simulationTimeStepMultiplier = "1",
                    simulationStorePointsSampler = "1",
                    simulationAimSpawnTime = 0.25,
                    simulationAimSpeed = 3
                }
                deadBlocks = { generation = 1 }
                objects = { currentTimeStep = 0.1 }
                createCircle("BirdSimulation", "", 0, 0, 1, 1, 0, 0,
                    true, false, 1)
                setWorldGravity(0, 0)
                native_setAdditionalBirdGravity(0)
                setVelocity("BirdSimulation", 1, 0)
                updateBirdTrajectoryTable()
                populateAimingAid()
                enableAimingAid(true)
                startNewTrajectory()
                addToTrajectory(0, 12, 34)
                assert(select('#', loadLevel("explicit")) == 0)
                nativeDeadBlocks = deadBlocks
                "#,
        )
        .unwrap();
    {
        let bridge = runtime.render.lock().unwrap();
        assert_eq!(bridge.gravity_force_multiplier, 3.25);
        assert_eq!(bridge.water_force_multiplier, 10.0);
        assert_eq!(bridge.simulation_iterations, 4);
        assert_eq!(bridge.simulation_time_step_multiplier, 1.0);
        assert_eq!(bridge.simulation_store_points_sampler, 1);
        assert!(!bridge.aim_stream_active);
        assert_eq!(bridge.aim_stream_spawn_time, 0.25);
        assert_eq!(bridge.aim_stream_speed, 3.0);
        assert!(bridge.aim_stream_control_points.is_empty());
        assert!(bridge.aim_stream_particles.is_empty());
        assert_eq!(bridge.trajectory_stream_index, 0);
        assert!(bridge.trajectory_streams[0].points.is_empty());
        assert!(bridge.trajectory_streams[1].points.is_empty());
        assert!(bridge.trajectory_streams[0].normal_sprite.is_empty());
        assert!(bridge.trajectory_streams[1].normal_sprite.is_empty());
    }

    {
        let mut bridge = runtime.render.lock().unwrap();
        bridge.aim_stream_control_points = (0..12).map(|x| (f64::from(x), 0.0)).collect();
    }
    runtime
        .execute_source(
            r#"
                worldAttributes.simulationAimSpawnTime = 9
                worldAttributes.simulationAimSpeed = 9
                worldAttributes.simulationIterations = 90
                worldAttributes.simulationTimeStepMultiplier = 90
                worldAttributes.simulationStorePointsSampler = 90
                deadBlocks = { generation = 2 }
                shadowDeadBlocks = deadBlocks
                populateAimingAid()
            "#,
        )
        .unwrap();
    {
        let bridge = runtime.render.lock().unwrap();
        assert_eq!(bridge.aim_stream_spawn_time, 0.25);
        assert_eq!(bridge.aim_stream_speed, 3.0);
        assert_eq!(bridge.simulation_iterations, 4);
        assert_eq!(bridge.simulation_time_step_multiplier, 1.0);
        assert_eq!(bridge.simulation_store_points_sampler, 1);
        assert_eq!(bridge.aim_stream_spawn_timer, 0.25);
        assert_eq!(bridge.aim_stream_particles.len(), 12);
    }
    assert!(native_queue_dead_block(runtime.lua(), "BirdSimulation", 0.0).unwrap());
    {
        let environment = game_environment(runtime.lua()).unwrap();
        let retained = environment.get::<mlua::Table>("nativeDeadBlocks").unwrap();
        let shadow = environment.get::<mlua::Table>("shadowDeadBlocks").unwrap();
        assert!(matches!(
            retained.raw_get::<Value>("BirdSimulation").unwrap(),
            Value::Table(_)
        ));
        assert!(matches!(
            shadow.raw_get::<Value>("BirdSimulation").unwrap(),
            Value::Nil
        ));
    }

    runtime
        .execute_source(
            r#"
                local aimingBeforeReplacement = getAimingTime()
                worldAttributes = {
                    defaultGravityForceMultiplier = "2.5",
                    defaultWaterForceMultiplier = "0x2",
                    simulationIterations = 7,
                    simulationTimeStepMultiplier = 2,
                    simulationStorePointsSampler = 3,
                    simulationAimSpawnTime = 8,
                    simulationAimSpeed = 6
                }
                -- GameLua+0x4D0 still owns the preceding table until the
                -- next successful loadLevelImpl replacement.
                assert(getAimingTime() == aimingBeforeReplacement)
                assert(select('#', loadLevel("defaults")) == 0)
            "#,
        )
        .unwrap();
    {
        let bridge = runtime.render.lock().unwrap();
        assert_eq!(bridge.gravity_force_multiplier, 2.5);
        assert_eq!(bridge.water_force_multiplier, 2.0);
        assert_eq!(bridge.aim_stream_spawn_time, 8.0);
        assert_eq!(bridge.aim_stream_speed, 6.0);
        assert_eq!(bridge.simulation_iterations, 7);
        assert_eq!(bridge.simulation_time_step_multiplier, 2.0);
        assert_eq!(bridge.simulation_store_points_sampler, 3);
        assert_eq!(bridge.aim_stream_spawn_timer, 0.25);
        assert!(bridge.aim_stream_control_points.is_empty());
        assert!(bridge.aim_stream_particles.is_empty());
    }
    assert!(native_queue_dead_block(runtime.lua(), "BirdSimulation", 0.0).unwrap());
    {
        let environment = game_environment(runtime.lua()).unwrap();
        let shadow = environment.get::<mlua::Table>("shadowDeadBlocks").unwrap();
        assert!(matches!(
            shadow.raw_get::<Value>("BirdSimulation").unwrap(),
            Value::Table(_)
        ));
    }
    runtime
        .execute_source(
            r#"
                local ok = pcall(function() loadLevel("mismatch") end)
                assert(not ok)
                local suffixed_ok = pcall(function() loadLevel("explicit.lua") end)
                assert(not suffixed_ok)
                local missing_ok = pcall(loadLevel)
                assert(not missing_ok)
                local wrong_type_ok = pcall(loadLevel, false)
                assert(not wrong_type_ok)
                "#,
        )
        .unwrap();

    drop(runtime);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn level_loader_resets_the_fixed_step_remainder_before_the_new_level() {
    let unique = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let root = std::env::temp_dir().join(format!(
        "stella-level-physics-clock-{}-{unique}",
        std::process::id()
    ));
    fs::create_dir_all(&root).unwrap();
    fs::write(root.join("clock.lua"), b"filename = 'clock.lua'").unwrap();

    let runtime = StellaLua::new(&root).unwrap();
    runtime.render.lock().unwrap().physics_accumulator = 1.0_f32 / 60.0_f32;
    runtime.execute_source("loadLevel('clock')").unwrap();
    assert_eq!(runtime.render.lock().unwrap().physics_accumulator, 0.0);

    runtime
        .execute_source(
            r#"
                g_outOfBoundariesObjects = {}
                physics_steps = 0
                updatePhysics = function()
                    physics_steps = physics_steps + 1
                end
                removeBlocks = function() end
                clearLuaForceFunctions = function() end
                update = function() end
                setPhysicsEnabled(true)
            "#,
        )
        .unwrap();

    runtime.update(1.0 / 60.0).unwrap();
    let environment = game_environment(runtime.lua()).unwrap();
    assert_eq!(environment.get::<i64>("physics_steps").unwrap(), 0);
    runtime.update(1.0 / 60.0).unwrap();
    assert_eq!(environment.get::<i64>("physics_steps").unwrap(), 1);

    drop(runtime);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn level_loaders_invalidate_native_rolling_handles_before_opening_the_file() {
    let unique = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let root = std::env::temp_dir().join(format!(
        "stella-level-rolling-handles-{}-{unique}",
        std::process::id()
    ));
    let data_root = root.join("data");
    let app_data_root = root.join("appdata");
    fs::create_dir_all(&data_root).unwrap();
    fs::create_dir_all(&app_data_root).unwrap();
    fs::write(data_root.join("bundle.lua"), b"filename = 'bundle.lua'").unwrap();
    fs::write(
        app_data_root.join("saved.lua"),
        b"filename = 'saved.lua'; world = {}",
    )
    .unwrap();

    let runtime = StellaLua::new(&data_root).unwrap();
    runtime.render.lock().unwrap().rolling_audio_handles = [11, 12, 13];
    runtime.execute_source("loadLevel('bundle')").unwrap();
    assert_eq!(
        runtime.render.lock().unwrap().rolling_audio_handles,
        [-1; 3]
    );

    runtime.render.lock().unwrap().rolling_audio_handles = [21, 22, 23];
    runtime
        .execute_source("loadLevelFromAppData('saved')")
        .unwrap();
    assert_eq!(
        runtime.render.lock().unwrap().rolling_audio_handles,
        [-1; 3]
    );

    runtime.render.lock().unwrap().rolling_audio_handles = [31, 32, 33];
    runtime
        .execute_source("bundle_missing = not pcall(loadLevel, 'missing')")
        .unwrap();
    assert!(
        game_environment(runtime.lua())
            .unwrap()
            .get::<bool>("bundle_missing")
            .unwrap()
    );
    assert_eq!(
        runtime.render.lock().unwrap().rolling_audio_handles,
        [-1; 3]
    );

    runtime.render.lock().unwrap().rolling_audio_handles = [41, 42, 43];
    runtime
        .execute_source("app_data_missing = not pcall(loadLevelFromAppData, 'missing')")
        .unwrap();
    assert!(
        game_environment(runtime.lua())
            .unwrap()
            .get::<bool>("app_data_missing")
            .unwrap()
    );
    assert_eq!(
        runtime.render.lock().unwrap().rolling_audio_handles,
        [-1; 3]
    );

    drop(runtime);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn level_loaders_clear_only_active_native_particles_before_opening_the_file() {
    let unique = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let root = std::env::temp_dir().join(format!(
        "stella-level-active-particles-{}-{unique}",
        std::process::id()
    ));
    let data_root = root.join("data");
    let app_data_root = root.join("appdata");
    fs::create_dir_all(&data_root).unwrap();
    fs::create_dir_all(&app_data_root).unwrap();
    fs::write(data_root.join("bundle.lua"), b"filename = 'bundle.lua'").unwrap();
    fs::write(
        app_data_root.join("saved.lua"),
        b"filename = 'saved.lua'; world = {}",
    )
    .unwrap();

    let runtime = StellaLua::new(&data_root).unwrap();
    runtime
        .execute_source(
            r#"
                particleTable = { particles = { retained = {
                    amount=1, sprites={"RED_CROSS"}, lifeTime=10,
                    gravityX=0, gravityY=0,
                    minVel=0, maxVel=0,
                    minAngleEmitter=0, maxAngleEmitter=0,
                    minAngle=0, maxAngle=0,
                    minAngleVel=0, maxAngleVel=0,
                    minScaleBegin=1, maxScaleBegin=1,
                    minScaleEnd=1, maxScaleEnd=1
                } } }
                function seed_native_particle()
                    particles.native_addParticlesWithMode({
                        definitionName="retained", amount=1,
                        x=0, y=0, w=0, h=0, angle=0, mode=1
                    })
                end
            "#,
        )
        .unwrap();
    runtime.render.lock().unwrap().particle_system.scale = 2.75;

    for load in [
        "loadLevel('bundle')",
        "loadLevelFromAppData('saved')",
        "assert(not pcall(loadLevel, 'missing'))",
        "assert(not pcall(loadLevelFromAppData, 'missing'))",
    ] {
        runtime.execute_source("seed_native_particle()").unwrap();
        assert_eq!(
            runtime
                .render
                .lock()
                .unwrap()
                .particle_system
                .particles
                .len(),
            1
        );
        runtime.execute_source(load).unwrap();
        let bridge = runtime.render.lock().unwrap();
        assert!(bridge.particle_system.particles.is_empty(), "{load}");
        assert_eq!(bridge.particle_system.definitions.len(), 1, "{load}");
        assert_eq!(bridge.particle_system.scale, 2.75, "{load}");
    }

    drop(runtime);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn rejects_parent_traversal() {
    let error = resolve_script(Path::new("/tmp"), "../secret.lua").unwrap_err();
    assert!(matches!(error, ScriptError::UnsafePath(_)));
}
