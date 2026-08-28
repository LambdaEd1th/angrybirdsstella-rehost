use super::*;

#[test]
fn update_passes_only_scaled_and_unscaled_frame_deltas_in_native_order() {
    let runtime = unlocked_test_runtime();
    runtime
        .execute_source(
            r##"
                setDeltaTimeMultiplier(0.5)
                update = function(scaled_delta, unscaled_delta)
                    captured_scaled_delta = scaled_delta
                    captured_unscaled_delta = unscaled_delta
                    captured_delta_time = rawget(gamelua, "deltaTime")
                    captured_time_step = rawget(gamelua, "currentTimeStep")
                    captured_g_time = rawget(gamelua, "g_time")
                end
                "##,
        )
        .unwrap();

    runtime.update(0.2).unwrap();
    let environment = game_environment(runtime.lua()).unwrap();
    let raw = 0.2_f64 as f32;
    let scaled = 0.5_f32 * raw;
    assert_eq!(
        environment.get::<f64>("captured_scaled_delta").unwrap(),
        f64::from(scaled)
    );
    assert_eq!(
        environment.get::<f64>("captured_unscaled_delta").unwrap(),
        f64::from(raw)
    );
    for name in [
        "captured_delta_time",
        "captured_time_step",
        "captured_g_time",
    ] {
        assert!(matches!(
            environment.raw_get::<Value>(name).unwrap(),
            Value::Nil
        ));
    }
}

#[test]
fn native_frame_quantizes_before_the_fixed_step_threshold_and_subtracts_in_f32() {
    let runtime = unlocked_test_runtime();
    runtime
        .execute_source(
            r##"
                createCircle("body", "", 0, 0, 1, 1, 0, 0, true, false, 1)
                setWorldGravity(0, 0)
                physics_steps = 0
                updatePhysics = function(step)
                    physics_steps = physics_steps + 1
                    captured_physics_step = step
                end
                update = function(_, raw) captured_raw_delta = raw end
                "##,
        )
        .unwrap();

    // 0.1 rounds to 0x3DCCCCCD at the S0 boundary. Two repeated
    // 0x3D088889 subtractions leave 0x3D088887, just below a third step; the
    // former f64 division/floor path incorrectly ran three steps at once.
    runtime.update(0.1).unwrap();

    let environment = game_environment(runtime.lua()).unwrap();
    assert_eq!(environment.get::<i64>("physics_steps").unwrap(), 2);
    assert_eq!(
        environment.get::<f64>("captured_raw_delta").unwrap(),
        f64::from(0.1_f32)
    );
    assert_eq!(
        environment.get::<f64>("captured_physics_step").unwrap(),
        f64::from(f32::from_bits(0x3D08_8889))
    );
    assert_eq!(
        runtime.render.lock().unwrap().physics_accumulator.to_bits(),
        0x3D08_8887
    );
}

#[test]
fn catch_up_frame_retains_the_final_two_consecutive_solved_poses() {
    let runtime = unlocked_test_runtime();
    runtime
        .execute_source(
            r##"
                createCircle("body", "", 0, 0, 1, 1, 0, 0, true, false, 1)
                setWorldGravity(0, 0)
                setVelocity("body", 1, 0)
                updatePhysics = function() end
                update = function() end
            "##,
        )
        .unwrap();

    runtime.update(0.1).unwrap();

    let bridge = runtime.render.lock().unwrap();
    let body = &bridge.scene["body"];
    let current_slot = bridge.physics_interpolation_slot;
    let previous_slot = usize::from(current_slot == 0);
    let current = body.interpolation_poses[current_slot];
    let previous = body.interpolation_poses[previous_slot];
    let step = f32::from_bits(0x3D08_8889);
    assert_eq!(current.x, body.x as f32);
    assert_eq!(previous.x, step);
    assert_eq!(current.x, step + step);
    assert_eq!((current.y, previous.y), (0.0, 0.0));
}

#[test]
fn catch_up_steps_do_not_publish_intermediate_body_poses_to_lua() {
    let runtime = unlocked_test_runtime();
    runtime
        .execute_source(
            r##"
                createCircle("body", "", 0, 0, 1, 1, 0, 0, true, true, 1)
                setActive("body", true)
                setWorldGravity(0, 0)
                setVelocity("body", 1, 0)
                physics_pose_reads = {}
                updatePhysics = function()
                    table.insert(physics_pose_reads, objects.world.body.x)
                end
                update = function() end
            "##,
        )
        .unwrap();

    // The float32 accumulator executes two fixed steps. Purple leaves the
    // preceding display-frame pose visible to both updatePhysics callbacks,
    // then publishes the interpolated display pose once after the loop.
    runtime.update(0.1).unwrap();

    let environment = game_environment(runtime.lua()).unwrap();
    let reads = environment
        .get::<mlua::Table>("physics_pose_reads")
        .unwrap();
    assert_eq!(reads.raw_len(), 2);
    assert_eq!(reads.raw_get::<f64>(1).unwrap(), 0.0);
    assert_eq!(reads.raw_get::<f64>(2).unwrap(), 0.0);

    let world = object_world(runtime.lua()).unwrap();
    let lua_x = world
        .get::<mlua::Table>("body")
        .unwrap()
        .get::<f64>("x")
        .unwrap();
    let bridge = runtime.render.lock().unwrap();
    assert_eq!(lua_x, f64::from(bridge.scene["body"].render_x as f32));
    assert!(lua_x > 0.0);
}

#[test]
fn native_fixed_step_runs_without_bodies_and_publishes_box2d_timing() {
    let runtime = unlocked_test_runtime();
    runtime
        .execute_source(
            r##"
                physics_steps = 0
                updatePhysics = function()
                    physics_steps = physics_steps + 1
                end
                clearLuaForceFunctions = function() end
                update = function() end
                "##,
        )
        .unwrap();

    runtime
        .update(f64::from(f32::from_bits(0x3D08_8889)))
        .unwrap();

    let environment = game_environment(runtime.lua()).unwrap();
    assert_eq!(environment.get::<i64>("physics_steps").unwrap(), 1);
    let update_millis = environment.get::<f64>("g_physicsUpdateMillis").unwrap();
    assert!(update_millis.is_finite() && update_millis >= 0.0);
    assert_eq!(update_millis, f64::from(update_millis as f32));
}

#[test]
fn native_render_pose_interpolation_uses_two_slots_and_short_angle_arc() {
    let runtime = unlocked_test_runtime();
    runtime
        .execute_source(
            r##"
                createCircle("body", "", 0, 0, 1, 1, 0, 0, true, false, 1)
                update = function() end
            "##,
        )
        .unwrap();
    {
        let mut bridge = runtime.render.lock().unwrap();
        bridge.physics_interpolation_slot = 1;
        bridge.physics_accumulator = 1.0_f32 / 60.0_f32;
        let body = bridge.scene.get_mut("body").unwrap();
        body.interpolation_poses[0] = NativeInterpolationPose {
            x: 2.0,
            y: 4.0,
            angle: 6.2,
        };
        body.interpolation_poses[1] = NativeInterpolationPose {
            x: 6.0,
            y: 8.0,
            angle: 0.1,
        };
        bridge.interpolate_native_scene_poses();
    }

    let bridge = runtime.render.lock().unwrap();
    let body = &bridge.scene["body"];
    let alpha = (1.0_f32 / 60.0_f32) * f32::from_bits(0x41EF_FFFF);
    let previous_weight = 1.0_f32 - alpha;
    let expected_x = alpha.mul_add(6.0, previous_weight * 2.0);
    let expected_y = alpha.mul_add(8.0, previous_weight * 4.0);
    let adjusted_previous_angle = 6.2_f32 - (f32::from_bits(0x4049_0FDB) * 2.0);
    let expected_angle = alpha.mul_add(0.1, previous_weight * adjusted_previous_angle);
    assert_eq!(body.render_x.to_bits(), f64::from(expected_x).to_bits());
    assert_eq!(body.render_y.to_bits(), f64::from(expected_y).to_bits());
    assert_eq!(
        body.render_angle.to_bits(),
        f64::from(expected_angle).to_bits()
    );
}

#[test]
fn explicit_pose_setters_reset_render_pose_and_both_native_slots() {
    let runtime = unlocked_test_runtime();
    runtime
        .execute_source(
            r##"
                createCircle("body", "", 0, 0, 1, 1, 0, 0, true, false, 1)
                setPosition("body", 3.25, -4.5)
                setRotation("body", 6.4)
                setVelocity("body", 2.5, -3.5)
            "##,
        )
        .unwrap();

    let bridge = runtime.render.lock().unwrap();
    let body = &bridge.scene["body"];
    let x = 3.25_f32;
    let y = -4.5_f32;
    let tau = std::f32::consts::PI + std::f32::consts::PI;
    let angle = 6.4_f32 % tau;
    assert_eq!(body.render_x, f64::from(x));
    assert_eq!(body.render_y, f64::from(y));
    assert_eq!(body.render_angle, f64::from(angle));
    for pose in body.interpolation_poses {
        assert_eq!((pose.x, pose.y, pose.angle), (x, y, angle));
    }
    for velocity in body.display_interpolation_velocities {
        assert_eq!((velocity.x, velocity.y), (2.5, -3.5));
    }
}

#[test]
fn physics_lock_retains_the_prelock_float_accumulator_without_banking_time() {
    let runtime = unlocked_test_runtime();
    runtime
        .execute_source(
            r##"
                physics_steps = 0
                updatePhysics = function()
                    physics_steps = physics_steps + 1
                end
                clearLuaForceFunctions = function() end
                update = function() end
                "##,
        )
        .unwrap();

    runtime.update(1.0 / 60.0).unwrap();
    let retained = runtime.render.lock().unwrap().physics_accumulator;
    assert!(retained > 0.0);

    runtime
        .execute_source("setPhysicsEnabled(false, 'transition')")
        .unwrap();
    runtime.update(1.0).unwrap();
    assert_eq!(runtime.render.lock().unwrap().physics_accumulator, retained);
    let environment = game_environment(runtime.lua()).unwrap();
    assert_eq!(environment.get::<i64>("physics_steps").unwrap(), 0);
    assert_eq!(
        environment.get::<f64>("g_physicsUpdateMillis").unwrap(),
        0.0
    );

    runtime
        .execute_source("setPhysicsEnabled(true, 'transition')")
        .unwrap();
    runtime.update(1.0 / 60.0).unwrap();
    assert_eq!(environment.get::<i64>("physics_steps").unwrap(), 1);
}

#[test]
fn native_time_and_force_multiplier_adapters_are_strict_slot_one_float32() {
    let runtime = unlocked_test_runtime();
    runtime
        .execute_source(
            r##"
                setDeltaTimeMultiplier(1.00000006, 9)
                setGravityForceMultiplier(2.00000012, 8)
                setWaterForceMultiplier(3.00000018, 7)
                delta_multiplier = getDeltaTimeMultiplier("ignored")
                gravity_multiplier = getGravityForceMultiplier("ignored")
                water_multiplier = getWaterForceMultiplier("ignored")

                delta_missing_fails = not pcall(setDeltaTimeMultiplier)
                gravity_missing_fails = not pcall(setGravityForceMultiplier)
                water_missing_fails = not pcall(setWaterForceMultiplier)
                delta_type_fails = not pcall(setDeltaTimeMultiplier, false)
                gravity_type_fails = not pcall(setGravityForceMultiplier, {})
                water_type_fails = not pcall(setWaterForceMultiplier, "3")
            "##,
        )
        .unwrap();

    let environment = game_environment(runtime.lua()).unwrap();
    assert_eq!(
        environment.get::<f64>("delta_multiplier").unwrap(),
        f64::from(1.00000006_f64 as f32)
    );
    assert_eq!(
        environment.get::<f64>("gravity_multiplier").unwrap(),
        f64::from(2.00000012_f64 as f32)
    );
    assert_eq!(
        environment.get::<f64>("water_multiplier").unwrap(),
        f64::from(3.00000018_f64 as f32)
    );
    for field in [
        "delta_missing_fails",
        "gravity_missing_fails",
        "water_missing_fails",
        "delta_type_fails",
        "gravity_type_fails",
        "water_type_fails",
    ] {
        assert!(environment.get::<bool>(field).unwrap(), "{field}");
    }
}

#[test]
fn fixed_physics_step_applies_impulse_and_writes_body_state_back_to_lua() {
    let runtime = unlocked_test_runtime();
    runtime
        .execute_source(
            r##"
                createCircle("bird", "", 10, 20, 1, 1, 0, 0, true, false, 1)
                setWorldGravity(0, 0)
                setRecordVelocity("bird", true)
                applyImpulse("bird", 3, 0, 10, 20)
                update = function() end
                updatePhysics = function(step) captured_physics_step = step end
                "##,
        )
        .unwrap();

    runtime.update(1.0 / 30.0).unwrap();
    let environment = game_environment(runtime.lua()).unwrap();
    let world = object_world(runtime.lua()).unwrap();
    let bird: mlua::Table = world.get("bird").unwrap();
    let bridge = runtime.render.lock().unwrap();
    assert!(bridge.scene["bird"].x > 10.0);
    assert_eq!(
        bird.get::<f64>("x").unwrap(),
        f64::from(bridge.scene["bird"].render_x as f32)
    );
    assert!(bird.get::<f64>("xVel").unwrap() > 0.0);
    drop(bridge);
    assert_eq!(
        environment.get::<f64>("captured_physics_step").unwrap(),
        f64::from(f32::from_bits(0x3D08_8889))
    );
}

#[test]
fn set_game_on_does_not_override_the_native_physics_lock() {
    let runtime = unlocked_test_runtime();
    runtime
        .execute_source(
            r##"
                createCircle("body", "", 0, 0, 1, 1, 0, 0, true, false, 1)
                setWorldGravity(0, 10)
                setGameOn(false)
                update = function() end
                updatePhysics = function() end
                "##,
        )
        .unwrap();

    runtime.update(1.0 / 30.0).unwrap();
    assert!(
        runtime.render.lock().unwrap().scene["body"].y > 0.0,
        "setGameOn(false) must not suppress Purple's Box2D step"
    );

    runtime
        .execute_source("setPhysicsEnabled(false, 'tutorial')")
        .unwrap();
    let paused_y = runtime.render.lock().unwrap().scene["body"].y;
    runtime.update(1.0).unwrap();
    assert_eq!(runtime.render.lock().unwrap().scene["body"].y, paused_y);

    runtime
        .execute_source("setPhysicsEnabled(true, 'tutorial')")
        .unwrap();
    runtime.update(1.0 / 30.0).unwrap();
    let resumed_y = runtime.render.lock().unwrap().scene["body"].y;
    assert!(resumed_y > paused_y);
    assert!(
        resumed_y - paused_y < 0.1,
        "physics-locked time was incorrectly simulated after resume"
    );
}

#[test]
fn new_dynamic_box2d_body_starts_awake_and_falls_under_gravity() {
    let runtime = unlocked_test_runtime();
    runtime
        .execute_source(
            r##"
                createCircle("body", "", 0, 5, 1, 1, 0, 0, true, false, 1)
                setWorldGravity(0, -10)
                update = function() end
                updatePhysics = function() end
                "##,
        )
        .unwrap();

    {
        let bridge = runtime.render.lock().unwrap();
        let body = &bridge.scene["body"];
        assert!(body.motion_started);
        assert!(!body.sleeping);
    }
    runtime.update(1.0 / 30.0).unwrap();
    let bridge = runtime.render.lock().unwrap();
    assert!(bridge.scene["body"].y < 5.0);
    assert!(bridge.scene["body"].velocity_y < 0.0);
}

#[test]
fn joint_island_wakes_sleeping_endpoint_before_same_step_integration() {
    let runtime = unlocked_test_runtime();
    runtime
        .execute_source(
            r#"
                createCircle("a", "", 0, 0, 0.5, 1, 1, 0, true, false, 1)
                createCircle("b", "", 4, 0, 0.5, 1, 1, 0, true, false, 1)
                createJoint({
                    name = "link", end1 = "a", end2 = "b", type = 1,
                    x1 = 0, y1 = 0, x2 = 0, y2 = 0
                })
                setWorldGravity(0, 0)
                setVelocity("a", 3, 0)
                setSleeping("b", true)
                update = function() end
                updatePhysics = function() end
                "#,
        )
        .unwrap();

    runtime.update(1.0 / 30.0).unwrap();
    let bridge = runtime.render.lock().unwrap();
    assert!(!bridge.scene["b"].sleeping);
    assert!(
        bridge.scene["b"].x > 4.0,
        "the endpoint woken through the joint must integrate this island step"
    );
}

#[test]
fn connected_island_sleeps_only_at_common_minimum_time() {
    let runtime = unlocked_test_runtime();
    runtime
        .execute_source(
            r#"
                createCircle("a", "", 0, 0, 0.5, 1, 1, 0, true, false, 1)
                createCircle("b", "", 4, 0, 0.5, 1, 1, 0, true, false, 1)
                createJoint({
                    name = "link", end1 = "a", end2 = "b", type = 1,
                    x1 = 0, y1 = 0, x2 = 0, y2 = 0
                })
                "#,
        )
        .unwrap();

    let mut bridge = runtime.render.lock().unwrap();
    bridge.scene.get_mut("a").unwrap().sleep_time = 0.49;
    bridge.scene.get_mut("b").unwrap().sleep_time = 0.0;
    bridge.assemble_box2d_islands();
    bridge.update_box2d_island_sleep(1.0 / 30.0, true);
    assert!(!bridge.scene["a"].sleeping);
    assert!(!bridge.scene["b"].sleeping);

    bridge.scene.get_mut("a").unwrap().sleep_time = 0.49;
    bridge.scene.get_mut("b").unwrap().sleep_time = 0.49;
    bridge.update_box2d_island_sleep(1.0 / 30.0, true);
    assert!(bridge.scene["a"].sleeping);
    assert!(bridge.scene["b"].sleeping);
    assert_eq!(bridge.scene["a"].velocity_x, 0.0);
    assert_eq!(bridge.scene["b"].velocity_x, 0.0);
}

#[test]
fn island_sleep_uses_native_threshold_word_and_nonfused_linear_sum() {
    let runtime = unlocked_test_runtime();
    runtime
        .execute_source(
            r#"
                createCircle("body", "", 0, 0, 0.5, 1, 1, 0, true, false, 1)
                "#,
        )
        .unwrap();

    let mut bridge = runtime.render.lock().unwrap();
    {
        let body = bridge.scene.get_mut("body").unwrap();
        body.velocity_x = f64::from(f32::from_bits(0x3CF5_C07C));
        body.velocity_y = f64::from(f32::from_bits(0x3D23_D7D2));
        body.angular_velocity = 0.0;
        body.motion_started = true;
        body.sleeping = false;
    }
    bridge.assemble_box2d_islands();
    bridge.update_box2d_island_sleep(f64::from(f32::from_bits(0x3D08_8889)), false);
    assert_eq!(
        bridge.scene["body"].sleep_time,
        f64::from(f32::from_bits(0x3D08_8889)),
        "the exact native 0x3B23D70B linear threshold is inclusive"
    );

    {
        let body = bridge.scene.get_mut("body").unwrap();
        body.velocity_x = f64::from(f32::from_bits(0x3CF5_C20D));
        body.velocity_y = f64::from(f32::from_bits(0x3D23_D73C));
        body.sleep_time = 0.25;
    }
    bridge.update_box2d_island_sleep(f64::from(f32::from_bits(0x3D08_8889)), false);
    assert_eq!(
        bridge.scene["body"].sleep_time, 0.0,
        "native separate squares sum to 0x3B23D70C and exceed the threshold"
    );
}

#[test]
fn native_force_accumulates_torque_while_rotation_is_fixed() {
    let runtime = unlocked_test_runtime();
    runtime
        .execute_source(
            r##"
                createBox("body", "", 0, 0, 2, 2, 1, 0, 0, true, false, 1)
                setWorldGravity(0, 0)
                setFixedRotation("body", true)
                applyForceNative("body", 0, 12, 1, 0)
                setFixedRotation("body", false)
                update = function() end
                updatePhysics = function() end
                "##,
        )
        .unwrap();

    runtime.update(1.0 / 30.0).unwrap();
    let environment = game_environment(runtime.lua()).unwrap();
    let angular_velocity = match environment.get::<Value>("getAngularVelocity").unwrap() {
        Value::Function(function) => function.call::<f64>("body").unwrap(),
        _ => unreachable!(),
    };
    assert!(angular_velocity > 0.1);
}

#[test]
fn native_frame_clears_lua_force_closures_even_without_a_fixed_step() {
    let runtime = unlocked_test_runtime();
    runtime
        .execute_source(
            r##"
                createCircle("body", "", 0, 0, 1, 1, 0, 0, true, false, 1)
                setWorldGravity(0, 0)
                applied_force_count = 0
                clear_force_count = 0
                g_forceFunctions = {
                    function()
                        applied_force_count = applied_force_count + 1
                        applyForceNative("body", 1, 0, 0, 0)
                    end
                }
                updatePhysics = function()
                    for _, force in ipairs(g_forceFunctions) do force() end
                end
                clearLuaForceFunctions = function()
                    clear_force_count = clear_force_count + 1
                    g_forceFunctions = {}
                end
                update = function() end
                "##,
        )
        .unwrap();

    // 1/120 does not execute a 1/30 fixed step, but Purple still clears
    // closures after testing the accumulator.
    runtime.update(1.0 / 120.0).unwrap();
    let environment = game_environment(runtime.lua()).unwrap();
    assert_eq!(environment.get::<i64>("applied_force_count").unwrap(), 0);
    assert_eq!(environment.get::<i64>("clear_force_count").unwrap(), 1);
    assert_eq!(
        environment
            .get::<mlua::Table>("g_forceFunctions")
            .unwrap()
            .raw_len(),
        0
    );

    runtime
        .execute_source(
            r##"
                g_forceFunctions = {
                    function()
                        applied_force_count = applied_force_count + 1
                        applyForceNative("body", 1, 0, 0, 0)
                    end
                }
                "##,
        )
        .unwrap();
    {
        let mut bridge = runtime.render.lock().unwrap();
        bridge.scene.get_mut("body").unwrap().motion_started = false;
        bridge.scene.get_mut("body").unwrap().sleeping = true;
    }
    runtime.update(1.0 / 30.0).unwrap();
    runtime.update(1.0 / 30.0).unwrap();
    assert_eq!(environment.get::<i64>("applied_force_count").unwrap(), 1);
    assert_eq!(environment.get::<i64>("clear_force_count").unwrap(), 3);
}

#[test]
fn physics_torque_uses_shape_inverse_inertia_instead_of_inverse_mass() {
    let runtime = unlocked_test_runtime();
    runtime
        .execute_source(
            r##"
                createBox("body", "", 0, 0, 2, 1, 1, 0, 0, true, false, 1)
                setWorldGravity(0, 0)
                applyImpulse("body", 1, 0, 0, 1)
                impulse_angular_velocity = getAngularVelocity("body")
                setAngularVelocity("body", 0)
                applyForceNative("body", 1, 0, 0, 1)
                update = function() end
                updatePhysics = function() end
                "##,
        )
        .unwrap();

    // A 2x1 box at density 1 has mass 2, inertia 5/6 and inverse inertia 1.2.
    // The native wrapper performs this accumulation in float32.
    let environment = game_environment(runtime.lua()).unwrap();
    assert!((environment.get::<f64>("impulse_angular_velocity").unwrap() + 1.2).abs() < 1e-6);
    runtime.update(1.0 / 30.0).unwrap();
    let native_step = f32::from_bits(0x3d08_8889);
    let native_velocity = (native_step * 1.2_f32).mul_add(-1.0_f32, 0.0_f32);
    let native_angular_drag = (-native_step).mul_add(1.0_f32, 1.0_f32);
    assert_eq!(
        runtime.render.lock().unwrap().scene["body"].angular_velocity,
        f64::from(native_velocity * native_angular_drag)
    );
}

#[test]
fn island_velocity_integration_uses_native_f32_fma_and_unit_damping_clamp() {
    let runtime = unlocked_test_runtime();
    runtime
        .execute_source(
            r#"
                createBox("body", "", 0, 0, 1, 1, 1, 0, 0, true, false, 1)
                "#,
        )
        .unwrap();

    let mut bridge = runtime.render.lock().unwrap();
    let body = bridge.scene.get_mut("body").unwrap();
    body.inverse_mass = 1.0;
    body.gravity_scale = 4.0;
    body.velocity_x = 1.0;
    body.velocity_y = -1.0;
    body.angular_velocity = 2.0;
    body.force_x = 3.0;
    body.force_y = -2.0;
    body.torque = 100.0;
    body.linear_damping = -4.0;
    body.angular_damping = 2.0;
    body.fixed_rotation = true;
    bridge.integrate_island_velocities(&["body".to_owned()], (2.0, -3.0), 0.25);

    let body = &bridge.scene["body"];
    // Negative damping saturates at one instead of amplifying velocity.
    assert_eq!(body.velocity_x, 3.75);
    assert_eq!(body.velocity_y, -4.5);
    // A fixed-rotation body has zero inverse inertia, but Box2D still
    // applies angular damping to an explicitly assigned angular velocity.
    assert_eq!(body.angular_velocity, 1.0);
    bridge.integrate_positions(0.25, 100.0, 100.0);
    assert_eq!(bridge.scene["body"].angle, 0.25);
}

#[test]
fn off_center_polygon_uses_box2d_center_of_mass_for_inertia_and_sweep() {
    let runtime = unlocked_test_runtime();
    runtime
        .execute_source(
            r#"
                clearVertices()
                addVertex(0, 0)
                addVertex(2, 0)
                addVertex(0, 2)
                createPolygon("triangle", "", 10, 20, 2, 2, 1, 0, 0, true, false, 1)
                "#,
        )
        .unwrap();

    let mut bridge = runtime.render.lock().unwrap();
    let triangle = bridge.scene.get_mut("triangle").unwrap();
    let local_center = triangle.local_center();
    let native_center = f64::from(0.666_666_6_f32);
    assert_eq!(local_center, (native_center, native_center));
    assert_eq!(triangle.inverse_inertia(), f64::from(1.124_999_9_f32));
    let center_before = triangle.native_world_center();
    let origin_before = (triangle.x, triangle.y);
    triangle.motion_started = true;
    triangle.sleeping = false;
    triangle.velocity_x = 0.0;
    triangle.velocity_y = 0.0;
    triangle.angular_velocity = 1.0;

    bridge.integrate_positions(0.1, 100.0, 100.0);
    let triangle = &bridge.scene["triangle"];
    let center_after = triangle.native_world_center();
    assert_eq!(center_after, center_before);
    assert_eq!(triangle.angle, f64::from(0.1_f32));
    assert!((triangle.x - origin_before.0).abs() > 1e-3);
    assert!((triangle.y - origin_before.1).abs() > 1e-3);
}

#[test]
fn object_parameter_mass_data_converts_origin_inertia_to_com_inertia() {
    let runtime = unlocked_test_runtime();
    runtime
        .execute_source(
            r#"
                clearVertices()
                addVertex(0, 0)
                addVertex(2, 0)
                addVertex(0, 2)
                createPolygon("triangle", "", 10, 20, 2, 2, 1, 0, 0, true, false, 1)
                setObjectParameter("triangle", 38, 10)
                "#,
        )
        .unwrap();

    let bridge = runtime.render.lock().unwrap();
    let triangle = &bridge.scene["triangle"];
    let center = triangle.local_center();
    let center_squared = (center.0 as f32).mul_add(center.0 as f32, (center.1 as f32).powi(2));
    let expected = (-triangle.body_mass).mul_add(center_squared, 10.0_f32);
    assert_eq!(triangle.moment_of_inertia, Some(f64::from(expected)));
    assert_eq!(triangle.inverse_inertia(), f64::from(expected.recip()));
}

#[test]
fn mass_reset_preserves_box2d_origin_and_shifts_center_velocity() {
    let runtime = unlocked_test_runtime();
    runtime
        .execute_source(
            r#"
                clearVertices()
                addVertex(0, 0)
                addVertex(2, 0)
                addVertex(0, 2)
                createPolygon("triangle", "", 0, 0, 2, 2, 1, 0, 0, true, false, 1)
                setAngularVelocity("triangle", 1)
                native_setDensity("triangle", 0)
                "#,
        )
        .unwrap();

    let bridge = runtime.render.lock().unwrap();
    let triangle = &bridge.scene["triangle"];
    assert_eq!((triangle.x, triangle.y), (0.0, 0.0));
    assert_eq!(triangle.local_center(), (0.0, 0.0));
    assert_eq!(triangle.inverse_mass, 1.0);
    assert_eq!(triangle.inverse_inertia(), 0.0);
    let native_center = f64::from(0.666_666_6_f32);
    assert_eq!(triangle.velocity_x, native_center);
    assert_eq!(triangle.velocity_y, -native_center);
}

#[test]
fn native_density_changes_only_head_fixture_before_resetting_compound_mass() {
    let runtime = unlocked_test_runtime();
    runtime
        .execute_source(
            r#"
                clearVertices()
                addVertex(2, -2); addVertex(8, -2); addVertex(8, 2)
                addVertex(6, 2); addVertex(6, 0); addVertex(4, 0)
                addVertex(4, 2); addVertex(2, 2)
                createPolygon("compound", "", 0, 0, 10, 10, 1, 0.2, 0.3, true, false, 1)
                "#,
        )
        .unwrap();

    let fixtures = match &runtime.render.lock().unwrap().scene["compound"].collision_shape {
        CollisionShape::Polygon { fixtures, .. } => fixtures.clone(),
        shape => panic!("expected polygon, got {shape:?}"),
    };
    assert!(fixtures.len() >= 2);

    runtime
        .execute_source(r#"native_setDensity("compound", 4)"#)
        .unwrap();
    {
        let bridge = runtime.render.lock().unwrap();
        let compound = &bridge.scene["compound"];
        let mut expected_densities = vec![1.0; fixtures.len()];
        *expected_densities.last_mut().unwrap() = 4.0;
        assert_eq!(compound.fixture_densities, expected_densities);
        let (expected_mass, expected_center, _) = compound.native_fixture_mass_data_f32();
        assert_eq!(compound.inverse_mass, f64::from(expected_mass.recip()));
        let center = compound.local_center();
        assert_eq!(
            center,
            (f64::from(expected_center.0), f64::from(expected_center.1))
        );
    }
    assert_eq!(
        object_world(runtime.lua())
            .unwrap()
            .get::<mlua::Table>("compound")
            .unwrap()
            .get::<f64>("density")
            .unwrap(),
        4.0
    );

    // A subsequent setPhysicsScale snapshots that Lua value and gives all
    // replacement fixtures the same density again.
    runtime
        .execute_source(r#"setPhysicsScale("compound", 1, 1)"#)
        .unwrap();
    let bridge = runtime.render.lock().unwrap();
    let compound = &bridge.scene["compound"];
    assert_eq!(compound.fixture_densities, vec![4.0; fixtures.len()]);
    let rebuilt_mass = compound.native_fixture_mass_data_f32().0;
    assert_eq!(compound.inverse_mass, f64::from(rebuilt_mass.recip()));
}

#[test]
fn native_density_reads_the_lua_stack_tail_and_reflects_after_mass_reset() {
    let runtime = unlocked_test_runtime();
    runtime
        .execute_source(
            r#"
                createBox("body", "", 0, 0, 2, 3, 2, 0, 0, true, false, 1)
                -- sub_100030D1C consumes only the last two stack values.
                density_leading_ok = pcall(
                    native_setDensity, "ignored", "body", 0.99999999
                )
                density_trailing_fails = not pcall(
                    native_setDensity, "body", 2, 3
                )
                density_short_fails = not pcall(native_setDensity, "body")
                density_top_type_fails = not pcall(
                    native_setDensity, "body", "bad"
                )

                objects.world.lua_only = { density = 9 }
                density_unknown_fails = not pcall(
                    native_setDensity, "lua_only", 4
                )
                density_unknown_value = objects.world.lua_only.density

                createNonPhysicsObject("visual", "", 0, 0, 1)
                density_bodyless_fails = not pcall(
                    native_setDensity, "visual", 5
                )
                "#,
        )
        .unwrap();

    let environment = game_environment(runtime.lua()).unwrap();
    for field in [
        "density_leading_ok",
        "density_trailing_fails",
        "density_short_fails",
        "density_top_type_fails",
        "density_unknown_fails",
        "density_bodyless_fails",
    ] {
        assert!(environment.get::<bool>(field).unwrap(), "{field}");
    }
    assert_eq!(
        environment.get::<f64>("density_unknown_value").unwrap(),
        9.0
    );
    let bridge = runtime.render.lock().unwrap();
    let body = &bridge.scene["body"];
    assert_eq!(body.density, 2.0);
    assert_eq!(body.fixture_densities, vec![f64::from(0.99999999_f32)]);
    assert_eq!(
        body.inverse_mass,
        f64::from((6.0_f32 * 0.99999999_f32).recip())
    );
    drop(bridge);
    assert_eq!(
        object_world(runtime.lua())
            .unwrap()
            .get::<mlua::Table>("body")
            .unwrap()
            .get::<f64>("density")
            .unwrap(),
        f64::from(0.99999999_f32)
    );
}

#[test]
fn native_resize_radius_replaces_fixture_without_lua_or_sensor_restoration() {
    let runtime = unlocked_test_runtime();
    runtime
        .execute_source(
            r#"
                createCircle("target", "", 0, 0, 1, 1, 0.2, 0.3, true, false, 1)
                createBox("other", "", 0, 0, 4, 4, 0, 0.2, 0.3, true, false, 1)
                setPhysicsScale("target", 0.5, 0.5)
                setAsSensor("target", true)
                resize_exit_count = 0
                exitCollision = function()
                    resize_exit_count = resize_exit_count + 1
                    resize_exit_radius = objects.world.target.radius
                    resize_exit_density = objects.world.target.density
                    resize_exit_scale = objects.world.target.scaleX
                end
                resize_name_rejected = not pcall(
                    native_resizeRadius, false, 2, 4, 0.7, 0.8
                )
                resize_radius_rejected = not pcall(
                    native_resizeRadius, "target", false, 4, 0.7, 0.8
                )
                resize_density_rejected = not pcall(
                    native_resizeRadius, "target", 2, false, 0.7, 0.8
                )
                resize_friction_rejected = not pcall(
                    native_resizeRadius, "target", 2, 4, false, 0.8
                )
                resize_restitution_rejected = not pcall(
                    native_resizeRadius, "target", 2, 4, 0.7, false
                )
                resize_short_rejected = not pcall(
                    native_resizeRadius, "target", 2, 4, 0.7
                )
                resize_missing_ok, resize_missing_error = pcall(
                    native_resizeRadius, "missing", 2, 4, 0.7, 0.8
                )
                resize_missing_error = tostring(resize_missing_error)
                "#,
        )
        .unwrap();
    {
        let mut bridge = runtime.render.lock().unwrap();
        assert!(bridge.refresh_contacts().iter().any(|event| event.began));
        let target = bridge.scene.get_mut("target").unwrap();
        target.sleeping = true;
        target.sleep_time = 0.5;
    }

    runtime
        .execute_source(r#"native_resizeRadius("target", 2, 4, 0.7, 0.8)"#)
        .unwrap();
    let environment = game_environment(runtime.lua()).unwrap();
    for field in [
        "resize_name_rejected",
        "resize_radius_rejected",
        "resize_density_rejected",
        "resize_friction_rejected",
        "resize_restitution_rejected",
        "resize_short_rejected",
    ] {
        assert!(environment.get::<bool>(field).unwrap(), "{field}");
    }
    assert!(!environment.get::<bool>("resize_missing_ok").unwrap());
    assert!(
        environment
            .get::<String>("resize_missing_error")
            .unwrap()
            .contains("Missing object: missing")
    );
    assert_eq!(environment.get::<i64>("resize_exit_count").unwrap(), 1);
    assert_eq!(environment.get::<f64>("resize_exit_radius").unwrap(), 1.0);
    assert_eq!(environment.get::<f64>("resize_exit_density").unwrap(), 1.0);
    assert_eq!(environment.get::<f64>("resize_exit_scale").unwrap(), 0.5);
    let world_target = object_world(runtime.lua())
        .unwrap()
        .get::<mlua::Table>("target")
        .unwrap();
    assert_eq!(world_target.get::<f64>("radius").unwrap(), 1.0);
    assert_eq!(world_target.get::<f64>("density").unwrap(), 1.0);
    assert_eq!(
        world_target.get::<f64>("friction").unwrap(),
        f64::from(0.2_f32)
    );
    assert_eq!(
        world_target.get::<f64>("restitution").unwrap(),
        f64::from(0.3_f32)
    );

    let mut bridge = runtime.render.lock().unwrap();
    let target = &bridge.scene["target"];
    assert!(matches!(target.collision_shape, CollisionShape::Circle { radius } if radius == 2.0));
    assert_eq!((target.physics_scale_x, target.physics_scale_y), (1.0, 1.0));
    assert_eq!(target.native_shape_radius, 2.0);
    assert_eq!(target.fixture_densities, vec![4.0]);
    assert_eq!(target.fixture_frictions, vec![f64::from(0.7_f32)]);
    assert_eq!(target.fixture_restitutions, vec![f64::from(0.8_f32)]);
    assert!(!target.sensor);
    assert!(!target.sleeping);
    assert_eq!(target.sleep_time, 0.0);
    assert_eq!(target.collision_aabb(), Some((-2.0, -2.0, 2.0, 2.0)));
    assert!(bridge.active_contacts.is_empty());
    let events = bridge.refresh_contacts();
    assert!(events.iter().any(|event| event.began && !event.sensor));
    assert_eq!(bridge.broad_phase_contacts.len(), 1);
}

#[test]
fn off_center_polygon_impulse_uses_box2d_world_center() {
    let runtime = unlocked_test_runtime();
    runtime
        .execute_source(
            r#"
                clearVertices()
                addVertex(0, 0)
                addVertex(2, 0)
                addVertex(0, 2)
                createPolygon("triangle", "", 0, 0, 2, 2, 1, 0, 0, true, false, 1)
                applyImpulse("triangle", 2, 0, 2 / 3, 2 / 3)
                "#,
        )
        .unwrap();

    let bridge = runtime.render.lock().unwrap();
    let triangle = &bridge.scene["triangle"];
    assert!((triangle.velocity_x - 1.0).abs() < 1e-9);
    assert!(triangle.velocity_y.abs() < 1e-9);
    assert!(triangle.angular_velocity.abs() < 1e-6);
}

#[test]
fn physics_step_uses_recovered_box2d_motion_clamps_and_sleep_thresholds() {
    let runtime = unlocked_test_runtime();
    runtime
        .execute_source(
            r##"
                createCircle("body", "", 0, 0, 1, 1, 0, 0, true, false, 1)
                setWorldGravity(0, 0)
                setVelocity("body", 1000, 0)
                setAngularVelocity("body", 1000)
                update = function() end
                updatePhysics = function() end
                "##,
        )
        .unwrap();

    runtime.update(1.0 / 30.0).unwrap();
    {
        let bridge = runtime.render.lock().unwrap();
        let body = &bridge.scene["body"];
        let native_step = f32::from_bits(0x3d08_8889);
        let native_angular_drag = (-native_step).mul_add(1.0_f32, 1.0_f32);
        let mut native_angular_velocity = 1_000.0_f32 * native_angular_drag;
        let native_rotation = native_step * native_angular_velocity;
        let native_max_rotation = NATIVE_MAX_ROTATION;
        assert_eq!(native_max_rotation.to_bits(), 0x3FC9_0FDB);
        assert_eq!(NATIVE_MAX_ROTATION_SQUARED.to_bits(), 0x401D_E9E7);
        native_angular_velocity *= native_max_rotation / native_rotation.abs();
        assert_eq!(body.x, 0.159_999_981_522_560_12);
        assert_eq!(body.angle, f64::from(native_step * native_angular_velocity));
        assert_eq!(body.velocity_x, 4.799_999_237_060_547);
        assert_eq!(body.angular_velocity, f64::from(native_angular_velocity));
    }

    runtime
        .execute_source(
            r#"
                setVelocity("body", 0, 0)
                setAngularVelocity("body", 0)
                "#,
        )
        .unwrap();
    for _ in 0..16 {
        runtime.update(1.0 / 30.0).unwrap();
    }
    assert!(runtime.render.lock().unwrap().scene["body"].sleeping);
}

#[test]
fn island_position_write_uses_native_fmadd_rounding() {
    let runtime = unlocked_test_runtime();
    runtime
        .execute_source(
            r#"
                createCircle("body", "", -100, 0, 1, 1, 0, 0, true, false, 1)
                "#,
        )
        .unwrap();

    let step = f32::from_bits(0x3D08_8889);
    let velocity_x = f32::from_bits(0xC043_851F);
    let separate = step * velocity_x + -100.0_f32;
    let fused = velocity_x.mul_add(step, -100.0_f32);
    assert_eq!(separate.to_bits(), 0xC2C8_3424);
    assert_eq!(fused.to_bits(), 0xC2C8_3423);

    let mut bridge = runtime.render.lock().unwrap();
    let body = bridge.scene.get_mut("body").unwrap();
    body.velocity_x = f64::from(velocity_x);
    body.motion_started = true;
    body.sleeping = false;
    bridge.integrate_positions(
        f64::from(step),
        f64::from(NATIVE_MAX_TRANSLATION),
        f64::from(NATIVE_MAX_ROTATION),
    );

    let body = &bridge.scene["body"];
    assert_eq!((body.x as f32).to_bits(), fused.to_bits());
    assert_ne!((body.x as f32).to_bits(), separate.to_bits());
}
