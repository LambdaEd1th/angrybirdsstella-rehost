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
