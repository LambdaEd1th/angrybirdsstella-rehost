use super::*;

fn native_atlas_quad(command: &RenderCommand) -> [[f64; 2]; 4] {
    let Some(SpriteGeometrySubmission::NativeAtlasQuad(quad)) = command.geometry.as_ref() else {
        panic!("command did not retain a native atlas quad");
    };
    **quad
}

fn load_native_simulation_settings(
    runtime: &StellaLua,
    iterations: i32,
    time_step_multiplier: f32,
    point_sampler: i32,
) {
    runtime
        .render
        .lock()
        .unwrap()
        .load_native_simulation_settings(iterations, time_step_multiplier, point_sampler);
}

#[test]
fn trajectory_draw_submission_retains_the_resolved_atlas_across_shadow_and_release() {
    let unique = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let root = std::env::temp_dir().join(format!("stella-trajectory-pointer-{unique}"));
    let data_root = root.join("data");
    for directory in ["first", "second"] {
        fs::create_dir_all(data_root.join(directory)).unwrap();
    }
    fs::write(
        data_root.join("first/FIRST.dat"),
        test_textured_sprite_sheet("TRAIL", "first.pvr", 10, 20),
    )
    .unwrap();
    fs::write(data_root.join("first/first.pvr"), []).unwrap();
    fs::write(
        data_root.join("second/SECOND.dat"),
        test_textured_sprite_sheet("TRAIL", "second.pvr", 30, 40),
    )
    .unwrap();
    fs::write(data_root.join("second/second.pvr"), []).unwrap();

    let runtime = StellaLua::new(&data_root).unwrap();
    runtime
        .execute_source(
            r#"
                res.createSpriteSheet("first/FIRST.dat")
                startNewTrajectory()
                setNormalTrailSprite("TRAIL")
                addToTrajectory(0, 1, 2)
                createNonPhysicsObject("trajectory_anchor", "", 0, 0, 1)
                setObjectParameter("trajectory_anchor", 1, 1)
                drawGameNative()
                res.createSpriteSheet("second/SECOND.dat")
                res.releaseSpriteSheet("first/FIRST.dat", false)
            "#,
        )
        .unwrap();

    assert_eq!(
        runtime.sprite_catalog_snapshot_since(0).unwrap().regions["TRAIL"]
            .sprite
            .width,
        30
    );
    let bridge = runtime.render.lock().unwrap();
    assert_eq!(bridge.commands.len(), 1);
    let retained = bridge.commands[0].bound_region.as_ref().unwrap();
    assert_eq!(retained.sprite.width, 10);
    assert!(retained.texture_source.ends_with("first/first.pvr"));
    drop(bridge);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn native_trajectory_is_inserted_immediately_before_the_first_anchor() {
    let runtime = unlocked_test_runtime();
    register_test_sprite_sheet(&runtime, &["BACK", "ANCHOR", "FRONT", "TRAIL"]);
    runtime
        .execute_source(
            r#"
                startNewTrajectory()
                setNormalTrailSprite("TRAIL")
                addToTrajectory(0, 1, 2)
                createNonPhysicsObject("back", "BACK", 0, 0, 1)
                createNonPhysicsObject("anchor", "ANCHOR", 0, 0, 2)
                setObjectParameter("anchor", 1, 1)
                createNonPhysicsObject("front", "FRONT", 0, 0, 3)
                drawGameNative()
            "#,
        )
        .unwrap();

    let bridge = runtime.render.lock().unwrap();
    assert_eq!(
        bridge
            .commands
            .iter()
            .map(|command| command.sprite.as_str())
            .collect::<Vec<_>>(),
        ["BACK", "TRAIL", "ANCHOR", "FRONT"]
    );
}

#[test]
fn native_trajectory_is_not_drawn_without_a_visible_anchor() {
    let runtime = unlocked_test_runtime();
    runtime
        .execute_source(
            r#"
                startNewTrajectory()
                setNormalTrailSprite("TRAIL")
                addToTrajectory(0, 1, 2)
                drawGameNative()
            "#,
        )
        .unwrap();

    assert!(runtime.render.lock().unwrap().commands.is_empty());
}

#[test]
fn game_parameters_reads_only_the_native_stack_top_table() {
    let runtime = unlocked_test_runtime();
    runtime
        .execute_source(
            r#"
                setGameParameters(
                    { deterministicPhysics = false, gameWorldScale = 0.125 },
                    { deterministicPhysics = true }
                )
                "#,
        )
        .unwrap();

    let bridge = runtime.render.lock().unwrap();
    assert!(bridge.deterministic_physics);
    assert_eq!(bridge.game_world_scale, 1.0);
}

#[test]
fn native_trajectory_snapshots_steps_while_aiming_time_reads_live_iterations() {
    let runtime = unlocked_test_runtime();
    load_native_simulation_settings(&runtime, 3, 2.0, 2);
    runtime
        .execute_source(
            r#"
                worldAttributes = {
                    simulationIterations = 3.9,
                    simulationTimeStepMultiplier = 2,
                    simulationStorePointsSampler = 2
                }
                objects = { currentTimeStep = "0.25" }
                createCircle("BirdSimulation", "", 0, 0, 1, 1, 0, 0,
                    true, false, 1)
                createCircle("selected", "", 10, 0, 1, 1, 0, 0,
                    true, false, 1)
                setWorldGravity(0, 0)
                native_setAdditionalBirdGravity(0)
                setVelocity("BirdSimulation", 0.1, 0)
                setSelectedBirdDuringSimulation("selected")
                updateBirdTrajectoryTable()
                native_aiming_time = getAimingTime()
                native_trajectory = getSimulationTrajectoryPoints()
                worldAttributes.simulationIterations = 99
                worldAttributes.simulationTimeStepMultiplier = 99
                worldAttributes.simulationStorePointsSampler = 99
                objects = { currentTimeStep = 4 }
                worldAttributes = { simulationIterations = 7 }
                updateBirdTrajectoryTable()
                native_aiming_time_after_mutation = getAimingTime()
                native_trajectory_after_mutation = getSimulationTrajectoryPoints()
                "#,
        )
        .unwrap();

    let environment = game_environment(runtime.lua()).unwrap();
    assert_eq!(
        environment.get::<f64>("native_aiming_time").unwrap(),
        f64::from(0.25_f32 * 3.0_f32)
    );
    assert_eq!(
        environment
            .get::<f64>("native_aiming_time_after_mutation")
            .unwrap(),
        f64::from(0.25_f32 * 99.0_f32)
    );
    let trajectory = environment.get::<mlua::Table>("native_trajectory").unwrap();
    let trajectory_after_mutation = environment
        .get::<mlua::Table>("native_trajectory_after_mutation")
        .unwrap();
    assert_eq!(trajectory.raw_len(), 2);
    assert_eq!(trajectory_after_mutation.raw_len(), 2);
    let first = trajectory.get::<mlua::Table>(1).unwrap();
    let second = trajectory.get::<mlua::Table>(2).unwrap();
    assert_eq!(first.get::<f64>("x").unwrap(), f64::from(0.05_f32));
    assert_eq!(second.get::<f64>("x").unwrap(), f64::from(0.15_f32));
    assert!(
        runtime
            .render
            .lock()
            .unwrap()
            .selected_simulation_bird
            .is_none()
    );
}

#[test]
fn trajectory_additional_gravity_is_independent_from_water_color_blue() {
    let runtime = unlocked_test_runtime();
    load_native_simulation_settings(&runtime, 1, 1.0, 1);
    runtime
        .execute_source(
            r#"
                worldAttributes = {
                    simulationIterations = 1,
                    simulationTimeStepMultiplier = 1,
                    simulationStorePointsSampler = 1
                }
                objects.currentTimeStep = 0.25
                createCircle("BirdSimulation", "", 0, 0, 1, 1, 1, 0,
                    true, false, 1)
                setWorldGravity(0, 0)
                native_setAdditionalBirdGravity(-1)
                native_setWaterColor(0.1, 0.2, 9, 0.4)
                updateBirdTrajectoryTable()
                native_trajectory = getSimulationTrajectoryPoints()
                "#,
        )
        .unwrap();

    let environment = game_environment(runtime.lua()).unwrap();
    let trajectory = environment.get::<mlua::Table>("native_trajectory").unwrap();
    let point = trajectory.get::<mlua::Table>(1).unwrap();
    assert_eq!(point.get::<f64>("y").unwrap(), 0.0);
    let bridge = runtime.render.lock().unwrap();
    assert_eq!(bridge.additional_bird_gravity, -1.0);
    assert_eq!(bridge.water_color[2], 9.0);
}

#[test]
fn locked_world_preserves_the_selected_bird_and_previous_prediction() {
    let runtime = unlocked_test_runtime();
    {
        let mut bridge = runtime.render.lock().unwrap();
        bridge.physics_world_locked = true;
        bridge.selected_simulation_bird = Some("selected".to_owned());
        bridge.trajectory_points = vec![(12.0, 34.0)];
        bridge.aim_stream_control_points = vec![(1.0, 2.0); 4];
    }

    // Native checks b2World::IsLocked before reading worldAttributes or
    // objects, so these globals intentionally remain absent.
    runtime
        .execute_source("updateBirdTrajectoryTable()")
        .unwrap();

    let bridge = runtime.render.lock().unwrap();
    assert_eq!(bridge.selected_simulation_bird.as_deref(), Some("selected"));
    assert_eq!(bridge.trajectory_points, vec![(12.0, 34.0)]);
    assert_eq!(bridge.aim_stream_control_points, vec![(1.0, 2.0); 4]);
}

#[test]
fn native_trajectory_uses_custom_single_body_gravity_and_zero_sampler_remainder() {
    let runtime = unlocked_test_runtime();
    load_native_simulation_settings(&runtime, 3, 1.0, 0);
    runtime
        .execute_source(
            r#"
                worldAttributes = {
                    simulationIterations = 3,
                    simulationTimeStepMultiplier = 1,
                    simulationStorePointsSampler = 0
                }
                objects = { currentTimeStep = 0.1 }
                createCircle("BirdSimulation", "", 0, 0, 1, 1, 0, 0,
                    true, false, 1)
                setGravityScale("BirdSimulation", 0)
                setWorldGravity(0, 1)
                native_setAdditionalBirdGravity(0)
                updateBirdTrajectoryTable()
                zero_sampler_trajectory = getSimulationTrajectoryPoints()
                "#,
        )
        .unwrap();

    let environment = game_environment(runtime.lua()).unwrap();
    let trajectory = environment
        .get::<mlua::Table>("zero_sampler_trajectory")
        .unwrap();
    // SDIV/MSUB with a zero divisor samples iteration zero only.
    assert_eq!(trajectory.raw_len(), 1);
    let first = trajectory.get::<mlua::Table>(1).unwrap();
    // sub_10086F6AC adds the dedicated world's gravity directly and does
    // not consult the body's ordinary gravityScale field.
    assert_eq!(
        first.get::<f64>("y").unwrap(),
        f64::from(0.1_f32.mul_add(0.1_f32, 0.0_f32))
    );
}

#[test]
fn native_trajectory_single_body_step_clamps_translation_and_rotation() {
    let runtime = unlocked_test_runtime();
    runtime
        .execute_source(
            r#"
                createCircle("body", "", 0, 0, 1, 1, 0, 0,
                    true, false, 1)
                "#,
        )
        .unwrap();
    let mut body = runtime.render.lock().unwrap().scene["body"].clone();
    body.velocity_x = 100.0;
    body.velocity_y = 0.0;
    body.angular_velocity = -100.0;

    step_native_trajectory_body(&mut body, (0.0, 0.0), 0.1_f32);

    let translation = (body.x as f32).hypot(body.y as f32);
    assert!((translation - f32::from_bits(0x3e23_d70a)).abs() < 1.0e-7);
    assert_eq!(body.angle as f32, -f32::from_bits(0x3fc9_0fdb));
    assert!((body.velocity_x as f32 - 1.6_f32).abs() < 2.0e-7);
    assert!((body.angular_velocity as f32 + 15.707_963_f32).abs() < 2.0e-6);
}

#[test]
fn native_trajectory_applies_registered_overlapping_sensor_forces() {
    let runtime = unlocked_test_runtime();
    load_native_simulation_settings(&runtime, 1, 1.0, 1);
    runtime
        .execute_source(
            r#"
                worldAttributes = {
                    simulationIterations = 1,
                    simulationTimeStepMultiplier = 1,
                    simulationStorePointsSampler = 1
                }
                objects = { currentTimeStep = 0.1 }
                createCircle("BirdSimulation", "", 0, 0, 1, 1, 0, 0,
                    true, false, 1)
                createBox("aim_gravity", "", 3, 0, 10, 10, 0, 0, 0,
                    true, false, 0)
                setObjectParameter("aim_gravity", 20, 1)
                setObjectParameter("aim_gravity", 21, 2)
                setObjectParameter("aim_gravity", 22, 1)
                setObjectParameter("aim_gravity", 27, 10)
                setObjectParameter("aim_gravity", 32, 1)
                setSensorMinimumAndMaximumForces("aim_gravity", 10, 20)
                setWorldGravity(0, 0)
                native_setAdditionalBirdGravity(0)
                updateBirdTrajectoryTable()
                sensor_trajectory = getSimulationTrajectoryPoints()
                "#,
        )
        .unwrap();

    let environment = game_environment(runtime.lua()).unwrap();
    let trajectory = environment.get::<mlua::Table>("sensor_trajectory").unwrap();
    let point = trajectory.get::<mlua::Table>(1).unwrap();
    assert!(
        point.get::<f64>("x").unwrap() > 0.0,
        "registered overlapping gravity sensor did not bend the prediction"
    );
}

#[test]
fn native_aim_stream_populates_updates_and_draws_catmull_particles() {
    let runtime = unlocked_test_runtime();
    load_native_simulation_settings(&runtime, 10, 1.0, 1);
    runtime
        .render
        .lock()
        .unwrap()
        .load_native_aim_stream_settings(0.5, 2.0);
    runtime
        .execute_source(
            r#"
                worldAttributes = {
                    simulationIterations = 10,
                    simulationTimeStepMultiplier = 1,
                    simulationStorePointsSampler = 1,
                    simulationAimSpawnTime = 0.5,
                    simulationAimSpeed = 2
                }
                -- Shipped gamelogic.initParams installs physicsToWorld=20
                -- after GameLua's native 1.0 constructor default.
                setPhysicsSimulationScale(20)
                objects = { currentTimeStep = 0.1 }
                createCircle("BirdSimulation", "", 0, 0, 1, 1, 0, 0,
                    true, false, 1)
                setWorldGravity(0, 0)
                native_setAdditionalBirdGravity(0)
                setVelocity("BirdSimulation", 1, 0)
                updateBirdTrajectoryTable()
                update = function() end
                updatePhysics = function() end
                "#,
        )
        .unwrap();
    runtime
        .execute_source(
            r#"
                setAimingAidSprite("TRAIL_AIM_STELLA")
                populateAimingAid()
                enable_type_ok = pcall(enableAimingAid, 1)
                enableAimingAid(true)
                native_drawSimulationTrajectory()
                "#,
        )
        .unwrap();

    let environment = game_environment(runtime.lua()).unwrap();
    assert!(!environment.get::<bool>("enable_type_ok").unwrap());
    {
        let bridge = runtime.render.lock().unwrap();
        assert!(bridge.aim_stream_active);
        assert_eq!(bridge.aim_stream_control_points.len(), 12);
        assert_eq!(bridge.aim_stream_particles.len(), 9);
        assert_eq!(bridge.commands.len(), 9);
        assert_eq!(bridge.aim_stream_particles[0].path_parameter, 0.0);
        assert_eq!(bridge.aim_stream_particles[1].path_parameter, 1.0);
        assert_eq!(bridge.aim_stream_particles[0].scale, 1.2_f32);
        let first = &bridge.commands[0];
        assert_eq!(first.sprite, "TRAIL_AIM_STELLA");
        assert!(!first.world_space);
        assert_eq!(first.state.pivot_x, 10.0);
        assert_eq!(first.state.pivot_y, 10.0);
        assert!((first.state.scale_x - 1.2_f32).abs() < 1.0e-7);
        // GL_Context's divided draw coordinate and particle scale cancel:
        // the first 0.1-physics-unit sample lands at 2 screen units.
        assert!((first.x * first.state.scale_x - 2.0).abs() < 1.0e-6);
    }

    runtime.update(0.25).unwrap();
    runtime
        .execute_source("native_drawSimulationTrajectory()")
        .unwrap();
    {
        let bridge = runtime.render.lock().unwrap();
        assert_eq!(bridge.aim_stream_particles.len(), 9);
        assert_eq!(bridge.aim_stream_particles[0].path_parameter, 0.5);
        assert_eq!(bridge.aim_stream_particles[8].path_parameter, 8.5);
        assert_eq!(bridge.aim_stream_spawn_timer, 0.25);
        let halfway = &bridge.commands[9];
        let halfway_screen_x = halfway.x * halfway.state.scale_x;
        assert!(
            (halfway_screen_x - 2.875).abs() < 1.0e-5,
            "half-segment Catmull-Rom x was {halfway_screen_x}"
        );
    }

    runtime.update(0.3).unwrap();
    let bridge = runtime.render.lock().unwrap();
    assert_eq!(bridge.aim_stream_particles.len(), 9);
    assert!((bridge.aim_stream_particles[0].path_parameter - 1.1_f32).abs() < 1.0e-6);
    assert_eq!(
        bridge.aim_stream_particles.last().unwrap().path_parameter,
        0.0
    );
    assert!((bridge.aim_stream_spawn_timer - 0.45_f32).abs() < 1.0e-6);
}

#[test]
fn physics_lock_after_lua_update_freezes_native_aim_stream() {
    let runtime = unlocked_test_runtime();
    runtime
        .execute_source(
            r#"
                worldAttributes = {
                    simulationAimSpawnTime = 0.5,
                    simulationAimSpeed = 2
                }
                update = function()
                    setPhysicsEnabled(false, "during-update")
                end
            "#,
        )
        .unwrap();
    {
        let mut bridge = runtime.render.lock().unwrap();
        bridge.load_native_aim_stream_settings(0.5, 2.0);
        bridge.aim_stream_control_points = (0..12).map(|x| (f64::from(x), 0.0)).collect();
        bridge.populate_native_aim_stream();
        bridge.aim_stream_active = true;
    }
    let before = {
        let bridge = runtime.render.lock().unwrap();
        (
            bridge.aim_stream_particles.clone(),
            bridge.aim_stream_spawn_timer,
        )
    };

    runtime.update(0.25).unwrap();

    let bridge = runtime.render.lock().unwrap();
    assert_eq!(bridge.aim_stream_particles, before.0);
    assert_eq!(bridge.aim_stream_spawn_timer, before.1);
}

#[test]
fn aim_stream_draw_applies_pending_enabled_state_only_after_points_are_valid() {
    let runtime = unlocked_test_runtime();
    runtime.execute_source("enableAimingAid(true)").unwrap();

    // sub_10004C4CC returns before both draw and setActive when AimStream has
    // fewer than four control points.
    runtime
        .execute_source("native_drawSimulationTrajectory()")
        .unwrap();
    assert!(!runtime.render.lock().unwrap().aim_stream_active);

    // A valid stream reaches setActive even when its sprite name is empty;
    // AimStream::draw emits the native warning but the wrapper still applies
    // the pending enabled flag after the draw call.
    runtime.render.lock().unwrap().aim_stream_control_points =
        vec![(0.0, 0.0), (0.0, 0.0), (1.0, 0.0), (1.0, 0.0)];
    runtime
        .execute_source("native_drawSimulationTrajectory()")
        .unwrap();
    let bridge = runtime.render.lock().unwrap();
    assert!(bridge.aim_stream_active);
    assert!(bridge.commands.is_empty());
}

#[test]
fn clear_aiming_aid_prunes_the_normalized_prefix_and_deactivates_the_stream() {
    let runtime = unlocked_test_runtime();
    load_native_simulation_settings(&runtime, 10, 1.0, 1);
    runtime
        .render
        .lock()
        .unwrap()
        .load_native_aim_stream_settings(0.5, 2.0);
    runtime
        .execute_source(
            r#"
                worldAttributes = {
                    simulationIterations = 10,
                    simulationTimeStepMultiplier = 1,
                    simulationStorePointsSampler = 1,
                    simulationAimSpawnTime = 0.5,
                    simulationAimSpeed = 2
                }
                objects = { currentTimeStep = 0.1 }
                createCircle("BirdSimulation", "", 0, 0, 1, 1, 0, 0,
                    true, false, 1)
                setWorldGravity(0, 0)
                native_setAdditionalBirdGravity(0)
                setVelocity("BirdSimulation", 1, 0)
                updateBirdTrajectoryTable()
                "#,
        )
        .unwrap();
    runtime
        .execute_source(
            r#"
                populateAimingAid()
                enableAimingAid(true)
                clear_type_ok = pcall(clearAimingAid, true)
                clearAimingAid(0.5)
                "#,
        )
        .unwrap();

    let environment = game_environment(runtime.lua()).unwrap();
    assert!(!environment.get::<bool>("clear_type_ok").unwrap());
    let bridge = runtime.render.lock().unwrap();
    assert!(!bridge.aim_stream_active);
    assert_eq!(bridge.aim_stream_particles.len(), 4);
    assert_eq!(bridge.aim_stream_particles[0].path_parameter, 5.0);
    assert_eq!(bridge.aim_stream_particles[3].path_parameter, 8.0);
}

#[test]
fn clear_aiming_aid_one_removes_every_particle_without_erasing_the_path() {
    let runtime = unlocked_test_runtime();
    load_native_simulation_settings(&runtime, 4, 1.0, 1);
    runtime
        .execute_source(
            r#"
                worldAttributes = {
                    simulationIterations = 4,
                    simulationTimeStepMultiplier = 1,
                    simulationStorePointsSampler = 1
                }
                objects = { currentTimeStep = 0.1 }
                createCircle("BirdSimulation", "", 0, 0, 1, 1, 0, 0,
                    true, false, 1)
                setWorldGravity(0, 0)
                native_setAdditionalBirdGravity(0)
                setVelocity("BirdSimulation", 1, 0)
                updateBirdTrajectoryTable()
                populateAimingAid()
                enableAimingAid(true)
                clearAimingAid(1)
                "#,
        )
        .unwrap();

    let bridge = runtime.render.lock().unwrap();
    assert!(!bridge.aim_stream_active);
    assert!(bridge.aim_stream_particles.is_empty());
    assert_eq!(bridge.aim_stream_control_points.len(), 6);
}

#[test]
fn trajectory_and_textured_line_emit_render_commands() {
    let runtime = unlocked_test_runtime();
    load_native_simulation_settings(&runtime, 4, 1.0, 1);
    register_test_sprite_sheet(&runtime, &["RUBBER"]);
    runtime
        .execute_source(
            r#"
                worldAttributes = {
                    simulationIterations = 4,
                    simulationTimeStepMultiplier = 1,
                    simulationStorePointsSampler = 1
                }
                objects = { currentTimeStep = 0.1 }
                createCircle("BirdSimulation", "", 0, 0, 1, 1, 0, 0,
                    true, false, 1)
                setWorldGravity(0, 0)
                native_setAdditionalBirdGravity(0)
                setVelocity("BirdSimulation", 1, 0)
                updateBirdTrajectoryTable()
                setAimingAidSprite("TRAIL_AIM_STELLA")
                populateAimingAid()
                native_drawSimulationTrajectory()
                drawTexturedLine2D("RUBBER", 10, 20, 30, 20, 8, 255, 255, 255, 128)
                trajectory = getSimulationTrajectoryPoints()
                "#,
        )
        .unwrap();
    let environment = game_environment(runtime.lua()).unwrap();
    let trajectory: mlua::Table = environment.get("trajectory").unwrap();
    assert_eq!(trajectory.raw_len(), 4);
    assert_eq!(
        trajectory
            .raw_get::<mlua::Table>(4)
            .unwrap()
            .get::<f64>("x")
            .unwrap(),
        f64::from(0.4_f32)
    );

    let bridge = runtime.render.lock().unwrap();
    assert_eq!(bridge.commands.len(), 2);
    let line = bridge.commands.last().unwrap();
    assert_eq!(line.sprite, "RUBBER");
    assert_eq!((line.x, line.y), (10.0, 16.0));
    assert_eq!(line.state.matrix, Some([20.0, 0.0, 0.0, 8.0]));
    // sub_10004DB90 ignores the four trailing floats, including 128.
    assert_eq!(line.state.alpha, 1.0);
}

#[test]
fn native_flight_trajectory_is_strict_double_buffered_and_independent() {
    let runtime = unlocked_test_runtime();
    load_native_simulation_settings(&runtime, 4, 1.0, 1);
    runtime
        .execute_source(
            r#"
                worldAttributes = {
                    simulationIterations = 4,
                    simulationTimeStepMultiplier = 1,
                    simulationStorePointsSampler = 1
                }
                objects = { currentTimeStep = 0.1 }
                createCircle("BirdSimulation", "", 0, 0, 1, 1, 0, 0,
                    true, false, 1)
                createNonPhysicsObject("trajectory_anchor", "", 0, 0, 2)
                setObjectParameter("trajectory_anchor", 1, 1)
                setWorldGravity(0, 0)
                native_setAdditionalBirdGravity(0)
                setVelocity("BirdSimulation", 1, 0)
                updateBirdTrajectoryTable()

                startNewTrajectory()
                setNormalTrailSprite("TRAIL_WHITE_1")
                setSpecialTrailSprite("BIRD_SPECIAL")
                addToTrajectory(99, 10.123456789, 20.987654321)
                addPuffToTrajectory(88, 30.123456789, 40.987654321)
                short_add_ok = pcall(addToTrajectory, 1, 2)
                hole_add_ok = pcall(addToTrajectory, 1, nil, 3)
                short_puff_ok = pcall(addPuffToTrajectory, 1, 2)
                sprite_type_ok = pcall(setNormalTrailSprite, 123, "ignored")
                simulation_after_flight_start = getSimulationTrajectoryPoints()

                setAimingAidSprite("TRAIL_AIM_STELLA")
                populateAimingAid()
                ClearSimulationTrajectory()
                simulation_after_clear = getSimulationTrajectoryPoints()
                native_drawSimulationTrajectory()

                startNewTrajectory()
                "#,
        )
        .unwrap();

    let environment = game_environment(runtime.lua()).unwrap();
    assert!(!environment.get::<bool>("short_add_ok").unwrap());
    assert!(!environment.get::<bool>("hole_add_ok").unwrap());
    assert!(!environment.get::<bool>("short_puff_ok").unwrap());
    assert!(!environment.get::<bool>("sprite_type_ok").unwrap());
    assert_eq!(
        environment
            .get::<mlua::Table>("simulation_after_flight_start")
            .unwrap()
            .raw_len(),
        4
    );
    assert!(matches!(
        environment.get::<Value>("simulation_after_clear").unwrap(),
        Value::Nil
    ));

    {
        let bridge = runtime.render.lock().unwrap();
        assert_eq!(bridge.trajectory_stream_index, 0);
        let populated = &bridge.trajectory_streams[1];
        assert_eq!(populated.normal_sprite, "TRAIL_WHITE_1");
        assert_eq!(populated.special_sprite, "BIRD_SPECIAL");
        assert_eq!(
            populated.points,
            vec![(f64::from(10.123_457_f32), f64::from(20.987_654_f32))]
        );
        assert_eq!(
            populated.puff,
            Some((f64::from(30.123_457_f32), f64::from(40.987_656_f32)))
        );
        assert!(bridge.trajectory_streams[0].points.is_empty());
        // Clearing +0x590 leaves the prepared AimStream drawable.
        assert_eq!(bridge.commands.len(), 1);
    }

    {
        let mut bridge = runtime.render.lock().unwrap();
        bridge.top_left_x = 2.0;
        bridge.top_left_y = 3.0;
        bridge.world_scale = 2.0;
        bridge.game_world_scale = 0.25;
    }
    runtime.execute_source("drawGameNative()").unwrap();
    let bridge = runtime.render.lock().unwrap();
    assert_eq!(bridge.commands.len(), 3);
    assert_eq!(bridge.commands[1].sprite, "TRAIL_WHITE_1");
    assert_eq!(
        (bridge.commands[1].x, bridge.commands[1].y),
        (10.123_457_f32 / 0.25_f32, 20.987_654_f32 / 0.25_f32)
    );
    assert_eq!(bridge.commands[1].state.scale_x, 0.5);
    assert_eq!(bridge.commands[1].state.scale_y, 0.5);
    assert_eq!(
        (
            bridge.commands[1].state.translate_x,
            bridge.commands[1].state.translate_y
        ),
        (-8.0, -12.0)
    );
    assert!(!bridge.commands[1].world_space);
    assert_eq!(
        (
            (bridge.commands[1].state.translate_x + bridge.commands[1].x)
                * bridge.commands[1].state.scale_x,
            (bridge.commands[1].state.translate_y + bridge.commands[1].y)
                * bridge.commands[1].state.scale_y,
        ),
        (
            ((10.123_457_f32 / 0.25_f32) - (2.0_f32 / 0.25_f32)) * (2.0_f32 * 0.25_f32),
            ((20.987_654_f32 / 0.25_f32) - (3.0_f32 / 0.25_f32)) * (2.0_f32 * 0.25_f32),
        )
    );
    assert_eq!(bridge.commands[2].sprite, "BIRD_SPECIAL");
}

#[test]
fn textured_line_uses_native_context_geometry_and_strict_adapter() {
    let runtime = unlocked_test_runtime();
    register_test_sprite_sheet(&runtime, &["RUBBER"]);
    runtime
        .execute_source(
            r#"
                setRenderState(3, 4, 2, 3, 0, 5, 7, 0.25)
                drawTexturedLine2D("RUBBER", 10, 20, 30, 20, 8, 1, 2, 3, 4)
                short_ok = pcall(drawTexturedLine2D, "RUBBER", 10, 20, 30, 20, 8)
                type_ok = pcall(drawTexturedLine2D, {}, 10, 20, 30, 20, 8, 1, 2, 3, 4)
                missing_resource_fails = not pcall(
                    drawTexturedLine2D,
                    "MISSING", 10, 20, 30, 20, 8, 1, 2, 3, 4
                )
                drawTexturedLine2D("RUBBER", 0, 0, 0.49, 0, 8, 1, 2, 3, 4)
                "#,
        )
        .unwrap();
    let environment = game_environment(runtime.lua()).unwrap();
    assert!(!environment.get::<bool>("short_ok").unwrap());
    assert!(!environment.get::<bool>("type_ok").unwrap());
    assert!(environment.get::<bool>("missing_resource_fails").unwrap());

    let bridge = runtime.render.lock().unwrap();
    assert_eq!(bridge.commands.len(), 1);
    let line = &bridge.commands[0];
    assert_eq!((line.x, line.y), (26.0, 60.0));
    assert_eq!(line.state.matrix, Some([40.0, 0.0, 0.0, 24.0]));
    assert_eq!(
        native_atlas_quad(line),
        [[26.0, 60.0], [66.0, 60.0], [26.0, 84.0], [66.0, 84.0]]
    );
    assert_eq!(line.state.alpha, 0.25);
    assert!(line.world_space);
}

#[test]
fn line_sprite_generated_adapters_require_exact_tags_and_ignore_extras() {
    let runtime = unlocked_test_runtime();
    register_test_sprite_sheet(&runtime, &["RUBBER"]);
    runtime
        .execute_source(
            r#"
                textured_rejects_numeric_sprite = not pcall(
                    drawTexturedLine2D, 123, 0, 0, 10, 0, 2, 1, 2, 3, 4)
                textured_rejects_string_number = not pcall(
                    drawTexturedLine2D, "RUBBER", "0", 0, 10, 0, 2, 1, 2, 3, 4)
                textured_rejects_wrong_inert_slot = not pcall(
                    drawTexturedLine2D, "RUBBER", 0, 0, 10, 0, 2, 1, 2, 3, "4")
                textured_accepts_trailing = pcall(
                    drawTexturedLine2D, "RUBBER", 0, 0, 10, 0, 2, 1, 2, 3, 4, "ignored")
                rubber_rejects_string_number = not pcall(
                    drawRubberband, "0", 10, 10, 10, 2, "RUBBER")
                rubber_rejects_numeric_sprite = not pcall(
                    drawRubberband, 0, 10, 10, 10, 2, 123)
                rubber_accepts_trailing = pcall(
                    drawRubberband, 0, 10, 10, 10, 2, "RUBBER", "ignored")
                "#,
        )
        .unwrap();

    let environment = game_environment(runtime.lua()).unwrap();
    for name in [
        "textured_rejects_numeric_sprite",
        "textured_rejects_string_number",
        "textured_rejects_wrong_inert_slot",
        "textured_accepts_trailing",
        "rubber_rejects_string_number",
        "rubber_rejects_numeric_sprite",
        "rubber_accepts_trailing",
    ] {
        assert!(environment.get::<bool>(name).unwrap(), "{name}");
    }
    assert_eq!(runtime.render.lock().unwrap().commands.len(), 2);
}

#[test]
fn textured_line_quantizes_lua_coordinates_before_native_geometry() {
    let runtime = unlocked_test_runtime();
    register_test_sprite_sheet(&runtime, &["RUBBER"]);
    runtime
        .execute_source(
            r#"
                drawTexturedLine2D(
                    "RUBBER", 16777217, 0, 16777219, 0, 2, 1, 2, 3, 4)
                "#,
        )
        .unwrap();

    let bridge = runtime.render.lock().unwrap();
    assert_eq!(bridge.commands.len(), 1);
    let line = &bridge.commands[0];
    // The Lua adapter at 0x100084A9C narrows both endpoints before the
    // subtraction: 16777217 -> 16777216 and 16777219 -> 16777220.
    assert_eq!(line.state.matrix, Some([4.0, 0.0, 0.0, 2.0]));
    assert_eq!(
        native_atlas_quad(line),
        [
            [16_777_216.0, -1.0],
            [16_777_220.0, -1.0],
            [16_777_216.0, 1.0],
            [16_777_220.0, 1.0],
        ]
    );
}

#[test]
fn rubberband_uses_native_argument_and_uv_axis_order() {
    let runtime = unlocked_test_runtime();
    register_test_sprite_sheet(&runtime, &["RUBBER", "ZERO_RUBBER"]);
    runtime
        .execute_source(
            r#"
                setRenderState(3, 4, 2, 3, 1, 5, 7, 0.25)
                drawRubberband(10, 20, 30, 20, 8, "RUBBER")
                drawRubberband(10, 20, 10, 20, 8, "ZERO_RUBBER")
                old_order_ok = pcall(drawRubberband, "RUBBER", 10, 20, 30, 20, 8)
                "#,
        )
        .unwrap();
    let environment = game_environment(runtime.lua()).unwrap();
    assert!(!environment.get::<bool>("old_order_ok").unwrap());

    let bridge = runtime.render.lock().unwrap();
    assert_eq!(bridge.commands.len(), 2);
    let rubber = &bridge.commands[0];
    assert_eq!(rubber.sprite, "RUBBER");
    assert_eq!((rubber.x, rubber.y), (10.0, 16.0));
    assert_eq!(rubber.state.matrix, Some([0.0, 20.0, 8.0, 0.0]));
    assert_eq!(
        native_atlas_quad(rubber),
        [[10.0, 16.0], [10.0, 24.0], [30.0, 16.0], [30.0, 24.0]]
    );
    assert_eq!(rubber.state.alpha, 0.25);
    assert!(rubber.world_space);

    // Purple still submits the exactly-zero segment: its two end pairs
    // coincide and the GPU discards the zero-area triangles naturally.
    let zero = &bridge.commands[1];
    assert_eq!(
        native_atlas_quad(zero),
        [[10.0, 16.0], [10.0, 24.0], [10.0, 16.0], [10.0, 24.0]]
    );
}
