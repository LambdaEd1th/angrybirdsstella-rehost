use super::*;

#[test]
fn renderer_and_object_alpha_bindings_are_distinct_strict_and_float32() {
    let runtime = StellaLua::new("/tmp").unwrap();
    register_test_sprite_sheet(&runtime, &["CONTEXT_ALPHA", "CONTEXT_ALPHA_AFTER_FAILURES"]);
    runtime
        .execute_source(
            r#"
                setRenderState(0, 0, 1, 1, 0, 0, 0, 0.25)
                native_setAlpha(0.69999999)
                res.drawSprite("CONTEXT_ALPHA", 0, 0)
                native_alpha_missing_fails = not pcall(native_setAlpha)
                native_alpha_string_fails = not pcall(native_setAlpha, "0.5")
                res.drawSprite("CONTEXT_ALPHA_AFTER_FAILURES", 0, 0)

                createBox(
                    "alpha_object", "", 0, 0, 1, 1,
                    0, 0, 0, true, false, 1
                )
                objects.world.alpha_object.alpha = 0.75
                setObjectAlpha("alpha_object", 0.49999999)
                changeZOrder("alpha_object", 2.9999999)

                object_alpha_name_fails = not pcall(setObjectAlpha, 3, 0.1)
                object_alpha_value_fails = not pcall(
                    setObjectAlpha, "alpha_object", "0.1"
                )
                missing_object_alpha_ok, missing_object_alpha_error = pcall(
                    setObjectAlpha, "missing", 0.1
                )
                missing_object_alpha_error = tostring(missing_object_alpha_error)
                z_order_name_fails = not pcall(changeZOrder, false, 5)
                z_order_value_fails = not pcall(
                    changeZOrder, "alpha_object", false
                )
                missing_z_order_ok, missing_z_order_error = pcall(
                    changeZOrder, "missing", 5
                )
                missing_z_order_error = tostring(missing_z_order_error)
                mirrored_alpha_after = objects.world.alpha_object.alpha
                mirrored_z_order_after = objects.world.alpha_object.z_order
                "#,
        )
        .unwrap();

    let environment = game_environment(runtime.lua()).unwrap();
    for field in [
        "native_alpha_missing_fails",
        "native_alpha_string_fails",
        "object_alpha_name_fails",
        "object_alpha_value_fails",
        "z_order_name_fails",
        "z_order_value_fails",
    ] {
        assert!(environment.get::<bool>(field).unwrap(), "{field}");
    }
    assert!(!environment.get::<bool>("missing_object_alpha_ok").unwrap());
    assert!(!environment.get::<bool>("missing_z_order_ok").unwrap());
    assert!(
        environment
            .get::<String>("missing_object_alpha_error")
            .unwrap()
            .contains("Missing object: missing")
    );
    assert!(
        environment
            .get::<String>("missing_z_order_error")
            .unwrap()
            .contains("Missing object: missing")
    );

    let commands = runtime.take_render_commands();
    assert_eq!(commands.len(), 2);
    let native_context_alpha = 0.69999999_f64 as f32;
    assert_eq!(commands[0].state.alpha, native_context_alpha);
    assert_eq!(commands[1].state.alpha, native_context_alpha);

    let bridge = runtime.render.lock().unwrap();
    let object = bridge.scene.get("alpha_object").unwrap();
    assert_eq!(object.alpha, f64::from(0.49999999_f64 as f32));
    assert_eq!(object.z_order, f64::from(2.9999999_f64 as f32));
    assert_eq!(bridge.state.alpha, f64::from(native_context_alpha));
    // sub_100044E60 only writes RenderObjectData+0xC8. In contrast,
    // sub_1000592C4 explicitly writes the reflected `z_order` attribute.
    assert_eq!(
        environment.get::<f64>("mirrored_alpha_after").unwrap(),
        0.75
    );
    assert_eq!(
        environment.get::<f64>("mirrored_z_order_after").unwrap(),
        f64::from(2.9999999_f64 as f32)
    );
}

#[test]
fn object_visibility_bindings_use_strict_native_state_without_lua_reflection() {
    let runtime = StellaLua::new("/tmp").unwrap();
    runtime
        .execute_source(
            r#"
                createBox(
                    "visible_object", "", 0, 0, 1, 1,
                    0, 0, 0, true, false, 1
                )
                objects.world.visible_object.visible = true
                setVisible("visible_object", false)
                native_visible_after = isVisible("visible_object")
                mirrored_visible_after = objects.world.visible_object.visible

                set_visible_name_fails = not pcall(setVisible, 7, true)
                set_visible_missing_fails = not pcall(
                    setVisible, "visible_object"
                )
                set_visible_number_fails = not pcall(
                    setVisible, "visible_object", 1
                )
                is_visible_name_fails = not pcall(isVisible, false)
                missing_set_ok, missing_set_error = pcall(
                    setVisible, "missing", true
                )
                missing_set_error = tostring(missing_set_error)
                missing_query_ok, missing_query_error = pcall(
                    isVisible, "missing"
                )
                missing_query_error = tostring(missing_query_error)
                native_visible_after_failures = isVisible("visible_object")
                nul_visible_name = "visible_object" .. string.char(0) .. "ignored"
                setVisible(nul_visible_name, true)
                native_visible_after_nul_name = isVisible("visible_object")
                native_nul_query = isVisible(nul_visible_name)
                non_utf8_nul_suffix_name = nul_visible_name .. string.char(255)
                setVisible(non_utf8_nul_suffix_name, false)
                native_visible_after_non_utf8_nul_suffix = isVisible(
                    non_utf8_nul_suffix_name
                )
                "#,
        )
        .unwrap();

    let environment = game_environment(runtime.lua()).unwrap();
    for field in [
        "set_visible_name_fails",
        "set_visible_missing_fails",
        "set_visible_number_fails",
        "is_visible_name_fails",
    ] {
        assert!(environment.get::<bool>(field).unwrap(), "{field}");
    }
    assert!(!environment.get::<bool>("missing_set_ok").unwrap());
    assert!(!environment.get::<bool>("missing_query_ok").unwrap());
    assert!(
        environment
            .get::<String>("missing_set_error")
            .unwrap()
            .contains("Missing object: missing")
    );
    assert!(
        environment
            .get::<String>("missing_query_error")
            .unwrap()
            .contains("Missing object: missing")
    );
    assert!(!environment.get::<bool>("native_visible_after").unwrap());
    assert!(
        !environment
            .get::<bool>("native_visible_after_failures")
            .unwrap()
    );
    assert!(environment.get::<bool>("mirrored_visible_after").unwrap());
    assert!(
        environment
            .get::<bool>("native_visible_after_nul_name")
            .unwrap()
    );
    assert!(environment.get::<bool>("native_nul_query").unwrap());
    assert!(
        !environment
            .get::<bool>("native_visible_after_non_utf8_nul_suffix")
            .unwrap()
    );
    assert!(
        !runtime
            .render
            .lock()
            .unwrap()
            .scene
            .get("visible_object")
            .unwrap()
            .visible
    );
}

#[test]
fn object_queries_distinguish_render_objects_from_optional_native_bodies() {
    let runtime = StellaLua::new("/tmp").unwrap();
    runtime
        .execute_source(
            r#"
                createBox(
                    "query_body", "", 0, 0, 1, 1,
                    1, 0, 0, true, false, 1
                )
                createNonPhysicsObject("query_visual", "", 0, 0, 2)
                setScale("query_body", 0.99999999, 1.9999999)
                setAngle("query_body", 0.69999999)
                setObjectParameter("query_body", 8, 1)
                setVelocity("query_body", 0.99999999, 2.9999999)
                setAngularVelocity("query_body", 0.69999999)
                setPosition("query_body", 4.0000001, 6.9999999)

                -- Native getters do not consult these Lua mirror fields.
                objects.world.query_body.scaleX = -123
                objects.world.query_body.scaleY = -456
                objects.world.query_body.angle = 5.5

                query_scale_x, query_scale_y = getScale("query_body")
                query_angle = getAngle("query_body")
                query_rotation = getRotation("query_body")
                query_flipped = isHorizontallyFlipped("query_body")
                query_velocity = getVelocity("query_body")
                query_velocity_x, query_velocity_y = getLinearVelocity("query_body")
                query_angular_velocity = getAngularVelocity("query_body")
                query_body_sleeping = isSleeping("query_body")
                query_world_x, query_world_y = getWorldPoint(
                    "query_body", 1.9999999, 2.9999999
                )
                query_local_x, query_local_y = getLocalPoint(
                    "query_body", query_world_x, query_world_y
                )

                visual_velocity = getVelocity("query_visual")
                visual_velocity_x, visual_velocity_y = getLinearVelocity(
                    "query_visual"
                )
                visual_angular_velocity = getAngularVelocity("query_visual")
                visual_sleeping = isSleeping("query_visual")
                missing_velocity = getVelocity("missing")
                missing_velocity_x, missing_velocity_y = getLinearVelocity(
                    "missing"
                )
                missing_angular_velocity = getAngularVelocity("missing")
                missing_sleeping = isSleeping("missing")

                missing_scale_ok, missing_scale_error = pcall(
                    getScale, "missing"
                )
                missing_scale_error = tostring(missing_scale_error)
                missing_angle_ok, missing_angle_error = pcall(
                    getAngle, "missing"
                )
                missing_angle_error = tostring(missing_angle_error)
                missing_flip_ok, missing_flip_error = pcall(
                    isHorizontallyFlipped, "missing"
                )
                missing_flip_error = tostring(missing_flip_error)

                scale_type_fails = not pcall(getScale, 1)
                flip_type_fails = not pcall(isHorizontallyFlipped, false)
                angle_type_fails = not pcall(getAngle, {})
                rotation_type_fails = not pcall(getRotation, nil)
                velocity_type_fails = not pcall(getVelocity, 1)
                linear_velocity_type_fails = not pcall(
                    getLinearVelocity, false
                )
                angular_velocity_type_fails = not pcall(
                    getAngularVelocity, {}
                )
                sleeping_type_fails = not pcall(isSleeping, 1)
                world_point_name_type_fails = not pcall(
                    getWorldPoint, 1, 2, 3
                )
                world_point_x_type_fails = not pcall(
                    getWorldPoint, "query_body", false, 3
                )
                local_point_y_type_fails = not pcall(
                    getLocalPoint, "query_body", 2, {}
                )
                missing_world_point_ok, missing_world_point_error = pcall(
                    getWorldPoint, "missing", 2, 3
                )
                missing_world_point_error = tostring(missing_world_point_error)
                visual_local_point_ok, visual_local_point_error = pcall(
                    getLocalPoint, "query_visual", 2, 3
                )
                visual_local_point_error = tostring(visual_local_point_error)
                -- Both direct members read numeric slots before dereferencing
                -- the nullable body returned by sub_100061AE4.
                missing_bad_world_ok, missing_bad_world_error = pcall(
                    getWorldPoint, "missing", false, 3
                )
                missing_bad_world_error = tostring(missing_bad_world_error)
                "#,
        )
        .unwrap();

    let environment = game_environment(runtime.lua()).unwrap();
    assert!(matches!(
        environment.raw_get::<Value>("getPosition").unwrap(),
        Value::Nil
    ));
    for field in [
        "scale_type_fails",
        "flip_type_fails",
        "angle_type_fails",
        "rotation_type_fails",
        "velocity_type_fails",
        "linear_velocity_type_fails",
        "angular_velocity_type_fails",
        "sleeping_type_fails",
        "world_point_name_type_fails",
        "world_point_x_type_fails",
        "local_point_y_type_fails",
    ] {
        assert!(environment.get::<bool>(field).unwrap(), "{field}");
    }
    for (ok_field, error_field) in [
        ("missing_scale_ok", "missing_scale_error"),
        ("missing_angle_ok", "missing_angle_error"),
        ("missing_flip_ok", "missing_flip_error"),
    ] {
        assert!(!environment.get::<bool>(ok_field).unwrap(), "{ok_field}");
        assert!(
            environment
                .get::<String>(error_field)
                .unwrap()
                .contains("Missing object: missing"),
            "{error_field}"
        );
    }

    assert_eq!(
        environment.get::<f64>("query_scale_x").unwrap(),
        f64::from(0.99999999_f64 as f32)
    );
    assert_eq!(
        environment.get::<f64>("query_scale_y").unwrap(),
        f64::from(1.9999999_f64 as f32)
    );
    let expected_angle = f64::from(0.69999999_f64 as f32);
    assert_eq!(
        environment.get::<f64>("query_angle").unwrap(),
        expected_angle
    );
    assert_eq!(
        environment.get::<f64>("query_rotation").unwrap(),
        expected_angle
    );
    assert!(environment.get::<bool>("query_flipped").unwrap());

    let velocity_x = 0.99999999_f64 as f32;
    let velocity_y = 2.9999999_f64 as f32;
    let expected_velocity = velocity_x
        .mul_add(velocity_x, velocity_y * velocity_y)
        .sqrt();
    assert_eq!(
        environment.get::<f64>("query_velocity").unwrap(),
        f64::from(expected_velocity)
    );
    assert_eq!(
        environment.get::<f64>("query_velocity_x").unwrap(),
        f64::from(velocity_x)
    );
    assert_eq!(
        environment.get::<f64>("query_velocity_y").unwrap(),
        f64::from(velocity_y)
    );
    assert_eq!(
        environment.get::<f64>("query_angular_velocity").unwrap(),
        expected_angle
    );
    assert!(!environment.get::<bool>("query_body_sleeping").unwrap());

    for field in [
        "visual_velocity",
        "visual_velocity_x",
        "visual_velocity_y",
        "visual_angular_velocity",
        "missing_velocity",
        "missing_velocity_x",
        "missing_velocity_y",
        "missing_angular_velocity",
    ] {
        assert_eq!(environment.get::<f64>(field).unwrap(), 0.0, "{field}");
    }
    assert!(environment.get::<bool>("visual_sleeping").unwrap());
    assert!(environment.get::<bool>("missing_sleeping").unwrap());

    for (ok_field, error_field, missing_name) in [
        (
            "missing_world_point_ok",
            "missing_world_point_error",
            "missing",
        ),
        (
            "visual_local_point_ok",
            "visual_local_point_error",
            "query_visual",
        ),
    ] {
        assert!(!environment.get::<bool>(ok_field).unwrap(), "{ok_field}");
        assert!(
            environment
                .get::<String>(error_field)
                .unwrap()
                .contains(&format!("Missing physics body: {missing_name}")),
            "{error_field}"
        );
    }
    assert!(!environment.get::<bool>("missing_bad_world_ok").unwrap());
    assert!(
        environment
            .get::<String>("missing_bad_world_error")
            .unwrap()
            .contains("bad argument #2 to 'getWorldPoint' (number expected)")
    );

    let position_x = 4.0000001_f64 as f32;
    let position_y = 6.9999999_f64 as f32;
    let angle = 0.69999999_f64 as f32;
    let point_x = 1.9999999_f64 as f32;
    let point_y = 2.9999999_f64 as f32;
    let (sine, cosine) = angle.sin_cos();
    let expected_world_x = position_x + point_x.mul_add(cosine, -(point_y * sine));
    let expected_world_y = position_y + point_y.mul_add(cosine, point_x * sine);
    assert_eq!(
        environment.get::<f64>("query_world_x").unwrap(),
        f64::from(expected_world_x)
    );
    assert_eq!(
        environment.get::<f64>("query_world_y").unwrap(),
        f64::from(expected_world_y)
    );
    let delta_x = expected_world_x - position_x;
    let delta_y = expected_world_y - position_y;
    let expected_local_x = delta_x.mul_add(cosine, delta_y * sine);
    let expected_local_y = cosine.mul_add(delta_y, -(delta_x * sine));
    assert_eq!(
        environment.get::<f64>("query_local_x").unwrap(),
        f64::from(expected_local_x)
    );
    assert_eq!(
        environment.get::<f64>("query_local_y").unwrap(),
        f64::from(expected_local_y)
    );
}

#[test]
fn object_transform_and_velocity_setters_match_native_adapter_and_member_order() {
    let runtime = StellaLua::new("/tmp").unwrap();
    runtime
        .execute_source(
            r#"
                createBox(
                    "setter_body", "", 0, 0, 1, 1,
                    1, 0, 0, true, false, 1
                )
                setPosition("setter_body", 0.99999999, 1.9999999)
                setScale("setter_body", 2.9999999, 3.9999999)
                setAngle("setter_body", -0.69999999)
                setVelocity("setter_body", 4.9999999, 5.9999999)
                setAngularVelocity("setter_body", 0.79999999)

                set_position_type_fails = not pcall(
                    setPosition, 1, 7, 8
                )
                set_position_y_fails = not pcall(
                    setPosition, "setter_body", 7, "bad"
                )
                set_scale_y_fails = not pcall(
                    setScale, "setter_body", 7, "bad"
                )
                set_angle_value_fails = not pcall(
                    setAngle, "setter_body", false
                )
                set_velocity_y_fails = not pcall(
                    setVelocity, "setter_body", 7, false
                )
                set_angular_value_fails = not pcall(
                    setAngularVelocity, "setter_body", {}
                )

                setter_x = objects.world.setter_body.x
                setter_y = objects.world.setter_body.y
                setter_scale_x, setter_scale_y = getScale("setter_body")
                setter_mirror_scale_x = objects.world.setter_body.scaleX
                setter_mirror_scale_y = objects.world.setter_body.scaleY
                setter_angle = getAngle("setter_body")
                setter_mirror_angle = objects.world.setter_body.angle
                setter_velocity_x, setter_velocity_y = getLinearVelocity(
                    "setter_body"
                )
                setter_angular_velocity = getAngularVelocity("setter_body")

                objects.world.transform_missing = { x = 0, y = 0, angle = 0 }
                missing_position_ok = pcall(
                    setPosition, "transform_missing", 9, 10
                )
                missing_angle_ok = pcall(
                    setAngle, "transform_missing", 1.25
                )
                missing_position_x = objects.world.transform_missing.x
                missing_position_y = objects.world.transform_missing.y
                missing_angle_value = objects.world.transform_missing.angle

                objects.world.scale_missing = { scaleX = 0, scaleY = 0 }
                missing_scale_ok = pcall(
                    setScale, "scale_missing", 11.9999999, 12.9999999
                )
                missing_scale_x = objects.world.scale_missing.scaleX
                missing_scale_y = objects.world.scale_missing.scaleY

                missing_velocity_ok = pcall(
                    setVelocity, "missing", 1, 2
                )
                missing_angular_ok = pcall(
                    setAngularVelocity, "missing", 3
                )

                createBox(
                    "nan_body", "", 0, 0, 1, 1,
                    1, 0, 0, true, false, 1
                )
                setSleeping("nan_body", true)
                setVelocity("nan_body", 0 / 0, 0)
                setAngularVelocity("nan_body", 0 / 0)
                nan_body_sleeping = isSleeping("nan_body")
                nan_velocity_x, nan_velocity_y = getLinearVelocity("nan_body")
                nan_angular_velocity = getAngularVelocity("nan_body")
                "#,
        )
        .unwrap();

    let environment = game_environment(runtime.lua()).unwrap();
    for field in [
        "set_position_type_fails",
        "set_position_y_fails",
        "set_scale_y_fails",
        "set_angle_value_fails",
        "set_velocity_y_fails",
        "set_angular_value_fails",
    ] {
        assert!(environment.get::<bool>(field).unwrap(), "{field}");
    }

    let expected_x = f64::from(0.99999999_f64 as f32);
    let expected_y = f64::from(1.9999999_f64 as f32);
    assert_eq!(environment.get::<f64>("setter_x").unwrap(), expected_x);
    assert_eq!(environment.get::<f64>("setter_y").unwrap(), expected_y);
    let expected_scale_x = f64::from(2.9999999_f64 as f32);
    let expected_scale_y = f64::from(3.9999999_f64 as f32);
    for (field, expected) in [
        ("setter_scale_x", expected_scale_x),
        ("setter_mirror_scale_x", expected_scale_x),
        ("setter_scale_y", expected_scale_y),
        ("setter_mirror_scale_y", expected_scale_y),
    ] {
        assert_eq!(environment.get::<f64>(field).unwrap(), expected, "{field}");
    }

    let tau = std::f32::consts::PI + std::f32::consts::PI;
    let mut expected_angle = (-0.69999999_f64 as f32) % tau;
    if expected_angle < 0.0 {
        expected_angle += tau;
    }
    for field in ["setter_angle", "setter_mirror_angle"] {
        assert_eq!(
            environment.get::<f64>(field).unwrap(),
            f64::from(expected_angle),
            "{field}"
        );
    }
    assert_eq!(
        environment.get::<f64>("setter_velocity_x").unwrap(),
        f64::from(4.9999999_f64 as f32)
    );
    assert_eq!(
        environment.get::<f64>("setter_velocity_y").unwrap(),
        f64::from(5.9999999_f64 as f32)
    );
    assert_eq!(
        environment.get::<f64>("setter_angular_velocity").unwrap(),
        f64::from(0.79999999_f64 as f32)
    );

    assert!(!environment.get::<bool>("missing_position_ok").unwrap());
    assert!(!environment.get::<bool>("missing_angle_ok").unwrap());
    assert_eq!(environment.get::<f64>("missing_position_x").unwrap(), 0.0);
    assert_eq!(environment.get::<f64>("missing_position_y").unwrap(), 0.0);
    assert_eq!(environment.get::<f64>("missing_angle_value").unwrap(), 0.0);
    assert!(!environment.get::<bool>("missing_scale_ok").unwrap());
    assert_eq!(
        environment.get::<f64>("missing_scale_x").unwrap(),
        f64::from(11.9999999_f64 as f32)
    );
    assert_eq!(
        environment.get::<f64>("missing_scale_y").unwrap(),
        f64::from(12.9999999_f64 as f32)
    );
    assert!(environment.get::<bool>("missing_velocity_ok").unwrap());
    assert!(environment.get::<bool>("missing_angular_ok").unwrap());

    assert!(environment.get::<bool>("nan_body_sleeping").unwrap());
    let nan_velocity_x = environment.get::<f64>("nan_velocity_x").unwrap();
    let nan_angular_velocity = environment.get::<f64>("nan_angular_velocity").unwrap();
    assert!(nan_velocity_x.is_nan());
    assert_eq!(environment.get::<f64>("nan_velocity_y").unwrap(), 0.0);
    assert!(nan_angular_velocity.is_nan());
}

#[test]
fn sprite_and_physics_scale_preserve_native_lookup_and_reflection_order() {
    let runtime = StellaLua::new("/tmp").unwrap();
    runtime
        .execute_source(
            r#"
                createBox("body", "OLD_SPRITE", 0, 0, 2, 2, 1, 0.2, 0.3,
                    true, false, 1)
                native_setSprite("body", "NEW_SPRITE")
                body_lua_sprite = objects.world.body.sprite
                sprite_short_fails = not pcall(native_setSprite, "body")
                sprite_type_fails = not pcall(native_setSprite, "body", 4)
                objects.world.sprite_only = { sprite = "LUA_SPRITE" }
                sprite_unknown_fails = not pcall(
                    native_setSprite, "sprite_only", "NEW"
                )
                sprite_unknown_value = objects.world.sprite_only.sprite

                physics_scale_short_fails = not pcall(
                    setPhysicsScale, "body", 2
                )
                physics_scale_type_fails = not pcall(
                    setPhysicsScale, "body", 2, "3"
                )
                objects.world.scale_only = {
                    scaleX = 1, scaleY = 1, width = 2, height = 2,
                    density = 1, friction = 0.2, restitution = 0.3
                }
                physics_scale_unknown_fails = not pcall(
                    setPhysicsScale, "scale_only", 4, 5
                )
                scale_only_x = objects.world.scale_only.scaleX
                scale_only_y = objects.world.scale_only.scaleY

                createNonPhysicsObject("visual", "", 0, 0, 1)
                visual_scale_ok = pcall(setPhysicsScale, "visual", 2, 3)

                createBox("bad_coeff", "", 0, 0, 2, 2, 1, 0.2, 0.3,
                    true, false, 1)
                objects.world.bad_coeff.density = nil
                bad_coeff_fails = not pcall(
                    setPhysicsScale, "bad_coeff", 2, 3
                )
                bad_coeff_scale_x = objects.world.bad_coeff.scaleX
                bad_coeff_scale_y = objects.world.bad_coeff.scaleY
                bad_coeff_width = objects.world.bad_coeff.width
                bad_coeff_height = objects.world.bad_coeff.height
                "#,
        )
        .unwrap();

    let environment = game_environment(runtime.lua()).unwrap();
    for field in [
        "sprite_short_fails",
        "sprite_type_fails",
        "sprite_unknown_fails",
        "physics_scale_short_fails",
        "physics_scale_type_fails",
        "physics_scale_unknown_fails",
        "visual_scale_ok",
        "bad_coeff_fails",
    ] {
        assert!(environment.get::<bool>(field).unwrap(), "{field}");
    }
    assert_eq!(
        environment.get::<String>("body_lua_sprite").unwrap(),
        "OLD_SPRITE"
    );
    assert_eq!(
        environment.get::<String>("sprite_unknown_value").unwrap(),
        "LUA_SPRITE"
    );
    assert_eq!(environment.get::<f64>("scale_only_x").unwrap(), 1.0);
    assert_eq!(environment.get::<f64>("scale_only_y").unwrap(), 1.0);
    assert_eq!(environment.get::<f64>("bad_coeff_scale_x").unwrap(), 2.0);
    assert_eq!(environment.get::<f64>("bad_coeff_scale_y").unwrap(), 3.0);
    assert_eq!(environment.get::<f64>("bad_coeff_width").unwrap(), 4.0);
    assert_eq!(environment.get::<f64>("bad_coeff_height").unwrap(), 6.0);

    let bridge = runtime.render.lock().unwrap();
    assert_eq!(bridge.scene["body"].sprite.as_ref(), "NEW_SPRITE");
    assert_eq!(
        (
            bridge.scene["visual"].scale_x,
            bridge.scene["visual"].scale_y
        ),
        (2.0, 3.0)
    );
    assert_eq!(
        (
            bridge.scene["visual"].base_scale_x,
            bridge.scene["visual"].base_scale_y
        ),
        (2.0, 3.0)
    );
    let bad_coeff = &bridge.scene["bad_coeff"];
    assert_eq!((bad_coeff.scale_x, bad_coeff.scale_y), (2.0, 3.0));
    assert_eq!((bad_coeff.base_scale_x, bad_coeff.base_scale_y), (2.0, 3.0));
    assert_eq!(
        (bad_coeff.native_shape_width, bad_coeff.native_shape_height),
        (4.0, 6.0)
    );
    // Coefficient failure happens before DestroyFixture/recreation.
    assert_eq!(
        (bad_coeff.physics_scale_x, bad_coeff.physics_scale_y),
        (1.0, 1.0)
    );
    assert_eq!(bad_coeff.fixture_densities, vec![1.0]);
}

#[test]
fn poppy_drill_sprite_changes_are_discrete_native_resource_rebindings() {
    let runtime = StellaLua::new("/tmp").unwrap();
    let unique = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let file_name = format!("stella-poppy-tween-{unique}.dat");
    let path = runtime.data_root().join(&file_name);
    fs::write(
        &path,
        test_textured_sprite_sheet_with_names(
            "poppy-tween.pvr",
            &[
                ("REGULAR", 8, 12),
                ("POPPY_POWER_1", 10, 20),
                ("POPPY_POWER_2", 30, 40),
            ],
        ),
    )
    .unwrap();
    let environment = game_environment(runtime.lua()).unwrap();
    let resources = environment.get::<mlua::Table>("res").unwrap();
    resources
        .get::<Function>("createSpriteSheet")
        .unwrap()
        .call::<()>((file_name.as_str(), true))
        .unwrap();
    fs::remove_file(path).unwrap();

    runtime
        .execute_source(
            r#"
                createNonPhysicsObject("poppy", "REGULAR", 0, 0, 1)
                native_setSprite("poppy", "POPPY_POWER_1")
                drawGameNative()
            "#,
        )
        .unwrap();
    let first = runtime.take_render_commands();
    assert_eq!(first.len(), 1);
    assert_eq!(first[0].state.draw_size, None);
    assert_eq!(first[0].state.sprite_pivot, None);

    runtime
        .render
        .lock()
        .unwrap()
        .advance_native_scene_frame(1.0 / 60.0);
    runtime
        .execute_source(
            r#"
                drawGameNative()
            "#,
        )
        .unwrap();
    let repeated = runtime.take_render_commands();
    assert_eq!(repeated.len(), 1);
    assert_eq!(repeated[0].state.draw_size, None);
    assert_eq!(repeated[0].state.sprite_pivot, None);

    runtime
        .execute_source(
            r#"
                native_setSprite("poppy", "POPPY_POWER_2")
                drawGameNative()
            "#,
        )
        .unwrap();
    let second = runtime.take_render_commands();
    assert_eq!(second.len(), 1);
    assert_eq!(second[0].sprite, "POPPY_POWER_2");
    assert_eq!(second[0].state.draw_size, None);
    assert_eq!(second[0].state.sprite_pivot, None);

    runtime
        .execute_source(
            r#"
                native_setSprite("poppy", "REGULAR")
                native_setSprite("poppy", "REGULAR")
                drawGameNative()
            "#,
        )
        .unwrap();
    let regular = runtime.take_render_commands();
    assert_eq!(regular.len(), 1);
    assert_eq!(regular[0].state.draw_size, None);
    assert_eq!(regular[0].state.sprite_pivot, None);
}

#[test]
fn visual_scale_member_requires_world_table_before_render_store() {
    let runtime = StellaLua::new("/tmp").unwrap();
    runtime
        .execute_source(
            r#"
                createBox("body", "", 0, 0, 2, 2, 1, 0, 0,
                    true, false, 1)
                objects.world.body = nil
                scale_without_world_ok = pcall(setScale, "body", 2, 3)
                "#,
        )
        .unwrap();

    assert!(
        !game_environment(runtime.lua())
            .unwrap()
            .get::<bool>("scale_without_world_ok")
            .unwrap()
    );
    let bridge = runtime.render.lock().unwrap();
    assert_eq!(
        (bridge.scene["body"].scale_x, bridge.scene["body"].scale_y),
        (1.0, 1.0)
    );
}
