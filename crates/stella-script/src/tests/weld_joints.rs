use super::*;

#[test]
fn recovered_weld_matrix_couples_off_center_linear_and_angular_velocity() {
    let runtime = unlocked_test_runtime();
    runtime
        .execute_source(
            r#"
                createBox("anchor", "", 0, 0, 1, 1, 0, 0, 0, true, false, 1)
                createBox("body", "", 2, 0, 1, 1, 1, 0, 0, true, false, 1)
                createJoint({
                    name = "weld", end1 = "anchor", end2 = "body",
                    type = 2, x1 = 1, y1 = 0, x2 = -1, y2 = 0
                })
                setVelocity("body", 0, 3)
                "#,
        )
        .unwrap();

    let mut bridge = runtime.render.lock().unwrap();
    bridge.begin_joint_step(1.0 / 30.0);
    {
        let joint = &bridge.joints["weld"];
        for value in [
            joint.weld_radius_first.0,
            joint.weld_radius_first.1,
            joint.weld_radius_second.0,
            joint.weld_radius_second.1,
            joint.weld_inverse_mass_first,
            joint.weld_inverse_mass_second,
            joint.weld_inverse_inertia_first,
            joint.weld_inverse_inertia_second,
            joint.weld_mass_matrix.0,
            joint.weld_mass_matrix.1,
            joint.weld_mass_matrix.2,
            joint.weld_mass_matrix.3,
            joint.weld_mass_matrix.4,
            joint.weld_mass_matrix.5,
        ] {
            assert_eq!(value, f64::from(value as f32));
        }
    }
    for _ in 0..10 {
        bridge.solve_joints(1.0 / 30.0, true, false);
    }
    let body = &bridge.scene["body"];
    assert!(body.velocity_x.abs() < 1e-9);
    assert!(body.velocity_y.abs() < 1e-9);
    assert!(body.angular_velocity.abs() < 1e-9);
    let joint = &bridge.joints["weld"];
    assert!(joint.linear_impulse_y.abs() > 0.0);
    assert!(joint.angular_impulse.abs() > 0.0);
}

#[test]
fn weld_solver_uses_the_initialization_mass_cache_for_velocity_and_position_writes() {
    let runtime = unlocked_test_runtime();
    runtime
        .execute_source(
            r#"
                createBox("anchor", "", 0, 0, 1, 1, 0, 0, 0, true, false, 1)
                createBox("body", "", 2, 0, 1, 1, 1, 0, 0, true, false, 1)
                createJoint({
                    name = "weld", end1 = "anchor", end2 = "body",
                    type = 2, x1 = 1, y1 = 0, x2 = -1, y2 = 0
                })
            "#,
        )
        .unwrap();

    let mut bridge = runtime.render.lock().unwrap();
    bridge.begin_joint_step(1.0 / 60.0);
    let cached_mass = bridge.joints["weld"].weld_inverse_mass_second;
    assert!(cached_mass > 0.0);
    {
        let body = bridge.scene.get_mut("body").unwrap();
        body.velocity_y = 3.0;
        // b2Island copied this coefficient into the joint solver record.
        // A later live-body mutation cannot alter the current island step.
        body.inverse_mass = 0.0;
    }
    bridge.solve_joints(1.0 / 60.0, true, false);
    assert!(bridge.scene["body"].velocity_y.abs() < 1.0e-6);

    {
        let body = bridge.scene.get_mut("body").unwrap();
        body.set_native_sweep_transform(
            (body.sweep_center_x + 1.0, body.sweep_center_y),
            body.angle as f32,
        );
    }
    let displaced_x = bridge.scene["body"].sweep_center_x;
    bridge.solve_joints(1.0 / 60.0, false, true);
    assert!(bridge.scene["body"].sweep_center_x < displaced_x);
    assert_eq!(bridge.joints["weld"].weld_inverse_mass_second, cached_mass);
}

#[test]
fn weld_position_solver_keeps_unwrapped_box2d_angle_error() {
    let runtime = unlocked_test_runtime();
    runtime
        .execute_source(
            r#"
                createBox("anchor", "", 0, 0, 1, 1, 0, 0, 0, false, false, 1)
                createBox("body", "", 0, 0, 1, 1, 1, 0, 0, false, false, 1)
                createJoint({
                    name = "weld", end1 = "anchor", end2 = "body", type = 2,
                    coordType = 2, x1 = 0, y1 = 0, x2 = 0, y2 = 0
                })
                "#,
        )
        .unwrap();

    let mut bridge = runtime.render.lock().unwrap();
    bridge.begin_joint_step(1.0 / 60.0);
    bridge.scene.get_mut("body").unwrap().angle = std::f64::consts::TAU + 0.01;
    let joint = bridge.joints["weld"].clone();
    let first = bridge.scene["anchor"].clone();
    let second = bridge.scene["body"].clone();
    // Convergence is based on the pre-correction unwrapped 2π+0.01
    // error, not on its modulo-2π equivalent.
    assert!(!bridge.solve_weld_joint_position(&joint, &first, &second));
    assert!(bridge.scene["body"].angle.abs() < 1.0e-6);
}

#[test]
fn recovered_breakable_joint_uses_collision_force_and_compacts_lua_metadata() {
    let runtime = unlocked_test_runtime();
    runtime
        .execute_source(
            r#"
                createCircle("anchor", "", 0, 0, 1, 1, 0, 0, true, false, 1)
                createCircle("payload", "", 3, 0, 1, 1, 1, 0, true, false, 1)
                createCircle("collider", "", 4.5, 0, 1, 1, 1, 0, true, false, 1)
                worldAttributes = { forceDamageMultiplier = 2250 }
                local descriptor = {
                    name = "fragile", end1 = "anchor", end2 = "payload",
                    type = 2, x1 = 0, y1 = 0, x2 = 0, y2 = 0,
                    breakable = true, breakForce = 0.01
                }
                objects.joints = { descriptor }
                createJoint(descriptor)
                local publishedDescriptor = objects.joints.fragile
                assert(publishedDescriptor ~= descriptor)
                jointRemovalCallbacks = {}
                lua_onBeforeJointRemove = function(name)
                    assert(objects.joints[name] == publishedDescriptor)
                    table.insert(jointRemovalCallbacks, "before:" .. name)
                    publishedDescriptor.isDrawn = true
                end
                lua_addParticlesToJoint = function(name)
                    assert(objects.joints[name] == publishedDescriptor)
                    table.insert(jointRemovalCallbacks, "particles:" .. name)
                end
                setVelocity("collider", -5, 0)
                setWorldGravity(0, 0)
                update = function() end
                updatePhysics = function() end
                "#,
        )
        .unwrap();

    // A displaced constraint alone does not trigger RemovePredicate;
    // sub_10006510C receives collision strength from blockCollision.
    {
        let mut bridge = runtime.render.lock().unwrap();
        bridge.scene.get_mut("payload").unwrap().x = 30.0;
        bridge.solve_joints(1.0 / 30.0, false, true);
        assert!(bridge.joints.contains_key("fragile"));
        bridge.scene.get_mut("payload").unwrap().x = 3.0;
    }

    runtime.update(1.0 / 30.0).unwrap();
    assert!(
        !runtime
            .render
            .lock()
            .unwrap()
            .joints
            .contains_key("fragile")
    );
    let environment = game_environment(runtime.lua()).unwrap();
    let objects: mlua::Table = environment.get("objects").unwrap();
    let joints: mlua::Table = objects.get("joints").unwrap();
    assert_eq!(joints.raw_len(), 0);
    assert!(matches!(
        joints.raw_get::<Value>("fragile").unwrap(),
        Value::Nil
    ));
    assert_eq!(
        environment
            .get::<mlua::Table>("jointRemovalCallbacks")
            .unwrap()
            .sequence_values::<String>()
            .collect::<Result<Vec<_>, _>>()
            .unwrap(),
        ["before:fragile", "particles:fragile"]
    );
}

#[test]
fn breakable_joints_keep_native_constraints_until_reverse_frame_tail_drain() {
    let runtime = unlocked_test_runtime();
    runtime
        .execute_source(
            r#"
                createCircle("anchor", "", 0, 0, 1, 1, 0, 0, true, false, 1)
                createCircle("payload", "", 3, 0, 1, 1, 1, 0, true, false, 1)
                createJoint({
                    name = "z_first", end1 = "anchor", end2 = "payload",
                    type = 2, x1 = 0, y1 = 0, x2 = 0, y2 = 0,
                    breakable = true, breakForce = 1
                })
                createJoint({
                    name = "a_second", end1 = "anchor", end2 = "payload",
                    type = 2, x1 = 0, y1 = 0, x2 = 0, y2 = 0,
                    breakable = true, breakForce = 1
                })
                "#,
        )
        .unwrap();

    let mut bridge = runtime.render.lock().unwrap();
    bridge.scene.get_mut("anchor").unwrap().sleeping = true;
    bridge.scene.get_mut("payload").unwrap().sleeping = true;

    // RemovePredicate scans the insertion-order jointData vector, queues both
    // records, and makes them logically invisible without touching b2World.
    assert_eq!(
        bridge.break_joints_attached_to("payload", 2.0),
        ["z_first", "a_second"]
    );
    assert_eq!(
        bridge.pending_native_joint_destructions,
        ["z_first", "a_second"]
    );
    assert!(bridge.joints.contains_key("z_first"));
    assert!(bridge.joints.contains_key("a_second"));
    assert_eq!(
        bridge
            .native_joint_world_order
            .values()
            .rev()
            .map(String::as_str)
            .collect::<Vec<_>>(),
        ["a_second", "z_first"]
    );
    assert!(bridge.attached_joint_names("payload").is_empty());
    assert!(bridge.native_joint_endpoint_exports().is_empty());
    assert!(bridge.scene["anchor"].sleeping);
    assert!(bridge.scene["payload"].sleeping);

    // GameLua::update walks the pending vector backwards and only now runs
    // the Box2D destruction side effects, including waking both endpoints.
    assert_eq!(
        bridge.drain_pending_native_joint_destructions(),
        ["a_second", "z_first"]
    );
    assert!(bridge.joints.is_empty());
    assert!(bridge.native_joint_world_order.is_empty());
    assert!(bridge.native_joint_body_orders.is_empty());
    assert!(bridge.native_body_joint_edges.values().all(Vec::is_empty));
    assert!(bridge.pending_native_joint_destructions.is_empty());
    assert!(!bridge.scene["anchor"].sleeping);
    assert!(!bridge.scene["payload"].sleeping);
}

#[test]
fn begin_contact_damage_uses_pre_island_velocities() {
    let runtime = unlocked_test_runtime();
    runtime
        .execute_source(
            r#"
                createCircle("anchor", "", -3, 0, 1, 1, 1, 0, true, false, 1)
                createCircle("payload", "", 0, 0, 1, 1, 1, 0, true, false, 1)
                createCircle("collider", "", 1.5, 0, 1, 1, 0.05, 0, true, false, 1)
                worldAttributes = { forceDamageMultiplier = 2250 }
                local descriptor = {
                    name = "fragile", end1 = "anchor", end2 = "payload",
                    type = 2, x1 = 0, y1 = 0, x2 = 0, y2 = 0,
                    breakable = true, breakForce = 0.01
                }
                objects.joints = { descriptor }
                createJoint(descriptor)
                setWorldGravity(0, 2)
                update = function() end
                updatePhysics = function() end
                "#,
        )
        .unwrap();

    runtime.update(1.0 / 30.0).unwrap();
    let bridge = runtime.render.lock().unwrap();
    assert!(
        bridge.joints.contains_key("fragile"),
        "the first gravity integration must not leak into BeginContact force"
    );
    assert!(bridge.scene["payload"].velocity_y > 0.0);
    assert!(bridge.scene["collider"].velocity_y > 0.0);
}
