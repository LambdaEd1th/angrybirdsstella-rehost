use super::*;

#[test]
fn aiming_aid_force_source_registration_preserves_native_order_and_duplicates() {
    let runtime = unlocked_test_runtime();
    runtime
        .execute_source(
            r#"
                createCircle("z_sensor", "", 0, 0, 1, 1, 0, 0,
                    true, false, 1)
                createCircle("a_sensor", "", 0, 0, 1, 1, 0, 0,
                    true, false, 1)
                createNonPhysicsObject("visual", "", 0, 0, 1)

                setObjectParameter("z_sensor", 32, 1)
                setObjectParameter("a_sensor", 32, 1)
                setObjectParameter("z_sensor", 32, 1)
                setObjectParameter("z_sensor", 32, 0)
                setObjectParameter("a_sensor", 32, 2)
                setObjectParameter("visual", 32, 1)
                "#,
        )
        .unwrap();

    let mut bridge = runtime.render.lock().unwrap();
    assert_eq!(
        bridge
            .aiming_aid_force_sources
            .iter()
            .map(|source| source.name.as_str())
            .collect::<Vec<_>>(),
        vec!["z_sensor", "a_sensor", "z_sensor"]
    );
    assert_eq!(
        bridge.aiming_aid_force_sources[0].physics_creation_order,
        bridge.aiming_aid_force_sources[2].physics_creation_order
    );
    assert!(bridge.scene["z_sensor"].aiming_aid_collideable);
    assert!(bridge.scene["a_sensor"].aiming_aid_collideable);
    assert!(!bridge.scene["visual"].aiming_aid_collideable);

    bridge.clear_native_level_scene();
    assert!(bridge.aiming_aid_force_sources.is_empty());
}

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
fn static_to_dynamic_body_type_change_requeues_fixture_proxies() {
    let runtime = unlocked_test_runtime();
    runtime
        .execute_source(
            r#"
                createBox("static_a", "", 0, 0, 2, 2, 0, 0, 0, true, false, 1)
                createBox("static_b", "", 0, 0, 2, 2, 0, 0, 0, true, false, 1)
                "#,
        )
        .unwrap();

    {
        let mut bridge = runtime.render.lock().unwrap();
        // Two static bodies can share fat-AABB space but never create a
        // contact because UpdatePairs requires at least one dynamic body.
        assert!(bridge.refresh_contacts().is_empty());
        assert!(bridge.broad_phase_contacts.is_empty());
    }

    runtime
        .execute_source(r#"setObjectParameter("static_a", 39, 2)"#)
        .unwrap();
    let mut bridge = runtime.render.lock().unwrap();
    // Native SetType -> b2Fixture::Refilter queues every fixture proxy in the
    // move buffer. The transition therefore discovers the overlapping static_b
    // fixture on the very next Collide pass.
    let events = bridge.refresh_contacts();
    assert!(events.iter().any(|event| {
        event.began
            && ((event.first == "static_a" && event.second == "static_b")
                || (event.first == "static_b" && event.second == "static_a"))
    }));
    assert!(bridge.broad_phase_contacts.contains(&(
        "static_a".to_owned(),
        "static_b".to_owned(),
        0,
        0
    )));
}

#[test]
fn body_type_change_only_resets_sleep_time_when_native_body_was_asleep() {
    let runtime = unlocked_test_runtime();
    runtime
        .execute_source(
            r#"
                createBox("body", "", 0, 0, 2, 2, 1, 0, 0, true, false, 1)
                "#,
        )
        .unwrap();

    {
        let mut bridge = runtime.render.lock().unwrap();
        let body = bridge.scene.get_mut("body").unwrap();
        // Box2D's awake flag and sleepTime are independent fields. Preserve
        // an awake body's accumulated timer across SetType.
        body.sleeping = false;
        body.sleep_time = 0.375;
    }
    runtime
        .execute_source(r#"setObjectParameter("body", 39, 1)"#)
        .unwrap();
    {
        let bridge = runtime.render.lock().unwrap();
        let body = &bridge.scene["body"];
        assert!(body.kinematic_body);
        assert!(!body.sleeping);
        assert_eq!(body.sleep_time, 0.375);
    }

    {
        let mut bridge = runtime.render.lock().unwrap();
        let body = bridge.scene.get_mut("body").unwrap();
        body.sleeping = true;
        body.sleep_time = 0.625;
    }
    runtime
        .execute_source(r#"setObjectParameter("body", 39, 2)"#)
        .unwrap();
    {
        let bridge = runtime.render.lock().unwrap();
        let body = &bridge.scene["body"];
        assert!(body.dynamic_body);
        assert!(!body.sleeping);
        assert_eq!(body.sleep_time, 0.0);
    }
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
fn graphics_auto_flip_uses_native_velocity_sign_threshold_and_body_state_gate() {
    let runtime = unlocked_test_runtime();
    runtime
        .execute_source(
            r#"
                createBox("dynamic", "", 0, 0, 2, 2, 1, 0, 0, true, false, 1)
                createBox("static", "", 0, 0, 2, 2, 0, 0, 0, true, false, 1)
                createBox("kinematic", "", 0, 0, 2, 2, 1, 0, 0, true, false, 1)
                createNonPhysicsObject("visual", "", 0, 0, 1)
                setObjectParameter("dynamic", 9, 1)
                setObjectParameter("static", 9, 1)
                setObjectParameter("kinematic", 9, 1)
                setObjectParameter("kinematic", 39, 1)
                setObjectParameter("visual", 9, 1)
                "#,
        )
        .unwrap();

    let mut bridge = runtime.render.lock().unwrap();

    {
        let body = bridge.scene.get_mut("dynamic").unwrap();
        body.velocity_x = -4.0;
        body.horizontal_flip = true;
    }
    bridge.advance_native_scene_frame(0.0);
    assert!(!bridge.scene["dynamic"].horizontal_flip);

    // Both exact threshold values retain the preceding byte.
    {
        let body = bridge.scene.get_mut("dynamic").unwrap();
        body.velocity_x = 3.0;
        body.horizontal_flip = false;
    }
    bridge.advance_native_scene_frame(0.0);
    assert!(!bridge.scene["dynamic"].horizontal_flip);
    {
        let body = bridge.scene.get_mut("dynamic").unwrap();
        body.velocity_x = -3.0;
        body.horizontal_flip = true;
    }
    bridge.advance_native_scene_frame(0.0);
    assert!(bridge.scene["dynamic"].horizontal_flip);

    {
        let body = bridge.scene.get_mut("dynamic").unwrap();
        body.velocity_x = 4.0;
        body.horizontal_flip = false;
    }
    bridge.advance_native_scene_frame(0.0);
    assert!(bridge.scene["dynamic"].horizontal_flip);

    // A fully sleeping body is skipped, while the one transition frame from
    // awake to sleeping still consumes the cached previous-awake byte.
    {
        let body = bridge.scene.get_mut("dynamic").unwrap();
        body.velocity_x = -4.0;
        body.horizontal_flip = true;
        body.sleeping = true;
        body.native_was_awake = false;
    }
    bridge.advance_native_scene_frame(0.0);
    assert!(bridge.scene["dynamic"].horizontal_flip);
    {
        let body = bridge.scene.get_mut("dynamic").unwrap();
        body.horizontal_flip = true;
        body.native_was_awake = true;
    }
    bridge.advance_native_scene_frame(0.0);
    assert!(!bridge.scene["dynamic"].horizontal_flip);

    // Native requires a dynamic-mass or kinematic body. Static bodies and
    // non-physics render objects keep their authored horizontal flip.
    {
        let body = bridge.scene.get_mut("static").unwrap();
        body.velocity_x = -4.0;
        body.horizontal_flip = true;
        let kinematic = bridge.scene.get_mut("kinematic").unwrap();
        kinematic.velocity_x = 4.0;
        kinematic.horizontal_flip = false;
        let visual = bridge.scene.get_mut("visual").unwrap();
        visual.velocity_x = -4.0;
        visual.horizontal_flip = true;
    }
    bridge.advance_native_scene_frame(0.0);
    assert!(bridge.scene["static"].horizontal_flip);
    assert!(bridge.scene["kinematic"].horizontal_flip);
    assert!(bridge.scene["visual"].horizontal_flip);
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
