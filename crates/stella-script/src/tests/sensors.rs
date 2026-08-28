use super::*;

fn load_chapter02_level02(label: &str) -> (ShippedDataSandbox, StellaLua) {
    let sandbox = ShippedDataSandbox::new(label);
    let runtime = StellaLua::new(&sandbox.data_root).unwrap();
    runtime.boot("scripts/game.lua").unwrap();
    runtime.execute_source("initializeGameCommon()").unwrap();
    runtime
        .execute_source(
            r#"
                SpriteSheetManager.useGroupSet('INGAME')
                currentFolder = 'Chapter02'
                currentPack = 'Chapter02'
                currentLevel = 2
                levelFolder = 'levels/Chapter02/'
                levelName = 'Chapter02_L02'
                loadLevelInternal(levelFolder .. levelName)
                blocks.BlockComponentManager.triggerGlobalEvent(blocks.events.EID_START)
                setPhysicsEnabled(true)
            "#,
        )
        .unwrap();
    (sandbox, runtime)
}

#[test]
fn chapter02_level02_trap_sucker_retains_authored_fixture() {
    let (_sandbox, runtime) = load_chapter02_level02("chapter02-l02-trap-sucker-fixture");
    let world = object_world(runtime.lua()).unwrap();
    let hub = world.get::<mlua::Table>("BLOCK_SUCKER_TRAP_HUB_3").unwrap();
    let sensor_name = hub.get::<String>("suckerSensorName").unwrap();
    let bridge = runtime.render.lock().unwrap();
    let sensor = &bridge.scene[&sensor_name];

    assert!(sensor.sensor);
    let CollisionShape::Box { width, height } = sensor.collision_shape else {
        panic!("name={sensor_name} shape={:?}", sensor.collision_shape);
    };
    assert_eq!(
        (
            width * sensor.physics_scale_x,
            height * sensor.physics_scale_y
        ),
        (f64::from(0.5_f32), f64::from(0.5_f32)),
        "name={sensor_name} authored=({width}, {height}) physics_scale=({}, {})",
        sensor.physics_scale_x,
        sensor.physics_scale_y
    );
}

#[test]
fn chapter02_level02_moving_static_intake_does_not_wake_a_sleeping_structure() {
    let (_sandbox, runtime) = load_chapter02_level02("chapter02-l02-sleeping-intake");
    const TARGET: &str = "BLOCK_WOOD_1X10_1_9";
    {
        let mut bridge = runtime.render.lock().unwrap();
        let target = bridge.scene.get_mut(TARGET).unwrap();
        target.sleeping = true;
        target.sleep_time = 0.0;
        target.velocity_x = 0.0;
        target.velocity_y = 0.0;
        target.angular_velocity = 0.0;
    }
    runtime
        .execute_source(
            r#"
                getAudioName = function() return "" end
                isAudioPlaying = function() return false end
                blocks.BlockComponentManager.triggerDelayedEvent = function(object, eventName, arg)
                    blocks.BlockComponentManager.triggerEvent(object, eventName, arg)
                end
                update = function() end
                local target = objects.world.BLOCK_WOOD_1X10_1_9
                setPosition("TrapSuckerSensor_1", target.x, target.y)
                setRotation("TrapSuckerSensor_1", target.angle)
            "#,
        )
        .unwrap();

    let pair_matches = |key: &ContactKey| {
        (key.0 == TARGET && key.1 == "TrapSuckerSensor_1")
            || (key.0 == "TrapSuckerSensor_1" && key.1 == TARGET)
    };
    {
        let bridge = runtime.render.lock().unwrap();
        assert!(
            bridge.scene[TARGET].sleeping,
            "SetTransform must not wake the sleeping dynamic endpoint"
        );
        assert!(
            bridge.broad_phase_contacts.iter().any(pair_matches),
            "SetTransform must still create the broad-phase pair"
        );
        assert!(
            !bridge.active_contacts.keys().any(pair_matches),
            "the sleeping pair must remain outside Contact::Update"
        );
    }

    // Purple runs physics at 30 Hz while the host callback runs at the
    // display cadence, so two 60 Hz updates cross one complete contact step.
    runtime.update(1.0 / 60.0).unwrap();
    runtime.update(1.0 / 60.0).unwrap();

    let target = object_world(runtime.lua())
        .unwrap()
        .get::<mlua::Table>(TARGET)
        .unwrap();
    assert!(
        !target.get::<bool>("inTrapSucker").unwrap_or(false),
        "a moving static sensor must not bypass ContactManager::Collide's awake gate"
    );
}

#[test]
fn chapter02_level02_toppled_intake_captures_the_right_structure() {
    let (_sandbox, runtime) = load_chapter02_level02("chapter02-l02-toppled-capture");
    runtime
        .execute_source(
            r#"
                getAudioName = function() return "" end
                isAudioPlaying = function() return false end
                blocks.BlockComponentManager.triggerDelayedEvent = function(object, eventName, arg)
                    blocks.BlockComponentManager.triggerEvent(object, eventName, arg)
                end
                removeBlocks = function() deadBlocks = {} end
                local target = objects.world.BLOCK_WOOD_1X10_1_9
                setPosition("TrapSuckerSensor_1", target.x, target.y)
                setRotation("TrapSuckerSensor_1", math.pi * 0.5)
                update = function() end
            "#,
        )
        .unwrap();

    let mut captured = false;
    for _ in 0..2 {
        runtime.update(1.0 / 60.0).unwrap();
        let target = object_world(runtime.lua())
            .unwrap()
            .get::<mlua::Table>("BLOCK_WOOD_1X10_1_9")
            .unwrap();
        if target.get::<bool>("inTrapSucker").unwrap_or(false) {
            captured = true;
            break;
        }
    }
    assert!(
        captured,
        "the toppled intake crossed the right-hand structure without capturing it"
    );
}

#[test]
fn sensor_force_member_preserves_exact_string_slots_and_ignores_trailing_values() {
    let runtime = StellaLua::new("/tmp").unwrap();
    runtime
        .execute_source(
            r#"
                createBox("sensor", "", 0, 0, 2, 2, 0, 0, 0, true, false, 1)
                createBox("target", "", 0, 0, 2, 2, 1, 0, 0, true, false, 1)
                missing_fails = not pcall(native_applySensorForces, "sensor")
                sensor_number_fails = not pcall(
                    native_applySensorForces, 1, "target")
                target_number_fails = not pcall(
                    native_applySensorForces, "sensor", 2)
                trailing_ok = pcall(
                    native_applySensorForces, "sensor", "target", false)
            "#,
        )
        .unwrap();

    let environment = game_environment(runtime.lua()).unwrap();
    assert!(environment.get::<bool>("missing_fails").unwrap());
    assert!(environment.get::<bool>("sensor_number_fails").unwrap());
    assert!(environment.get::<bool>("target_number_fails").unwrap());
    assert!(environment.get::<bool>("trailing_ok").unwrap());
}

#[test]
fn recovered_completion_bindings_update_native_physics_and_render_state() {
    let runtime = StellaLua::new("/tmp").unwrap();
    runtime
        .execute_source(
            r#"
                createBox("sensor", "", 0, 0, 10, 10, 0, 0, 0, true, false, 1)
                createBox("target", "", 3, 0, 2, 2, 1, 0.25, 0.1, true, false, 1)
                setObjectParameter("sensor", 20, 1)
                setObjectParameter("sensor", 21, 2)
                setObjectParameter("sensor", 22, 1)
                setObjectParameter("sensor", 27, 10)
                setSensorMinimumAndMaximumForces("sensor", 10, 20)
                native_applySensorForces("sensor", "target")
                target_vertices = getObjectVertices("target")
                createJoint({
                    name = "completion_joint", end1 = "sensor", end2 = "target",
                    type = 3, x1 = 0, y1 = 0, x2 = 0, y2 = 0
                })
                setJointParameters({
                    name = "completion_joint", motor = true, motorSpeed = 3,
                    maxTorque = 12, limit = true, lowerLimit = -0.5,
                    upperLimit = 0.75
                })
                native_resizeRadius("target", 4, 2, 0.4, 0.2)
                setSpriteRotation("target", 7)
                render_disable_missing_fails = not pcall(setGameRenderingDisabled)
                render_disable_number_fails = not pcall(setGameRenderingDisabled, 1)
                setGameRenderingDisabled(true, false)
                render_disable_uses_top = not isGameRenderingDisabled()
                setGameRenderingDisabled(true)
                render_disabled_result = isGameRenderingDisabled()
                "#,
        )
        .unwrap();

    let environment = game_environment(runtime.lua()).unwrap();
    assert!(environment.get::<bool>("render_disabled_result").unwrap());
    assert!(
        environment
            .get::<bool>("render_disable_missing_fails")
            .unwrap()
    );
    assert!(
        environment
            .get::<bool>("render_disable_number_fails")
            .unwrap()
    );
    assert!(environment.get::<bool>("render_disable_uses_top").unwrap());
    let vertices: mlua::Table = environment.get("target_vertices").unwrap();
    assert_eq!(vertices.raw_get::<mlua::Table>(1).unwrap().raw_len(), 4);
    let bridge = runtime.render.lock().unwrap();
    let target = &bridge.scene["target"];
    assert!((target.force_x - f64::from(-6.8_f32)).abs() < 1e-6);
    assert_eq!(target.force_y, 0.0);
    assert!(matches!(target.collision_shape, CollisionShape::Circle { radius } if radius == 4.0));
    let native_turn = std::f32::consts::PI + std::f32::consts::PI;
    assert_eq!(target.sprite_rotation, f64::from(7.0_f32 % native_turn));
    let joint = &bridge.joints["completion_joint"];
    assert!(joint.motor_enabled);
    assert_eq!(joint.motor_speed, Some(3.0));
    assert_eq!(joint.max_torque, 12.0);
    assert!(joint.limits_enabled);
    assert_eq!((joint.lower_limit, joint.upper_limit), (-0.5, 0.75));
}

#[test]
fn recovered_object_extension_members_keep_native_float_and_lua_mirror_boundaries() {
    let runtime = StellaLua::new("/tmp").unwrap();
    runtime
        .execute_source(
            r#"
                createBox("body", "", 0, 0, 2, 2, 1, 0, 0, true, false, 1)
                objects.world.body.timeSinceCollision = 77
                objects.world.body.revertGravityWithMultiplier = "lua-only"
                objects.world.body.revertGravityForce = 88
                objects.world.body.revertGravityMaxVelocity = 99

                native_setTimeSinceCollision("body", 0.123456789)
                setRevertGravityWithMultiplier("body", true, 0.123456789, 9.87654321)
                setSpriteRotation("body", 123456.7890123, "ignored")
                setVelocity("body", 2, 3)
                multiplyVelocity("body", 2, "ignored")

                mirror_collision_time = objects.world.body.timeSinceCollision
                mirror_revert_mode = objects.world.body.revertGravityWithMultiplier
                mirror_revert_force = objects.world.body.revertGravityForce
                mirror_revert_limit = objects.world.body.revertGravityMaxVelocity
                mirror_sprite_angle = objects.world.body.spriteAngle

                bad_collision_name = pcall(native_setTimeSinceCollision, false, 1)
                bad_collision_value = pcall(native_setTimeSinceCollision, "body", false)
                bad_revert_flag = pcall(setRevertGravityWithMultiplier, "body", 1, 2, 3)
                bad_revert_force = pcall(setRevertGravityWithMultiplier, "body", true, false, 3)
                bad_sprite_name = pcall(setSpriteRotation, false, 1)
                bad_sprite_angle = pcall(setSpriteRotation, "body", "1")
                bad_velocity_name = pcall(multiplyVelocity, false, 1)
                bad_velocity_multiplier = pcall(multiplyVelocity, "body", "2")
            "#,
        )
        .unwrap();

    let environment = game_environment(runtime.lua()).unwrap();
    assert_eq!(
        environment.get::<f64>("mirror_collision_time").unwrap(),
        77.0
    );
    assert_eq!(
        environment.get::<String>("mirror_revert_mode").unwrap(),
        "lua-only"
    );
    assert_eq!(environment.get::<f64>("mirror_revert_force").unwrap(), 88.0);
    assert_eq!(environment.get::<f64>("mirror_revert_limit").unwrap(), 99.0);
    assert!(!environment.get::<bool>("bad_collision_name").unwrap());
    assert!(!environment.get::<bool>("bad_collision_value").unwrap());
    assert!(!environment.get::<bool>("bad_revert_flag").unwrap());
    assert!(!environment.get::<bool>("bad_revert_force").unwrap());
    assert!(!environment.get::<bool>("bad_sprite_name").unwrap());
    assert!(!environment.get::<bool>("bad_sprite_angle").unwrap());
    assert!(!environment.get::<bool>("bad_velocity_name").unwrap());
    assert!(!environment.get::<bool>("bad_velocity_multiplier").unwrap());

    let expected_angle = {
        let angle = 123456.7890123_f64 as f32;
        let turn = std::f32::consts::PI + std::f32::consts::PI;
        f64::from(angle % turn)
    };
    assert_eq!(
        environment.get::<f64>("mirror_sprite_angle").unwrap(),
        expected_angle
    );
    let bridge = runtime.render.lock().unwrap();
    let body = &bridge.scene["body"];
    assert_eq!(body.time_since_collision, f64::from(0.123456789_f64 as f32));
    assert!(body.revert_gravity_with_multiplier);
    assert_eq!(
        body.revert_gravity_force,
        f64::from(-(0.123456789_f64 as f32))
    );
    assert_eq!(
        body.revert_gravity_max_velocity,
        f64::from(9.87654321_f64 as f32)
    );
    assert_eq!(body.sprite_rotation, expected_angle);
    assert_eq!((body.velocity_x, body.velocity_y), (4.0, 6.0));
}

#[test]
fn native_water_and_sensor_scalar_members_are_strict_float32_and_native_only() {
    let runtime = StellaLua::new("/tmp").unwrap();
    runtime
        .execute_source(
            r#"
                createBox("water", "", 0, 0, 2, 2, 0, 0, 0, true, false, 0)
                objects.world.water.isWater = "lua-is-water"
                objects.world.water.waterDensity = 77
                objects.world.water.sensorMinimumForce = 88
                objects.world.water.sensorMaximumForce = 99

                native_setIsWater("water", true)
                native_setWaterDensity("water", 0.123456789)
                setSensorMinimumAndMaximumForces(
                    "water", 1.23456789, 9.87654321
                )

                mirror_is_water = objects.world.water.isWater
                mirror_water_density = objects.world.water.waterDensity
                mirror_sensor_minimum = objects.world.water.sensorMinimumForce
                mirror_sensor_maximum = objects.world.water.sensorMaximumForce

                water_name_rejected = not pcall(native_setIsWater, false, true)
                water_flag_rejected = not pcall(native_setIsWater, "water", 1)
                density_value_rejected = not pcall(
                    native_setWaterDensity, "water", "1"
                )
                sensor_minimum_rejected = not pcall(
                    setSensorMinimumAndMaximumForces, "water", false, 1
                )
                sensor_maximum_rejected = not pcall(
                    setSensorMinimumAndMaximumForces, "water", 1, false
                )

                missing_water_ok, missing_water_error = pcall(
                    native_setIsWater, "missing", true
                )
                missing_water_error = tostring(missing_water_error)
                missing_density_ok, missing_density_error = pcall(
                    native_setWaterDensity, "missing", 1
                )
                missing_density_error = tostring(missing_density_error)
                missing_sensor_ok, missing_sensor_error = pcall(
                    setSensorMinimumAndMaximumForces, "missing", 1, 2
                )
                missing_sensor_error = tostring(missing_sensor_error)
                "#,
        )
        .unwrap();

    let environment = game_environment(runtime.lua()).unwrap();
    assert_eq!(
        environment.get::<String>("mirror_is_water").unwrap(),
        "lua-is-water"
    );
    assert_eq!(
        environment.get::<f64>("mirror_water_density").unwrap(),
        77.0
    );
    assert_eq!(
        environment.get::<f64>("mirror_sensor_minimum").unwrap(),
        88.0
    );
    assert_eq!(
        environment.get::<f64>("mirror_sensor_maximum").unwrap(),
        99.0
    );
    for field in [
        "water_name_rejected",
        "water_flag_rejected",
        "density_value_rejected",
        "sensor_minimum_rejected",
        "sensor_maximum_rejected",
    ] {
        assert!(environment.get::<bool>(field).unwrap(), "{field}");
    }
    for (ok_field, error_field) in [
        ("missing_water_ok", "missing_water_error"),
        ("missing_density_ok", "missing_density_error"),
        ("missing_sensor_ok", "missing_sensor_error"),
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
    let water = &bridge.scene["water"];
    assert!(water.is_water);
    assert_eq!(water.water_density, f64::from(0.123456789_f64 as f32));
    assert_eq!(water.sensor_minimum_force, f64::from(1.23456789_f64 as f32));
    assert_eq!(water.sensor_maximum_force, f64::from(9.87654321_f64 as f32));
}

#[test]
fn native_gravity_sensor_matches_masks_float32_mass_and_revert_modes() {
    let runtime = StellaLua::new("/tmp").unwrap();
    runtime
        .execute_source(
            r#"
                gravity_default = getGravityForceMultiplier()
                water_default = getWaterForceMultiplier()
                createBox("sensor", "", 0, 0, 10, 10, 0, 0, 0, true, false, 0)
                setObjectParameter("sensor", 20, 1)
                setObjectParameter("sensor", 21, 2)
                setObjectParameter("sensor", 22, 1)
                setObjectParameter("sensor", 27, 10)
                setSensorMinimumAndMaximumForces("sensor", 10, 20)

                createBox("masked", "", 3, 0, 2, 2, 1, 0, 0, true, false, 0)
                setObjectGravityCategory("masked", 2)
                setSensorGravityMask("sensor", 4)
                native_applySensorForces("sensor", "masked")

                createBox("normal", "", 3, 0, 2, 2, 1, 0, 0, true, false, 0)
                setObjectGravityCategory("normal", 2)
                setSensorGravityMask("sensor", 2)
                native_applySensorForces("sensor", "normal")

                createBox("reversed", "", 3, 0, 2, 2, 1, 0, 0, true, false, 0)
                setObjectGravityCategory("reversed", 2)
                setRevertGravity("reversed", true)
                native_applySensorForces("sensor", "reversed")

                createBox("multiplied", "", 3, 0, 2, 2, 1, 0, 0, true, false, 0)
                setObjectGravityCategory("multiplied", 2)
                setRevertGravityWithMultiplier("multiplied", true, 0.5, 10)
                native_applySensorForces("sensor", "multiplied")

                createBox("too_fast", "", 3, 0, 2, 2, 1, 0, 0, true, false, 0)
                setObjectGravityCategory("too_fast", 2)
                setVelocity("too_fast", 10, 0)
                setRevertGravityWithMultiplier("too_fast", true, 0.5, 10)
                native_applySensorForces("sensor", "too_fast")
                "#,
        )
        .unwrap();

    let environment = game_environment(runtime.lua()).unwrap();
    assert_eq!(environment.get::<f64>("gravity_default").unwrap(), 4.0);
    assert_eq!(environment.get::<f64>("water_default").unwrap(), 2.0);
    let bridge = runtime.render.lock().unwrap();
    assert_eq!(bridge.scene["masked"].force_x, 0.0);
    assert!((bridge.scene["normal"].force_x - f64::from(-6.8_f32)).abs() < 1e-6);
    assert!((bridge.scene["reversed"].force_x - f64::from(0.68_f32)).abs() < 1e-6);
    assert!((bridge.scene["multiplied"].force_x - f64::from(3.4_f32)).abs() < 1e-6);
    assert_eq!(bridge.scene["too_fast"].force_x, 0.0);
    assert!(bridge.scene["multiplied"].revert_gravity_with_multiplier);
    assert!(!bridge.scene["multiplied"].revert_gravity);
}

#[test]
fn native_water_sensor_applies_recovered_buoyancy_and_drag_branches() {
    let runtime = StellaLua::new("/tmp").unwrap();
    runtime
        .execute_source(
            r#"
                setGameParameters({ gameWorldScale = 1 })
                createBox("water", "", 0, 0, 10, 10, 0, 0, 0, true, false, 0)
                setObjectParameter("water", 20, 1)
                setObjectParameter("water", 21, 2)
                setObjectParameter("water", 22, 1)
                setObjectParameter("water", 27, 10)
                setSensorMinimumAndMaximumForces("water", 10, 20)
                native_setIsWater("water", true)
                native_setWaterDensity("water", 1)

                createCircle("floating", "", 0, 0, 1, 1, 0, 0, true, false, 0)
                native_setWaterDensity("floating", 2)
                native_applySensorForces("water", "floating")

                createCircle("dragged", "", 0, 0, 1, 1, 0, 0, true, false, 0)
                native_setWaterDensity("dragged", 1)
                setVelocity("dragged", 3, -4)
                native_setObjectWaterDrag(0.5)
                native_applySensorForces("water", "dragged")
                "#,
        )
        .unwrap();

    let bridge = runtime.render.lock().unwrap();
    let mass = std::f32::consts::PI;
    let expected_buoyancy = mass * 0.1_f32 * 15.0_f32;
    assert_eq!(bridge.scene["floating"].force_x, 0.0);
    assert!((bridge.scene["floating"].force_y - f64::from(expected_buoyancy)).abs() < 1e-6);
    assert!((bridge.scene["dragged"].force_x - f64::from(-(mass * 0.5 * 3.0))).abs() < 1e-6);
    assert!((bridge.scene["dragged"].force_y - f64::from(mass * 0.5 * 4.0)).abs() < 1e-6);
}

#[test]
fn native_contact_filter_honors_exclusion_groups_types_and_material_lists() {
    let runtime = StellaLua::new("/tmp").unwrap();
    runtime
        .execute_source(
            r#"
                createBox("free", "", 0, 0, 2, 2, 1, 0, 0, true, false, 0)
                createBox("solid", "", 0, 0, 2, 2, 1, 0, 0, true, false, 0)
                assert(select('#', setMaterial("solid", "wood")) == 0)
                assert(not pcall(setMaterial, "solid"))
                -- The native enum set above is separate from the Lua string
                -- consulted by GameLua's contact filter.
                objects.world.solid.material = "wood"
                objects.world.free.collisionMaterials = { "wood" }
                "#,
        )
        .unwrap();
    runtime.sync_native_collision_filter_state().unwrap();

    let mut bridge = runtime.render.lock().unwrap();
    assert!(bridge.scene["free"].block_collision_enabled);
    bridge.scene.get_mut("free").unwrap().collision_group = 4;
    bridge.scene.get_mut("solid").unwrap().collision_group = 4;
    assert!(!RenderBridge::native_objects_should_collide(
        &bridge.scene["free"],
        &bridge.scene["solid"]
    ));

    bridge.scene.get_mut("free").unwrap().collision_group = -4;
    bridge.scene.get_mut("solid").unwrap().collision_group = -4;
    assert!(RenderBridge::native_objects_should_collide(
        &bridge.scene["free"],
        &bridge.scene["solid"]
    ));

    bridge
        .scene
        .get_mut("free")
        .unwrap()
        .block_collision_enabled = false;
    assert!(RenderBridge::native_objects_should_collide(
        &bridge.scene["free"],
        &bridge.scene["solid"]
    ));
    bridge
        .scene
        .get_mut("free")
        .unwrap()
        .collision_materials
        .clear();
    assert!(!RenderBridge::native_objects_should_collide(
        &bridge.scene["free"],
        &bridge.scene["solid"]
    ));

    bridge
        .scene
        .get_mut("free")
        .unwrap()
        .block_collision_enabled = true;
    bridge.scene.get_mut("free").unwrap().sensor_type = 7;
    bridge.scene.get_mut("solid").unwrap().controllable = true;
    assert!(!RenderBridge::native_objects_should_collide(
        &bridge.scene["free"],
        &bridge.scene["solid"]
    ));
}

#[test]
fn active_and_collision_setters_destroy_contacts_in_native_callback_order() {
    let runtime = StellaLua::new("/tmp").unwrap();
    runtime
        .execute_source(
            r#"
                createBox("a", "", 0, 0, 2, 2, 1, 0, 0, true, false, 1)
                createBox("b", "", 0, 0, 2, 2, 1, 0, 0, true, false, 1)
                exit_snapshots = {}
                exitCollision = function(first, second)
                    table.insert(exit_snapshots, {
                        first = first,
                        second = second,
                        collisionEnabled = objects.world.a.collisionEnabled
                    })
                end
                "#,
        )
        .unwrap();
    {
        let mut bridge = runtime.render.lock().unwrap();
        let events = bridge.refresh_contacts();
        assert!(events.iter().any(|event| event.began));
        assert_eq!(bridge.active_contacts.len(), 1);
        let body = bridge.scene.get_mut("a").unwrap();
        body.velocity_x = 3.0;
        body.velocity_y = 4.0;
        body.angular_velocity = 2.0;
        body.force_x = 7.0;
        body.force_y = 8.0;
        body.torque = 9.0;
    }

    runtime
        .execute_source(r#"setCollisionEnabled("a", false)"#)
        .unwrap();
    {
        let mut bridge = runtime.render.lock().unwrap();
        let body = &bridge.scene["a"];
        assert!(body.active);
        assert!(!body.collision_enabled);
        // SetActive(false/true) preserves the body's motion and force
        // accumulators; it is not SetAwake(false/true).
        assert_eq!((body.velocity_x, body.velocity_y), (3.0, 4.0));
        assert_eq!(body.angular_velocity, 2.0);
        assert_eq!((body.force_x, body.force_y, body.torque), (7.0, 8.0, 9.0));
        assert!(bridge.active_contacts.is_empty());
        assert!(bridge.broad_phase_contacts.is_empty());
        let rejected = bridge.refresh_contacts();
        assert!(rejected.is_empty(), "rejected events: {rejected:?}");
        assert!(bridge.active_contacts.is_empty());
        assert!(bridge.broad_phase_contacts.is_empty());
    }
    let environment = game_environment(runtime.lua()).unwrap();
    let snapshots = environment.get::<mlua::Table>("exit_snapshots").unwrap();
    assert_eq!(snapshots.raw_len(), 1);
    let first_snapshot = snapshots.raw_get::<mlua::Table>(1).unwrap();
    assert!(first_snapshot.get::<bool>("collisionEnabled").unwrap());
    assert!(
        !object_world(runtime.lua())
            .unwrap()
            .get::<mlua::Table>("a")
            .unwrap()
            .get::<bool>("collisionEnabled")
            .unwrap()
    );

    runtime
        .execute_source(r#"setCollisionEnabled("a", true)"#)
        .unwrap();
    {
        let mut bridge = runtime.render.lock().unwrap();
        assert!(bridge.refresh_contacts().iter().any(|event| event.began));
        assert_eq!(bridge.active_contacts.len(), 1);
    }

    runtime.execute_source(r#"setActive("a", false)"#).unwrap();
    {
        let bridge = runtime.render.lock().unwrap();
        assert!(!bridge.scene["a"].active);
        assert!(bridge.active_contacts.is_empty());
        assert_eq!(
            (bridge.scene["a"].velocity_x, bridge.scene["a"].velocity_y),
            (3.0, 4.0)
        );
    }
    // The direct native wrapper at sub_10004DB2C only calls
    // b2Body::SetActive; it does not rewrite objects.world[name].active.
    assert!(matches!(
        object_world(runtime.lua())
            .unwrap()
            .get::<mlua::Table>("a")
            .unwrap()
            .get::<mlua::Value>("active")
            .unwrap(),
        mlua::Value::Nil
    ));
    assert_eq!(snapshots.raw_len(), 2);

    runtime.execute_source(r#"setActive("a", true)"#).unwrap();
    let mut bridge = runtime.render.lock().unwrap();
    assert!(bridge.scene["a"].active);
    assert!(bridge.refresh_contacts().iter().any(|event| event.began));
}

#[test]
fn set_transform_drains_broad_phase_pairs_immediately_without_waking() {
    let runtime = StellaLua::new("/tmp").unwrap();
    runtime
        .execute_source(
            r#"
                createBox("a", "", 0, 0, 2, 2, 1, 0, 0, true, false, 1)
                createBox("b", "", 10, 0, 2, 2, 1, 0, 0, true, false, 1)
                "#,
        )
        .unwrap();
    {
        let mut bridge = runtime.render.lock().unwrap();
        assert!(bridge.refresh_contacts().is_empty());
        bridge.scene.get_mut("a").unwrap().sleeping = true;
        bridge.scene.get_mut("b").unwrap().sleeping = true;
    }

    runtime.execute_source(r#"setPosition("b", 0, 0)"#).unwrap();
    let key = ("a".to_owned(), "b".to_owned(), 0, 0);
    let mut bridge = runtime.render.lock().unwrap();
    // SetTransform (`sub_10086B794`) calls UpdatePairs before returning,
    // but does not change either body's awake flag.
    assert!(bridge.broad_phase_contacts.contains(&key));
    assert!(bridge.scene["a"].sleeping);
    assert!(bridge.scene["b"].sleeping);
    assert!(bridge.refresh_contacts().is_empty());
    assert!(!bridge.active_contacts.contains_key(&key));
    drop(bridge);
    assert_eq!(
        object_world(runtime.lua())
            .unwrap()
            .get::<mlua::Table>("b")
            .unwrap()
            .get::<f64>("x")
            .unwrap(),
        0.0
    );
}
