use super::*;

#[test]
fn recovered_type_four_is_prismatic_and_fieldless_type_five_is_metadata_only() {
    let runtime = StellaLua::new("/tmp").unwrap();
    runtime
        .execute_source(
            r#"
                createBox("rail", "", 0, 0, 1, 1, 0, 0, 0, false, false, 1)
                createBox("slider", "", 2, 0, 1, 1, 1, 0, 0, false, false, 1)
                createJoint({
                    name = "prismatic4", end1 = "rail", end2 = "slider", type = 4,
                    coordType = 2, x1 = 0, y1 = 0, x2 = 0, y2 = 0,
                    worldAxisX = 1, worldAxisY = 0
                })
                createJoint({
                    name = "prismatic5", end1 = "rail", end2 = "slider", type = 5,
                    coordType = 2, x1 = 0, y1 = 0, x2 = 0, y2 = 0,
                    worldAxisY = 1, limit = false, motor = false
                })
                "#,
        )
        .unwrap();

    let bridge = runtime.render.lock().unwrap();
    let type_four = &bridge.joints["prismatic4"];
    assert!(type_four.is_physical);
    assert_eq!(type_four.local_axis, (1.0, 0.0));
    assert!(type_four.limits_enabled);
    assert_eq!(type_four.lower_limit, 0.0);
    assert_eq!(type_four.upper_limit, 5.0);
    assert!(type_four.motor_enabled);
    assert_eq!(type_four.max_torque, 10_000.0);
    let type_five = &bridge.joints["prismatic5"];
    assert!(!type_five.is_physical);
    assert_eq!(type_five.local_axis, (0.0, 0.0));
    assert!(!type_five.limits_enabled);
    assert!(!type_five.motor_enabled);
    assert_eq!(type_five.destroy_timer, 1.0);
    assert!(!type_five.one_way_destroy);
    assert_eq!(bridge.native_joint_body_orders.len(), 1);
}

#[test]
fn prismatic_creation_uses_native_float_reference_angle_and_axis_rotation() {
    let runtime = unlocked_test_runtime();
    runtime
        .execute_source(
            r#"
                createBox("rail", "", 0, 0, 1, 1, 0, 0, 0, false, false, 1)
                createBox("slider", "", 2, 0, 1, 1, 1, 0, 0, false, false, 1)
                setRotation("rail", 0.1)
                setRotation("slider", 6.0)
                createJoint({
                    name = "precise", end1 = "rail", end2 = "slider", type = 4,
                    coordType = 2, x1 = 0, y1 = 0, x2 = 0, y2 = 0,
                    worldAxisX = 1.00000006, worldAxisY = 0.20000002,
                    limit = false, motor = false
                })
                createJoint({
                    name = "wrong_types", end1 = "rail", end2 = "slider", type = 4,
                    coordType = 2, x1 = 0, y1 = 0, x2 = 0, y2 = 0,
                    worldAxisX = "1", worldAxisY = true,
                    limit = false, motor = false
                })
                "#,
        )
        .unwrap();

    let bridge = runtime.render.lock().unwrap();
    let joint = &bridge.joints["precise"];
    let first_angle = 0.1_f32;
    let second_angle = 6.0_f32;
    let axis_x = 1.000_000_06_f64 as f32;
    let axis_y = 0.200_000_02_f64 as f32;
    let (sine, cosine) = first_angle.sin_cos();
    let expected_local_axis = (
        f64::from(cosine.mul_add(axis_x, sine * axis_y)),
        f64::from(cosine.mul_add(axis_y, -(sine * axis_x))),
    );
    assert_eq!(joint.local_axis, expected_local_axis);
    assert_eq!(joint.rest_angle, f64::from(second_angle - first_angle));
    assert_ne!(
        joint.rest_angle,
        f64::from(second_angle) - f64::from(first_angle)
    );
    assert_eq!(bridge.joints["wrong_types"].local_axis, (0.0, 0.0));

    let geometry = prismatic_geometry(joint, &bridge.scene["rail"], &bridge.scene["slider"]);
    let local_axis = (expected_local_axis.0 as f32, expected_local_axis.1 as f32);
    let expected_world_axis = (
        f64::from(cosine.mul_add(local_axis.0, -(sine * local_axis.1))),
        f64::from(cosine.mul_add(local_axis.1, sine * local_axis.0)),
    );
    assert_eq!(geometry.axis, expected_world_axis);
    drop(bridge);

    let descriptors = game_environment(runtime.lua())
        .unwrap()
        .get::<mlua::Table>("objects")
        .unwrap()
        .get::<mlua::Table>("joints")
        .unwrap();
    let wrong_types = descriptors.get::<mlua::Table>("wrong_types").unwrap();
    assert_eq!(wrong_types.get::<f64>("worldAxisX").unwrap(), 0.0);
    assert_eq!(wrong_types.get::<f64>("worldAxisY").unwrap(), 0.0);
}

#[test]
fn recovered_prismatic_solver_removes_perpendicular_and_angular_motion() {
    let runtime = StellaLua::new("/tmp").unwrap();
    runtime
        .execute_source(
            r#"
                createBox("rail", "", 0, 0, 1, 1, 0, 0, 0, false, false, 1)
                createBox("slider", "", 2, 0, 1, 1, 1, 0, 0, false, false, 1)
                createJoint({
                    name = "slide", end1 = "rail", end2 = "slider", type = 4,
                    coordType = 2, x1 = 0, y1 = 0, x2 = 0, y2 = 0,
                    worldAxisX = 1, worldAxisY = 0, limit = false, motor = false
                })
                setVelocity("slider", 3, 4)
                setAngularVelocity("slider", 2)
                "#,
        )
        .unwrap();

    let mut bridge = runtime.render.lock().unwrap();
    bridge.begin_joint_step(1.0 / 30.0);
    let joint = &bridge.joints["slide"];
    for value in [
        joint.prismatic_axis.0,
        joint.prismatic_axis.1,
        joint.prismatic_perpendicular.0,
        joint.prismatic_perpendicular.1,
        joint.prismatic_s1,
        joint.prismatic_s2,
        joint.prismatic_a1,
        joint.prismatic_a2,
        joint.prismatic_inverse_mass_first,
        joint.prismatic_inverse_mass_second,
        joint.prismatic_inverse_inertia_first,
        joint.prismatic_inverse_inertia_second,
        joint.prismatic_mass_matrix.0,
        joint.prismatic_mass_matrix.1,
        joint.prismatic_mass_matrix.2,
        joint.prismatic_mass_matrix.3,
        joint.prismatic_mass_matrix.4,
        joint.prismatic_mass_matrix.5,
        joint.prismatic_motor_mass,
    ] {
        assert_eq!(value, f64::from(value as f32));
    }
    for _ in 0..10 {
        bridge.solve_joints(1.0 / 30.0, true, false);
    }
    let slider = &bridge.scene["slider"];
    assert!(slider.velocity_x > 2.9);
    assert!(slider.velocity_y.abs() < 1e-9);
    assert!(slider.angular_velocity.abs() < 1e-9);
}

#[test]
fn prismatic_solver_uses_the_initialization_geometry_and_mass_caches() {
    let runtime = unlocked_test_runtime();
    runtime
        .execute_source(
            r#"
                createBox("rail", "", 0, 0, 1, 1, 0, 0, 0, false, false, 1)
                createBox("slider", "", 2, 0, 1, 1, 1, 0, 0, false, false, 1)
                createJoint({
                    name = "slide", end1 = "rail", end2 = "slider", type = 4,
                    coordType = 2, x1 = 0, y1 = 0, x2 = 0, y2 = 0,
                    worldAxisX = 1, worldAxisY = 0, limit = false, motor = false
                })
            "#,
        )
        .unwrap();

    let mut bridge = runtime.render.lock().unwrap();
    let first = bridge.scene["rail"].clone();
    let second = bridge.scene["slider"].clone();
    let mut joint = bridge.joints["slide"].clone();
    joint.linear_impulse_x = 1.0;
    bridge.scene.get_mut("slider").unwrap().inverse_mass = 0.0;

    // Init consumes the island records captured before the live-body edit and
    // warm-starts through the mass copied into the joint solver record.
    bridge.initialize_prismatic_velocity_constraints(&mut joint, &first, &second, 1.0 / 60.0);
    let cached_mass = joint.prismatic_inverse_mass_second;
    let cached_axis = joint.prismatic_axis;
    let cached_matrix = joint.prismatic_mass_matrix;
    assert!(cached_mass > 0.0);
    assert!(bridge.scene["slider"].velocity_y > 0.0);

    // Velocity iterations retain the initialized axis even if the owning
    // body transform changes during the same island step.
    {
        let rail = bridge.scene.get_mut("rail").unwrap();
        rail.set_native_sweep_transform(
            (rail.sweep_center_x, rail.sweep_center_y),
            std::f32::consts::FRAC_PI_2,
        );
    }
    bridge.scene.get_mut("slider").unwrap().velocity_y = 3.0;
    let first = bridge.scene["rail"].clone();
    let second = bridge.scene["slider"].clone();
    bridge.solve_prismatic_joint_velocity(&mut joint, &first, &second, 1.0 / 60.0);
    assert!(bridge.scene["slider"].velocity_y < 1.0e-6);

    // Position iterations rebuild the live error, but use the same frozen
    // masses and lever arms for the matrix and body write coefficients.
    {
        let rail = bridge.scene.get_mut("rail").unwrap();
        rail.set_native_sweep_transform((rail.sweep_center_x, rail.sweep_center_y), 0.0);
        let slider = bridge.scene.get_mut("slider").unwrap();
        slider.set_native_sweep_transform(
            (slider.sweep_center_x, slider.sweep_center_y + 1.0),
            slider.angle as f32,
        );
    }
    let displaced_y = bridge.scene["slider"].sweep_center_y;
    let first = bridge.scene["rail"].clone();
    let second = bridge.scene["slider"].clone();
    bridge.solve_prismatic_joint_position(&mut joint, &first, &second);
    assert!(bridge.scene["slider"].sweep_center_y < displaced_y);
    assert_eq!(joint.prismatic_inverse_mass_second, cached_mass);
    assert_eq!(joint.prismatic_axis, cached_axis);
    assert_eq!(joint.prismatic_mass_matrix, cached_matrix);
}

#[test]
fn prismatic_motor_keeps_native_float_clamp_for_negative_force_limit() {
    let runtime = unlocked_test_runtime();
    runtime
        .execute_source(
            r#"
                createBox("rail", "", 0, 0, 1, 1, 0, 0, 0, false, false, 1)
                createBox("slider", "", 2, 0, 1, 1, 1, 0, 0, false, false, 1)
                createJoint({
                    name = "slide", end1 = "rail", end2 = "slider", type = 4,
                    coordType = 2, x1 = 0, y1 = 0, x2 = 0, y2 = 0,
                    worldAxisX = 1, worldAxisY = 0,
                    limit = false, motor = true, motorSpeed = 10, maxTorque = -3
                })
                "#,
        )
        .unwrap();

    let mut bridge = runtime.render.lock().unwrap();
    let step = 1.0 / 60.0;
    bridge.begin_joint_step(step);
    bridge.solve_joints(step, true, false);

    let maximum_impulse = step as f32 * -3.0_f32;
    let expected_impulse = 10.0_f32.min(maximum_impulse).max(-maximum_impulse);
    assert_eq!(
        bridge.joints["slide"].motor_impulse,
        f64::from(expected_impulse)
    );
    let inverse_mass = bridge.scene["slider"].inverse_mass_for_solver() as f32;
    assert_eq!(
        bridge.scene["slider"].velocity_x,
        f64::from(inverse_mass * expected_impulse)
    );
}

#[test]
fn prismatic_position_solver_rechecks_live_limit_translation() {
    let runtime = StellaLua::new("/tmp").unwrap();
    runtime
        .execute_source(
            r#"
                createBox("rail", "", 0, 0, 1, 1, 0, 0, 0, false, false, 1)
                createBox("slider", "", 3, 0, 1, 1, 1, 0, 0, false, false, 1)
                createJoint({
                    name = "slide", end1 = "rail", end2 = "slider", type = 4,
                    coordType = 2, x1 = 0, y1 = 0, x2 = 0, y2 = 0,
                    worldAxisX = 1, worldAxisY = 0,
                    limit = true, lowerLimit = 0, upperLimit = 5, motor = false
                })
                "#,
        )
        .unwrap();

    let mut bridge = runtime.render.lock().unwrap();
    bridge.begin_joint_step(1.0 / 30.0);
    assert_ne!(bridge.joints["slide"].limit_state, JointLimitState::AtUpper);
    bridge.scene.get_mut("slider").unwrap().x = 10.0;
    let mut joint = bridge.joints["slide"].clone();
    let first = bridge.scene["rail"].clone();
    let second = bridge.scene["slider"].clone();
    let geometry = prismatic_geometry(&joint, &first, &second);
    let translation = dot_2d(geometry.delta, geometry.axis);
    assert!(
        translation > joint.upper_limit,
        "translation={translation} limits=({}, {}) axis={:?} delta={:?}",
        joint.lower_limit,
        joint.upper_limit,
        geometry.axis,
        geometry.delta
    );
    assert!(!bridge.solve_prismatic_joint_position(&mut joint, &first, &second));
    assert!(bridge.scene["slider"].x < 10.0);
}
