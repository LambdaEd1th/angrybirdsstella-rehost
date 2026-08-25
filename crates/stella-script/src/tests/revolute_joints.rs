use super::*;

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
    // The cached motor impulse is warm-started on the next island step,
    // adding one more dt*maxTorque contribution without exceeding the
    // per-step accumulator limit.
    let bridge = runtime.render.lock().unwrap();
    let second_step_velocity = native_velocity + native_velocity;
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
