use super::*;

#[test]
fn native_collision_force_reads_retained_world_attributes_not_shadow_global() {
    let runtime = unlocked_test_runtime();
    runtime
        .execute_source(
            r#"
                forceDamageMultiplier = 99
                worldAttributes = { forceDamageMultiplier = 2250 }
                retainedWorldAttributes = worldAttributes
                "#,
        )
        .unwrap();
    assert_eq!(
        native_force_damage_multiplier(runtime.lua()).unwrap(),
        2250.0
    );

    runtime
        .execute_source("worldAttributes = nil; retainedWorldAttributes.forceDamageMultiplier = 2")
        .unwrap();
    assert_eq!(native_force_damage_multiplier(runtime.lua()).unwrap(), 2.0);
}

#[test]
fn native_collision_factors_resolve_named_block_table_definition() {
    let runtime = unlocked_test_runtime();
    runtime
        .execute_source(
            r#"
                worldAttributes = { forceDamageMultiplier = 2250 }
                blockTable = { damageFactors = {
                    ExactFactors = {
                        damageMultiplier = { wood = 1.25 },
                        velocityMultiplier = { wood = 0.75 }
                    }
                } }
                objects = { world = {
                    attacker = {
                        damageFactors = "ExactFactors",
                        powerupDamageMultiplier = 2
                    },
                    target = { material = "wood" }
                } }
                "#,
        )
        .unwrap();

    let factors = native_collision_force_factors(runtime.lua(), "attacker", "target").unwrap();
    assert_eq!(factors.force_damage_multiplier, 2250.0);
    assert_eq!(factors.damage_multiplier, 1.25);
    assert_eq!(factors.powerup_damage_multiplier, 2.0);
    assert_eq!(factors.velocity_multiplier, 0.75);

    runtime
        .execute_source(
            r#"
                objects.world.attacker.damageFactors = {
                    damageMultiplier = { wood = 99 },
                    velocityMultiplier = { wood = 99 }
                }
                "#,
        )
        .unwrap();
    let factors = native_collision_force_factors(runtime.lua(), "attacker", "target").unwrap();
    assert_eq!(factors.damage_multiplier, 1.0);
    assert_eq!(factors.velocity_multiplier, 1.0);
}

#[test]
fn physics_contact_resolves_body_and_dispatches_native_collision_callback() {
    let runtime = unlocked_test_runtime();
    runtime
        .execute_source(
            r#"
                createCircle("bird", "", 0, 0, 1, 1, 0, 0, true, false, 1)
                createBox("wall", "", 3, 0, 2, 4, 0, 0, 0, true, false, 1)
                setWorldGravity(0, 0)
                setVelocity("bird", 6, 0)
                objects.world.bird.strength = 100
                objects.world.bird.defence = 0
                objects.world.wall.strength = 100
                objects.world.wall.defence = 0
                scoreTable = { blocks = { score = 0 } }
                worldAttributes = { scoreDamageMultiplier = 1 }
                collision_count = 0
                blockCollision = function(first, second, force, damaged, secondary,
                                          strength_delta, point_x, point_y, normal_x, normal_y)
                    collision_count = collision_count + 1
                    collision_first = first
                    collision_second = second
                    collision_impulse = force
                    collision_damaged = damaged
                    collision_secondary = secondary
                    collision_strength_delta = strength_delta
                    collision_point_x = point_x
                    collision_normal_x = normal_x
                end
                update = function() end
                updatePhysics = function() end
                "#,
        )
        .unwrap();

    for _ in 0..10 {
        runtime.update(1.0 / 30.0).unwrap();
    }
    let environment = game_environment(runtime.lua()).unwrap();
    let world = object_world(runtime.lua()).unwrap();
    let bird: mlua::Table = world.get("bird").unwrap();
    assert!(bird.get::<f64>("x").unwrap() < 2.0);
    assert!(bird.get::<f64>("xVel").unwrap().abs() < 1e-9);
    assert_eq!(environment.get::<i64>("collision_count").unwrap(), 1);
    assert_eq!(
        environment.get::<String>("collision_first").unwrap(),
        "wall"
    );
    assert_eq!(
        environment.get::<String>("collision_second").unwrap(),
        "bird"
    );
    assert!(environment.get::<f64>("collision_impulse").unwrap() > 0.0);
    assert!(environment.get::<bool>("collision_damaged").unwrap());
    assert!(!environment.get::<bool>("collision_secondary").unwrap());
    assert_eq!(
        environment.get::<f64>("collision_strength_delta").unwrap(),
        1.0
    );
    assert!(environment.get::<f64>("collision_point_x").unwrap() > 1.9);
    assert_eq!(environment.get::<f64>("collision_normal_x").unwrap(), -1.0);
    let wall: mlua::Table = world.get("wall").unwrap();
    assert_eq!(bird.get::<f64>("strength").unwrap(), 99.0);
    assert_eq!(wall.get::<f64>("strength").unwrap(), 99.0);
    let score_table: mlua::Table = environment.get("scoreTable").unwrap();
    let block_score: mlua::Table = score_table.get("blocks").unwrap();
    assert_eq!(block_score.get::<f64>("score").unwrap(), 2.0);
}

#[test]
fn native_bird_collision_reorders_names_damages_once_and_uses_eight_value_abi() {
    let runtime = unlocked_test_runtime();
    runtime
        .execute_source(
            r##"
                createCircle("bird", "", 0, 0, 1, 1, 0, 0, true, true, 1)
                createBox("target", "", 3, 0, 2, 4, 0, 0, 0, true, false, 1)
                setActive("bird", true)
                setWorldGravity(0, 0)
                setVelocity("bird", 6, 0)
                objects.world.target.strength = 10
                objects.world.target.defence = 0
                objects.world.target.material = "wood"
                blockTable = { damageFactors = {
                    TestBirdDamageFactors = {
                        damageMultiplier = { wood = 1 },
                        velocityMultiplier = { wood = 9 }
                    }
                } }
                objects.world.bird.damageFactors = "TestBirdDamageFactors"
                bird_count = 0
                birdCollision = function(...)
                    bird_count = bird_count + 1
                    bird_arg_count = select("#", ...)
                    local values = {...}
                    bird_first = values[1]
                    bird_second = values[2]
                    bird_force = values[3]
                    bird_damage = values[4]
                    bird_point_x = values[5]
                    bird_normal_x = values[7]
                    bird_ninth = values[9]
                end
                update = function() end
                updatePhysics = function() end
                "##,
        )
        .unwrap();

    for _ in 0..10 {
        runtime.update(1.0 / 30.0).unwrap();
    }
    let environment = game_environment(runtime.lua()).unwrap();
    assert_eq!(environment.get::<i64>("bird_count").unwrap(), 1);
    assert_eq!(environment.get::<i64>("bird_arg_count").unwrap(), 8);
    assert_eq!(environment.get::<String>("bird_first").unwrap(), "bird");
    assert_eq!(environment.get::<String>("bird_second").unwrap(), "target");
    let bird_force = environment.get::<f64>("bird_force").unwrap();
    assert!(bird_force > 1.5, "bird force was {bird_force}");
    assert!(
        bird_force < 2.0,
        "velocityMultiplier leaked into collision force: {bird_force}"
    );
    assert_eq!(environment.get::<f64>("bird_damage").unwrap(), 1.0);
    assert!(environment.get::<f64>("bird_point_x").unwrap() > 1.9);
    // ContactFactory stores the polygon target as fixture A. The bird name is
    // reordered to the first Lua argument, but Purple does not invert the
    // already constructed world-manifold normal.
    assert_eq!(environment.get::<f64>("bird_normal_x").unwrap(), -1.0);
    assert!(matches!(
        environment.get::<Value>("bird_ninth").unwrap(),
        Value::Nil
    ));
    let world = object_world(runtime.lua()).unwrap();
    let target: mlua::Table = world.get("target").unwrap();
    assert_eq!(target.get::<f64>("strength").unwrap(), 9.0);
}

#[test]
fn native_nonlegacy_destruction_consumes_scaled_preimpact_collision_velocity() {
    let runtime = unlocked_test_runtime();
    runtime
        .execute_source(
            r##"
                createCircle("bird", "", 0, 0, 1, 1, 0, 0, true, true, 1)
                createBox("target", "", 3, 0, 2, 4, 0, 0, 0, true, false, 1)
                setActive("bird", true)
                setWorldGravity(0, 0)
                setVelocity("bird", 6, 0)
                objects.world.target.strength = 1
                objects.world.target.defence = 0
                objects.world.target.material = "wood"
                blockTable = { damageFactors = {
                    TestBirdDamageFactors = {
                        damageMultiplier = { wood = 1 },
                        velocityMultiplier = { wood = 0.5 }
                    }
                } }
                objects.world.bird.damageFactors = "TestBirdDamageFactors"
                callback_order = {}
                birdCollision = function(_, _, force)
                    captured_force = force
                    table.insert(callback_order, "bird")
                end
                removeBlocks = function()
                    if captured_force then
                        velocity_during_remove = objects.world.bird.xVel
                        table.insert(callback_order, "remove")
                    end
                end
                update = function() end
                updatePhysics = function() end
                "##,
        )
        .unwrap();

    for _ in 0..10 {
        runtime.update(1.0 / 30.0).unwrap();
        if game_environment(runtime.lua())
            .unwrap()
            .get::<Value>("captured_force")
            .is_ok_and(|value| !matches!(value, Value::Nil))
        {
            break;
        }
    }

    let environment = game_environment(runtime.lua()).unwrap();
    let force = environment.get::<f64>("captured_force").unwrap();
    let callback_order: mlua::Table = environment.get("callback_order").unwrap();
    assert_eq!(callback_order.raw_len(), 2);
    assert_eq!(callback_order.raw_get::<String>(1).unwrap(), "bird");
    assert_eq!(callback_order.raw_get::<String>(2).unwrap(), "remove");
    let preimpact_velocity = environment.get::<f64>("velocity_during_remove").unwrap();
    assert_eq!(preimpact_velocity, 4.799_999_237_060_547);
    let expected_factor = 0.5_f32 * ((force as f32 - 1.0_f32) / force as f32);
    let bridge = runtime.render.lock().unwrap();
    let velocity_x = bridge.scene["bird"].velocity_x;
    let velocity_y = bridge.scene["bird"].velocity_y;
    let expected_velocity_x = f64::from(preimpact_velocity as f32 * expected_factor);
    assert!(
        (velocity_x - expected_velocity_x).abs() < 1e-6,
        "velocity_x={velocity_x}, expected={expected_velocity_x}, force={force}"
    );
    assert!(velocity_y.abs() < 1e-9);
    assert!(bridge.collision_velocities.is_empty());
    drop(bridge);
    let bird: mlua::Table = object_world(runtime.lua()).unwrap().get("bird").unwrap();
    assert!((bird.get::<f64>("xVel").unwrap() - velocity_x).abs() < 1e-9);
    assert_eq!(
        object_world(runtime.lua())
            .unwrap()
            .get::<mlua::Table>("target")
            .unwrap()
            .get::<f64>("strength")
            .unwrap(),
        0.0
    );
}

#[test]
fn native_legacy_destruction_replaces_body_velocity_without_map_entry() {
    let runtime = unlocked_test_runtime();
    runtime
        .execute_source(
            r##"
                createCircle("bird", "", 0, 0, 1, 1, 0, 0, true, true, 1)
                createBox("target", "", 3, 0, 2, 4, 0, 0, 0, true, false, 1)
                setActive("bird", true)
                setWorldGravity(0, 0)
                setVelocity("bird", 6, 0)
                objects.world.bird.useLegacyCollisionPath = true
                objects.world.target.strength = 1
                objects.world.target.defence = 0
                birdCollision = function() end
                update = function() end
                updatePhysics = function() end
                "##,
        )
        .unwrap();

    for _ in 0..10 {
        runtime.update(1.0 / 30.0).unwrap();
    }

    let bridge = runtime.render.lock().unwrap();
    assert!(bridge.scene["bird"].velocity_x.abs() < 1e-9);
    assert!(bridge.scene["bird"].velocity_y.abs() < 1e-9);
    assert!(!bridge.collision_velocities.contains_key("bird"));
    drop(bridge);
    let bird: mlua::Table = object_world(runtime.lua()).unwrap().get("bird").unwrap();
    assert!(bird.get::<f64>("xVel").unwrap().abs() < 1e-9);
    assert!(bird.get::<f64>("yVel").unwrap().abs() < 1e-9);
}

#[test]
fn native_sensor_end_calls_trigger_exit_before_ordinary_exit() {
    let runtime = unlocked_test_runtime();
    runtime
        .execute_source(
            r#"
                createBox("sensor", "", 0, 0, 4, 4, 0, 0, 0, true, false, 1)
                createCircle("body", "", 0, 0, 1, 1, 0, 0, true, false, 1)
                setAsSensor("sensor", true)
                setWorldGravity(0, 0)
                setVelocity("body", 0.1, 0)
                contact_order = {}
                enterCollision = function()
                    table.insert(contact_order, "enter")
                end
                exitTriggerCollision = function()
                    table.insert(contact_order, "trigger_exit")
                end
                exitCollision = function()
                    table.insert(contact_order, "exit")
                end
                update = function() end
                updatePhysics = function() end
                "#,
        )
        .unwrap();

    runtime.update(1.0 / 30.0).unwrap();
    runtime
        .execute_source(r#"setPosition("body", 20, 0)"#)
        .unwrap();
    runtime.update(1.0 / 30.0).unwrap();

    let environment = game_environment(runtime.lua()).unwrap();
    let order: mlua::Table = environment.get("contact_order").unwrap();
    assert_eq!(order.raw_len(), 3);
    assert_eq!(order.raw_get::<String>(1).unwrap(), "enter");
    assert_eq!(order.raw_get::<String>(2).unwrap(), "trigger_exit");
    assert_eq!(order.raw_get::<String>(3).unwrap(), "exit");
}

#[test]
fn contact_listener_lua_velocity_mutation_reaches_same_island_step() {
    let runtime = unlocked_test_runtime();
    runtime
        .execute_source(
            r#"
                createBox("sensor", "", 0, 0, 4, 4, 0, 0, 0, true, false, 1)
                createCircle("body", "", 0, 0, 0.5, 1, 0, 0, true, false, 1)
                setAsSensor("sensor", true)
                setWorldGravity(0, 0)
                setVelocity("body", 0.03, 0)
                enter_count = 0
                enterCollision = function(first, second)
                    enter_count = enter_count + 1
                    setVelocity("body", 3, 0)
                end
                update = function() end
                updatePhysics = function() end
                "#,
        )
        .unwrap();

    runtime.update(1.0 / 30.0).unwrap();
    let environment = game_environment(runtime.lua()).unwrap();
    assert_eq!(environment.get::<i64>("enter_count").unwrap(), 1);
    let expected_x = f64::from((1.0_f64 / 30.0_f64) as f32 * 3.0_f32);
    let bridge = runtime.render.lock().unwrap();
    assert!(
        (bridge.scene["body"].x - expected_x).abs() < 1.0e-9,
        "BeginContact must run before the current island integrates positions"
    );
    assert_eq!(bridge.scene["body"].velocity_x, 3.0);
}

#[test]
fn contact_listener_body_type_mutation_obeys_native_world_lock() {
    let runtime = unlocked_test_runtime();
    runtime
        .execute_source(
            r#"
                createBox("sensor", "", 0, 0, 4, 4, 0, 0, 0, true, false, 1)
                createCircle("body", "", 0, 0, 0.5, 1, 0, 0, true, false, 1)
                setAsSensor("sensor", true)
                setWorldGravity(0, 0)
                setVelocity("body", 0.03, 0)
                enterCollision = function()
                    setObjectParameter("body", 39, 0)
                end
                update = function() end
                updatePhysics = function() end
            "#,
        )
        .unwrap();

    runtime.update(1.0 / 30.0).unwrap();
    let bridge = runtime.render.lock().unwrap();
    // b2Body::SetType starts by testing b2World::e_locked. BeginContact runs
    // inside World::Step, so this callback mutation is a silent no-op.
    assert!(bridge.scene["body"].dynamic_body);
    assert!(!bridge.physics_world_locked);
}

#[test]
fn contact_listener_transform_and_mass_data_mutations_obey_native_world_lock() {
    let runtime = unlocked_test_runtime();
    runtime
        .execute_source(
            r#"
                createBox("sensor", "", 0, 0, 4, 4, 0, 0, 0, true, false, 1)
                createCircle("body", "", 0, 0, 0.5, 1, 0, 0, true, false, 1)
                createNonPhysicsObject("marker", "", 0, 0, 1)
                setAsSensor("sensor", true)
                setWorldGravity(0, 0)
                setObjectParameter("body", 38, 4)
                enterCollision = function()
                    setPosition("body", 20, 30)
                    setRotation("body", 1.25)
                    setObjectParameter("body", 38, 17)
                    setPosition("marker", 7, 8)
                    setRotation("marker", 0.75)
                end
                update = function() end
                updatePhysics = function() end
            "#,
        )
        .unwrap();

    runtime.update(1.0 / 30.0).unwrap();
    let bridge = runtime.render.lock().unwrap();
    let body = &bridge.scene["body"];
    // SetTransform and SetMassData both return before touching b2Body while
    // BeginContact owns the world lock. Their outer GameLua pose writes do
    // not alter the native body transform.
    assert_eq!((body.x, body.y, body.angle), (0.0, 0.0, 0.0));
    assert_eq!(body.moment_of_inertia, Some(4.0));

    // The non-physics RenderObjectData path has no b2Body lock gate.
    let marker = &bridge.scene["marker"];
    assert_eq!((marker.x, marker.y, marker.angle), (7.0, 8.0, 0.75));
    assert!(!bridge.physics_world_locked);
}

#[test]
fn contact_listener_set_active_destroys_touching_contact_synchronously() {
    let runtime = unlocked_test_runtime();
    runtime
        .execute_source(
            r#"
                createBox("sensor", "", 0, 0, 4, 4, 0, 0, 0, true, false, 1)
                createCircle("body", "", 0, 0, 0.5, 1, 0, 0, true, false, 1)
                setAsSensor("sensor", true)
                setWorldGravity(0, 0)
                callback_order = {}
                enterCollision = function()
                    table.insert(callback_order, "enter-before")
                    setActive("body", false)
                    table.insert(callback_order, "enter-after")
                end
                exitCollision = function()
                    table.insert(callback_order, "exit")
                    -- The inactive bit is already visible to this nested
                    -- listener, so reactivation survives the outer call.
                    setActive("body", true)
                end
                update = function() end
                updatePhysics = function() end
            "#,
        )
        .unwrap();

    runtime.update(1.0 / 30.0).unwrap();
    let environment = game_environment(runtime.lua()).unwrap();
    let order = environment.get::<mlua::Table>("callback_order").unwrap();
    assert_eq!(order.raw_len(), 3);
    assert_eq!(order.raw_get::<String>(1).unwrap(), "enter-before");
    assert_eq!(order.raw_get::<String>(2).unwrap(), "exit");
    assert_eq!(order.raw_get::<String>(3).unwrap(), "enter-after");

    let bridge = runtime.render.lock().unwrap();
    assert!(bridge.scene["body"].active);
    assert!(bridge.active_contacts.is_empty());
    // Reactivation buffers fresh proxies. World::Solve's trailing
    // FindNewContacts recreates a proxy-only contact in the same Step, but it
    // cannot become touching until the next Collide traversal.
    assert_eq!(bridge.native_contact_world_order.len(), 1);
}

#[test]
fn contact_listener_physics_constructors_contain_native_locked_world_crash() {
    let runtime = unlocked_test_runtime();
    runtime
        .execute_source(
            r#"
                createBox("sensor", "", 0, 0, 10, 10, 0, 0, 0, true, false, 1)
                createCircle("body", "", 0, 0, 0.5, 1, 0, 0, true, false, 1)
                setAsSensor("sensor", true)
                setWorldGravity(0, 0)
                locked_constructor_attempts = 0
                locked_constructor_results = {}
                enterCollision = function()
                    if locked_constructor_attempts ~= 0 then return end
                    locked_constructor_attempts = 1
                    locked_constructor_results.box = pcall(function()
                        createBox("locked_box", "", 20, 0, 1, 1,
                            1, 0, 0, true, false, 1)
                    end)
                    locked_constructor_results.circle = pcall(function()
                        createCircle("locked_circle", "", 22, 0, 1,
                            1, 0, 0, true, false, 1)
                    end)
                    clearVertices()
                    addVertex(-1, -1)
                    addVertex(1, -1)
                    addVertex(0, 1)
                    locked_constructor_results.polygon = pcall(function()
                        createPolygon("locked_polygon", "", 24, 0, 2, 2,
                            1, 0, 0, true, false, 1)
                    end)
                    clearVertices()
                    addVertex(-1, 0)
                    addVertex(1, 0)
                    locked_constructor_results.line = pcall(function()
                        createLineShape("locked_line", "", 26, 0, 2, 1,
                            0, 0, 0, true, false, 1)
                    end)
                    locked_constructor_results.none = pcall(function()
                        createNonPhysicsObject("locked_none", "", 28, 0, 1)
                    end)
                end
                update = function() end
                updatePhysics = function() end
            "#,
        )
        .unwrap();

    let initial_body_slot = runtime.render.lock().unwrap().next_body_allocation_slot;
    runtime.update(1.0 / 30.0).unwrap();

    let environment = game_environment(runtime.lua()).unwrap();
    assert_eq!(
        environment
            .get::<i64>("locked_constructor_attempts")
            .unwrap(),
        1
    );
    let results = environment
        .get::<mlua::Table>("locked_constructor_results")
        .unwrap();
    for kind in ["box", "circle", "polygon", "line"] {
        assert!(!results.get::<bool>(kind).unwrap(), "{kind}");
    }
    assert!(results.get::<bool>("none").unwrap());

    let world = object_world(runtime.lua()).unwrap();
    let bridge = runtime.render.lock().unwrap();
    assert!(!bridge.physics_world_locked);
    assert_eq!(bridge.next_body_allocation_slot, initial_body_slot);
    assert!(bridge.active_contacts.keys().any(|key| {
        (key.0 == "sensor" && key.1 == "body") || (key.0 == "body" && key.1 == "sensor")
    }));
    for name in [
        "locked_box",
        "locked_circle",
        "locked_polygon",
        "locked_line",
    ] {
        assert!(matches!(world.raw_get::<Value>(name).unwrap(), Value::Nil));
        assert!(!bridge.scene.contains_key(name));
        assert!(
            !bridge
                .native_body_world_order
                .values()
                .any(|entry| entry == name)
        );
    }
    assert!(matches!(
        world.raw_get::<Value>("locked_none").unwrap(),
        Value::Table(_)
    ));
    assert!(bridge.scene.contains_key("locked_none"));
    assert!(!bridge.scene["locked_none"].has_physics_body());
    drop(bridge);
    let callbacks = runtime.draw_callbacks.borrow();
    for name in [
        "locked_box",
        "locked_circle",
        "locked_polygon",
        "locked_line",
    ] {
        assert!(!callbacks.records.contains_key(name));
    }
    assert!(callbacks.records.contains_key("locked_none"));
}

#[test]
fn contact_listener_joint_construction_preserves_locked_native_null_split() {
    let runtime = unlocked_test_runtime();
    runtime
        .execute_source(
            r#"
                createBox("sensor", "", 0, 0, 10, 10, 0, 0, 0, true, false, 1)
                createCircle("body", "", 0, 0, 0.5, 1, 0, 0, true, false, 1)
                setAsSensor("sensor", true)
                setWorldGravity(0, 0)
                objects.joints = {}
                locked_joint_attempts = 0
                locked_joint_results = {}
                local function make(name, kind)
                    return createJoint({
                        name = name, end1 = "sensor", end2 = "body",
                        type = kind, coordType = 2,
                        x1 = 0, y1 = 0, x2 = 0, y2 = 0,
                        collideConnected = false
                    })
                end
                enterCollision = function()
                    if locked_joint_attempts ~= 0 then return end
                    locked_joint_attempts = 1
                    locked_joint_results.distance = pcall(make, "locked_distance", 1)
                    locked_joint_results.weld = pcall(make, "locked_weld", 2)
                    locked_joint_results.revolute = pcall(make, "locked_revolute", 3)
                    locked_joint_results.prismatic = pcall(make, "locked_prismatic", 4)
                    locked_joint_results.metadata = pcall(make, "locked_metadata", 5)
                    locked_joint_results.rope = pcall(make, "locked_rope", 6)
                end
                update = function() end
                updatePhysics = function() end
            "#,
        )
        .unwrap();

    runtime.update(1.0 / 30.0).unwrap();

    let environment = game_environment(runtime.lua()).unwrap();
    assert_eq!(environment.get::<i64>("locked_joint_attempts").unwrap(), 1);
    let results = environment
        .get::<mlua::Table>("locked_joint_results")
        .unwrap();
    assert!(!results.get::<bool>("distance").unwrap());
    for kind in ["weld", "revolute", "prismatic", "metadata", "rope"] {
        assert!(results.get::<bool>(kind).unwrap(), "{kind}");
    }

    let descriptors = native_lua_object(runtime.lua(), NativeLuaObject::Objects)
        .unwrap()
        .unwrap()
        .get::<mlua::Table>("joints")
        .unwrap();
    assert!(matches!(
        descriptors.raw_get::<Value>("locked_distance").unwrap(),
        Value::Nil
    ));
    for name in [
        "locked_weld",
        "locked_revolute",
        "locked_prismatic",
        "locked_metadata",
        "locked_rope",
    ] {
        assert!(matches!(
            descriptors.raw_get::<Value>(name).unwrap(),
            Value::Table(_)
        ));
    }

    let bridge = runtime.render.lock().unwrap();
    assert!(!bridge.physics_world_locked);
    assert!(!bridge.joints.contains_key("locked_distance"));
    for name in [
        "locked_weld",
        "locked_revolute",
        "locked_prismatic",
        "locked_rope",
    ] {
        let joint = &bridge.joints[name];
        assert!(joint.is_physical, "{name}");
        assert!(!joint.native_joint_present, "{name}");
        assert!(!joint.has_native_joint(), "{name}");
    }
    let metadata = &bridge.joints["locked_metadata"];
    assert!(!metadata.is_physical);
    assert!(!metadata.native_joint_present);
    assert!(bridge.native_joint_world_order.is_empty());
    assert!(bridge.native_joint_endpoint_exports().is_empty());
    assert!(bridge.active_contacts.keys().any(|key| {
        (key.0 == "sensor" && key.1 == "body") || (key.0 == "body" && key.1 == "sensor")
    }));
}

#[test]
fn contact_listener_track_construction_obeys_native_world_lock() {
    let runtime = unlocked_test_runtime();
    runtime
        .execute_source(
            r#"
                createBox("sensor", "", 0, 0, 10, 10, 0, 0, 0, true, false, 1)
                createCircle("body", "", 0, 0, 0.5, 1, 0, 0, true, false, 1)
                setAsSensor("sensor", true)
                setWorldGravity(0, 0)
                locked_track_attempts = 0
                enterCollision = function()
                    if locked_track_attempts ~= 0 then return end
                    locked_track_attempts = 1
                    locked_track_result = pcall(createTrack, {
                        points = {{ x = -1, y = 0 }, { x = 1, y = 0 }},
                        blocks = {"body"},
                        openEnded = true,
                        rotateBlock = true
                    })
                end
                update = function() end
                updatePhysics = function() end
            "#,
        )
        .unwrap();

    runtime.update(1.0 / 30.0).unwrap();

    let environment = game_environment(runtime.lua()).unwrap();
    assert_eq!(environment.get::<i64>("locked_track_attempts").unwrap(), 1);
    assert!(environment.get::<bool>("locked_track_result").unwrap());
    let bridge = runtime.render.lock().unwrap();
    assert!(!bridge.physics_world_locked);
    assert!(!bridge.tracks.contains_key("body"));
    assert!(bridge.active_contacts.keys().any(|key| {
        (key.0 == "sensor" && key.1 == "body") || (key.0 == "body" && key.1 == "sensor")
    }));
}

#[test]
fn contact_listener_fixture_replacement_obeys_native_world_lock() {
    let runtime = unlocked_test_runtime();
    runtime
        .execute_source(
            r#"
                createBox("sensor", "", 0, 0, 10, 10, 0, 0, 0, true, false, 1)
                createCircle("scaled", "", -2, 0, 1, 2, 0.2, 0.3, true, false, 1)
                createCircle("resized", "", 2, 0, 1, 3, 0.4, 0.5, true, false, 1)
                setAsSensor("sensor", true)
                setAsSensor("resized", true)
                setWorldGravity(0, 0)
                locked_fixture_mutations = 0
                enterCollision = function()
                    if locked_fixture_mutations == 0 then
                        locked_fixture_mutations = 1
                        setPhysicsScale("scaled", 3, 5)
                        native_resizeRadius("resized", 4, 9, 0.8, 0.7)
                    end
                end
                update = function() end
                updatePhysics = function() end
            "#,
        )
        .unwrap();

    let (scaled_proxy, scaled_mass, resized_proxy, resized_mass) = {
        let bridge = runtime.render.lock().unwrap();
        (
            bridge.scene["scaled"].fixture_proxy_ids.clone(),
            bridge.scene["scaled"].inverse_mass,
            bridge.scene["resized"].fixture_proxy_ids.clone(),
            bridge.scene["resized"].inverse_mass,
        )
    };
    runtime.update(1.0 / 30.0).unwrap();

    assert_eq!(
        game_environment(runtime.lua())
            .unwrap()
            .get::<i64>("locked_fixture_mutations")
            .unwrap(),
        1
    );
    let bridge = runtime.render.lock().unwrap();
    assert!(!bridge.physics_world_locked);

    let scaled = &bridge.scene["scaled"];
    assert_eq!((scaled.scale_x, scaled.scale_y), (3.0, 5.0));
    assert_eq!((scaled.physics_scale_x, scaled.physics_scale_y), (1.0, 1.0));
    assert_eq!(
        scaled.native_shape_radius,
        f64::from((3.0_f32 + 0.0001_f32) * 1.0_f32)
    );
    assert!(matches!(scaled.collision_shape, CollisionShape::Circle { radius } if radius == 1.0));
    assert_eq!(scaled.fixture_densities, vec![2.0]);
    assert_eq!(scaled.fixture_frictions, vec![f64::from(0.2_f32)]);
    assert_eq!(scaled.fixture_restitutions, vec![f64::from(0.3_f32)]);
    assert_eq!(scaled.fixture_proxy_ids, scaled_proxy);
    assert_eq!(scaled.inverse_mass, scaled_mass);

    let resized = &bridge.scene["resized"];
    assert_eq!(resized.native_shape_radius, 4.0);
    assert!(matches!(resized.collision_shape, CollisionShape::Circle { radius } if radius == 1.0));
    assert_eq!(resized.fixture_densities, vec![3.0]);
    assert_eq!(resized.fixture_frictions, vec![f64::from(0.4_f32)]);
    assert_eq!(resized.fixture_restitutions, vec![f64::from(0.5_f32)]);
    assert!(resized.sensor);
    assert_eq!(resized.fixture_proxy_ids, resized_proxy);
    assert_eq!(resized.inverse_mass, resized_mass);
    assert!(bridge.active_contacts.keys().any(|key| {
        (key.0 == "sensor" && key.1 == "scaled") || (key.0 == "scaled" && key.1 == "sensor")
    }));
    assert!(bridge.active_contacts.keys().any(|key| {
        (key.0 == "sensor" && key.1 == "resized") || (key.0 == "resized" && key.1 == "sensor")
    }));
}

#[test]
fn joint_destruction_obeys_native_world_lock() {
    let runtime = unlocked_test_runtime();
    runtime
        .execute_source(
            r#"
                createBox("anchor", "", -2, 0, 1, 1, 0, 0, 0, true, false, 1)
                createCircle("body", "", 0, 0, 0.5, 1, 0, 0, true, false, 1)
                objects.joints = {}
                createJoint({
                    name = "locked_joint", end1 = "anchor", end2 = "body",
                    type = 3, x1 = -2, y1 = 0, x2 = 0, y2 = 0,
                    collideConnected = true
                })
                createJoint({
                    name = "metadata_joint", end1 = "anchor", end2 = "body",
                    type = 5, x1 = 0, y1 = 0, x2 = 0, y2 = 0,
                    destroyTimer = 0
                })
            "#,
        )
        .unwrap();

    runtime.render.lock().unwrap().physics_world_locked = true;
    runtime
        .execute_source(
            r#"
                destroyJoint("locked_joint")
                destroyJoint("metadata_joint")
            "#,
        )
        .unwrap();
    let mut bridge = runtime.render.lock().unwrap();
    // The generated destroyJoint adapter removes GameLua's descriptor before
    // calling b2World::DestroyJoint. The latter sees e_locked and leaves the
    // native joint and both body edges untouched.
    assert!(bridge.joints.contains_key("locked_joint"));
    assert!(bridge.orphaned_native_joints.contains("locked_joint"));
    assert!(bridge.attached_joint_names("body").is_empty());
    assert!(bridge.native_joint_endpoint_exports().is_empty());
    // The alternate metadata vector does not call b2World and therefore
    // remains removable even when that world is locked.
    assert!(!bridge.joints.contains_key("metadata_joint"));
    assert!(bridge.physics_world_locked);
    bridge.physics_world_locked = false;
    drop(bridge);
    // The first call erased GameLua's jointData record. A later explicit
    // name lookup cannot rediscover or destroy the orphaned b2Joint.
    runtime
        .execute_source(
            r#"
                destroyJoint("locked_joint")
                setJointParameters({
                    name = "locked_joint", motor = true, motorSpeed = 12
                })
            "#,
        )
        .unwrap();
    let bridge = runtime.render.lock().unwrap();
    assert!(bridge.joints.contains_key("locked_joint"));
    assert!(!bridge.joints["locked_joint"].motor_enabled);
    assert_eq!(bridge.joints["locked_joint"].motor_speed, None);
    drop(bridge);
    let joints = native_lua_object(runtime.lua(), NativeLuaObject::Objects)
        .unwrap()
        .unwrap()
        .get::<mlua::Table>("joints")
        .unwrap();
    assert!(matches!(
        joints.raw_get::<Value>("locked_joint").unwrap(),
        Value::Nil
    ));
    runtime.execute_source(r#"removeObject("anchor")"#).unwrap();
    let bridge = runtime.render.lock().unwrap();
    assert!(!bridge.joints.contains_key("locked_joint"));
    assert!(bridge.orphaned_native_joints.is_empty());
}

#[test]
fn contact_listener_body_destruction_obeys_native_world_lock() {
    let runtime = unlocked_test_runtime();
    runtime
        .execute_source(
            r#"
                createBox("sensor", "", 0, 0, 4, 4, 0, 0, 0, true, false, 1)
                createCircle("body", "", 0, 0, 0.5, 1, 0, 0, true, false, 1)
                setAsSensor("sensor", true)
                objects.joints = {}
                createJoint({
                    name = "body_joint", end1 = "sensor", end2 = "body",
                    type = 3, x1 = 0, y1 = 0, x2 = 0, y2 = 0,
                    collideConnected = true
                })
                setWorldGravity(0, 0)
                enterCollision = function(first, second)
                    if first == "body" or second == "body" then
                        removeObject("body")
                    end
                end
                update = function() end
                updatePhysics = function() end
            "#,
        )
        .unwrap();
    runtime.update(1.0 / 30.0).unwrap();

    let world = object_world(runtime.lua()).unwrap();
    assert!(matches!(
        world.raw_get::<Value>("body").unwrap(),
        Value::Nil
    ));
    let joints = native_lua_object(runtime.lua(), NativeLuaObject::Objects)
        .unwrap()
        .unwrap()
        .get::<mlua::Table>("joints")
        .unwrap();
    assert!(matches!(
        joints.raw_get::<Value>("body_joint").unwrap(),
        Value::Table(_)
    ));

    let bridge = runtime.render.lock().unwrap();
    // b2World::DestroyBody returns immediately on e_locked, but
    // RenderObjectData's wrapper still clears its logical body pointer and
    // removeObject erases the GameLua name-map/render-tree entry.
    assert!(bridge.scene.contains_key("body"));
    assert!(bridge.orphaned_native_bodies.contains("body"));
    assert!(
        bridge
            .native_body_world_order
            .values()
            .any(|name| name == "body")
    );
    assert!(bridge.joints.contains_key("body_joint"));
    assert!(bridge.attached_joint_names("body").is_empty());
    assert!(bridge.active_contacts.keys().any(|key| {
        (key.0 == "sensor" && key.1 == "body") || (key.0 == "body" && key.1 == "sensor")
    }));
    assert!(!bridge.scene_range_names().iter().any(|name| name == "body"));
    assert!(!bridge.physics_world_locked);
    let native_position = (bridge.scene["body"].x, bridge.scene["body"].y);
    let native_velocity = (
        bridge.scene["body"].velocity_x,
        bridge.scene["body"].velocity_y,
    );
    let native_collision_time = bridge.scene["body"].time_since_collision;
    drop(bridge);

    // Every generated/manual RenderObject lookup uses the erased GameLua
    // tree. Throwing members report a missing object; nullable members and
    // queries return their native empty defaults without mutating the body
    // payload that remains in b2World.
    runtime
        .execute_source(
            r#"
                removed_position_ok = pcall(setPosition, "body", 99, 98)
                removed_visible_ok = pcall(isVisible, "body")
                removed_parameter_ok = pcall(setObjectParameter, "body", 5, 7)
                setVelocity("body", 97, 96)
                native_setTimeSinceCollision("body", 95)
                removed_velocity = getVelocity("body")
                removed_sleeping = isSleeping("body")
                removed_vertices = #getObjectVertices("body")
            "#,
        )
        .unwrap();
    let environment = game_environment(runtime.lua()).unwrap();
    assert!(!environment.get::<bool>("removed_position_ok").unwrap());
    assert!(!environment.get::<bool>("removed_visible_ok").unwrap());
    assert!(!environment.get::<bool>("removed_parameter_ok").unwrap());
    assert_eq!(environment.get::<f64>("removed_velocity").unwrap(), 0.0);
    assert!(environment.get::<bool>("removed_sleeping").unwrap());
    assert_eq!(environment.get::<i64>("removed_vertices").unwrap(), 0);
    let bridge = runtime.render.lock().unwrap();
    assert_eq!(
        (bridge.scene["body"].x, bridge.scene["body"].y),
        native_position
    );
    assert_eq!(
        (
            bridge.scene["body"].velocity_x,
            bridge.scene["body"].velocity_y
        ),
        native_velocity
    );
    assert_eq!(
        bridge.scene["body"].time_since_collision,
        native_collision_time
    );
    drop(bridge);

    // A following fixed step still visits the orphan through b2World's body,
    // contact and joint lists even though the GameLua tree no longer exposes
    // it to frame export or public name-based members.
    runtime.update(1.0 / 30.0).unwrap();
    let bridge = runtime.render.lock().unwrap();
    assert!(bridge.scene.contains_key("body"));
    assert!(bridge.joints.contains_key("body_joint"));
    assert!(bridge.active_contacts.keys().any(|key| {
        (key.0 == "sensor" && key.1 == "body") || (key.0 == "body" && key.1 == "sensor")
    }));
    drop(bridge);

    // The RenderObjectData body pointer was cleared by the first call, so a
    // repeated logical remove cannot rediscover the orphaned native body.
    runtime.execute_source(r#"removeObject("body")"#).unwrap();
    let mut bridge = runtime.render.lock().unwrap();
    assert!(bridge.scene.contains_key("body"));
    assert!(bridge.orphaned_native_bodies.contains("body"));

    // The b2World owner still releases every native allocation at level
    // teardown, including the otherwise unreachable body and its joint.
    bridge.clear_native_level_scene();
    assert!(bridge.scene.is_empty());
    assert!(bridge.orphaned_native_bodies.is_empty());
    assert!(bridge.joints.is_empty());
    assert!(bridge.native_body_world_order.is_empty());
}

#[test]
fn contact_listener_mutation_reaches_later_contact_update_in_same_collide_walk() {
    let runtime = unlocked_test_runtime();
    runtime
        .execute_source(
            r#"
                -- Proxy-pair sorting creates target/bird first and
                -- sensor/bird second. AddPair pushes at the contact-list
                -- head, so the sensor callback must execute first.
                createBox("target", "", 0, 0, 4, 4, 0, 0, 0, true, false, 1)
                createBox("sensor", "", 0, 0, 4, 4, 0, 0, 0, true, false, 1)
                setAsSensor("sensor", true)
                createCircle("bird", "", 0, 0, 0.5, 1, 0, 0, true, true, 1)
                setActive("bird", true)
                setWorldGravity(0, 0)
                setVelocity("bird", 0.03, 0)
                objects.world.target.strength = 100
                objects.world.target.defence = 0
                callback_order = {}
                enterCollision = function(first, second)
                    if first == "sensor" or second == "sensor" then
                        table.insert(callback_order, "sensor")
                        setVelocity("bird", 6, 0)
                    end
                end
                birdCollision = function(_, _, force)
                    table.insert(callback_order, "solid")
                    later_contact_force = force
                end
                update = function() end
                updatePhysics = function() end
                "#,
        )
        .unwrap();

    runtime.update(1.0 / 30.0).unwrap();

    let environment = game_environment(runtime.lua()).unwrap();
    let order: mlua::Table = environment.get("callback_order").unwrap();
    assert_eq!(order.raw_len(), 2);
    assert_eq!(order.raw_get::<String>(1).unwrap(), "sensor");
    assert_eq!(order.raw_get::<String>(2).unwrap(), "solid");
    let force = environment.get::<f64>("later_contact_force").unwrap();
    assert!(
        force > 0.1,
        "later Contact::Update prepared from the stale pre-callback velocity: {force}"
    );
}

#[test]
fn native_sensor_overlap_preserves_and_clears_inside_gravity_in_callback_order() {
    let runtime = unlocked_test_runtime();
    runtime
        .execute_source(
            r#"
                createBox("gravity", "", 0, 0, 4, 4, 0, 0, 0, true, false, 1)
                setObjectParameter("gravity", 20, 1)
                setObjectParameter("gravity", 21, 2)
                setObjectParameter("gravity", 22, 1)
                setAsSensor("gravity", true)
                createBox("water", "", 0, 0, 4, 4, 0, 0, 0, true, false, 1)
                setObjectParameter("water", 20, 1)
                setObjectParameter("water", 21, 3)
                setObjectParameter("water", 22, 1)
                setAsSensor("water", true)
                createCircle("body", "", 0, 0, 1, 1, 0, 0, true, false, 1)
                setWorldGravity(0, 0)
                setVelocity("body", 0.1, 0)
                water_trigger_saw_inside = false
                water_exit_saw_cleared = false
                enterCollision = function() end
                exitTriggerCollision = function(first, second)
                    if first == "water" or second == "water" then
                        water_trigger_saw_inside = objects.world.body.insideGravity == true
                    end
                end
                exitCollision = function(first, second)
                    if first == "water" or second == "water" then
                        water_exit_saw_cleared = objects.world.body.insideGravity == nil
                    end
                end
                update = function() end
                updatePhysics = function() end
                "#,
        )
        .unwrap();

    runtime.update(1.0 / 30.0).unwrap();
    let body: mlua::Table = object_world(runtime.lua()).unwrap().get("body").unwrap();
    assert!(body.get::<bool>("insideGravity").unwrap());

    runtime
        .execute_source(r#"setPosition("gravity", 20, 0)"#)
        .unwrap();
    runtime.update(1.0 / 30.0).unwrap();
    assert!(body.get::<bool>("insideGravity").unwrap());

    runtime
        .execute_source(r#"setPosition("water", 20, 0)"#)
        .unwrap();
    runtime.update(1.0 / 30.0).unwrap();
    assert!(matches!(
        body.get::<Value>("insideGravity").unwrap(),
        Value::Nil
    ));
    let environment = game_environment(runtime.lua()).unwrap();
    assert!(environment.get::<bool>("water_trigger_saw_inside").unwrap());
    assert!(environment.get::<bool>("water_exit_saw_cleared").unwrap());
}

#[test]
fn remove_object_dispatches_native_end_contact_before_lua_record_removal() {
    let runtime = unlocked_test_runtime();
    runtime
        .execute_source(
            r#"
                createCircle("a", "", 0, 0, 1, 1, 0, 0, true, false, 1)
                createCircle("b", "", 1.5, 0, 1, 1, 0, 0, true, false, 1)
                setWorldGravity(0, 0)
                setVelocity("a", 0.1, 0)
                remove_exit_count = 0
                remove_exit_records_live = false
                remove_exit_native_body_live = false
                exitCollision = function(first, second)
                    remove_exit_count = remove_exit_count + 1
                    remove_exit_records_live =
                        objects.world[first] ~= nil and objects.world[second] ~= nil
                    if first == "a" or second == "a" then
                        -- sub_10004F608 performs a strict RenderObjectData
                        -- lookup. DestroyBody EndContact runs before the
                        -- enclosing removeObject erases that record.
                        setGravityScale("a", 0.25)
                        remove_exit_native_body_live = true
                    end
                end
                update = function() end
                updatePhysics = function() end
                "#,
        )
        .unwrap();

    runtime.update(1.0 / 30.0).unwrap();
    {
        let mut bridge = runtime.render.lock().unwrap();
        bridge.scene.get_mut("b").unwrap().sleeping = true;
        bridge.scene.get_mut("b").unwrap().sleep_time = 0.5;
    }
    runtime.execute_source(r#"removeObject("a")"#).unwrap();
    let environment = game_environment(runtime.lua()).unwrap();
    assert_eq!(environment.get::<i64>("remove_exit_count").unwrap(), 1);
    assert!(environment.get::<bool>("remove_exit_records_live").unwrap());
    assert!(
        environment
            .get::<bool>("remove_exit_native_body_live")
            .unwrap()
    );
    assert!(matches!(
        object_world(runtime.lua())
            .unwrap()
            .get::<Value>("a")
            .unwrap(),
        Value::Nil
    ));
    let bridge = runtime.render.lock().unwrap();
    // ContactManager::Destroy (`sub_10086B9B8`) does not write body flags
    // itself, but it invokes Purple's EndContact listener first. The listener
    // wakes both endpoints at `0x1000653DC..0x100065410` before dispatching
    // either Lua callback.
    assert!(!bridge.scene["b"].sleeping);
    assert_eq!(bridge.scene["b"].sleep_time, 0.0);
    drop(bridge);

    runtime.update(1.0 / 30.0).unwrap();
    assert_eq!(environment.get::<i64>("remove_exit_count").unwrap(), 1);
}

#[test]
fn fixture_contact_factory_destroy_resets_only_positive_manifold_sleep_times() {
    let runtime = unlocked_test_runtime();
    runtime
        .execute_source(
            r#"
                createCircle("solid_removed", "", 0, 0, 1, 1, 0, 0, true, false, 1)
                createCircle("solid_survivor", "", 1.5, 0, 1, 1, 0, 0, true, false, 1)

                createCircle("sensor_removed", "", 10, 0, 1, 1, 0, 0, true, false, 1)
                setAsSensor("sensor_removed", true)
                createCircle("sensor_survivor", "", 11.5, 0, 1, 1, 0, 0, true, false, 1)
                setWorldGravity(0, 0)
            "#,
        )
        .unwrap();

    let mut bridge = runtime.render.lock().unwrap();
    let began = bridge.refresh_contacts();
    assert_eq!(began.iter().filter(|event| event.began).count(), 2);
    bridge.scene.get_mut("solid_survivor").unwrap().sleep_time = 0.61;
    bridge.scene.get_mut("sensor_survivor").unwrap().sleep_time = 0.62;

    let solid_exits = bridge.drain_contacts_for_destroyed_fixture("solid_removed", 0);
    assert_eq!(solid_exits.len(), 1);
    assert!(!solid_exits[0].2);
    assert_eq!(bridge.scene["solid_survivor"].sleep_time, 0.0);

    let sensor_exits = bridge.drain_contacts_for_destroyed_fixture("sensor_removed", 0);
    assert_eq!(sensor_exits.len(), 1);
    assert!(sensor_exits[0].2);
    assert_eq!(bridge.scene["sensor_survivor"].sleep_time, 0.62);
}

#[test]
fn remove_object_keeps_body_live_during_sensor_exit_effect_restoration() {
    let runtime = unlocked_test_runtime();
    runtime
        .execute_source(
            r#"
                createBox("gravity_sensor", "", 0, 0, 4, 4, 0, 0, 0, true, false, 1)
                setAsSensor("gravity_sensor", true)
                createBox("BLOCK_JUNGLE_LOG_2_58", "", 0, 0, 1, 1, 1, 0, 0, true, false, 1)
                setWorldGravity(0, 0)
                setVelocity("BLOCK_JUNGLE_LOG_2_58", 0.1, 0)
                sensor_exit_restored_gravity = false
                exitTriggerCollision = function(first, second)
                    if first == "BLOCK_JUNGLE_LOG_2_58" or
                       second == "BLOCK_JUNGLE_LOG_2_58" then
                        setGravityScale("BLOCK_JUNGLE_LOG_2_58", 1)
                        sensor_exit_restored_gravity = true
                    end
                end
                update = function() end
                updatePhysics = function() end
                "#,
        )
        .unwrap();

    runtime.update(1.0 / 30.0).unwrap();
    runtime
        .execute_source(r#"removeObject("BLOCK_JUNGLE_LOG_2_58")"#)
        .unwrap();

    let environment = game_environment(runtime.lua()).unwrap();
    assert!(
        environment
            .get::<bool>("sensor_exit_restored_gravity")
            .unwrap()
    );
    assert!(matches!(
        object_world(runtime.lua())
            .unwrap()
            .get::<Value>("BLOCK_JUNGLE_LOG_2_58")
            .unwrap(),
        Value::Nil
    ));
    assert!(
        !runtime
            .render
            .lock()
            .unwrap()
            .scene
            .contains_key("BLOCK_JUNGLE_LOG_2_58")
    );
}

#[test]
fn luca_support_destruction_wakes_and_drops_a_sleeping_stack() {
    let runtime = unlocked_test_runtime();
    runtime
        .execute_source(
            r#"
                createBox("BLOCK_LIGHT_1X10_1_36", "", -0.5, 0, 1, 1, 0, 0, 0, true, false, 1)
                createBox("BLOCK_LIGHT_1X10_1_37", "", 0.5, 0, 1, 1, 0, 0, 0, true, false, 1)
                createBox("BLOCK_LIGHT_1X10_1_38", "", 0, -0.9, 2, 1, 1, 0, 0, true, false, 1)
                setWorldGravity(0, 10)
                update = function() end
                updatePhysics = function() end
                "#,
        )
        .unwrap();

    // Establish the same touching support edge as Chapter01_L23, then model
    // the stable cage after Box2D has put its island to sleep.
    runtime.update(1.0 / 30.0).unwrap();
    {
        let mut bridge = runtime.render.lock().unwrap();
        assert_eq!(bridge.active_contacts.len(), 2);
        let cage = bridge.scene.get_mut("BLOCK_LIGHT_1X10_1_38").unwrap();
        cage.sleeping = true;
        cage.sleep_time = 0.5;
        cage.velocity_x = 0.0;
        cage.velocity_y = 0.0;
        cage.angular_velocity = 0.0;
    }
    let supported_y = runtime.render.lock().unwrap().scene["BLOCK_LIGHT_1X10_1_38"].y;

    // LucaAbility queues the breakable glass in deadBlocks; removeBlocks then
    // reaches this exact removeObject boundary without applying an impulse.
    runtime
        .execute_source(
            r#"
                removeObject("BLOCK_LIGHT_1X10_1_36")
                removeObject("BLOCK_LIGHT_1X10_1_37")
            "#,
        )
        .unwrap();
    {
        let bridge = runtime.render.lock().unwrap();
        let cage = &bridge.scene["BLOCK_LIGHT_1X10_1_38"];
        assert!(!cage.sleeping);
        assert_eq!(cage.sleep_time, 0.0);
    }

    runtime.update(1.0 / 30.0).unwrap();
    assert!(
        runtime.render.lock().unwrap().scene["BLOCK_LIGHT_1X10_1_38"].y > supported_y,
        "the Luca tutorial cage remained suspended after its glass support was removed"
    );
}

#[test]
fn shipped_luca_tutorial_support_destruction_drops_the_real_cage() {
    let sandbox = ShippedDataSandbox::new("luca-tutorial-support");
    let runtime = StellaLua::new(&sandbox.data_root).unwrap();
    runtime.boot("scripts/game.lua").unwrap();
    runtime
        .execute_source(
            r#"
                initializeGameCommon()
                currentPack = "Chapter01"
                currentLevel = 23
                levelName = "Chapter01_L23"
                levelFolder = levelPath .. "/Chapter01/"
                loadLevelInternal(levelFolder .. levelName)
                setPhysicsEnabled(true)
                update = function() end
                updatePhysics = function() end
                removeBlocks = function() end
            "#,
        )
        .unwrap();

    for _ in 0..180 {
        runtime.update(1.0 / 60.0).unwrap();
    }
    let supported_y = runtime.render.lock().unwrap().scene["BLOCK_LIGHT_1X10_1_38"].y;
    runtime
        .execute_source(
            r#"
                removeObject("BLOCK_LIGHT_1X10_1_36")
                removeObject("BLOCK_LIGHT_1X10_1_37")
            "#,
        )
        .unwrap();
    for _ in 0..60 {
        runtime.update(1.0 / 60.0).unwrap();
    }
    let bridge = runtime.render.lock().unwrap();
    let cage = &bridge.scene["BLOCK_LIGHT_1X10_1_38"];
    assert!(
        cage.y > supported_y + 0.25,
        "real Luca cage did not fall: before={supported_y}, after={}, sleeping={}",
        cage.y,
        cage.sleeping
    );
}

#[test]
fn shipped_luca_tutorial_ability_wakes_the_sleeping_cage() {
    let sandbox = ShippedDataSandbox::new("luca-tutorial-ability");
    let runtime = StellaLua::new(&sandbox.data_root).unwrap();
    runtime.boot("scripts/game.lua").unwrap();
    runtime
        .execute_source(
            r#"
                initializeGameCommon()
                currentPack = "Chapter01"
                currentLevel = 23
                levelName = "Chapter01_L23"
                levelFolder = levelPath .. "/Chapter01/"
                loadLevelInternal(levelFolder .. levelName)
                initDelayedCallbacks()
                g_realDt = 1 / 60
                g_dt = 1 / 60
                isAudioPlaying = function() return false end
                setPhysicsEnabled(true)
                update = function(dt, realDt)
                    g_dt = dt
                    g_realDt = realDt
                end
                updatePhysics = function() end
            "#,
        )
        .unwrap();

    for _ in 0..180 {
        runtime.update(1.0 / 60.0).unwrap();
    }
    let supported_y = runtime.render.lock().unwrap().scene["BLOCK_LIGHT_1X10_1_38"].y;
    runtime
        .execute_source(
            r#"
                local bird = objects.world.Luca_5
                setPosition(bird.name, 0, -2.316)
                bird.x = 0
                bird.y = -2.316
                flyingBird = bird
                bird.abilityDisabled = false
                startLucaAim(bird, 7, -2.316)
                updateLucaAim(bird, 7, -2.316, 0.2)
                startLucaAbility(bird, 7, -2.316)
                update = function(dt, realDt)
                    g_dt = dt
                    g_realDt = realDt
                    updateDelayedCallbacks(dt, realDt)
                    blocks.BlockComponentManager.triggerEvent(
                        bird,
                        blocks.events.EID_UPDATE_BLOCK,
                        dt,
                        realDt
                    )
                end
            "#,
        )
        .unwrap();

    for _ in 0..80 {
        runtime.update(1.0 / 60.0).unwrap();
    }
    let bridge = runtime.render.lock().unwrap();
    assert!(!bridge.scene.contains_key("BLOCK_LIGHT_1X10_1_36"));
    assert!(!bridge.scene.contains_key("BLOCK_LIGHT_1X10_1_37"));
    let cage = &bridge.scene["BLOCK_LIGHT_1X10_1_38"];
    assert!(
        cage.y > supported_y + 0.25,
        "the real Luca ability left the cage suspended: before={supported_y}, after={}, sleeping={}",
        cage.y,
        cage.sleeping
    );
}

#[test]
fn off_center_contact_applies_box2d_style_angular_impulse() {
    let runtime = unlocked_test_runtime();
    runtime
        .execute_source(
            r#"
                createBox("mover", "", 0, 0.75, 1, 1, 1, 0, 0, true, false, 1)
                createBox("wall", "", 0.85, -0.5, 1, 2, 0, 0, 0, true, false, 1)
                setWorldGravity(0, 0)
                setVelocity("mover", 6, 0)
                update = function() end
                updatePhysics = function() end
                "#,
        )
        .unwrap();

    let mut bridge = runtime.render.lock().unwrap();
    let events = bridge.solve_contacts();
    assert!(!events.is_empty());
    let mover = &bridge.scene["mover"];
    assert!(
        mover.velocity_x < 6.0,
        "unexpected off-center response: x_vel={}, angular_vel={}, x={}",
        mover.velocity_x,
        mover.angular_velocity,
        mover.x
    );
    assert!(
        mover.angular_velocity < -0.1,
        "unexpected off-center angular response: {}",
        mover.angular_velocity
    );
}
