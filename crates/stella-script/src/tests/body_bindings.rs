use super::*;

#[test]
fn constructor_keeps_additional_gravity_separate_from_water_color_and_starts_white() {
    let runtime = StellaLua::new("/tmp").unwrap();
    let bridge = runtime.render.lock().unwrap();
    assert_eq!(bridge.additional_bird_gravity, -1.0);
    assert_eq!(bridge.water_color, [1.0; 4]);
    assert_eq!(bridge.background_color, [0xff; 3]);
    assert_eq!(bridge.physics_simulation_scale, 1.0);
}

#[test]
fn physics_body_bindings_preserve_native_velocity_contracts() {
    let runtime = StellaLua::new("/tmp").unwrap();
    runtime
        .execute_source(
            r#"
                createBox("body", "", 10, 20, 4, 6, 2, 0.3, 0.4, true, false, 1)
                setVelocity("body", 3, 4)
                setAngularVelocity("body", 1.25)
                setLinearDamping("body", 0.15)
                setAsSensor("body", true)
                velocity_magnitude = getVelocity("body")
                velocity_x, velocity_y = getLinearVelocity("body")
                angular_velocity = getAngularVelocity("body")
                setPhysicsEnabled(false)
                physics_enabled = isPhysicsEnabled()
                "#,
        )
        .unwrap();
    let environment = game_environment(runtime.lua()).unwrap();
    assert_eq!(environment.get::<f64>("velocity_magnitude").unwrap(), 5.0);
    assert_eq!(environment.get::<f64>("velocity_x").unwrap(), 3.0);
    assert_eq!(environment.get::<f64>("velocity_y").unwrap(), 4.0);
    assert_eq!(environment.get::<f64>("angular_velocity").unwrap(), 1.25);
    assert!(!environment.get::<bool>("physics_enabled").unwrap());

    let bridge = runtime.render.lock().unwrap();
    let body = &bridge.scene["body"];
    assert!(body.dynamic_body);
    assert!(body.sensor);
    assert_eq!(body.linear_damping, f64::from(0.15_f32));
    assert!(body.collision_enabled);
}

#[test]
fn impulse_and_force_adapters_are_strict_float32_and_do_not_publish_lua_state() {
    let runtime = StellaLua::new("/tmp").unwrap();
    runtime
        .execute_source(
            r#"
                createBox("body", "", 0, 0, 2, 2, 1, 0, 0, true, false, 1)
                createNonPhysicsObject("visual", "", 0, 0, 1)
                objects.world.body.velocityX = 77
                objects.world.body.velocityY = 78
                objects.world.body.xVel = 79
                objects.world.body.yVel = 80
                objects.world.body.angularVelocity = 81
                objects.world.body.sleeping = "lua-sleeping"

                applyImpulse("body", 0.99999999, -2.0000001, 0.25, 1)
                applyForceNative("body", 3.0000001, 4.0000001, -1, 0.5)

                mirror_velocity_x = objects.world.body.velocityX
                mirror_velocity_y = objects.world.body.velocityY
                mirror_x_vel = objects.world.body.xVel
                mirror_y_vel = objects.world.body.yVel
                mirror_angular_velocity = objects.world.body.angularVelocity
                mirror_sleeping = objects.world.body.sleeping

                impulse_name_rejected = not pcall(
                    applyImpulse, false, 1, 2, 3, 4
                )
                impulse_x_rejected = not pcall(
                    applyImpulse, "body", false, 2, 3, 4
                )
                impulse_y_rejected = not pcall(
                    applyImpulse, "body", 1, false, 3, 4
                )
                impulse_point_x_rejected = not pcall(
                    applyImpulse, "body", 1, 2, false, 4
                )
                impulse_point_y_rejected = not pcall(
                    applyImpulse, "body", 1, 2, 3, false
                )
                force_short_rejected = not pcall(
                    applyForceNative, "body", 1, 2, 3
                )
                unknown_impulse_ok = pcall(
                    applyImpulse, "missing", 1, 2, 3, 4
                )
                bodyless_force_ok = pcall(
                    applyForceNative, "visual", 1, 2, 3, 4
                )
                "#,
        )
        .unwrap();

    let environment = game_environment(runtime.lua()).unwrap();
    for field in [
        "impulse_name_rejected",
        "impulse_x_rejected",
        "impulse_y_rejected",
        "impulse_point_x_rejected",
        "impulse_point_y_rejected",
        "force_short_rejected",
    ] {
        assert!(environment.get::<bool>(field).unwrap(), "{field}");
    }
    assert!(environment.get::<bool>("unknown_impulse_ok").unwrap());
    assert!(environment.get::<bool>("bodyless_force_ok").unwrap());
    assert_eq!(environment.get::<f64>("mirror_velocity_x").unwrap(), 77.0);
    assert_eq!(environment.get::<f64>("mirror_velocity_y").unwrap(), 78.0);
    assert_eq!(environment.get::<f64>("mirror_x_vel").unwrap(), 79.0);
    assert_eq!(environment.get::<f64>("mirror_y_vel").unwrap(), 80.0);
    assert_eq!(
        environment.get::<f64>("mirror_angular_velocity").unwrap(),
        81.0
    );
    assert_eq!(
        environment.get::<String>("mirror_sleeping").unwrap(),
        "lua-sleeping"
    );

    let bridge = runtime.render.lock().unwrap();
    let body = &bridge.scene["body"];
    assert_eq!((body.velocity_x, body.velocity_y), (0.25, -0.5));
    assert_eq!(body.angular_velocity, -0.5625);
    assert_eq!((body.force_x, body.force_y), (3.0, 4.0));
    assert_eq!(body.torque, -5.5);
    assert!(!body.sleeping);
    assert!(body.motion_started);
}

#[test]
fn impulse_and_force_writes_preserve_native_fmadd_boundaries() {
    let runtime = StellaLua::new("/tmp").unwrap();
    let old_velocity = (0.1234567_f32, -0.7654321_f32);
    let old_angular_velocity = 0.2468135_f32;
    let force = (f32::from_bits(0x410C_8719), f32::from_bits(0xC018_0EFF));
    let point = (f32::from_bits(0xC117_DB7A), f32::from_bits(0xBF54_0828));
    let impulse = (0.9876543_f32, -0.8765432_f32);

    runtime
        .execute_source(&format!(
            r#"
                createBox("body", "", 0, 0, 1.3, 2.7, 0.9, 0, 0, true, false, 1)
                setVelocity("body", {}, {})
                setAngularVelocity("body", {})
                applyForceNative("body", {}, {}, {}, {})
                applyImpulse("body", {}, {}, {}, {})
                "#,
            old_velocity.0,
            old_velocity.1,
            old_angular_velocity,
            force.0,
            force.1,
            point.0,
            point.1,
            impulse.0,
            impulse.1,
            point.0,
            point.1,
        ))
        .unwrap();

    let bridge = runtime.render.lock().unwrap();
    let body = &bridge.scene["body"];
    let inverse_mass = body.inverse_mass as f32;
    let inverse_inertia = body.inverse_inertia() as f32;
    let (center_x, center_y) = body.native_world_center();
    let force_first = (center_y - point.1) * force.0;
    let force_torque = (point.0 - center_x).mul_add(force.1, force_first);
    let impulse_first = (center_y - point.1) * impulse.0;
    let impulse_torque = (point.0 - center_x).mul_add(impulse.1, impulse_first);

    assert_eq!(
        (body.force_x as f32).to_bits(),
        force.0.to_bits(),
        "force x is one native FADD from zero"
    );
    assert_eq!((body.torque as f32).to_bits(), force_torque.to_bits());
    assert_eq!(
        (body.velocity_x as f32).to_bits(),
        inverse_mass.mul_add(impulse.0, old_velocity.0).to_bits()
    );
    assert_eq!(
        (body.velocity_y as f32).to_bits(),
        inverse_mass.mul_add(impulse.1, old_velocity.1).to_bits()
    );
    assert_eq!(
        (body.angular_velocity as f32).to_bits(),
        inverse_inertia
            .mul_add(impulse_torque, old_angular_velocity)
            .to_bits()
    );
}

#[test]
fn direct_body_flag_setters_preserve_native_wake_and_lua_mirroring_rules() {
    let runtime = StellaLua::new("/tmp").unwrap();
    runtime
        .execute_source(
            r#"
                createBox("body", "", 0, 0, 2, 2, 1, 0, 0, true, false, 1)
                setVelocity("body", 3, 4)
                setAngularVelocity("body", 2)
                "#,
        )
        .unwrap();

    runtime
        .execute_source(r#"setSleeping("body", true)"#)
        .unwrap();
    {
        let bridge = runtime.render.lock().unwrap();
        let body = &bridge.scene["body"];
        assert!(body.sleeping);
        assert_eq!((body.velocity_x, body.velocity_y), (0.0, 0.0));
        assert_eq!(body.angular_velocity, 0.0);
    }
    let world_body = object_world(runtime.lua())
        .unwrap()
        .get::<mlua::Table>("body")
        .unwrap();
    // sub_10004DAD4 changes only b2Body. Lua values are refreshed later
    // by the fixed-step write-back; the preceding setVelocity and
    // setAngularVelocity wrappers are native-only for the same reason.
    for field in ["sleeping", "velocityX", "velocityY", "angularVelocity"] {
        assert!(matches!(
            world_body.get::<Value>(field).unwrap(),
            Value::Nil
        ));
    }

    runtime
        .execute_source(r#"setSleeping("body", false)"#)
        .unwrap();
    {
        let mut bridge = runtime.render.lock().unwrap();
        assert!(!bridge.scene["body"].sleeping);
        bridge.scene.get_mut("body").unwrap().sleep_time = 0.4;
    }
    runtime
        .execute_source(r#"setSleeping("body", false)"#)
        .unwrap();
    assert_eq!(runtime.render.lock().unwrap().scene["body"].sleep_time, 0.4);

    runtime
        .execute_source(
            r#"
                setVelocity("body", 0.1, 0.2)
                multiplyVelocity("body", 3)
                setAngularVelocity("body", 0.4)
                "#,
        )
        .unwrap();
    {
        let bridge = runtime.render.lock().unwrap();
        assert_eq!(
            bridge.scene["body"].velocity_x,
            f64::from(0.1_f32 * 3.0_f32)
        );
        assert_eq!(
            bridge.scene["body"].velocity_y,
            f64::from(0.2_f32 * 3.0_f32)
        );
        assert_eq!(bridge.scene["body"].angular_velocity, f64::from(0.4_f32));
    }

    {
        let mut bridge = runtime.render.lock().unwrap();
        let body = bridge.scene.get_mut("body").unwrap();
        body.sleeping = true;
        body.sleep_time = 0.5;
    }
    runtime
        .execute_source(r#"setAsSensor("body", true)"#)
        .unwrap();
    {
        let mut bridge = runtime.render.lock().unwrap();
        let body = bridge.scene.get_mut("body").unwrap();
        assert!(body.sensor);
        assert!(!body.sleeping);
        assert_eq!(body.sleep_time, 0.0);
        body.sleeping = true;
        body.sleep_time = 0.7;
    }
    runtime
        .execute_source(r#"setAsSensor("body", true)"#)
        .unwrap();
    {
        let bridge = runtime.render.lock().unwrap();
        assert!(bridge.scene["body"].sleeping);
        assert_eq!(bridge.scene["body"].sleep_time, 0.7);
    }
    assert!(matches!(
        world_body.get::<Value>("sensor").unwrap(),
        Value::Nil
    ));

    runtime
        .execute_source(r#"setFixedRotation("body", true)"#)
        .unwrap();
    assert!(runtime.render.lock().unwrap().scene["body"].fixed_rotation);
    assert!(matches!(
        world_body.get::<Value>("fixedRotation").unwrap(),
        Value::Nil
    ));

    runtime
        .execute_source(
            r#"
                setLinearDamping("body", 0.15)
                setAngularDamping("body", 0.25)
                setGravityScale("body", 1.75)
                clearVertices()
                addVertex(-2, 0)
                addVertex(0, 0)
                addVertex(2, 0)
                createLineShape("chain", "", 0, 0, 4, 0, 0, 0.2, 0.3, true, false, 1)
                setRestitution("chain", 0.8)
                setFriction("chain", 0.6)
                createBox("static", "", 20, 0, 2, 2, 0, 0, 0, true, false, 1)
                setVelocity("static", 9, 9)
                setAngularVelocity("static", 9)
                "#,
        )
        .unwrap();
    let bridge = runtime.render.lock().unwrap();
    assert_eq!(bridge.scene["body"].linear_damping, f64::from(0.15_f32));
    assert_eq!(bridge.scene["body"].angular_damping, f64::from(0.25_f32));
    assert_eq!(bridge.scene["body"].gravity_scale, f64::from(1.75_f32));
    let chain = &bridge.scene["chain"];
    assert_eq!(
        chain.fixture_restitutions,
        vec![f64::from(0.3_f32), f64::from(0.8_f32)]
    );
    assert_eq!(
        chain.fixture_frictions,
        vec![f64::from(0.2_f32), f64::from(0.6_f32)]
    );
    assert_eq!(
        (
            bridge.scene["static"].velocity_x,
            bridge.scene["static"].velocity_y,
            bridge.scene["static"].angular_velocity,
        ),
        (0.0, 0.0, 0.0)
    );
    drop(bridge);
    assert!(matches!(
        world_body.get::<Value>("linearDamping").unwrap(),
        Value::Nil
    ));
    assert!(matches!(
        world_body.get::<Value>("angularDamping").unwrap(),
        Value::Nil
    ));
    assert!(matches!(
        world_body.get::<Value>("gravityScale").unwrap(),
        Value::Nil
    ));
    let world_chain = object_world(runtime.lua())
        .unwrap()
        .get::<mlua::Table>("chain")
        .unwrap();
    assert_eq!(
        world_chain.get::<f64>("restitution").unwrap(),
        f64::from(0.3_f32)
    );
    assert_eq!(
        world_chain.get::<f64>("friction").unwrap(),
        f64::from(0.2_f32)
    );
}

#[test]
fn generated_body_scalar_and_boolean_adapters_preserve_lookup_contracts() {
    let runtime = StellaLua::new("/tmp").unwrap();
    runtime
        .execute_source(
            r#"
                createBox("body", "", 0, 0, 2, 2, 1, 0, 0, true, false, 1)
                setRestitution("body", 0.123456789)
                setFriction("body", 0.234567891)
                setLinearDamping("body", 0.345678912)
                setAngularDamping("body", 0.456789123)
                setGravityScale("body", 0.567891234)
                setFixedRotation("body", true)
                setAsSensor("body", true)
                setSleeping("body", true)
                setCollisionEnabled("body", false)
                setActive("body", false)

                scalar_functions = {
                    setRestitution, setFriction, setLinearDamping,
                    setAngularDamping, setGravityScale
                }
                scalar_name_rejected = {}
                scalar_value_rejected = {}
                for index, fn in ipairs(scalar_functions) do
                    scalar_name_rejected[index] = not pcall(fn, false, 1)
                    scalar_value_rejected[index] = not pcall(fn, "body", false)
                end

                boolean_functions = {
                    setFixedRotation, setAsSensor, setSleeping,
                    setActive, setCollisionEnabled
                }
                boolean_name_rejected = {}
                boolean_value_rejected = {}
                for index, fn in ipairs(boolean_functions) do
                    boolean_name_rejected[index] = not pcall(fn, false, true)
                    boolean_value_rejected[index] = not pcall(fn, "body", 1)
                end

                nullable_missing_ok = {
                    pcall(setRestitution, "missing", 1),
                    pcall(setFriction, "missing", 1),
                    pcall(setLinearDamping, "missing", 1),
                    pcall(setAngularDamping, "missing", 1),
                    pcall(setFixedRotation, "missing", true),
                    pcall(setAsSensor, "missing", true),
                    pcall(setSleeping, "missing", true),
                    pcall(setActive, "missing", true)
                }
                gravity_missing_ok, gravity_missing_error = pcall(
                    setGravityScale, "missing", 1
                )
                gravity_missing_error = tostring(gravity_missing_error)
                collision_missing_ok, collision_missing_error = pcall(
                    setCollisionEnabled, "missing", true
                )
                collision_missing_error = tostring(collision_missing_error)
                "#,
        )
        .unwrap();

    let environment = game_environment(runtime.lua()).unwrap();
    for table_name in [
        "scalar_name_rejected",
        "scalar_value_rejected",
        "boolean_name_rejected",
        "boolean_value_rejected",
    ] {
        let values = environment.get::<mlua::Table>(table_name).unwrap();
        assert_eq!(values.raw_len(), 5, "{table_name}");
        for index in 1..=5 {
            assert!(
                values.raw_get::<bool>(index).unwrap(),
                "{table_name}[{index}]"
            );
        }
    }
    let nullable = environment
        .get::<mlua::Table>("nullable_missing_ok")
        .unwrap();
    assert_eq!(nullable.raw_len(), 8);
    for index in 1..=8 {
        assert!(
            nullable.raw_get::<bool>(index).unwrap(),
            "nullable[{index}]"
        );
    }
    for (ok_field, error_field) in [
        ("gravity_missing_ok", "gravity_missing_error"),
        ("collision_missing_ok", "collision_missing_error"),
    ] {
        assert!(!environment.get::<bool>(ok_field).unwrap(), "{ok_field}");
        assert!(
            environment
                .get::<String>(error_field)
                .unwrap()
                .contains("Missing object: missing"),
            "{error_field}"
        );
    }

    let bridge = runtime.render.lock().unwrap();
    let body = &bridge.scene["body"];
    assert_eq!(
        body.fixture_restitutions,
        vec![f64::from(0.123456789_f64 as f32)]
    );
    assert_eq!(
        body.fixture_frictions,
        vec![f64::from(0.234567891_f64 as f32)]
    );
    assert_eq!(body.linear_damping, f64::from(0.345678912_f64 as f32));
    assert_eq!(body.angular_damping, f64::from(0.456789123_f64 as f32));
    assert_eq!(body.gravity_scale, f64::from(0.567891234_f64 as f32));
    assert!(body.fixed_rotation);
    assert!(body.sensor);
    assert!(body.sleeping);
    assert!(!body.active);
    assert!(!body.collision_enabled);
}

#[test]
fn fixed_rotation_reset_discards_custom_mass_data_inertia() {
    let runtime = StellaLua::new("/tmp").unwrap();
    runtime
        .execute_source(
            r#"
                createBox("inertia", "", 0, 0, 2, 4, 1, 0, 0, true, false, 1)
                setObjectParameter("inertia", 38, 100)
                setFixedRotation("inertia", true)
                setFixedRotation("inertia", false)
                "#,
        )
        .unwrap();

    let bridge = runtime.render.lock().unwrap();
    let body = &bridge.scene["inertia"];
    assert!(!body.fixed_rotation);
    assert_eq!(body.moment_of_inertia, None);
    let fixture_inertia = body.native_fixture_mass_data_f32().2;
    assert_eq!(body.inverse_inertia(), f64::from(fixture_inertia.recip()));
    assert_ne!(body.inverse_inertia(), f64::from(100.0_f32.recip()));
}

#[test]
fn body_flag_and_activity_members_ignore_non_physics_objects() {
    let runtime = StellaLua::new("/tmp").unwrap();
    runtime
        .execute_source(
            r#"
                createNonPhysicsObject("visual", "", 0, 0, 1)
                before_collision = objects.world.visual.collisionEnabled
                setFixedRotation("visual", true)
                setAsSensor("visual", true)
                setSleeping("visual", true)
                setLinearDamping("visual", 3)
                setAngularDamping("visual", 4)
                setGravityScale("visual", 5)
                setActive("visual", false)
                setCollisionEnabled("visual", false)
                after_collision = objects.world.visual.collisionEnabled
                "#,
        )
        .unwrap();

    let environment = game_environment(runtime.lua()).unwrap();
    assert_eq!(
        environment.get::<Value>("after_collision").unwrap(),
        environment.get::<Value>("before_collision").unwrap()
    );
    let bridge = runtime.render.lock().unwrap();
    let visual = &bridge.scene["visual"];
    assert!(!visual.fixed_rotation);
    assert!(!visual.sensor);
    assert!(!visual.sleeping);
    assert!(visual.active);
    assert_eq!(visual.linear_damping, 0.0);
    assert_eq!(visual.angular_damping, 0.0);
    assert_eq!(visual.gravity_scale, 1.0);
}

#[test]
fn physics_locks_match_native_reference_counting_and_force_unlock() {
    let runtime = StellaLua::new("/tmp").unwrap();
    runtime
        .execute_source(
            r#"
                constructor_physics_enabled = isPhysicsEnabled()
                setPhysicsEnabled(true)
                setPhysicsEnabled(false, "pause")
                setPhysicsEnabled(false, "pause")
                setPhysicsEnabled(false, "other")
                enabled_with_three_locks = isPhysicsEnabled()
                unlockPhysicsLock("pause")
                enabled_with_other_lock = isPhysicsEnabled()
                setPhysicsEnabled(true, "other")
                enabled_after_named_locks = isPhysicsEnabled()

                setPhysicsEnabled(false)
                setPhysicsEnabled(false)
                enabled_with_default_lock = isPhysicsEnabled()
                setPhysicsEnabled(true)
                enabled_after_default_lock = isPhysicsEnabled()

                physics_enabled_missing_fails = not pcall(setPhysicsEnabled)
                physics_enabled_number_fails = not pcall(setPhysicsEnabled, 1)
                physics_lock_number_fails = not pcall(setPhysicsEnabled, false, 1)
                "#,
        )
        .unwrap();

    let environment = game_environment(runtime.lua()).unwrap();
    assert!(
        !environment
            .get::<bool>("constructor_physics_enabled")
            .unwrap()
    );
    assert!(!environment.get::<bool>("enabled_with_three_locks").unwrap());
    assert!(!environment.get::<bool>("enabled_with_other_lock").unwrap());
    assert!(
        environment
            .get::<bool>("enabled_after_named_locks")
            .unwrap()
    );
    assert!(
        !environment
            .get::<bool>("enabled_with_default_lock")
            .unwrap()
    );
    assert!(
        environment
            .get::<bool>("enabled_after_default_lock")
            .unwrap()
    );
    assert!(
        environment
            .get::<bool>("physics_enabled_missing_fails")
            .unwrap()
    );
    assert!(
        environment
            .get::<bool>("physics_enabled_number_fails")
            .unwrap()
    );
    assert!(
        environment
            .get::<bool>("physics_lock_number_fails")
            .unwrap()
    );
}

#[test]
fn constructor_lock_skips_native_physics_until_the_unnamed_release() {
    let runtime = StellaLua::new("/tmp").unwrap();
    runtime
        .execute_source(
            r#"
                physics_steps = 0
                hasMovingObjects = "retained-while-locked"
                updatePhysics = function()
                    physics_steps = physics_steps + 1
                end
                clearLuaForceFunctions = function() end
                update = function() end
            "#,
        )
        .unwrap();

    let native_step = f64::from(f32::from_bits(0x3D08_8889));
    runtime.update(native_step).unwrap();
    runtime.update(native_step).unwrap();
    let environment = game_environment(runtime.lua()).unwrap();
    assert_eq!(environment.get::<i64>("physics_steps").unwrap(), 0);
    assert_eq!(
        environment.get::<String>("hasMovingObjects").unwrap(),
        "retained-while-locked"
    );

    runtime.execute_source("setPhysicsEnabled(true)").unwrap();
    runtime.update(native_step).unwrap();
    assert_eq!(environment.get::<i64>("physics_steps").unwrap(), 1);
    assert!(runtime.render.lock().unwrap().physics_enabled);
}

#[test]
fn physics_scale_rebuilds_fixture_while_visual_scale_does_not() {
    let runtime = StellaLua::new("/tmp").unwrap();
    runtime
        .execute_source(
            r#"
                createBox("visual_only", "", 0, 0, 4, 6, 0, 0, 0, true, false, 1)
                createBox("physics_scaled", "", 0, 0, 4, 6, 2, 0, 0, true, false, 1)
                setScale("visual_only", 0.5, 0.25)
                setPhysicsScale("physics_scaled", 0.5, 0.25)
                native_setDensity("physics_scaled", 4)
                setCameraLimits(38.75)
                setLevelLimits(-11.9, -12.8, 13.7, 5.6)
                setGameParameters({ deterministicPhysics = true, gameWorldScale = 0.0867 })
                setWorldScale(0.0867)
                setMaxWorldScale(-0.125)
                setPhysicsSimulationScale(-0.125)
                setTopLeft(0.1, 0.2)
                setGameOn(true)
                enableSmoothZooming(true)
                smooth_zoom_number_fails = not pcall(enableSmoothZooming, 1)
                smooth_zoom_missing_fails = not pcall(enableSmoothZooming)
                resetMouseWheelScale(0.75)
                setAccelerometerActive(true)
                setEditing(false)
                native_setObjectWaterDrag(1.5)
                native_setBirdWaterDrag(0.4)
                native_setAdditionalBirdGravity(-1)
                native_setWaterColor(0.1, 0.8, 1, 0.5)
                setNotificationCallback("onNotificationReceived")
                os_name = native_getOSName()
                "#,
        )
        .unwrap();

    let bridge = runtime.render.lock().unwrap();
    let visual = bridge.scene["visual_only"].collision_aabb().unwrap();
    let skin = 0.002_f32;
    assert_eq!(
        visual,
        (
            f64::from(-2.0_f32 - skin),
            f64::from(-3.0_f32 - skin),
            f64::from(2.0_f32 + skin),
            f64::from(3.0_f32 + skin),
        )
    );
    let physical = bridge.scene["physics_scaled"].collision_aabb().unwrap();
    assert_eq!(
        physical,
        (
            f64::from(-1.0_f32 - skin),
            f64::from(-0.75_f32 - skin),
            f64::from(1.0_f32 + skin),
            f64::from(0.75_f32 + skin),
        )
    );
    assert_eq!(bridge.scene["physics_scaled"].scale_x, 0.5);
    assert_eq!(bridge.scene["physics_scaled"].scale_y, 0.25);
    // native_setDensity changes only b2Body::m_fixtureList; the retained
    // constructor/fixture-definition density is not a RenderObject field.
    assert_eq!(bridge.scene["physics_scaled"].density, 2.0);
    assert_eq!(bridge.scene["physics_scaled"].fixture_densities, vec![4.0]);
    assert_eq!(
        bridge.scene["physics_scaled"].inverse_mass,
        f64::from(12.0_f32.recip())
    );
    assert_eq!(bridge.camera_limit, 38.75);
    assert_eq!(bridge.level_limits, [-11.0, 13.0, -12.0, 5.0]);
    assert!(bridge.deterministic_physics);
    assert_eq!(bridge.game_world_scale, f64::from(0.0867_f32));
    assert_eq!(bridge.world_scale, f64::from(0.0867_f32));
    assert_eq!(bridge.max_world_scale, -0.125);
    assert_eq!(bridge.physics_simulation_scale, -0.125);
    assert_eq!(bridge.top_left_x, f64::from(0.1_f32));
    assert_eq!(bridge.top_left_y, f64::from(0.2_f32));
    assert!(bridge.game_on);
    assert!(bridge.smooth_zooming);
    assert_eq!(bridge.input_zoom.current, 0.75);
    assert_eq!(bridge.input_zoom.previous, 0.75);
    assert!(bridge.accelerometer_active);
    assert!(!bridge.editing);
    assert_eq!(bridge.object_water_drag, 1.5);
    assert_eq!(bridge.bird_water_drag, f64::from(0.4_f32));
    // Additional gravity remains at byte offset +0x224; water color lives at
    // +0x540..+0x54C and cannot overwrite it.
    assert_eq!(bridge.additional_bird_gravity, -1.0);
    assert_eq!(
        bridge.notification_callback.as_deref(),
        Some("onNotificationReceived")
    );
    drop(bridge);
    let environment = game_environment(runtime.lua()).unwrap();
    assert!(environment.get::<bool>("smooth_zoom_number_fails").unwrap());
    assert!(
        environment
            .get::<bool>("smooth_zoom_missing_fails")
            .unwrap()
    );
    assert_eq!(environment.get::<String>("os_name").unwrap(), "iOS");
}

#[test]
fn generated_world_scalar_and_boolean_adapters_are_strict_and_f32() {
    let runtime = StellaLua::new("/tmp").unwrap();
    runtime
        .execute_source(
            r#"
                setWorldScale(0.99999999)
                world_scale_missing_fails = not pcall(setWorldScale)
                world_scale_string_fails = not pcall(setWorldScale, "1")
                world_scale_after = worldScale

                resetMouseWheelScale(0.99999999)
                wheel_missing_fails = not pcall(resetMouseWheelScale)
                wheel_boolean_fails = not pcall(resetMouseWheelScale, true)

                setEditing(true)
                editing_number_fails = not pcall(setEditing, 1)
                editing_missing_fails = not pcall(setEditing)

                setAccelerometerActive(true)
                accelerometer_number_fails = not pcall(setAccelerometerActive, 1)
                accelerometer_missing_fails = not pcall(setAccelerometerActive)

                setWorldGravity(0.99999999, -0.99999999)
                gravity_missing_fails = not pcall(setWorldGravity, 7)
                gravity_string_fails = not pcall(setWorldGravity, 7, "-7")

                setGameOn(false)
                game_on_number_fails = not pcall(setGameOn, 0)
                game_on_missing_fails = not pcall(setGameOn)

                native_setObjectWaterDrag(0.99999999)
                object_drag_missing_fails = not pcall(native_setObjectWaterDrag)
                object_drag_boolean_fails = not pcall(native_setObjectWaterDrag, true)
                native_setBirdWaterDrag(0.99999999)
                bird_drag_string_fails = not pcall(native_setBirdWaterDrag, "1")
                native_setAdditionalBirdGravity(0.99999999)
                additional_gravity_missing_fails = not pcall(native_setAdditionalBirdGravity)

                native_setWaterColor(0.99999999, 1.9999999, 2.9999999, 3.9999999)
                water_color_short_fails = not pcall(native_setWaterColor, 8, 8, 8)
                water_color_last_string_fails = not pcall(native_setWaterColor, 8, 8, 8, "8")

                setPhysicsSimulationScale(0.99999999)
                simulation_scale_missing_fails = not pcall(setPhysicsSimulationScale)
                simulation_scale_string_fails = not pcall(setPhysicsSimulationScale, "1")

                enableAimingAid(true)
                aiming_number_fails = not pcall(enableAimingAid, 1)
                aiming_missing_fails = not pcall(enableAimingAid)

                setMaxWorldScale(0.99999999, 7)
                max_scale_missing_fails = not pcall(setMaxWorldScale)
                max_scale_boolean_fails = not pcall(setMaxWorldScale, false)

                setCameraLimits(0.99999999)
                camera_limit_missing_fails = not pcall(setCameraLimits)
                camera_limit_string_fails = not pcall(setCameraLimits, "1")

                setStartingCameraValue(true)
                starting_camera_number_fails = not pcall(setStartingCameraValue, 1)
                starting_camera_missing_fails = not pcall(setStartingCameraValue)

                setTopLeft(0.99999999, -0.99999999)
                top_left_short_fails = not pcall(setTopLeft, 7)
                top_left_string_fails = not pcall(setTopLeft, 7, "-7")

                setLevelLimits(-0.99999999, -1.9999999, 2.9999999, 3.9999999)
                level_limits_short_fails = not pcall(setLevelLimits, 8, 8, 8)
                level_limits_last_string_fails = not pcall(setLevelLimits, 8, 8, 8, "8")
                "#,
        )
        .unwrap();

    let environment = game_environment(runtime.lua()).unwrap();
    assert_eq!(environment.get::<f64>("world_scale_after").unwrap(), 1.0);
    for field in [
        "world_scale_missing_fails",
        "world_scale_string_fails",
        "wheel_missing_fails",
        "wheel_boolean_fails",
        "editing_number_fails",
        "editing_missing_fails",
        "accelerometer_number_fails",
        "accelerometer_missing_fails",
        "gravity_missing_fails",
        "gravity_string_fails",
        "game_on_number_fails",
        "game_on_missing_fails",
        "object_drag_missing_fails",
        "object_drag_boolean_fails",
        "bird_drag_string_fails",
        "additional_gravity_missing_fails",
        "water_color_short_fails",
        "water_color_last_string_fails",
        "simulation_scale_missing_fails",
        "simulation_scale_string_fails",
        "aiming_number_fails",
        "aiming_missing_fails",
        "max_scale_missing_fails",
        "max_scale_boolean_fails",
        "camera_limit_missing_fails",
        "camera_limit_string_fails",
        "starting_camera_number_fails",
        "starting_camera_missing_fails",
        "top_left_short_fails",
        "top_left_string_fails",
        "level_limits_short_fails",
        "level_limits_last_string_fails",
    ] {
        assert!(environment.get::<bool>(field).unwrap(), "{field}");
    }

    let bridge = runtime.render.lock().unwrap();
    assert_eq!(bridge.world_scale, 1.0);
    assert_eq!(bridge.input_zoom.current, 1.0);
    assert_eq!(bridge.input_zoom.previous, 1.0);
    assert!(bridge.editing);
    assert!(bridge.accelerometer_active);
    assert_eq!(bridge.world_gravity_x, 1.0);
    assert_eq!(bridge.world_gravity_y, -1.0);
    assert!(!bridge.game_on);
    assert_eq!(bridge.object_water_drag, 1.0);
    assert_eq!(bridge.bird_water_drag, 1.0);
    assert_eq!(
        bridge.water_color,
        [
            f64::from(0.99999999_f64 as f32),
            f64::from(1.9999999_f64 as f32),
            f64::from(2.9999999_f64 as f32),
            f64::from(3.9999999_f64 as f32),
        ]
    );
    assert_eq!(bridge.additional_bird_gravity, 1.0);
    assert_eq!(bridge.physics_simulation_scale, 1.0);
    assert!(bridge.aiming_aid_enabled);
    // The generated adapter consumes slot one, so the trailing 7 is ignored.
    assert_eq!(bridge.max_world_scale, 1.0);
    assert_eq!(bridge.camera_limit, 1.0);
    assert!(bridge.starting_camera_value);
    assert_eq!(bridge.top_left_x, 1.0);
    assert_eq!(bridge.top_left_y, -1.0);
    assert_eq!(bridge.level_limits, [-1.0, 3.0, -1.0, 4.0]);
}

#[test]
fn physics_scale_matches_native_fixture_rebuild_lifecycle_and_sources() {
    let runtime = StellaLua::new("/tmp").unwrap();
    runtime
        .execute_source(
            r#"
                createBox("scaled", "", 0, 0, 2, 2, 1, 0.2, 0.3, true, false, 1)
                createBox("other", "", 0, 0, 2, 2, 0, 0.2, 0.3, true, false, 1)
                setAsSensor("scaled", true)
                objects.world.scaled.width = 999
                objects.world.scaled.density = 2.5
                objects.world.scaled.friction = 0.7
                objects.world.scaled.restitution = 0.8
                scale_exit_count = 0
                exitCollision = function()
                    scale_exit_count = scale_exit_count + 1
                    scale_exit_scale_x = objects.world.scaled.scaleX
                    scale_exit_width = objects.world.scaled.width
                    -- sub_10004050C snapshots fixture coefficients before
                    -- DestroyFixture invokes this callback.
                    objects.world.scaled.density = 9
                end

                clearVertices()
                addVertex(2, -2); addVertex(8, -2); addVertex(8, 2)
                addVertex(6, 2); addVertex(6, 0); addVertex(4, 0)
                addVertex(4, 2); addVertex(2, 2)
                createPolygon("compound", "", 30, 0, 10, 10, 1, 0.2, 0.3, true, false, 1)
                objects.world.compound.width = 777

                -- GameLua+0x458 retains blockTable; the global `blocks`
                -- namespace is the unrelated component system.
                blockTable = { blocks = { circle_definition = { scale = 2 } } }
                blocks = { circle_definition = { scale = 99 } }
                createCircle("circle", "", 50, 0, 1, 1, 0.1, 0.2, true, false, 1)
                objects.world.circle.definition = "circle_definition"
                objects.world.circle.radius = 3

                clearVertices()
                addVertex(-1, 0); addVertex(1, 0)
                createLineShape("edge", "", 70, 0, 2, 0, 0, 0.2, 0.3, true, false, 1)
                "#,
        )
        .unwrap();

    let old_compound_fixtures = {
        let mut bridge = runtime.render.lock().unwrap();
        assert!(bridge.refresh_contacts().iter().any(|event| event.began));
        assert_eq!(bridge.active_contacts.len(), 1);
        let scaled = bridge.scene.get_mut("scaled").unwrap();
        scaled.sleeping = true;
        scaled.sleep_time = 0.5;
        match &bridge.scene["compound"].collision_shape {
            CollisionShape::Polygon { fixtures, .. } => {
                assert!(fixtures.len() >= 2);
                fixtures.clone()
            }
            shape => panic!("expected compound polygon, got {shape:?}"),
        }
    };

    runtime
        .execute_source(
            r#"
                setPhysicsScale("scaled", -2, 0.5)
                setPhysicsScale("compound", -2, 0.5)
                setPhysicsScale("circle", 4, 6)
                -- Reassigning the global does not retarget GameLua+0x458.
                blockTable = { blocks = { circle_definition = { scale = 100 } } }
                setPhysicsScale("circle", 8, 10)
                edge_resize_ok, edge_resize_error = pcall(setPhysicsScale, "edge", -3, 4)
                edge_resize_error = tostring(edge_resize_error)
                "#,
        )
        .unwrap();

    let environment = game_environment(runtime.lua()).unwrap();
    assert_eq!(environment.get::<i64>("scale_exit_count").unwrap(), 1);
    assert_eq!(environment.get::<f64>("scale_exit_scale_x").unwrap(), -2.0);
    // The native width field (2), not the directly edited Lua value
    // (999), is multiplied before EndContact runs.
    assert_eq!(environment.get::<f64>("scale_exit_width").unwrap(), 4.0);
    assert!(!environment.get::<bool>("edge_resize_ok").unwrap());
    assert!(
        environment
            .get::<String>("edge_resize_error")
            .unwrap()
            .contains("unsupported type")
    );

    let world = object_world(runtime.lua()).unwrap();
    let scaled_world = world.get::<mlua::Table>("scaled").unwrap();
    assert_eq!(scaled_world.get::<f64>("density").unwrap(), 9.0);
    let edge_world = world.get::<mlua::Table>("edge").unwrap();
    assert_eq!(edge_world.get::<f64>("scaleX").unwrap(), -3.0);
    assert_eq!(edge_world.get::<f64>("scaleY").unwrap(), 4.0);

    let mut bridge = runtime.render.lock().unwrap();
    let scaled = &bridge.scene["scaled"];
    assert_eq!(
        (scaled.physics_scale_x, scaled.physics_scale_y),
        (-2.0, 0.5)
    );
    assert_eq!(
        (scaled.native_shape_width, scaled.native_shape_height),
        (4.0, 1.0)
    );
    assert_eq!(scaled.density, 2.5);
    assert_eq!(scaled.fixture_frictions, vec![f64::from(0.7_f32)]);
    assert_eq!(scaled.fixture_restitutions, vec![f64::from(0.8_f32)]);
    assert!(scaled.sensor);
    assert!(!scaled.sleeping);
    assert_eq!(scaled.sleep_time, 0.0);
    assert!(bridge.active_contacts.is_empty());

    let compound = &bridge.scene["compound"];
    assert_eq!(compound.physics_scale_x, -2.0);
    assert_eq!(compound.physics_scale_y, 0.5);
    assert_eq!(
        (compound.native_shape_width, compound.native_shape_height),
        (20.0, 5.0)
    );
    let new_compound_fixtures = match &compound.collision_shape {
        CollisionShape::Polygon { fixtures, .. } => fixtures,
        _ => unreachable!(),
    };
    assert_eq!(
        new_compound_fixtures,
        &old_compound_fixtures.into_iter().rev().collect::<Vec<_>>()
    );

    let circle = &bridge.scene["circle"];
    let expected_circle_scale = f64::from(4.0_f32 + 0.0001_f32);
    assert_eq!(circle.physics_scale_x, expected_circle_scale);
    assert_eq!(circle.physics_scale_y, expected_circle_scale);
    assert_eq!(
        circle.native_shape_radius,
        f64::from((4.0_f32 + 0.0001_f32) * 3.0_f32)
    );
    assert!(matches!(circle.collision_shape, CollisionShape::Circle { radius } if radius == 3.0));

    assert_eq!(bridge.scene["edge"].scale_x, -3.0);
    assert_eq!(bridge.scene["edge"].scale_y, 4.0);
    // The signed X ratio reverses the rebuilt box winding. Purple's solid
    // manifold path retains the inward normals and rejects this pair, but the
    // fixture is a sensor: b2Contact::Update instead calls radius-aware GJK,
    // whose distance proxy uses vertices and still reports the overlap.
    assert!(
        bridge.scene["scaled"]
            .collision_fixture_manifold(&bridge.scene["other"], 0, 0)
            .is_none()
    );
    assert!(bridge.scene["scaled"].native_fixture_overlaps(&bridge.scene["other"], 0, 0));
    assert!(
        bridge
            .refresh_contacts()
            .iter()
            .any(|event| event.began && event.sensor)
    );
}

#[test]
fn object_parameter_scale_does_not_resize_box2d_fixture() {
    let runtime = StellaLua::new("/tmp").unwrap();
    runtime
        .execute_source(
            r#"
                createCircle("pig", "", 5, -6, 1, 2, 0, 0, true, false, 1)
                setObjectParameter("pig", 5, 0.09)
                setObjectParameter("pig", 17, 0.2)
                setObjectParameter("pig", 18, 0.3)
                "#,
        )
        .unwrap();

    let bridge = runtime.render.lock().unwrap();
    let pig = &bridge.scene["pig"];
    assert_eq!(pig.scale_x, f64::from(0.2_f32));
    assert_eq!(pig.scale_y, f64::from(0.3_f32));
    assert_eq!(pig.physics_scale_x, 1.0);
    assert_eq!(pig.physics_scale_y, 1.0);
    assert_eq!(pig.collision_aabb().unwrap(), (4.0, -7.0, 6.0, -5.0));
    let native_mass = 2.0_f32 * std::f32::consts::PI;
    assert_eq!(pig.inverse_mass, f64::from(native_mass.recip()));
}

#[test]
fn dynamic_non_positive_fixture_mass_uses_box2d_unit_mass_fallback() {
    let runtime = StellaLua::new("/tmp").unwrap();
    runtime
        .execute_source(
            r#"
                createBox("zeroed", "", 0, 0, 2, 3, 2, 0, 0, true, false, 1)
                native_setDensity("zeroed", 0)
                createBox("negative", "", 0, 0, 2, 3, -2, 0, 0, true, false, 1)
                createCircle("resized", "", 0, 0, 2, 1, 0, 0, true, false, 1)
                native_resizeRadius("resized", 3, 0, 0, 0)
                "#,
        )
        .unwrap();

    let bridge = runtime.render.lock().unwrap();
    assert_eq!(bridge.scene["zeroed"].inverse_mass, 1.0);
    assert_eq!(bridge.scene["negative"].inverse_mass, 1.0);
    assert_eq!(bridge.scene["resized"].inverse_mass, 1.0);
    assert_eq!(bridge.scene["zeroed"].inverse_inertia(), 0.0);
    assert_eq!(bridge.scene["negative"].inverse_inertia(), 0.0);
    assert_eq!(bridge.scene["resized"].inverse_inertia(), 0.0);
    drop(bridge);
    let world = object_world(runtime.lua()).unwrap();
    assert_eq!(
        world
            .get::<mlua::Table>("negative")
            .unwrap()
            .get::<f64>("mass")
            .unwrap(),
        1.0
    );
}
