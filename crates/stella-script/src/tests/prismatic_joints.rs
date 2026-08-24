use super::*;

#[test]
fn recovered_type_four_and_fieldless_type_five_create_prismatic_joints() {
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
    assert!(type_five.is_physical);
    assert_eq!(type_five.local_axis, (0.0, 1.0));
    assert!(!type_five.limits_enabled);
    assert!(!type_five.motor_enabled);
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
    for _ in 0..10 {
        bridge.solve_joints(1.0 / 30.0, true, false);
    }
    let slider = &bridge.scene["slider"];
    assert!(slider.velocity_x > 2.9);
    assert!(slider.velocity_y.abs() < 1e-9);
    assert!(slider.angular_velocity.abs() < 1e-9);
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
    let joint = bridge.joints["slide"].clone();
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
    assert!(!bridge.solve_prismatic_joint_position(&joint, &first, &second));
    assert!(bridge.scene["slider"].x < 10.0);
}
