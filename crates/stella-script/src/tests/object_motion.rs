use super::*;

#[test]
fn object_parameter_switch_preserves_native_fields_and_body_type_side_effects() {
    let runtime = unlocked_test_runtime();
    runtime
        .execute_source(
            r#"
                createBox("body", "BODY", 0, 0, 2, 3, 1, 0, 0, true, false, 1)
                setObjectParameter("body", 1, 1)
                setObjectParameter("body", 6, 0.25)
                setObjectParameter("body", 7, 0.5)
                setObjectParameter("body", 9, 1)
                setObjectParameter("body", 11, 1)
                setObjectParameter("body", 12, 1)
                setObjectParameter("body", 15, 1)
                setObjectParameter("body", 16, 1)
                setObjectParameter("body", 20, 1)
                setObjectParameter("body", 21, 7)
                setObjectParameter("body", 22, 1)
                setAsSensor("body", true)
                setObjectParameter("body", 24, 2)
                setObjectParameter("body", 25, 0.1)
                setObjectParameter("body", 26, 0.9)
                setObjectParameter("body", 28, 4)
                setObjectParameter("body", 29, 1.25)
                setObjectParameter("body", 31, 3.5)
                setObjectParameter("body", 32, 1)
                setObjectParameter("body", 32, 0)
                setObjectParameter("body", 34, 1)
                setObjectParameter("body", 35, 5)
                setObjectParameter("body", 36, -3)
                setObjectParameter("body", 38, 12)
                setObjectParameter("body", 37, 1)

                createCircle("sensor_circle", "CIRCLE", 0, 0, 1, 1, 0, 0, true, false, 1)
                setObjectParameter("sensor_circle", 27, 3)
                setObjectParameter("sensor_circle", 33, 0)
                setObjectParameter("sensor_circle", 22, 1)
                setAsSensor("sensor_circle", true)
                "#,
        )
        .unwrap();

    {
        let bridge = runtime.render.lock().unwrap();
        let body = &bridge.scene["body"];
        assert!(body.level_goal);
        assert_eq!(body.bounce_amplitude_multiplier, 0.25);
        assert_eq!(body.bounce_frequency_multiplier, 0.5);
        assert!(body.graphics_flip_enabled);
        assert!(body.not_collided);
        assert!(body.ignore_motion);
        assert!(body.disable_immovable_collisions);
        assert_eq!(body.gravity_scale, 0.0);
        assert!(body.sensor_definition);
        assert!(body.sensor_active);
        assert!(body.sensor);
        assert!(body.fixed_rotation);
        assert!(body.bullet);
        assert_eq!(body.sensor_type, 7);
        assert_eq!(body.sensor_shape_type, 2);
        assert_eq!(body.sensor_minimum_force, f64::from(0.1_f32));
        assert_eq!(body.sensor_maximum_force, f64::from(0.9_f32));
        assert_eq!(body.sensor_width, 4.0);
        assert_eq!(body.sensor_height, 5.0);
        assert_eq!(body.sensor_force_angle, 1.25);
        assert_eq!(body.time_since_collision, 3.5);
        assert!(body.aiming_aid_collideable);
        assert!(body.bubble);
        assert_eq!(body.collision_group, -3);
        // Sensor type 7 first fixes rotation. The later SetType(kinematic)
        // resets mass data again, leaving no dynamic inertia override.
        assert_eq!(body.moment_of_inertia, None);
        assert!(!body.dynamic_body);
        assert!(body.kinematic_body);
        let sensor_circle = &bridge.scene["sensor_circle"];
        assert!(!sensor_circle.visible);
        assert!(sensor_circle.sensor);
        assert!(!sensor_circle.sensor_active);
        assert_eq!(sensor_circle.sensor_radius, 3.0);
        assert!(matches!(
            sensor_circle.collision_shape,
            CollisionShape::Circle { radius: 3.0 }
        ));
    }

    runtime
        .execute_source(
            r#"
                setObjectParameter("body", 39, 0)
                setObjectParameter("body", 39, 2)
                "#,
        )
        .unwrap();
    let bridge = runtime.render.lock().unwrap();
    let body = &bridge.scene["body"];
    assert!(body.dynamic_body);
    assert!(!body.kinematic_body);
    assert_eq!(body.inverse_mass, f64::from(6.0_f32.recip()));
    assert_eq!(body.inverse_inertia(), 0.0);
}

#[test]
fn body_type_changes_flag_every_attached_contact_for_native_refiltering() {
    let runtime = unlocked_test_runtime();
    runtime
        .execute_source(
            r#"
                createBox("a", "", 0, 0, 2, 2, 1, 0, 0, true, false, 1)
                createBox("b", "", 0, 0, 2, 2, 1, 0, 0, true, false, 1)
            "#,
        )
        .unwrap();
    let key = ("a".to_owned(), "b".to_owned(), 0, 0);
    {
        let mut bridge = runtime.render.lock().unwrap();
        assert!(bridge.refresh_contacts().iter().any(|event| event.began));
        assert!(!bridge.contact_filter_dirty.contains(&key));
    }

    runtime
        .execute_source(r#"setObjectParameter("a", 39, 1)"#)
        .unwrap();
    {
        let mut bridge = runtime.render.lock().unwrap();
        // b2Body::SetType (`sub_10086B0CC`) finishes by walking the body's
        // contact-edge list and setting e_filterFlag on every contact.
        assert!(bridge.contact_filter_dirty.contains(&key));
        assert!(!bridge.refresh_contacts().iter().any(|event| event.ended));
        assert!(!bridge.contact_filter_dirty.contains(&key));
    }

    runtime
        .execute_source(r#"setObjectParameter("a", 39, 1)"#)
        .unwrap();
    assert!(
        !runtime
            .render
            .lock()
            .unwrap()
            .contact_filter_dirty
            .contains(&key)
    );
}

#[test]
fn sensor_parameter_wakes_contact_neighbor_and_only_reactivates_body() {
    let runtime = unlocked_test_runtime();
    runtime
        .execute_source(
            r#"
                createBox("sensor_owner", "", 0, 0, 2, 2, 1, 0, 0, true, false, 1)
                createBox("neighbor", "", 0, 0, 2, 2, 1, 0, 0, true, false, 1)
                setObjectParameter("sensor_owner", 20, 1)
                "#,
        )
        .unwrap();

    {
        let mut bridge = runtime.render.lock().unwrap();
        assert!(bridge.refresh_contacts().iter().any(|event| event.began));
        bridge
            .scene
            .get_mut("neighbor")
            .expect("contact neighbor")
            .sleeping = true;
    }

    runtime
        .execute_source(r#"setObjectParameter("sensor_owner", 22, 1)"#)
        .unwrap();
    {
        let bridge = runtime.render.lock().unwrap();
        assert!(bridge.scene["sensor_owner"].sensor_active);
        assert!(bridge.scene["sensor_owner"].active);
        assert!(!bridge.scene["neighbor"].sleeping);
        assert_eq!(bridge.scene["neighbor"].sleep_time, 0.0);
    }

    runtime
        .execute_source(
            r#"
                setObjectParameter("sensor_owner", 22, 0)
                setActive("sensor_owner", false)
                setObjectParameter("sensor_owner", 22, 0)
                "#,
        )
        .unwrap();
    let bridge = runtime.render.lock().unwrap();
    assert!(!bridge.scene["sensor_owner"].sensor_active);
    // Parameter 22 always calls SetActive(true), even when disabling the
    // sensor byte; it is not the body-active switch.
    assert!(bridge.scene["sensor_owner"].active);
}

#[test]
fn collision_bounce_uses_native_impulse_decay_phase_and_float32_scale() {
    let runtime = unlocked_test_runtime();
    runtime
        .execute_source(
            r#"
                setGameParameters({ gameWorldScale = 0.08 })
                createBox("a", "A", 0, 0, 1, 1, 1, 0, 0, true, false, 1)
                createBox("b", "B", 0, 0, 1, 1, 1, 0, 0, true, false, 1)
                setScale("b", 2, 3)
                setObjectParameter("b", 6, 0.03)
                setObjectParameter("b", 7, 0.5)
                "#,
        )
        .unwrap();

    let mut bridge = runtime.render.lock().unwrap();
    bridge
        .scene
        .get_mut("b")
        .unwrap()
        .trigger_native_bounce(5.0);
    bridge.update_scene_bounce(0.1);

    let elapsed = 0.1_f32;
    let initial = (5.0_f32 * 0.02_f32).min(0.1_f32);
    let amplitude = 0.03_f32 * (-initial).mul_add(elapsed, initial);
    let frequency = amplitude.mul_add(100.0, 0.5_f32 * 5.0);
    let phase = frequency.mul_add(elapsed, std::f32::consts::PI * 0.5);
    let body = &bridge.scene["b"];
    assert!(body.bounce_active);
    assert_eq!(body.bounce_elapsed, f64::from(elapsed));
    assert_eq!(body.bounce_current_amplitude, f64::from(amplitude));
    assert_eq!(body.scale_x, f64::from(amplitude.mul_add(phase.sin(), 2.0)));
    assert_eq!(
        body.scale_y,
        f64::from(amplitude.mul_add((std::f32::consts::PI + phase).sin(), 3.0))
    );
    assert_eq!((body.base_scale_x, body.base_scale_y), (2.0, 3.0));
    assert_eq!((body.physics_scale_x, body.physics_scale_y), (1.0, 1.0));

    bridge.update_scene_bounce(1.0);
    let body = &bridge.scene["b"];
    assert!(!body.bounce_active);
    assert_eq!(body.bounce_elapsed, 0.0);
    assert_eq!(body.bounce_current_amplitude, 0.0);
    assert_eq!(body.bounce_initial_amplitude, 0.0);
}

#[test]
fn native_motion_globals_and_collision_timer_follow_frame_pass() {
    let runtime = unlocked_test_runtime();
    runtime
        .execute_source(
            r#"
                createBox("block", "BLOCK", 0, 0, 2, 2, 1, 0, 0, true, false, 1)
                createBox("bird", "BIRD", 0, 0, 2, 2, 1, 0, 0, true, true, 1)
                setVelocity("block", 4, 0)
                setObjectParameter("bird", 31, 2)
                "#,
        )
        .unwrap();

    runtime.update(0.01).unwrap();
    let environment = game_environment(&runtime.lua).unwrap();
    assert!(environment.get::<bool>("hasMovingObjects").unwrap());
    assert!(environment.get::<bool>("hasAwakeObjects").unwrap());
    assert!(
        environment
            .get::<bool>("hasMovingObjectsZeroTolerance")
            .unwrap()
    );
    assert_eq!(
        runtime.render.lock().unwrap().scene["bird"].time_since_collision,
        f64::from(2.0_f32 + 0.01_f32)
    );

    runtime
        .execute_source(
            r#"
                setObjectParameter("block", 12, 1)
                setObjectParameter("bird", 12, 1)
                "#,
        )
        .unwrap();
    runtime.update(0.01).unwrap();
    assert!(!environment.get::<bool>("hasMovingObjects").unwrap());
    assert!(environment.get::<bool>("hasAwakeObjects").unwrap());
    assert!(
        !environment
            .get::<bool>("hasMovingObjectsZeroTolerance")
            .unwrap()
    );

    runtime
        .execute_source(
            r#"
                setObjectParameter("block", 39, 0)
                setObjectParameter("bird", 39, 0)
                "#,
        )
        .unwrap();
    runtime.update(0.01).unwrap();
    assert!(!environment.get::<bool>("hasMovingObjects").unwrap());
    assert!(!environment.get::<bool>("hasAwakeObjects").unwrap());
    assert!(
        !environment
            .get::<bool>("hasMovingObjectsZeroTolerance")
            .unwrap()
    );
}

#[test]
fn second_physics_lock_branch_preserves_scene_globals_and_motion_timers() {
    let runtime = unlocked_test_runtime();
    runtime
        .execute_source(
            r#"
                setLevelLimits(-5, -5, 5, 5)
                createBox("bird", "BIRD", 0, 0, 2, 2, 1, 0, 0, true, true, 1)
                setObjectParameter("bird", 31, 2)
                update = function() end
            "#,
        )
        .unwrap();
    runtime.update(0.01).unwrap();
    let timer = runtime.render.lock().unwrap().scene["bird"].time_since_collision;

    runtime
        .execute_source(
            r#"
                hasMovingObjects = "moving-sentinel"
                hasAwakeObjects = "awake-sentinel"
                hasMovingObjectsZeroTolerance = "zero-sentinel"
                g_outOfBoundariesObjects = { sentinel = true }
                setPhysicsEnabled(false, "transition")
            "#,
        )
        .unwrap();
    runtime.update(0.5).unwrap();

    let environment = game_environment(runtime.lua()).unwrap();
    assert_eq!(
        environment.get::<String>("hasMovingObjects").unwrap(),
        "moving-sentinel"
    );
    assert_eq!(
        environment.get::<String>("hasAwakeObjects").unwrap(),
        "awake-sentinel"
    );
    assert_eq!(
        environment
            .get::<String>("hasMovingObjectsZeroTolerance")
            .unwrap(),
        "zero-sentinel"
    );
    assert!(
        environment
            .get::<mlua::Table>("g_outOfBoundariesObjects")
            .unwrap()
            .get::<bool>("sentinel")
            .unwrap()
    );
    assert_eq!(
        runtime.render.lock().unwrap().scene["bird"].time_since_collision,
        timer
    );
}

#[test]
fn not_collided_only_suppresses_two_controllable_object_bounce() {
    let runtime = unlocked_test_runtime();
    runtime
        .execute_source(
            r#"
                createBox("a", "A", 0, 0, 1, 1, 1, 0, 0, true, true, 1)
                createBox("b", "B", 0, 0, 1, 1, 1, 0, 0, true, true, 1)
                setObjectParameter("a", 6, 0.03)
                setObjectParameter("b", 6, 0.03)
                setObjectParameter("a", 11, 1)
                "#,
        )
        .unwrap();
    let event = ContactEvent {
        first: "a".to_owned(),
        second: "b".to_owned(),
        first_fixture: 0,
        second_fixture: 0,
        sensor: false,
        began: true,
        ended: false,
        impulse: 5.0,
        normal_x: 1.0,
        normal_y: 0.0,
        point_x: 0.0,
        point_y: 0.0,
        first_mass: 1.0,
        first_velocity_x: 0.0,
        first_velocity_y: 0.0,
        second_mass: 1.0,
        second_velocity_x: 0.0,
        second_velocity_y: 0.0,
    };
    let mut bridge = runtime.render.lock().unwrap();
    bridge.trigger_native_contact_bounce(&event, event.impulse);
    assert!(!bridge.scene["a"].bounce_active);
    assert!(bridge.scene["b"].bounce_active);

    bridge.scene.get_mut("b").unwrap().controllable = false;
    bridge.scene.get_mut("b").unwrap().bounce_active = false;
    bridge.trigger_native_contact_bounce(&event, event.impulse);
    assert!(bridge.scene["a"].bounce_active);
    assert!(bridge.scene["b"].bounce_active);
}

#[test]
fn direct_world_removal_does_not_destroy_native_scene_owner() {
    let runtime = unlocked_test_runtime();
    runtime
        .execute_source(
            r#"
                createNonPhysicsObject("old", "OLD_SPRITE", 1, 2, 3)
                createNonPhysicsObject("live", "LIVE_SPRITE", 4, 5, 6)
                objects.world.old = nil
                "#,
        )
        .unwrap();
    let bridge = runtime.render.lock().unwrap();
    // Purple's draw and physics paths retain RenderObjectData independently
    // of its Lua mirror. Only removeObject or level teardown destroys it.
    assert!(bridge.scene.contains_key("old"));
    assert!(bridge.scene.contains_key("live"));
}
