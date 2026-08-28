use super::*;

#[test]
fn joint_creation_reads_the_native_stack_top_and_strict_type_field() {
    let runtime = unlocked_test_runtime();
    runtime
        .execute_source(
            r#"
                joint_missing_fails = not pcall(createJoint)
                joint_type_fails = not pcall(createJoint, false)
                joint_top_fails = not pcall(createJoint, { type = 7 }, false)
                joint_type_missing_fails = not pcall(createJoint, {})
                joint_type_string_fails = not pcall(createJoint, { type = "7" })
                joints_missing_fails = not pcall(createJoints)
                joints_top_fails = not pcall(createJoints, {}, false)
                joints_descriptor_fails = not pcall(createJoints, { bad = false })
                joints_type_missing_fails = not pcall(createJoints, { bad = {} })
                "#,
        )
        .unwrap();

    let environment = game_environment(runtime.lua()).unwrap();
    for field in [
        "joint_missing_fails",
        "joint_type_fails",
        "joint_top_fails",
        "joint_type_missing_fails",
        "joint_type_string_fails",
        "joints_missing_fails",
        "joints_top_fails",
        "joints_descriptor_fails",
        "joints_type_missing_fails",
    ] {
        assert!(environment.get::<bool>(field).unwrap(), "{field}");
    }
}

#[test]
fn joint_batch_builds_and_solves_weld_constraint() {
    let runtime = unlocked_test_runtime();
    runtime
        .execute_source(
            r#"
                createBox("first", "", 0, 0, 0.5, 0.5, 1, 0, 0, true, false, 1)
                createBox("second", "", 2, 0, 0.5, 0.5, 1, 0, 0, true, false, 1)
                createJoints({
                    weld = {
                        name = "weld", end1 = "first", end2 = "second",
                        type = 2, coordType = 2, x1 = 1, y1 = 0, x2 = -1, y2 = 0,
                        breakable = false, breakForce = 0
                    }
                })
                setWorldGravity(0, 0)
                setVelocity("first", 3, 0)
                update = function() end
                updatePhysics = function() end
                "#,
        )
        .unwrap();

    runtime.update(1.0 / 30.0).unwrap();
    let world = object_world(runtime.lua()).unwrap();
    let second: mlua::Table = world.get("second").unwrap();
    let bridge = runtime.render.lock().unwrap();
    assert!(bridge.scene["second"].x > 2.0);
    assert_eq!(
        second.get::<f64>("x").unwrap(),
        f64::from(bridge.scene["second"].render_x as f32)
    );
    assert_eq!(bridge.joints.len(), 1);
    drop(bridge);
    let environment = game_environment(runtime.lua()).unwrap();
    let objects = environment.get::<mlua::Table>("objects").unwrap();
    let descriptors = objects.get::<mlua::Table>("joints").unwrap();
    assert_eq!(
        descriptors
            .get::<mlua::Table>("weld")
            .unwrap()
            .get::<String>("end2")
            .unwrap(),
        "second"
    );
    runtime.execute_source(r#"destroyJoint("weld")"#).unwrap();
    assert!(runtime.render.lock().unwrap().joints.is_empty());
    assert!(matches!(
        descriptors.raw_get::<Value>("weld").unwrap(),
        Value::Nil
    ));
}

#[test]
fn distance_joint_publishes_native_length_for_pulley_component() {
    let runtime = unlocked_test_runtime();
    runtime
        .execute_source(
            r#"
                createBox("pulley", "", 0, 0, 1, 1, 0, 0, 0, true, false, 1)
                createBox("rope_link", "", 3, 4, 1, 1, 1, 0, 0, true, false, 1)
                createJoint({
                    name = "pull_joint", end1 = "pulley", end2 = "rope_link",
                    type = 1, coordType = 2,
                    x1 = 0, y1 = 0, x2 = 0, y2 = 0,
                    collideConnected = false, frequency = 5, dampingRatio = 1
                })
                observed_pull_length = objects.joints.pull_joint.length
                setJointParameters({
                    name = "pull_joint",
                    length = objects.joints.pull_joint.length + 0.25
                })
                "#,
        )
        .unwrap();

    let environment = game_environment(runtime.lua()).unwrap();
    assert_eq!(environment.get::<f64>("observed_pull_length").unwrap(), 5.0);
    let descriptor = environment
        .get::<mlua::Table>("objects")
        .unwrap()
        .get::<mlua::Table>("joints")
        .unwrap()
        .get::<mlua::Table>("pull_joint")
        .unwrap();
    assert_eq!(descriptor.get::<f64>("length").unwrap(), 5.25);
    assert_eq!(
        runtime.render.lock().unwrap().joints["pull_joint"].rest_length,
        5.25
    );
}

#[test]
fn joint_creation_uses_native_float_defaults_and_precision() {
    let runtime = unlocked_test_runtime();
    runtime
        .execute_source(
            r#"
                createBox("anchor", "", 0, 0, 1, 1, 0, 0, 0, false, false, 1)
                createBox("payload", "", 3, 4, 1, 1, 1, 0, 0, true, false, 1)
                createJoint({
                    name = "spring", end1 = "anchor", end2 = "payload",
                    type = 1, coordType = 2,
                    x1 = 0, y1 = 0, x2 = 0, y2 = 0,
                    breakForce = 1.00000006
                })
                createJoint({
                    name = "hinge", end1 = "anchor", end2 = "payload",
                    type = 3, coordType = 2,
                    x1 = 0, y1 = 0, x2 = 0, y2 = 0,
                    limit = true, motorSpeed = 1.00000006
                })
            "#,
        )
        .unwrap();

    let bridge = runtime.render.lock().unwrap();
    let spring = &bridge.joints["spring"];
    assert_eq!(spring.frequency, f64::from(4.0_f32));
    assert_eq!(spring.damping_ratio, f64::from(0.5_f32));
    assert_eq!(spring.break_force, f64::from(1.000_000_1_f32));
    let hinge = &bridge.joints["hinge"];
    assert_eq!(hinge.lower_limit, f64::from(0.0_f32));
    assert_eq!(hinge.upper_limit, f64::from(std::f32::consts::PI));
    assert_eq!(hinge.motor_speed, Some(f64::from(1.000_000_1_f32)));
    drop(bridge);

    let environment = game_environment(runtime.lua()).unwrap();
    let spring_descriptor = environment
        .get::<mlua::Table>("objects")
        .unwrap()
        .get::<mlua::Table>("joints")
        .unwrap()
        .get::<mlua::Table>("spring")
        .unwrap();
    assert_eq!(spring_descriptor.get::<f64>("frequency").unwrap(), 4.0);
    assert_eq!(spring_descriptor.get::<f64>("dampingRatio").unwrap(), 0.5);
    assert!(!spring_descriptor.get::<bool>("collideConnected").unwrap());
}

#[test]
fn joint_creation_ignores_non_native_length_and_wrong_typed_optional_fields() {
    let runtime = unlocked_test_runtime();
    runtime
        .execute_source(
            r#"
                createBox("first", "", 0, 0, 1, 1, 0, 0, 0, false, false, 1)
                createBox("second", "", 10, 0, 1, 1, 1, 0, 0, false, false, 1)
                strict_distance_source = {
                    name = "strict_distance", end1 = "first", end2 = "second",
                    type = 1, coordType = "2",
                    x1 = 4, y1 = 0, x2 = -4, y2 = 0,
                    length = 99, collideConnected = "true", breakable = 1
                }
                createJoint(strict_distance_source)
                createJoint({
                    name = "strict_rope", end1 = "first", end2 = "second",
                    type = 6, coordType = 2,
                    x1 = 0, y1 = 0, x2 = 0, y2 = 0,
                    maxLength = "3"
                })
                createJoint({
                    name = "numeric_rope", end1 = "first", end2 = "second",
                    type = 6, coordType = 2,
                    x1 = 0, y1 = 0, x2 = 0, y2 = 0,
                    maxLength = 3
                })
            "#,
        )
        .unwrap();

    let bridge = runtime.render.lock().unwrap();
    let distance = &bridge.joints["strict_distance"];
    assert_eq!(distance.coord_type, 0);
    assert_eq!(distance.first_anchor, (0.0, 0.0));
    assert_eq!(distance.second_anchor, (0.0, 0.0));
    assert_eq!(distance.rest_length, 10.0);
    assert!(!distance.collide_connected);
    assert!(!distance.breakable);
    assert_eq!(bridge.joints["strict_rope"].rest_length, 10.0);
    assert_eq!(bridge.joints["numeric_rope"].rest_length, 3.0);
    drop(bridge);

    let environment = game_environment(runtime.lua()).unwrap();
    let source = environment
        .get::<mlua::Table>("strict_distance_source")
        .unwrap();
    assert_eq!(source.get::<f64>("length").unwrap(), 99.0);
    assert_eq!(source.get::<String>("coordType").unwrap(), "2");

    let joints = environment
        .get::<mlua::Table>("objects")
        .unwrap()
        .get::<mlua::Table>("joints")
        .unwrap();
    let distance = joints.get::<mlua::Table>("strict_distance").unwrap();
    assert_eq!(distance.get::<f64>("coordType").unwrap(), 0.0);
    assert_eq!(distance.get::<f64>("x1").unwrap(), 0.0);
    assert_eq!(distance.get::<f64>("x2").unwrap(), 10.0);
    assert_eq!(distance.get::<f64>("length").unwrap(), 10.0);
    assert!(!distance.get::<bool>("collideConnected").unwrap());
    assert!(matches!(
        distance.raw_get::<Value>("breakable").unwrap(),
        Value::Nil
    ));
    for name in ["strict_rope", "numeric_rope"] {
        assert!(matches!(
            joints
                .get::<mlua::Table>(name)
                .unwrap()
                .raw_get::<Value>("maxLength")
                .unwrap(),
            Value::Nil
        ));
    }
}

#[test]
fn joint_common_fields_use_purple_float_lua_string_and_anchor_coercions() {
    let runtime = unlocked_test_runtime();
    runtime
        .execute_source(
            r#"
                createBox("1e+09", "", 16777215, 0, 1, 1, 0, 0, 0, false, false, 1)
                createBox("9.99999975e-06", "", 0, 0, 1, 1, 1, 0, 0, false, false, 1)
                createJoint({
                    name = 123456789,
                    end1 = 1000000000,
                    end2 = 0.00001,
                    type = 1,
                    coordType = 1,
                    x1 = 16777217,
                    y1 = "0x10",
                    x2 = false,
                    y2 = {}
                })
            "#,
        )
        .unwrap();

    let bridge = runtime.render.lock().unwrap();
    let joint = &bridge.joints["123456792"];
    assert_eq!(joint.first, "1e+09");
    assert_eq!(joint.second, "9.99999975e-06");
    // The Lua number first narrows to 16,777,216f before subtracting the
    // body's exact 16,777,215f position. Double subtraction would yield 2.
    assert_eq!(joint.first_anchor, (1.0, 16.0));
    assert_eq!(joint.second_anchor, (0.0, 0.0));
    drop(bridge);

    let descriptor = game_environment(runtime.lua())
        .unwrap()
        .get::<mlua::Table>("objects")
        .unwrap()
        .get::<mlua::Table>("joints")
        .unwrap()
        .get::<mlua::Table>("123456792")
        .unwrap();
    assert_eq!(descriptor.get::<String>("name").unwrap(), "123456792");
    assert_eq!(descriptor.get::<String>("end1").unwrap(), "1e+09");
    assert_eq!(descriptor.get::<String>("end2").unwrap(), "9.99999975e-06");
    assert_eq!(descriptor.get::<f64>("x1").unwrap(), 16_777_216.0);
    assert_eq!(descriptor.get::<f64>("y1").unwrap(), 16.0);
    assert_eq!(descriptor.get::<f64>("x2").unwrap(), 0.0);
    assert_eq!(descriptor.get::<f64>("y2").unwrap(), 0.0);
}

#[test]
fn joint_type_dispatch_uses_native_float32_threshold_and_exact_class_values() {
    let runtime = unlocked_test_runtime();
    runtime
        .execute_source(
            r#"
                createBox("first", "", 0, 0, 1, 1, 0, 0, 0, false, false, 1)
                createBox("second", "", 10, 0, 1, 1, 1, 0, 0, false, false, 1)
                custom_types = {}
                createCustomJoint = function(descriptor)
                    table.insert(custom_types, descriptor.type)
                end
                local function make_joint(name, joint_type)
                    createJoint({
                        name = name, end1 = "first", end2 = "second",
                        type = joint_type, coordType = 2,
                        x1 = 0, y1 = 0, x2 = 0, y2 = 0
                    })
                end
                make_joint("fractional", 1.5)
                make_joint("rounds_to_distance", 1.00000001)
                make_joint("above_distance", 1.0000001)
                make_joint("not_a_number", 0 / 0)
                make_joint("rounds_to_custom", 6.9999999)
            "#,
        )
        .unwrap();

    let bridge = runtime.render.lock().unwrap();
    assert_eq!(bridge.joints["fractional"].joint_type, 0);
    assert!(!bridge.joints["fractional"].is_physical);
    assert_eq!(bridge.joints["rounds_to_distance"].joint_type, 1);
    assert!(bridge.joints["rounds_to_distance"].is_physical);
    assert_eq!(bridge.joints["above_distance"].joint_type, 0);
    assert!(!bridge.joints["above_distance"].is_physical);
    assert_eq!(bridge.joints["not_a_number"].joint_type, 0);
    assert!(!bridge.joints["not_a_number"].is_physical);
    assert!(!bridge.joints.contains_key("rounds_to_custom"));
    drop(bridge);

    let environment = game_environment(runtime.lua()).unwrap();
    let custom_types = environment.get::<mlua::Table>("custom_types").unwrap();
    assert_eq!(custom_types.raw_len(), 1);
    assert_eq!(custom_types.raw_get::<f64>(1).unwrap(), 6.999_999_9);

    let joints = environment
        .get::<mlua::Table>("objects")
        .unwrap()
        .get::<mlua::Table>("joints")
        .unwrap();
    assert_eq!(
        joints
            .get::<mlua::Table>("fractional")
            .unwrap()
            .get::<f64>("type")
            .unwrap(),
        f64::from(1.5_f32)
    );
    assert_eq!(
        joints
            .get::<mlua::Table>("rounds_to_distance")
            .unwrap()
            .get::<f64>("type")
            .unwrap(),
        1.0
    );
    assert!(matches!(
        joints
            .get::<mlua::Table>("above_distance")
            .unwrap()
            .raw_get::<Value>("length")
            .unwrap(),
        Value::Nil
    ));
    assert!(
        joints
            .get::<mlua::Table>("not_a_number")
            .unwrap()
            .get::<f64>("type")
            .unwrap()
            .is_nan()
    );
}

#[test]
fn custom_joints_dispatch_to_lua_and_allow_editor_reentry() {
    let runtime = unlocked_test_runtime();
    runtime
        .execute_source(
            r#"
                createBox("first", "", 0, 0, 1, 1, 0, 0, 0, true, false, 1)
                createBox("second", "", 4, 0, 1, 1, 1, 0, 0, true, false, 1)
                custom_calls = 0
                createCustomJoint = function(descriptor)
                    custom_calls = custom_calls + 1
                    observed_custom_type = descriptor.type
                end
                createJoint({
                    name = "rope_metadata", end1 = "first", end2 = "second",
                    type = 7, x1 = 0, y1 = 0, x2 = 0, y2 = 0,
                    width = 1, stability = 0.2
                })
                "#,
        )
        .unwrap();

    let environment = game_environment(runtime.lua()).unwrap();
    assert_eq!(environment.get::<i64>("custom_calls").unwrap(), 1);
    assert_eq!(environment.get::<i64>("observed_custom_type").unwrap(), 7);
    assert!(runtime.render.lock().unwrap().joints.is_empty());
    assert!(matches!(
        environment
            .get::<mlua::Table>("objects")
            .unwrap()
            .raw_get::<Value>("joints")
            .unwrap(),
        Value::Nil
    ));

    // CustomJointHandlers' editor branch changes type 7 to type 2 and
    // recursively enters createJoint. The native mutex must not still be
    // held when that happens.
    runtime
        .execute_source(
            r#"
                createCustomJoint = function(descriptor)
                    descriptor.type = 2
                    createJoint(descriptor)
                    descriptor.type = 7
                end
                createJoint({
                    name = "editor_rope", end1 = "first", end2 = "second",
                    type = 7, x1 = 0, y1 = 0, x2 = 0, y2 = 0
                })
                "#,
        )
        .unwrap();
    let bridge = runtime.render.lock().unwrap();
    let joint = &bridge.joints["editor_rope"];
    assert_eq!(joint.joint_type, 2);
    let first_anchor = bridge.scene["first"].transform_collision_point(joint.first_anchor);
    let second_anchor = bridge.scene["second"].transform_collision_point(joint.second_anchor);
    assert!((first_anchor.0 - 2.0).abs() < 1e-9);
    assert!((second_anchor.0 - 2.0).abs() < 1e-9);
    assert!(first_anchor.1.abs() < 1e-9 && second_anchor.1.abs() < 1e-9);
    drop(bridge);
    let descriptors = environment
        .get::<mlua::Table>("objects")
        .unwrap()
        .get::<mlua::Table>("joints")
        .unwrap();
    assert_eq!(
        descriptors
            .get::<mlua::Table>("editor_rope")
            .unwrap()
            .get::<f64>("type")
            .unwrap(),
        2.0
    );
}

#[test]
fn native_joint_descriptor_is_published_only_after_success_and_is_not_borrowed() {
    let runtime = unlocked_test_runtime();
    runtime
        .execute_source(
            r#"
                createBox("first", "", 0, 0, 1, 1, 1, 0, 0, true, false, 1)
                createBox("second", "", 2, 0, 1, 1, 1, 0, 0, true, false, 1)
                local descriptor = {
                    name = "owned_copy", end1 = "first", end2 = "second",
                    type = 2, coordType = 2,
                    x1 = 1, y1 = 0, x2 = -1, y2 = 0
                }
                createJoint(descriptor)
                published_is_distinct = objects.joints.owned_copy ~= descriptor
                descriptor.type = 6
                descriptor.end2 = "mutated"

                LevelSplitter = { active = true }
                createJoint({
                    name = "missing_endpoint", end1 = "first", end2 = "absent",
                    type = 2, coordType = 2,
                    x1 = 0, y1 = 0, x2 = 0, y2 = 0
                })
            "#,
        )
        .unwrap();

    let environment = game_environment(runtime.lua()).unwrap();
    assert!(environment.get::<bool>("published_is_distinct").unwrap());
    let descriptors = environment
        .get::<mlua::Table>("objects")
        .unwrap()
        .get::<mlua::Table>("joints")
        .unwrap();
    let published = descriptors.get::<mlua::Table>("owned_copy").unwrap();
    assert_eq!(published.get::<f64>("type").unwrap(), 2.0);
    assert_eq!(published.get::<String>("end2").unwrap(), "second");
    assert!(matches!(
        descriptors.raw_get::<Value>("missing_endpoint").unwrap(),
        Value::Nil
    ));
    let bridge = runtime.render.lock().unwrap();
    assert!(bridge.joints.contains_key("owned_copy"));
    assert!(!bridge.joints.contains_key("missing_endpoint"));
}

#[test]
fn missing_joint_endpoints_follow_level_splitter_and_body_presence_boundaries() {
    let runtime = unlocked_test_runtime();
    runtime
        .execute_source(
            r#"
                createBox("first", "", 0, 0, 1, 1, 1, 0, 0, true, false, 1)
                createBox("second", "", 2, 0, 1, 1, 1, 0, 0, true, false, 1)
                createNonPhysicsObject("visual", "", 1, 0, 3)

                local ok, message = pcall(createJoint, {
                    name = "missing_first", end1 = "absent_first", end2 = "second",
                    type = 2, coordType = 2, x1 = 0, y1 = 0, x2 = 0, y2 = 0
                })
                missing_first_failed = not ok
                missing_first_error = tostring(message)
                ok, message = pcall(createJoint, {
                    name = "missing_second", end1 = "first", end2 = "absent_second",
                    type = 2, coordType = 2, x1 = 0, y1 = 0, x2 = 0, y2 = 0
                })
                missing_second_failed = not ok
                missing_second_error = tostring(message)

                bodyless_first_succeeds = pcall(createJoint, {
                    name = "bodyless_first", end1 = "visual", end2 = "second",
                    type = 2, coordType = 2, x1 = 0, y1 = 0, x2 = 0, y2 = 0
                })
                bodyless_second_succeeds = pcall(createJoint, {
                    name = "bodyless_second", end1 = "first", end2 = "visual",
                    type = 2, coordType = 2, x1 = 0, y1 = 0, x2 = 0, y2 = 0
                })

                LevelSplitter = { active = true }
                splitter_missing_succeeds = pcall(createJoint, {
                    name = "split_missing", end1 = "first", end2 = "not_loaded",
                    type = 2, coordType = 2, x1 = 0, y1 = 0, x2 = 0, y2 = 0
                })

                createJoint({
                    name = "", end1 = "first", end2 = "second",
                    type = 2, coordType = 2, x1 = 0, y1 = 0, x2 = 0, y2 = 0
                })
                empty_name_published = objects.joints[""] ~= nil
            "#,
        )
        .unwrap();

    let environment = game_environment(runtime.lua()).unwrap();
    assert!(environment.get::<bool>("missing_first_failed").unwrap());
    assert!(environment.get::<bool>("missing_second_failed").unwrap());
    assert!(
        environment
            .get::<String>("missing_first_error")
            .unwrap()
            .starts_with(
                "runtime error: The block absent_first connected to joint missing_first doesn't exist"
            )
    );
    assert!(
        environment
            .get::<String>("missing_second_error")
            .unwrap()
            .starts_with(
                "runtime error: The block absent_second connected to joint missing_second doesn't exist"
            )
    );
    assert!(environment.get::<bool>("bodyless_first_succeeds").unwrap());
    assert!(environment.get::<bool>("bodyless_second_succeeds").unwrap());
    assert!(
        environment
            .get::<bool>("splitter_missing_succeeds")
            .unwrap()
    );
    assert!(environment.get::<bool>("empty_name_published").unwrap());

    let bridge = runtime.render.lock().unwrap();
    for absent in [
        "missing_first",
        "missing_second",
        "bodyless_first",
        "bodyless_second",
        "split_missing",
    ] {
        assert!(!bridge.joints.contains_key(absent), "{absent}");
    }
    assert!(bridge.joints.contains_key(""));
}

#[test]
fn native_joint_descriptor_uses_the_recovered_canonical_field_schema() {
    let runtime = unlocked_test_runtime();
    runtime
        .execute_source(
            r#"
                createBox("first", "", 0, 0, 1, 1, 0, 0, 0, true, false, 1)
                createBox("second", "", 2, 0, 1, 1, 1, 0, 0, true, false, 1)
                local source = {
                    name = "canonical_hinge", end1 = "first", end2 = "second",
                    type = 3, coordType = 2.49,
                    x1 = 1.00000006, y1 = 0, x2 = -1, y2 = 0,
                    breakable = "wrong type", breakForce = 1.00000006,
                    isDrawn = false, sprite = "joint_sprite",
                    angleTarget = 0.20000002,
                    angleCorrectionTorque = 30,
                    angleCorrectionMotorSpeed = 4,
                    isMenuJoint = false,
                    editorOnly = "must not leak"
                }
                createJoint(source)
                source_was_not_filled = source.motor == nil
                    and source.upperLimit == nil
                    and source.backAndForth == nil
            "#,
        )
        .unwrap();

    let environment = game_environment(runtime.lua()).unwrap();
    assert!(environment.get::<bool>("source_was_not_filled").unwrap());
    let descriptor = environment
        .get::<mlua::Table>("objects")
        .unwrap()
        .get::<mlua::Table>("joints")
        .unwrap()
        .get::<mlua::Table>("canonical_hinge")
        .unwrap();
    assert_eq!(
        string_key_set(&descriptor),
        std::collections::BTreeSet::from(
            [
                "angleTarget",
                "backAndForth",
                "breakForce",
                "collideConnected",
                "coordType",
                "end1",
                "end2",
                "isDrawn",
                "limit",
                "lowerLimit",
                "maxTorque",
                "motor",
                "motorSpeed",
                "name",
                "sprite",
                "type",
                "upperLimit",
                "x1",
                "x2",
                "y1",
                "y2",
            ]
            .map(str::to_owned)
        ),
    );
    assert_eq!(descriptor.get::<f64>("coordType").unwrap(), 2.0);
    assert_eq!(
        descriptor.get::<f64>("x1").unwrap(),
        f64::from(1.000_000_1_f32)
    );
    assert_eq!(
        descriptor.get::<f64>("breakForce").unwrap(),
        f64::from(1.000_000_1_f32)
    );
    assert_eq!(
        descriptor.get::<f64>("angleTarget").unwrap(),
        f64::from(0.200_000_02_f32)
    );
    assert!(!descriptor.get::<bool>("motor").unwrap());
    assert_eq!(descriptor.get::<f64>("motorSpeed").unwrap(), 0.0);
    assert_eq!(descriptor.get::<f64>("maxTorque").unwrap(), 10_000.0);
    assert!(!descriptor.get::<bool>("limit").unwrap());
    assert_eq!(descriptor.get::<f64>("lowerLimit").unwrap(), 0.0);
    assert_eq!(
        descriptor.get::<f64>("upperLimit").unwrap(),
        f64::from(std::f32::consts::PI)
    );
    assert!(!descriptor.get::<bool>("backAndForth").unwrap());
    assert!(!descriptor.get::<bool>("collideConnected").unwrap());
    assert!(!descriptor.get::<bool>("isDrawn").unwrap());
    assert_eq!(descriptor.get::<String>("sprite").unwrap(), "joint_sprite");
}

#[test]
fn each_native_joint_class_publishes_only_its_recovered_fields() {
    let runtime = unlocked_test_runtime();
    runtime
        .execute_source(
            r#"
                createBox("first", "", 3, 4, 1, 1, 0, 0, 0, true, false, 1)
                createBox("second", "", 8, 10, 1, 1, 1, 0, 0, true, false, 1)
                createJoint({
                    name = "distance", end1 = "first", end2 = "second",
                    type = 1, coordType = 0,
                    x1 = 99, y1 = 98, x2 = 97, y2 = 96, leaked = true
                })
                createJoint({
                    name = "weld", end1 = "first", end2 = "second",
                    type = 2, coordType = 2,
                    x1 = 1, y1 = 2, x2 = 3, y2 = 4, leaked = true
                })
                createJoint({
                    name = "slider", end1 = "first", end2 = "second",
                    type = 4, coordType = 2,
                    x1 = 1, y1 = 2, x2 = 3, y2 = 4,
                    worldAxisX = 1, worldAxisY = 0, leaked = true
                })
                createJoint({
                    name = "destroy_link", end1 = "first", end2 = "second",
                    type = 5, coordType = 2,
                    x1 = 1, y1 = 2, x2 = 3, y2 = 4,
                    oneWayDestroy = false, leaked = true
                })
                createJoint({
                    name = "rope", end1 = "first", end2 = "second",
                    type = 6, coordType = 2,
                    x1 = 1, y1 = 2, x2 = 3, y2 = 4,
                    maxLength = 9, leaked = true
                })
            "#,
        )
        .unwrap();

    let joints = game_environment(runtime.lua())
        .unwrap()
        .get::<mlua::Table>("objects")
        .unwrap()
        .get::<mlua::Table>("joints")
        .unwrap();
    let distance = joints.get::<mlua::Table>("distance").unwrap();
    assert_eq!(
        string_key_set(&distance),
        std::collections::BTreeSet::from(
            [
                "collideConnected",
                "coordType",
                "dampingRatio",
                "end1",
                "end2",
                "frequency",
                "length",
                "name",
                "type",
                "x1",
                "x2",
                "y1",
                "y2",
            ]
            .map(str::to_owned)
        ),
    );
    assert_eq!(distance.get::<f64>("x1").unwrap(), 3.0);
    assert_eq!(distance.get::<f64>("y1").unwrap(), 4.0);
    assert_eq!(distance.get::<f64>("x2").unwrap(), 8.0);
    assert_eq!(distance.get::<f64>("y2").unwrap(), 10.0);

    let weld = joints.get::<mlua::Table>("weld").unwrap();
    assert_eq!(
        string_key_set(&weld),
        std::collections::BTreeSet::from(
            [
                "collideConnected",
                "coordType",
                "end1",
                "end2",
                "name",
                "type",
                "x1",
                "x2",
                "y1",
                "y2",
            ]
            .map(str::to_owned)
        ),
    );

    let slider = joints.get::<mlua::Table>("slider").unwrap();
    assert_eq!(
        string_key_set(&slider),
        std::collections::BTreeSet::from(
            [
                "backAndForth",
                "collideConnected",
                "coordType",
                "end1",
                "end2",
                "limit",
                "lowerLimit",
                "maxTorque",
                "motor",
                "motorSpeed",
                "name",
                "type",
                "upperLimit",
                "worldAxisX",
                "worldAxisY",
                "x1",
                "x2",
                "y1",
                "y2",
            ]
            .map(str::to_owned)
        ),
    );
    assert!(slider.get::<bool>("limit").unwrap());
    assert!(slider.get::<bool>("motor").unwrap());
    assert!(slider.get::<bool>("backAndForth").unwrap());

    let destroy_link = joints.get::<mlua::Table>("destroy_link").unwrap();
    assert_eq!(
        string_key_set(&destroy_link),
        std::collections::BTreeSet::from(
            [
                "coordType",
                "destroyTimer",
                "end1",
                "end2",
                "name",
                "oneWayDestroy",
                "type",
                "x1",
                "x2",
                "y1",
                "y2",
            ]
            .map(str::to_owned)
        ),
    );
    assert_eq!(destroy_link.get::<f64>("destroyTimer").unwrap(), 1.0);
    assert!(!destroy_link.get::<bool>("oneWayDestroy").unwrap());

    let rope = joints.get::<mlua::Table>("rope").unwrap();
    assert_eq!(
        string_key_set(&rope),
        std::collections::BTreeSet::from(
            [
                "collideConnected",
                "coordType",
                "end1",
                "end2",
                "name",
                "type",
                "x1",
                "x2",
                "y1",
                "y2",
            ]
            .map(str::to_owned)
        ),
    );
    assert!(matches!(
        rope.raw_get::<Value>("maxLength").unwrap(),
        Value::Nil
    ));
}

fn string_key_set(table: &mlua::Table) -> std::collections::BTreeSet<String> {
    table
        .clone()
        .pairs::<Value, Value>()
        .filter_map(|pair| match pair.unwrap().0 {
            Value::String(key) => Some(key.to_string_lossy()),
            _ => None,
        })
        .collect()
}

#[test]
fn joint_batch_separates_native_and_custom_descriptors() {
    let runtime = unlocked_test_runtime();
    runtime
        .execute_source(
            r#"
                createBox("first", "", 0, 0, 1, 1, 0, 0, 0, true, false, 1)
                createBox("second", "", 2, 0, 1, 1, 1, 0, 0, true, false, 1)
                dispatched = {}
                createCustomJoint = function(descriptor)
                    table.insert(dispatched, descriptor.type)
                end
                createJoints({
                    native = {
                        name = "native", end1 = "first", end2 = "second",
                        type = 2, coordType = 2, x1 = 1, y1 = 0, x2 = -1, y2 = 0
                    },
                    trigger = {
                        name = "trigger", end1 = "first", end2 = "second", type = 8
                    },
                    area = {
                        name = "area", end1 = "first", end2 = "second", type = 9
                    }
                })
                "#,
        )
        .unwrap();

    let environment = game_environment(runtime.lua()).unwrap();
    let dispatched = environment.get::<mlua::Table>("dispatched").unwrap();
    let mut types = dispatched
        .sequence_values::<i64>()
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    types.sort_unstable();
    assert_eq!(types, vec![8, 9]);
    let bridge = runtime.render.lock().unwrap();
    assert!(bridge.joints.contains_key("native"));
    assert!(!bridge.joints.contains_key("trigger"));
    assert!(!bridge.joints.contains_key("area"));
}

#[test]
fn joint_batch_dispatches_each_custom_descriptor_before_the_next_native_value() {
    let runtime = unlocked_test_runtime();
    runtime
        .execute_source(
            r#"
                createBox("anchor", "", 0, 0, 1, 1, 0, 0, 0, true, false, 1)
                batch_events = {}
                createCustomJoint = function(descriptor)
                    table.insert(batch_events, "custom:" .. descriptor.name)
                    if descriptor.name == "builder" then
                        createBox(
                            "custom_endpoint", "", 2, 0,
                            1, 1, 1, 0, 0, true, false, 1
                        )
                    end
                end
                createJoints({
                    {
                        name = "builder", end1 = "anchor", end2 = "anchor",
                        type = 7, x1 = 0, y1 = 0, x2 = 0, y2 = 0
                    },
                    {
                        name = "dependent", end1 = "anchor", end2 = "custom_endpoint",
                        type = 2, coordType = 2,
                        x1 = 0, y1 = 0, x2 = 0, y2 = 0
                    },
                    {
                        name = "observer", end1 = "anchor", end2 = "anchor",
                        type = 8
                    }
                })
            "#,
        )
        .unwrap();

    let environment = game_environment(runtime.lua()).unwrap();
    let events = environment.get::<mlua::Table>("batch_events").unwrap();
    assert_eq!(events.raw_get::<String>(1).unwrap(), "custom:builder");
    assert_eq!(events.raw_get::<String>(2).unwrap(), "custom:observer");
    let bridge = runtime.render.lock().unwrap();
    assert!(bridge.scene.contains_key("custom_endpoint"));
    assert!(bridge.joints.contains_key("dependent"));
}

#[test]
fn recovered_type_five_is_a_destroy_link_not_a_collision_joint() {
    let runtime = unlocked_test_runtime();
    runtime
        .execute_source(
            r#"
                createCircle("source", "", 0, 0, 1, 1, 0, 0, true, false, 1)
                createCircle("linked", "", 0.5, 0, 1, 1, 0, 0, true, false, 1)
                createJoint({
                    name = "destroy_link", end1 = "source", end2 = "linked",
                    type = 5, destroyTimer = 0, oneWayDestroy = false
                })
                setVelocity("source", 0.01, 0)
                "#,
        )
        .unwrap();

    {
        let mut bridge = runtime.render.lock().unwrap();
        let events = bridge.solve_contacts();
        assert!(
            events.iter().any(|event| {
                event.first == "linked" && event.second == "source" && event.began
            })
        );
    }

    runtime
        .execute_source(r#"removeJointsFromObject("source")"#)
        .unwrap();
    let world = object_world(runtime.lua()).unwrap();
    assert!(matches!(
        world.raw_get::<Value>("linked").unwrap(),
        Value::Table(_)
    ));
    {
        let bridge = runtime.render.lock().unwrap();
        assert!(bridge.scene.contains_key("source"));
        assert!(bridge.scene.contains_key("linked"));
        assert!(!bridge.joints.contains_key("destroy_link"));
        assert_eq!(bridge.pending_object_destructions.get("linked"), Some(&0.0));
    }

    runtime.execute_source("update = function() end").unwrap();
    runtime.update(1.0 / 60.0).unwrap();
    let linked = world.raw_get::<mlua::Table>("linked").unwrap();
    assert_eq!(linked.get::<f64>("strength").unwrap(), 0.0);
    let environment = game_environment(runtime.lua()).unwrap();
    let dead_blocks = environment.get::<mlua::Table>("deadBlocks").unwrap();
    let queued = dead_blocks.raw_get::<mlua::Table>("linked").unwrap();
    assert_eq!(queued.to_pointer(), linked.to_pointer());
}

#[test]
fn metadata_destroy_link_uses_and_publishes_native_one_second_default() {
    let runtime = unlocked_test_runtime();
    runtime
        .execute_source(
            r#"
                createBox("source", "", 0, 0, 1, 1, 1, 0, 0, true, false, 1)
                createBox("linked", "", 2, 0, 1, 1, 1, 0, 0, true, false, 1)
                createJoint({
                    name = "default_delay", end1 = "source", end2 = "linked",
                    type = 5, oneWayDestroy = false
                })
                observed_default_delay = objects.joints.default_delay.destroyTimer
            "#,
        )
        .unwrap();

    let environment = game_environment(runtime.lua()).unwrap();
    assert_eq!(
        environment.get::<f64>("observed_default_delay").unwrap(),
        1.0
    );
    assert_eq!(
        runtime.render.lock().unwrap().joints["default_delay"].destroy_timer,
        f64::from(1.0_f32)
    );

    runtime
        .execute_source(r#"removeJointsFromObject("source")"#)
        .unwrap();
    let bridge = runtime.render.lock().unwrap();
    assert_eq!(
        bridge.pending_object_destructions.get("linked"),
        Some(&f64::from(1.0_f32))
    );
}

#[test]
fn object_joint_removal_callbacks_precede_descriptor_retirement() {
    let runtime = unlocked_test_runtime();
    runtime
        .execute_source(
            r#"
                createBox("source", "", 0, 0, 1, 1, 1, 0, 0, true, false, 1)
                createBox("first", "", 2, 0, 1, 1, 1, 0, 0, true, false, 1)
                createBox("second", "", 4, 0, 1, 1, 1, 0, 0, true, false, 1)
                local drawn = {
                    name = "a_drawn", end1 = "source", end2 = "first",
                    type = 2, isDrawn = true
                }
                local hidden = {
                    name = "b_hidden", end1 = "source", end2 = "second",
                    type = 2, isDrawn = false
                }
                createJoint(drawn)
                createJoint(hidden)
                removalOrder = {}
                lua_onBeforeJointRemove = function(name)
                    assert(objects.joints[name] ~= nil)
                    table.insert(removalOrder, "before:" .. name)
                end
                lua_addParticlesToJoint = function(name)
                    assert(objects.joints[name] ~= nil)
                    table.insert(removalOrder, "particles:" .. name)
                end

                removeJointsFromObject("source")
                assert(objects.joints.a_drawn == nil)
                assert(objects.joints.b_hidden == nil)

                local bodyRemove = {
                    name = "body_remove", end1 = "source", end2 = "first",
                    type = 2, isDrawn = true
                }
                createJoint(bodyRemove)
                removeObject("source")
                assert(objects.joints.body_remove == nil)
            "#,
        )
        .unwrap();

    let environment = game_environment(runtime.lua()).unwrap();
    assert_eq!(
        environment
            .get::<mlua::Table>("removalOrder")
            .unwrap()
            .sequence_values::<String>()
            .collect::<Result<Vec<_>, _>>()
            .unwrap(),
        [
            "before:a_drawn",
            "particles:a_drawn",
            "before:b_hidden",
            "before:body_remove",
            "particles:body_remove",
        ]
    );
    let world = object_world(runtime.lua()).unwrap();
    assert!(matches!(
        world.raw_get::<Value>("source").unwrap(),
        Value::Nil
    ));
    assert!(matches!(
        world.raw_get::<Value>("first").unwrap(),
        Value::Table(_)
    ));
}

#[test]
fn recovered_type_five_one_way_and_delayed_destruction_match_native_metadata() {
    let runtime = unlocked_test_runtime();
    runtime
        .execute_source(
            r#"
                createBox("source", "", 0, 0, 1, 1, 1, 0, 0, true, false, 1)
                createBox("linked", "", 2, 0, 1, 1, 1, 0, 0, true, false, 1)
                createBox("linked_anchor", "", 4, 0, 1, 1, 0, 0, 0, false, false, 1)
                createJoint({
                    name = "delayed", end1 = "source", end2 = "linked",
                    type = 5, destroyTimer = 5, oneWayDestroy = true
                })
                createJoint({
                    name = "linked_drawn", end1 = "linked", end2 = "linked_anchor",
                    type = 2, isDrawn = true
                })
                delayedRemovalOrder = {}
                lua_onBeforeJointRemove = function(name)
                    assert(objects.joints[name] ~= nil)
                    table.insert(delayedRemovalOrder, "before:" .. name)
                end
                lua_addParticlesToJoint = function(name)
                    assert(objects.joints[name] ~= nil)
                    table.insert(delayedRemovalOrder, "particles:" .. name)
                end
                removeJointsFromObject("source")
                update = function() end
                updatePhysics = function() end
                removeBlocks = function()
                    for name in pairs(deadBlocks or {}) do
                        removeObject(name)
                        deadBlocks[name] = nil
                    end
                end
                "#,
        )
        .unwrap();

    for _ in 0..299 {
        runtime.update(1.0 / 60.0).unwrap();
    }
    assert!(runtime.render.lock().unwrap().scene.contains_key("linked"));
    let environment = game_environment(runtime.lua()).unwrap();
    assert_eq!(
        environment
            .get::<mlua::Table>("delayedRemovalOrder")
            .unwrap()
            .sequence_values::<String>()
            .collect::<Result<Vec<_>, _>>()
            .unwrap(),
        ["before:delayed"]
    );
    for _ in 0..2 {
        runtime.update(1.0 / 60.0).unwrap();
    }
    assert!(runtime.render.lock().unwrap().scene.contains_key("linked"));
    let world = object_world(runtime.lua()).unwrap();
    let linked = world.raw_get::<mlua::Table>("linked").unwrap();
    assert_eq!(linked.get::<f64>("strength").unwrap(), 0.0);
    let dead_blocks = environment.get::<mlua::Table>("deadBlocks").unwrap();
    assert_eq!(
        dead_blocks
            .raw_get::<mlua::Table>("linked")
            .unwrap()
            .to_pointer(),
        linked.to_pointer()
    );
    assert_eq!(
        environment
            .get::<mlua::Table>("delayedRemovalOrder")
            .unwrap()
            .sequence_values::<String>()
            .collect::<Result<Vec<_>, _>>()
            .unwrap(),
        ["before:delayed"]
    );

    runtime.update(1.0 / 30.0).unwrap();
    assert!(!runtime.render.lock().unwrap().scene.contains_key("linked"));
    assert!(matches!(
        world.raw_get::<Value>("linked").unwrap(),
        Value::Nil
    ));
    assert_eq!(
        environment
            .get::<mlua::Table>("delayedRemovalOrder")
            .unwrap()
            .sequence_values::<String>()
            .collect::<Result<Vec<_>, _>>()
            .unwrap(),
        [
            "before:delayed",
            "before:linked_drawn",
            "particles:linked_drawn",
        ]
    );
    let joints = environment
        .get::<mlua::Table>("objects")
        .unwrap()
        .get::<mlua::Table>("joints")
        .unwrap();
    assert!(matches!(
        joints.raw_get::<Value>("linked_drawn").unwrap(),
        Value::Nil
    ));

    runtime
        .execute_source(
            r#"
                createBox("upstream", "", 4, 0, 1, 1, 0, 0, 0, true, false, 1)
                createBox("downstream", "", 6, 0, 1, 1, 0, 0, 0, true, false, 1)
                createJoint({
                    name = "one_way", end1 = "upstream", end2 = "downstream",
                    type = 5, destroyTimer = 0, oneWayDestroy = true
                })
                removeJointsFromObject("downstream")
                "#,
        )
        .unwrap();
    let bridge = runtime.render.lock().unwrap();
    assert!(bridge.scene.contains_key("upstream"));
    assert!(bridge.scene.contains_key("downstream"));
}

#[test]
fn type_five_expiry_keeps_lua_draw_target_alive_until_remove_blocks_disposes_it() {
    let runtime = unlocked_test_runtime();
    runtime
        .execute_source(
            r#"
                createBox("source", "", 0, 0, 1, 1, 1, 0, 0, true, false, 1)
                createBox("pig_medium_right_3", "", 2, 0, 1, 1, 1, 0, 0, true, false, 1)
                createJoint({
                    name = "pig_destroy_link",
                    end1 = "source",
                    end2 = "pig_medium_right_3",
                    type = 5,
                    destroyTimer = 0,
                    oneWayDestroy = true
                })
                drawCallbacks = {
                    pig_medium_right_3 = {
                        func = function()
                            local x, y = getScale("pig_medium_right_3")
                            lastPigScale = {x, y}
                        end
                    }
                }
                draw = function()
                    for _, callback in pairs(drawCallbacks) do
                        callback.func()
                    end
                end
                update = function() end
                updatePhysics = function() end
                removeBlocks = function()
                    for name in pairs(deadBlocks or {}) do
                        drawCallbacks[name] = nil
                        removeObject(name)
                        deadBlocks[name] = nil
                    end
                end
                removeJointsFromObject("source")
                "#,
        )
        .unwrap();

    runtime.update(1.0 / 60.0).unwrap();
    runtime.draw().unwrap();
    let environment = game_environment(runtime.lua()).unwrap();
    let scale = environment.get::<mlua::Table>("lastPigScale").unwrap();
    assert_eq!(scale.raw_get::<f64>(1).unwrap(), 1.0);
    assert_eq!(scale.raw_get::<f64>(2).unwrap(), 1.0);
    assert!(
        runtime
            .render
            .lock()
            .unwrap()
            .scene
            .contains_key("pig_medium_right_3")
    );

    runtime.update(1.0 / 30.0).unwrap();
    runtime.draw().unwrap();
    assert!(
        !runtime
            .render
            .lock()
            .unwrap()
            .scene
            .contains_key("pig_medium_right_3")
    );
    assert!(matches!(
        object_world(runtime.lua())
            .unwrap()
            .raw_get::<Value>("pig_medium_right_3")
            .unwrap(),
        Value::Nil
    ));
}

#[test]
fn recovered_joint_coordinate_modes_match_native_local_anchor_switch() {
    let runtime = unlocked_test_runtime();
    runtime
        .execute_source(
            r#"
                createBox("first", "", 10, 5, 1, 1, 0, 0, 0, false, false, 1)
                createBox("second", "", 20, 7, 1, 1, 1, 0, 0, false, false, 1)
                createJoint({
                    name = "center", end1 = "first", end2 = "second", type = 1,
                    coordType = 0, x1 = 99, y1 = 99, x2 = 99, y2 = 99
                })
                createJoint({
                    name = "world_offsets", end1 = "first", end2 = "second", type = 1,
                    coordType = 1, x1 = 12, y1 = 8, x2 = 24, y2 = 12
                })
                createJoint({
                    name = "local", end1 = "first", end2 = "second", type = 1,
                    coordType = 2, x1 = 3, y1 = 4, x2 = 5, y2 = 6
                })
                "#,
        )
        .unwrap();

    let bridge = runtime.render.lock().unwrap();
    assert_eq!(bridge.joints["center"].first_anchor, (0.0, 0.0));
    assert_eq!(bridge.joints["center"].second_anchor, (0.0, 0.0));
    assert_eq!(bridge.joints["world_offsets"].first_anchor, (2.0, 3.0));
    assert_eq!(bridge.joints["world_offsets"].second_anchor, (4.0, 5.0));
    assert_eq!(bridge.joints["local"].first_anchor, (3.0, 4.0));
    assert_eq!(bridge.joints["local"].second_anchor, (5.0, 6.0));
}

#[test]
fn joint_anchors_use_body_transform_without_reapplying_fixture_scale() {
    let runtime = unlocked_test_runtime();
    runtime
        .execute_source(
            r#"
                createBox("first", "", 0, 0, 1, 1, 0, 0, 0, true, false, 1)
                createBox("second", "", 10, 0, 1, 1, 1, 0, 0, true, false, 1)
                setPhysicsScale("first", 2, 2)
                setPhysicsScale("second", 2, 2)
                createJoint({
                    name = "scaled_distance", end1 = "first", end2 = "second",
                    type = 1, coordType = 2, x1 = 1, y1 = 0, x2 = -1, y2 = 0
                })
            "#,
        )
        .unwrap();

    let bridge = runtime.render.lock().unwrap();
    let joint = &bridge.joints["scaled_distance"];
    assert_eq!(joint.first_anchor, (1.0, 0.0));
    assert_eq!(joint.second_anchor, (-1.0, 0.0));
    // Native b2Body transforms place the anchors at x=1 and x=9. Using the
    // fixture projection path would incorrectly scale them to x=2 and x=8.
    assert_eq!(joint.rest_length, 8.0);
}

#[test]
fn frame_update_exports_nonlocal_joint_anchors_but_preserves_coord_type_two_fields() {
    let runtime = unlocked_test_runtime();
    runtime
        .execute_source(
            r#"
                createBox("first", "", 1, 2, 1, 1, 1, 0, 0, true, false, 1)
                createBox("second", "", 10, 20, 1, 1, 1, 0, 0, true, false, 1)
                createJoint({
                    name = "center_export", end1 = "first", end2 = "second",
                    type = 1, coordType = 0, x1 = 21, y1 = 22,
                    x2 = 23, y2 = 24
                })
                createJoint({
                    name = "local_preserved", end1 = "first", end2 = "second",
                    type = 3, coordType = 2, x1 = 0.25, y1 = -0.5,
                    x2 = -0.75, y2 = 1.25
                })
                createJoint({
                    name = "weld_export", end1 = "first", end2 = "second",
                    type = 2, coordType = 0, x1 = 31, y1 = 32, x2 = 33, y2 = 34
                })
                createJoint({
                    name = "metadata_only", end1 = "first", end2 = "second",
                    type = 5, coordType = 2, oneWayDestroy = false,
                    x1 = 41, y1 = 42, x2 = 43, y2 = 44
                })
                clearLuaForceFunctions = function() end
                update = function() end
            "#,
        )
        .unwrap();

    let (expected_center, expected_weld) = {
        let mut bridge = runtime.render.lock().unwrap();
        bridge.scene.get_mut("first").unwrap().angle = 0.375;
        bridge.scene.get_mut("second").unwrap().angle = -0.625;
        let center = &bridge.joints["center_export"];
        let center_anchors = (
            bridge.scene["first"].native_transform_body_point((
                center.first_anchor.0 as f32,
                center.first_anchor.1 as f32,
            )),
            bridge.scene["second"].native_transform_body_point((
                center.second_anchor.0 as f32,
                center.second_anchor.1 as f32,
            )),
        );
        let weld = &bridge.joints["weld_export"];
        let weld_anchors = (
            bridge.scene["first"].native_transform_body_point((
                weld.first_anchor.0 as f32,
                weld.first_anchor.1 as f32,
            )),
            bridge.scene["second"].native_transform_body_point((
                weld.second_anchor.0 as f32,
                weld.second_anchor.1 as f32,
            )),
        );
        (center_anchors, weld_anchors)
    };

    runtime.update(0.0).unwrap();
    let environment = game_environment(runtime.lua()).unwrap();
    let objects: mlua::Table = environment.get("objects").unwrap();
    let joints: mlua::Table = objects.get("joints").unwrap();
    let center: mlua::Table = joints.get("center_export").unwrap();
    assert_eq!(
        center.get::<f64>("x1").unwrap(),
        f64::from(expected_center.0.0)
    );
    assert_eq!(
        center.get::<f64>("y1").unwrap(),
        f64::from(expected_center.0.1)
    );
    assert_eq!(
        center.get::<f64>("x2").unwrap(),
        f64::from(expected_center.1.0)
    );
    assert_eq!(
        center.get::<f64>("y2").unwrap(),
        f64::from(expected_center.1.1)
    );

    let weld: mlua::Table = joints.get("weld_export").unwrap();
    assert_eq!(weld.get::<f64>("x1").unwrap(), f64::from(expected_weld.0.0));
    assert_eq!(weld.get::<f64>("y1").unwrap(), f64::from(expected_weld.0.1));
    assert_eq!(weld.get::<f64>("x2").unwrap(), f64::from(expected_weld.1.0));
    assert_eq!(weld.get::<f64>("y2").unwrap(), f64::from(expected_weld.1.1));
    let local: mlua::Table = joints.get("local_preserved").unwrap();
    assert_eq!(local.get::<f64>("x1").unwrap(), 0.25);
    assert_eq!(local.get::<f64>("y1").unwrap(), -0.5);
    assert_eq!(local.get::<f64>("x2").unwrap(), -0.75);
    assert_eq!(local.get::<f64>("y2").unwrap(), 1.25);
    let metadata: mlua::Table = joints.get("metadata_only").unwrap();
    assert_eq!(metadata.get::<f64>("x1").unwrap(), 41.0);

    center.set("x1", -99.0).unwrap();
    runtime
        .execute_source("setPhysicsEnabled(false, 'pause')")
        .unwrap();
    runtime.update(0.0).unwrap();
    assert_eq!(center.get::<f64>("x1").unwrap(), -99.0);
}

#[test]
fn coord_type_two_frame_export_still_requires_its_native_lua_descriptor_lookup() {
    let runtime = unlocked_test_runtime();
    runtime
        .execute_source(
            r#"
                createBox("first", "", 0, 0, 1, 1, 1, 0, 0, true, false, 1)
                createBox("second", "", 2, 0, 1, 1, 1, 0, 0, true, false, 1)
                createJoint({
                    name = "body_local", end1 = "first", end2 = "second", type = 3,
                    coordType = 2, x1 = 0, y1 = 0, x2 = 0, y2 = 0
                })
                objects.joints.body_local = nil
                clearLuaForceFunctions = function() end
                update = function() end
            "#,
        )
        .unwrap();

    assert!(runtime.update(0.0).is_err());
}

#[test]
fn distance_joint_uses_native_short_axis_thresholds() {
    let runtime = unlocked_test_runtime();
    runtime
        .execute_source(
            r#"
                createBox("anchor", "", 0, 0, 1, 1, 0, 0, 0, false, false, 1)
                createBox("payload", "", 1, 0, 1, 1, 1, 0, 0, false, false, 1)
                createJoint({
                    name = "distance", end1 = "anchor", end2 = "payload", type = 1,
                    coordType = 2, x1 = 0, y1 = 0, x2 = 0, y2 = 0,
                    frequency = 0, dampingRatio = 0
                })
                "#,
        )
        .unwrap();

    let mut bridge = runtime.render.lock().unwrap();
    let mut joint = bridge.joints["distance"].clone();
    {
        let payload = bridge.scene.get_mut("payload").unwrap();
        payload.x = 0.0005;
        payload.sync_native_sweep_from_transform();
        payload.velocity_x = 1.0;
    }
    let first = bridge.scene["anchor"].clone();
    let second = bridge.scene["payload"].clone();
    joint.distance_impulse = 2.0;
    bridge.initialize_distance_velocity_constraints(&mut joint, &first, &second, 1.0 / 30.0);
    assert_eq!(joint.distance_axis, (0.0, 0.0));
    assert_eq!(bridge.scene["payload"].velocity_x, 1.0);
    bridge.solve_distance_joint_velocity(&mut joint, &first, &second, 1.0 / 30.0);
    assert_eq!(bridge.scene["payload"].velocity_x, 1.0);

    {
        let payload = bridge.scene.get_mut("payload").unwrap();
        payload.x = 1.0e-8;
        payload.sync_native_sweep_from_transform();
    }
    let first = bridge.scene["anchor"].clone();
    let second = bridge.scene["payload"].clone();
    assert!(!bridge.solve_distance_joint_position(&joint, &first, &second));
    // b2Vec2::Normalize leaves the sub-FLT_EPSILON direction tiny. A
    // unit-normalized replacement would incorrectly move this body by
    // roughly the full 0.2 maximum correction.
    assert!(bridge.scene["payload"].x.abs() < 1.0e-6);
}

#[test]
fn soft_distance_joint_keeps_native_float_solver_cache_and_accumulator() {
    let runtime = unlocked_test_runtime();
    runtime
        .execute_source(
            r#"
                createBox("first", "", -0.375, 0.625, 1, 1, 1.75, 0, 0, false, false, 1)
                createBox("second", "", 2.875, -0.4375, 1, 1, 2.25, 0, 0, false, false, 1)
                createJoint({
                    name = "spring", end1 = "first", end2 = "second", type = 1,
                    coordType = 2, x1 = 0.3125, y1 = -0.1875, x2 = -0.28125, y2 = 0.21875,
                    frequency = 3.125, dampingRatio = 0.4375
                })
                "#,
        )
        .unwrap();

    let mut bridge = runtime.render.lock().unwrap();
    {
        let first = bridge.scene.get_mut("first").unwrap();
        first.velocity_x = 0.81234567;
        first.velocity_y = -0.45678901;
        first.angular_velocity = 0.23456789;
    }
    {
        let second = bridge.scene.get_mut("second").unwrap();
        second.velocity_x = -0.34567891;
        second.velocity_y = 0.67890123;
        second.angular_velocity = -0.12345678;
    }
    let first = bridge.scene["first"].clone();
    let second = bridge.scene["second"].clone();
    let mut joint = bridge.joints["spring"].clone();
    joint.distance_impulse = 0.123456789;
    bridge.initialize_distance_velocity_constraints(&mut joint, &first, &second, 1.0 / 59.94);

    assert_eq!(
        joint.distance_effective_mass,
        f64::from(joint.distance_effective_mass as f32)
    );
    assert_eq!(joint.distance_gamma, f64::from(joint.distance_gamma as f32));
    assert_eq!(joint.distance_bias, f64::from(joint.distance_bias as f32));
    bridge.solve_distance_joint_velocity(&mut joint, &first, &second, 1.0 / 59.94);
    assert_eq!(
        joint.distance_impulse,
        f64::from(joint.distance_impulse as f32)
    );
    assert_ne!(joint.distance_impulse, 0.123456789);
}
