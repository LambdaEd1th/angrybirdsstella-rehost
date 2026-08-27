use super::*;

#[test]
fn chapter02_level11_drawn_wheel_axles_keep_body_local_coordinates() {
    let sandbox = ShippedDataSandbox::new("chapter02-l11-wheel-axles");
    let runtime = StellaLua::new(&sandbox.data_root).unwrap();
    runtime.boot("scripts/game.lua").unwrap();
    runtime.execute_source("initializeGameCommon()").unwrap();
    runtime
        .execute_source(
            r#"
                SpriteSheetManager.useGroupSet('INGAME')
                currentFolder = 'Chapter02'
                currentPack = 'Chapter02'
                currentLevel = 11
                levelFolder = 'levels/Chapter02/'
                levelName = 'Chapter02_L11'
                loadLevelInternal(levelFolder .. levelName)
                blocks.BlockComponentManager.triggerGlobalEvent(blocks.events.EID_START)
                setPhysicsEnabled(true)
                update = function() end
            "#,
        )
        .unwrap();
    runtime.update(1.0 / 60.0).unwrap();
    runtime.draw().unwrap();
    runtime
        .execute_source(
            r#"
                wheelJointResults = {}
                wheelJointCount = 0
                for name, joint in pairs(objects.joints) do
                    if string.find(name, 'BLOCK_LIGHT_ROUND') then
                        wheelJointCount = wheelJointCount + 1
                        local a, b, c, d = getJointAnchorPositions(joint)
                        wheelJointResults[name] = {
                            x1 = joint.x1, y1 = joint.y1,
                            x2 = joint.x2, y2 = joint.y2,
                            a = a, b = b, c = c, d = d,
                            wheelX = objects.world[joint.end1].x,
                            wheelY = objects.world[joint.end1].y,
                        }
                    end
                end
            "#,
        )
        .unwrap();

    let environment = game_environment(runtime.lua()).unwrap();
    assert_eq!(environment.get::<i64>("wheelJointCount").unwrap(), 3);
    let results = environment.get::<mlua::Table>("wheelJointResults").unwrap();
    for name in [
        "BLOCK_LIGHT_ROUND_4X4_1_1BLOCK_WOOD_4X4_1_3",
        "BLOCK_LIGHT_ROUND_4X4_1_2BLOCK_WOOD_4X4_1_5",
        "BLOCK_LIGHT_ROUND_4X4_1_3BLOCK_WOOD_4X4_1_4",
    ] {
        let result = results.get::<mlua::Table>(name).unwrap();
        assert_eq!(result.get::<f64>("x1").unwrap(), 0.0, "{name}");
        assert_eq!(result.get::<f64>("y1").unwrap(), 0.0, "{name}");
        let first = (
            result.get::<f64>("a").unwrap(),
            result.get::<f64>("c").unwrap(),
        );
        let second = (
            result.get::<f64>("b").unwrap(),
            result.get::<f64>("d").unwrap(),
        );
        let wheel = (
            result.get::<f64>("wheelX").unwrap(),
            result.get::<f64>("wheelY").unwrap(),
        );
        assert!(
            (first.0 - wheel.0).hypot(first.1 - wheel.1) < 1.0e-6,
            "{name} first anchor {first:?} left wheel center {wheel:?}"
        );
        assert!(
            (first.0 - second.0).hypot(first.1 - second.1) < 0.01,
            "{name} endpoints {first:?} and {second:?} no longer overlap"
        );
    }
}

#[test]
fn revolute_motor_uses_type_three_and_accumulates_max_torque_once_per_step() {
    let runtime = unlocked_test_runtime();
    runtime
        .execute_source(
            r#"
                createBox("anchor", "", 0, 0, 1, 1, 0, 0, 0, true, false, 1)
                createBox("rotor", "", 0, 0, 1, 1, 1, 0, 0, true, false, 1)
                createJoint({
                    name = "motor", end1 = "anchor", end2 = "rotor",
                    type = 3, x1 = 0, y1 = 0, x2 = 0, y2 = 0,
                    motor = true, motorSpeed = 10, maxTorque = 0.1
                })
                setWorldGravity(0, 0)
                update = function() end
                updatePhysics = function() end
                "#,
        )
        .unwrap();

    runtime.update(1.0 / 30.0).unwrap();
    let bridge = runtime.render.lock().unwrap();
    // Unit square mass=1 has inverse inertia 6. The motor's per-step
    // impulse is clamped to dt*maxTorque, even across ten iterations.
    let native_step = 1.0_f32 / 30.0_f32;
    let native_motor_impulse = native_step * 0.1_f32;
    let native_velocity = 6.0_f32 * native_motor_impulse;
    assert_eq!(
        bridge.scene["rotor"].angular_velocity,
        f64::from(native_velocity)
    );
    // b2Island::Solve applies velocity constraints before integrating the
    // transform, so the freshly solved motor velocity rotates this frame.
    assert_eq!(
        bridge.scene["rotor"].angle,
        f64::from(native_step * native_velocity)
    );
    assert_eq!(
        bridge.joints["motor"].motor_impulse,
        f64::from(native_motor_impulse)
    );
    drop(bridge);

    runtime.update(1.0 / 30.0).unwrap();
    // The native unit angular damping first decays the prior velocity. The
    // cached motor impulse is then warm-started on the next island step,
    // adding one more dt*maxTorque contribution without exceeding the
    // per-step accumulator limit.
    let bridge = runtime.render.lock().unwrap();
    let native_angular_drag = (-native_step).mul_add(1.0_f32, 1.0_f32);
    let second_step_velocity = native_velocity * native_angular_drag + native_velocity;
    assert_eq!(
        bridge.scene["rotor"].angular_velocity,
        f64::from(second_step_velocity)
    );
    assert_eq!(
        bridge.scene["rotor"].angle,
        f64::from(native_step * native_velocity + native_step * second_step_velocity)
    );
}

#[test]
fn awake_island_borrows_native_joint_edges_and_wakes_the_complete_sleeping_chain() {
    let runtime = unlocked_test_runtime();
    runtime
        .execute_source(
            r#"
                createCircle("a", "", 0, 0, 0.25, 1, 0, 0, true, false, 1)
                createCircle("b", "", 4, 0, 0.25, 1, 0, 0, true, false, 1)
                createCircle("c", "", 8, 0, 0.25, 1, 0, 0, true, false, 1)
                createJoint({
                    name = "ab", end1 = "a", end2 = "b",
                    type = 3, x1 = 0, y1 = 0, x2 = 4, y2 = 0
                })
                createJoint({
                    name = "bc", end1 = "b", end2 = "c",
                    type = 3, x1 = 4, y1 = 0, x2 = 8, y2 = 0
                })
                "#,
        )
        .unwrap();

    let mut bridge = runtime.render.lock().unwrap();
    let a_order = bridge.scene["a"].physics_creation_order;
    let b_order = bridge.scene["b"].physics_creation_order;
    let c_order = bridge.scene["c"].physics_creation_order;
    let ab_order = bridge.joints["ab"].physics_creation_order;
    let bc_order = bridge.joints["bc"].physics_creation_order;
    assert_eq!(bridge.native_body_joint_edges[&a_order], [ab_order]);
    assert_eq!(
        bridge.native_body_joint_edges[&b_order],
        [ab_order, bc_order]
    );
    assert_eq!(bridge.native_body_joint_edges[&c_order], [bc_order]);
    bridge.scene.get_mut("a").unwrap().velocity_x = 1.0;
    bridge.scene.get_mut("b").unwrap().sleeping = true;
    bridge.scene.get_mut("c").unwrap().sleeping = true;
    bridge.assemble_box2d_islands();

    assert!(!bridge.scene["b"].sleeping);
    assert!(!bridge.scene["c"].sleeping);
    assert_eq!(bridge.solver_islands.len(), 1);
    assert_eq!(bridge.solver_islands[0].bodies, ["a", "b", "c"]);
    assert_eq!(bridge.solver_islands[0].joints, ["ab", "bc"]);
}

#[test]
fn island_joint_pointer_array_is_resolved_once_and_restored_after_all_passes() {
    let runtime = unlocked_test_runtime();
    runtime
        .execute_source(
            r#"
                createCircle("a", "", 0, 0, 0.25, 1, 1, 0, true, false, 1)
                createCircle("b", "", 2, 0, 0.25, 1, 1, 0, true, false, 1)
                createCircle("c", "", 4, 0, 0.25, 1, 1, 0, true, false, 1)
                createJoint({
                    name = "ab", end1 = "a", end2 = "b", type = 1,
                    coordType = 2, x1 = 0, y1 = 0, x2 = 0, y2 = 0
                })
                createJoint({
                    name = "bc", end1 = "b", end2 = "c", type = 1,
                    coordType = 2, x1 = 0, y1 = 0, x2 = 0, y2 = 0
                })
            "#,
        )
        .unwrap();

    let mut bridge = runtime.render.lock().unwrap();
    let ab_name_pointer = bridge.joints["ab"].name.as_ptr();
    let bc_first_pointer = bridge.joints["bc"].first.as_ptr();
    let names = vec!["ab".to_owned(), "bc".to_owned()];
    let mut constraints = bridge.take_island_joint_constraints(&names);
    assert_eq!(constraints.len(), 2);
    assert!(bridge.joints.is_empty());

    bridge.begin_island_joint_constraints(&mut constraints, 1.0 / 30.0);
    for _ in 0..3 {
        bridge.solve_island_joint_constraints(&mut constraints, 1.0 / 30.0, true, false);
        assert_eq!(constraints.len(), 2);
        assert!(bridge.joints.is_empty());
    }
    bridge.restore_island_joint_constraints(constraints);

    assert_eq!(
        bridge.joints.keys().map(String::as_str).collect::<Vec<_>>(),
        ["ab", "bc"]
    );
    assert_eq!(bridge.joints["ab"].name.as_ptr(), ab_name_pointer);
    assert_eq!(bridge.joints["bc"].first.as_ptr(), bc_first_pointer);
}

#[test]
fn cached_joint_with_a_missing_endpoint_uses_the_ordered_native_destructor() {
    let runtime = unlocked_test_runtime();
    runtime
        .execute_source(
            r#"
                createCircle("a", "", 0, 0, 0.25, 1, 1, 0, true, false, 1)
                createCircle("b", "", 2, 0, 0.25, 1, 1, 0, true, false, 1)
                createJoint({
                    name = "link", end1 = "a", end2 = "b", type = 1,
                    coordType = 2, x1 = 0, y1 = 0, x2 = 0, y2 = 0
                })
            "#,
        )
        .unwrap();

    let mut bridge = runtime.render.lock().unwrap();
    bridge.scene.remove("b");
    let mut constraints = bridge.take_island_joint_constraints(&["link".to_owned()]);
    assert!(bridge.solve_island_joint_constraints(&mut constraints, 1.0 / 30.0, true, false,));
    assert_eq!(constraints.len(), 0);
    assert!(!bridge.joints.contains_key("link"));
    assert!(bridge.native_joint_world_order.is_empty());
    bridge.restore_island_joint_constraints(constraints);
}

#[test]
fn recovered_revolute_limit_solver_couples_anchor_and_angular_impulses() {
    let runtime = unlocked_test_runtime();
    runtime
        .execute_source(
            r#"
                createBox("anchor", "", 0, 0, 1, 1, 0, 0, 0, true, false, 1)
                createBox("arm", "", 2, 0, 1, 1, 1, 0, 0, true, false, 1)
                createJoint({
                    name = "limited", end1 = "anchor", end2 = "arm",
                    type = 3, x1 = 0, y1 = 0, x2 = 0, y2 = 0,
                    limit = true, lowerLimit = -0.1, upperLimit = 0.1
                })
                setAngle("arm", 0.3)
                setAngularVelocity("arm", 4)
                "#,
        )
        .unwrap();

    let mut bridge = runtime.render.lock().unwrap();
    bridge.begin_joint_step(1.0 / 60.0);
    assert_eq!(
        bridge.joints["limited"].limit_state,
        JointLimitState::AtUpper
    );
    bridge.solve_joints(1.0 / 60.0, true, false);
    let joint = &bridge.joints["limited"];
    assert!(joint.limit_impulse < 0.0);
    assert!(joint.linear_impulse_x.abs() > 0.0 || joint.linear_impulse_y.abs() > 0.0);
    assert_eq!(
        bridge.scene["arm"].angular_velocity,
        f64::from(20.0_f32 * f32::EPSILON)
    );
}

#[test]
fn recovered_joint_limit_helpers_reverse_or_stop_motor_and_keep_void_abi() {
    let runtime = unlocked_test_runtime();
    runtime
        .execute_source(
            r##"
                createBox("anchor", "", 0, 0, 1, 1, 0, 0, 0, true, false, 1)
                createBox("rotor", "", 0, 0, 1, 1, 1, 0, 0, true, false, 1)
                local descriptor = {
                    name = "boundary", end1 = "anchor", end2 = "rotor",
                    type = 3, x1 = 0, y1 = 0, x2 = 0, y2 = 0,
                    motor = true, motorSpeed = 2, maxTorque = 10,
                    limit = true, lowerLimit = -0.1, upperLimit = 0.1
                }
                local numeric_name_descriptor = {
                    name = "123", end1 = "anchor", end2 = "rotor",
                    type = 3, x1 = 0, y1 = 0, x2 = 0, y2 = 0,
                    motor = true, motorSpeed = 1, maxTorque = 1
                }
                objects.joints = { descriptor, numeric_name_descriptor }
                createJoint(descriptor)
                createJoint(numeric_name_descriptor)
                parameter_result_count = select("#", setJointParameters(
                    { name = "ignored", motorSpeed = 99 },
                    { name = "boundary", motorSpeed = "3", upperLimit = "0.2" }
                ))
                parameter_missing_fails = not pcall(setJointParameters)
                parameter_non_table_top_fails = not pcall(
                    setJointParameters, { name = "boundary" }, false
                )
                setJointParameters({ name = 123, motorSpeed = "4" })
                numeric_name_mirrored_speed = objects.joints[2].motorSpeed
                mirrored_speed = objects.joints[1].motorSpeed
                mirrored_upper = objects.joints[1].upperLimit
                setAngle("rotor", 0.3)
                "##,
        )
        .unwrap();

    {
        let bridge = runtime.render.lock().unwrap();
        assert_eq!(bridge.joints["boundary"].motor_speed, Some(3.0));
        assert_eq!(bridge.joints["123"].motor_speed, Some(4.0));
        assert!(bridge.joints["boundary"].limits_enabled);
        assert_eq!(bridge.joints["boundary"].upper_limit, f64::from(0.2_f32));
        assert_eq!(bridge.scene["rotor"].angle, f64::from(0.3_f32));
    }
    runtime
        .execute_source(r##"check_result_count = select("#", checkJointLimits("boundary"))"##)
        .unwrap();
    assert_eq!(
        runtime.render.lock().unwrap().joints["boundary"].motor_speed,
        Some(-3.0)
    );
    runtime
        .execute_source(
            r##"
                setAngle("anchor", 0.2)
                setAngle("rotor", 0)
                handle_result_count = select("#", handleJointLimits("boundary", true))
                "##,
        )
        .unwrap();

    let environment = game_environment(runtime.lua()).unwrap();
    assert_eq!(environment.get::<i64>("parameter_result_count").unwrap(), 0);
    assert!(environment.get::<bool>("parameter_missing_fails").unwrap());
    assert!(
        environment
            .get::<bool>("parameter_non_table_top_fails")
            .unwrap()
    );
    assert_eq!(environment.get::<i64>("check_result_count").unwrap(), 0);
    assert_eq!(environment.get::<i64>("handle_result_count").unwrap(), 0);
    assert_eq!(environment.get::<f64>("mirrored_speed").unwrap(), 3.0);
    assert_eq!(
        environment
            .get::<f64>("numeric_name_mirrored_speed")
            .unwrap(),
        4.0
    );
    assert_eq!(
        environment.get::<f64>("mirrored_upper").unwrap(),
        f64::from(0.2_f32)
    );
    assert_eq!(
        runtime.render.lock().unwrap().joints["boundary"].motor_speed,
        Some(0.0)
    );
}

#[test]
fn joint_collide_connected_defaults_false_and_can_be_enabled() {
    let runtime = unlocked_test_runtime();
    runtime
        .execute_source(
            r#"
                createCircle("default_a", "", 0, 0, 1, 1, 0, 0, true, false, 1)
                createCircle("default_b", "", 0.5, 0, 1, 1, 0, 0, true, false, 1)
                createJoint({
                    name = "default_joint", end1 = "default_a", end2 = "default_b",
                    type = 3, x1 = 0, y1 = 0, x2 = 0, y2 = 0
                })
                createCircle("enabled_a", "", 10, 0, 1, 1, 0, 0, true, false, 1)
                createCircle("enabled_b", "", 10.5, 0, 1, 1, 0, 0, true, false, 1)
                createJoint({
                    name = "enabled_joint", end1 = "enabled_a", end2 = "enabled_b",
                    type = 3, x1 = 10, y1 = 0, x2 = 10, y2 = 0,
                    collideConnected = true
                })
                setVelocity("default_a", 0.01, 0)
                setVelocity("enabled_a", 0.01, 0)
                "#,
        )
        .unwrap();

    let mut bridge = runtime.render.lock().unwrap();
    let events = bridge.solve_contacts();
    assert!(!bridge.active_contacts.contains_key(&(
        "default_a".to_owned(),
        "default_b".to_owned(),
        0,
        0
    )));
    assert!(bridge.active_contacts.contains_key(&(
        "enabled_a".to_owned(),
        "enabled_b".to_owned(),
        0,
        0
    )));
    assert!(
        events.iter().any(|event| {
            event.first == "enabled_a" && event.second == "enabled_b" && event.began
        })
    );
}

#[test]
fn joint_topology_uses_deferred_native_contact_filter_flag_and_wake_order() {
    let runtime = unlocked_test_runtime();
    runtime
        .execute_source(
            r#"
                createCircle("filter_a", "", 0, 0, 1, 1, 0, 0, true, false, 1)
                createCircle("filter_b", "", 0.5, 0, 1, 1, 0, 0, true, false, 1)
                "#,
        )
        .unwrap();
    let key = ("filter_a".to_owned(), "filter_b".to_owned(), 0, 0);
    {
        let mut bridge = runtime.render.lock().unwrap();
        assert!(bridge.refresh_contacts().iter().any(|event| event.began));
        bridge.scene.get_mut("filter_a").unwrap().sleeping = true;
        bridge.scene.get_mut("filter_b").unwrap().sleeping = true;
    }

    runtime
        .execute_source(
            r#"
                createJoint({
                    name = "deferred_filter", end1 = "filter_a", end2 = "filter_b",
                    type = 3, x1 = 0, y1 = 0, x2 = 0, y2 = 0
                })
                "#,
        )
        .unwrap();
    {
        let mut bridge = runtime.render.lock().unwrap();
        // CreateJoint (`sub_10086E470`) flags the contact but does not
        // wake either endpoint. Collide's awake gate precedes flag use.
        assert!(bridge.scene["filter_a"].sleeping);
        assert!(bridge.scene["filter_b"].sleeping);
        assert!(bridge.contact_filter_dirty.contains(&key));
        assert!(bridge.refresh_contacts().is_empty());
        assert!(bridge.contact_filter_dirty.contains(&key));
        assert!(bridge.active_contacts.contains_key(&key));
    }

    runtime
        .execute_source(r#"destroyJoint("deferred_filter")"#)
        .unwrap();
    {
        let mut bridge = runtime.render.lock().unwrap();
        // DestroyJoint (`sub_10086E27C`) wakes both bodies. The already
        // dirty contact is then rechecked with the joint gone and kept.
        assert!(!bridge.scene["filter_a"].sleeping);
        assert!(!bridge.scene["filter_b"].sleeping);
        assert!(bridge.contact_filter_dirty.contains(&key));
        let events = bridge.refresh_contacts();
        assert!(!events.iter().any(|event| event.ended));
        assert!(!bridge.contact_filter_dirty.contains(&key));
        assert!(bridge.active_contacts.contains_key(&key));
    }

    runtime
        .execute_source(
            r#"
                createJoint({
                    name = "deferred_filter", end1 = "filter_a", end2 = "filter_b",
                    type = 3, x1 = 0, y1 = 0, x2 = 0, y2 = 0
                })
                "#,
        )
        .unwrap();
    let mut bridge = runtime.render.lock().unwrap();
    assert!(bridge.contact_filter_dirty.contains(&key));
    assert!(bridge.refresh_contacts().iter().any(|event| event.ended));
    assert!(!bridge.broad_phase_contacts.contains(&key));
    assert!(!bridge.contact_filter_dirty.contains(&key));
}
