use super::*;

#[test]
fn recovered_type_six_uses_rope_defaults_and_maximum_length_constraint() {
    let runtime = StellaLua::new("/tmp").unwrap();
    runtime
        .execute_source(
            r#"
                createBox("anchor", "", 0, 0, 1, 1, 0, 0, 0, false, false, 1)
                createBox("payload", "", 8, 0, 1, 1, 1, 0, 0, false, false, 1)
                createJoint({
                    name = "rope_default", end1 = "anchor", end2 = "payload",
                    type = 6, coordType = 0
                })
                createJoint({
                    name = "rope", end1 = "anchor", end2 = "payload",
                    type = 6, coordType = 2, x1 = 0, y1 = 0, x2 = 0, y2 = 0,
                    maxLength = 4
                })
                setVelocity("payload", 5, 0)
                "#,
        )
        .unwrap();

    let mut bridge = runtime.render.lock().unwrap();
    let native_default = &bridge.joints["rope_default"];
    assert!(native_default.is_physical);
    assert_eq!(native_default.first_anchor, (-1.0, 0.0));
    assert_eq!(native_default.second_anchor, (1.0, 0.0));
    assert_eq!(native_default.rest_length, 8.0);
    bridge.begin_joint_step(1.0 / 30.0);
    for _ in 0..30 {
        bridge.solve_joints(1.0 / 30.0, true, true);
    }
    let anchor = &bridge.scene["anchor"];
    let payload = &bridge.scene["payload"];
    assert!((payload.x - anchor.x).hypot(payload.y - anchor.y) <= 4.001);
    assert!(payload.velocity_x <= 1e-9);
}

#[test]
fn rope_joint_clears_warm_impulse_below_native_direction_threshold() {
    let runtime = StellaLua::new("/tmp").unwrap();
    runtime
        .execute_source(
            r#"
                createBox("anchor", "", 0, 0, 1, 1, 0, 0, 0, false, false, 1)
                createBox("payload", "", 1, 0, 1, 1, 1, 0, 0, false, false, 1)
                createJoint({
                    name = "rope", end1 = "anchor", end2 = "payload", type = 6,
                    coordType = 2, x1 = 0, y1 = 0, x2 = 0, y2 = 0, maxLength = 1
                })
                "#,
        )
        .unwrap();

    let mut bridge = runtime.render.lock().unwrap();
    bridge.scene.get_mut("payload").unwrap().x = 0.0005;
    bridge.joints.get_mut("rope").unwrap().distance_impulse = -3.0;
    bridge.begin_joint_step(1.0 / 30.0);
    assert_eq!(bridge.joints["rope"].distance_impulse, 0.0);
    assert_eq!(bridge.scene["payload"].velocity_x, 0.0);
}
