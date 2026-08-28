use super::*;

#[test]
fn decoration_draw_submission_retains_the_resolved_atlas_across_shadow_and_release() {
    let unique = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let root = std::env::temp_dir().join(format!("stella-decoration-pointer-{unique}"));
    let data_root = root.join("data");
    for directory in ["first", "second"] {
        fs::create_dir_all(data_root.join(directory)).unwrap();
    }
    fs::write(
        data_root.join("first/FIRST.dat"),
        test_textured_sprite_sheet("DECOR", "first.pvr", 10, 20),
    )
    .unwrap();
    fs::write(data_root.join("first/first.pvr"), []).unwrap();
    fs::write(
        data_root.join("second/SECOND.dat"),
        test_textured_sprite_sheet("DECOR", "second.pvr", 30, 40),
    )
    .unwrap();
    fs::write(data_root.join("second/second.pvr"), []).unwrap();

    let runtime = StellaLua::new(&data_root).unwrap();
    runtime
        .execute_source(
            r#"
                res.createSpriteSheet("first/FIRST.dat")
                blocks = { decorated = { decorations = { objects = {
                    amount=1, sprite="DECOR", angleIncrement=0, scale=1
                } } } }
                createNonPhysicsObject("body", "", 0, 0, 1)
                objects.world.body.definition = "decorated"
                setDecorationObjects("body")
                drawGameNative()
                res.createSpriteSheet("second/SECOND.dat")
                res.releaseSpriteSheet("first/FIRST.dat", false)
            "#,
        )
        .unwrap();

    assert_eq!(
        runtime.sprite_catalog_snapshot_since(0).unwrap().regions["DECOR"]
            .sprite
            .width,
        30
    );
    let bridge = runtime.render.lock().unwrap();
    assert_eq!(bridge.commands.len(), 1);
    let retained = bridge.commands[0].bound_region.as_ref().unwrap();
    assert_eq!(retained.sprite.width, 10);
    assert!(retained.texture_source.ends_with("first/first.pvr"));
    drop(bridge);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn native_track_projects_body_motion_onto_recovered_polyline() {
    let runtime = unlocked_test_runtime();
    runtime
        .execute_source(
            r#"
                createBox("cart", "", 2, 7, 2, 2, 1, 0, 0, true, false, 1)
                createTrack({
                    points = { { x = 0, y = 5 }, { x = 10, y = 5 } },
                    blocks = { "cart" },
                    openEnded = true,
                    rotateBlock = true
                })
                track_missing_fails = not pcall(createTrack)
                track_first_argument_fails = not pcall(createTrack, false, {
                    points = { { x = 0, y = 0 }, { x = 1, y = 0 } },
                    blocks = { "cart" }, openEnded = true, rotateBlock = false
                })
                track_points_fails = not pcall(createTrack, {
                    points = false, blocks = { "cart" },
                    openEnded = true, rotateBlock = false
                })
                track_point_fails = not pcall(createTrack, {
                    points = { false }, blocks = { "cart" },
                    openEnded = true, rotateBlock = false
                })
                createBox("coerced_coordinates", "", 0, 0, 1, 1,
                    1, 0, 0, true, false, 1)
                track_coordinates_coerce = pcall(createTrack, {
                    points = { { x = "0", y = false },
                        { x = "1", y = "not-a-number" } },
                    blocks = { "coerced_coordinates" },
                    openEnded = true, rotateBlock = false
                })
                track_blocks_fails = not pcall(createTrack, {
                    points = { { x = 0, y = 0 }, { x = 1, y = 0 } },
                    blocks = false, openEnded = true, rotateBlock = false
                })
                track_block_type_fails = not pcall(createTrack, {
                    points = { { x = 0, y = 0 }, { x = 1, y = 0 } },
                    blocks = { false }, openEnded = true, rotateBlock = false
                })
                track_missing_object_fails = not pcall(createTrack, {
                    points = { { x = 0, y = 0 }, { x = 1, y = 0 } },
                    blocks = { "missing" }, openEnded = true, rotateBlock = false
                })
                setVelocity("cart", 3, 4)
                update = function() end
                "#,
        )
        .unwrap();
    let environment = game_environment(runtime.lua()).unwrap();
    for field in [
        "track_missing_fails",
        "track_first_argument_fails",
        "track_points_fails",
        "track_point_fails",
        "track_blocks_fails",
        "track_block_type_fails",
        "track_missing_object_fails",
    ] {
        assert!(environment.get::<bool>(field).unwrap(), "{field}");
    }
    assert!(environment.get::<bool>("track_coordinates_coerce").unwrap());
    runtime.update(1.0 / 30.0).unwrap();

    {
        let bridge = runtime.render.lock().unwrap();
        assert_eq!(bridge.tracks.len(), 2);
        let cart = &bridge.scene["cart"];
        // sub_10086DB8C is a ten-pass velocity constraint. It does not
        // snap the position onto the line and its position solver is the
        // literal-true sub_10086DCE0.
        assert_eq!(cart.x, 2.099_999_904_632_568_4);
        assert_eq!(cart.y, 7.003_068_923_950_195);
        assert_eq!(cart.velocity_x, 3.0);
        assert_eq!(cart.velocity_y, f64::from(0.09207073_f32));
        assert!(cart.angle.abs() < 1e-9);
        let track = &bridge.tracks["cart"];
        assert_eq!(track.current_segment, 0);
        assert_eq!(track.impulse_y, f64::from(-1.9079297_f32));
    }

    runtime.update(1.0 / 30.0).unwrap();
    let bridge = runtime.render.lock().unwrap();
    let cart = &bridge.scene["cart"];
    // An unchanged chain child warm-starts the accumulated normal impulse
    // before the next ten velocity iterations.
    assert_eq!(cart.x, 2.199_999_809_265_136_7);
    assert_eq!(cart.y, 6.938_475_608_825_684);
    assert_eq!(cart.velocity_x, 3.0);
    assert_eq!(cart.velocity_y, f64::from(-1.9377928_f32));
    assert_eq!(bridge.tracks["cart"].impulse_y, f64::from(-0.026794612_f32));
}

#[test]
fn create_track_reads_native_flags_per_block_after_object_lookup() {
    let runtime = unlocked_test_runtime();
    runtime
        .execute_source(
            r#"
                createBox("first", "", 0, 0, 1, 1,
                    1, 0, 0, true, false, 1)
                createBox("second", "", 0, 0, 1, 1,
                    1, 0, 0, true, false, 1)

                flag_reads = {}
                local descriptor = setmetatable({
                    points = { { x = 0, y = 0 }, { x = 4, y = 0 } },
                    blocks = { "first", "second" }
                }, {
                    __index = function(_, key)
                        table.insert(flag_reads, key)
                        if key == "rotateBlock" then
                            return "truthy"
                        end
                        return false
                    end
                })
                createTrack(descriptor)

                missing_flag_reads = 0
                local missing = setmetatable({
                    points = { { x = 0, y = 0 }, { x = 4, y = 0 } },
                    blocks = { "missing" }
                }, {
                    __index = function()
                        missing_flag_reads = missing_flag_reads + 1
                        return true
                    end
                })
                missing_track_fails = not pcall(createTrack, missing)

                createBox("partial", "", 0, 0, 1, 1,
                    1, 0, 0, true, false, 1)
                partial_track_fails = not pcall(createTrack, {
                    points = { { x = 0, y = 0 }, { x = 4, y = 0 } },
                    blocks = { "partial", false },
                    openEnded = true,
                    rotateBlock = false
                })

                createBox("1", "", 0, 0, 1, 1,
                    1, 0, 0, true, false, 1)
                numeric_block_name_succeeds = pcall(createTrack, {
                    points = { { x = 0, y = 0 }, { x = 4, y = 0 } },
                    blocks = { 1 },
                    openEnded = true,
                    rotateBlock = false
                })

                createBox("extra_key", "", 0, 0, 1, 1,
                    1, 0, 0, true, false, 1)
                extra_point_key_fails = not pcall(createTrack, {
                    points = {
                        { x = 0, y = 0 }, { x = 4, y = 0 }, marker = true
                    },
                    blocks = { "extra_key" },
                    openEnded = true,
                    rotateBlock = false
                })
            "#,
        )
        .unwrap();

    let environment = game_environment(runtime.lua()).unwrap();
    let reads = environment.get::<mlua::Table>("flag_reads").unwrap();
    assert_eq!(reads.raw_len(), 4);
    assert_eq!(reads.raw_get::<String>(1).unwrap(), "openEnded");
    assert_eq!(reads.raw_get::<String>(2).unwrap(), "rotateBlock");
    assert_eq!(reads.raw_get::<String>(3).unwrap(), "openEnded");
    assert_eq!(reads.raw_get::<String>(4).unwrap(), "rotateBlock");
    assert_eq!(environment.get::<i64>("missing_flag_reads").unwrap(), 0);
    assert!(environment.get::<bool>("missing_track_fails").unwrap());
    assert!(environment.get::<bool>("partial_track_fails").unwrap());
    assert!(
        environment
            .get::<bool>("numeric_block_name_succeeds")
            .unwrap()
    );
    assert!(environment.get::<bool>("extra_point_key_fails").unwrap());

    let bridge = runtime.render.lock().unwrap();
    assert!(bridge.tracks["first"].rotate_block);
    assert!(bridge.tracks["second"].rotate_block);
    assert!(bridge.tracks.contains_key("partial"));
    assert!(bridge.tracks.contains_key("1"));
}

#[test]
fn current_track_angle_uses_native_chain_children_ties_and_float32() {
    let runtime = unlocked_test_runtime();
    runtime
        .execute_source(
            r#"
                createBox("tie", "", 1, 1, 1, 1, 1, 0, 0, true, false, 1)
                createTrack({
                    points = { { x = 0, y = 0 }, { x = 2, y = 0 },
                        { x = 2, y = 2 } },
                    blocks = { "tie" }, openEnded = true, rotateBlock = false
                })
                tie_angle = getCurrentTrackAngle("tie")
                setPosition("tie", 2, 1.5)
                vertical_angle = getCurrentTrackAngle("tie")

                createBox("precision", "", 1, 0, 1, 1,
                    1, 0, 0, true, false, 1)
                createTrack({
                    points = { { x = 0, y = 0 }, { x = 3, y = 0.123456789 } },
                    blocks = { "precision" }, openEnded = true,
                    rotateBlock = false
                })
                precision_angle = getCurrentTrackAngle("precision")

                createBox("untracked", "", 0, 0, 1, 1,
                    1, 0, 0, true, false, 1)
                untracked_angle = getCurrentTrackAngle("untracked")
                missing_track_angle_fails = not pcall(
                    getCurrentTrackAngle, "missing"
                )
                missing_destroy_track_fails = not pcall(
                    destroyTrack, "missing"
                )

                createBox("open_chain", "", 0, 2, 1, 1,
                    1, 0, 0, true, false, 1)
                createTrack({
                    points = { { x = 0, y = 0 }, { x = 4, y = 0 },
                        { x = 4, y = 4 } },
                    blocks = { "open_chain" }, openEnded = false,
                    rotateBlock = false
                })
                no_implicit_closing_angle = getCurrentTrackAngle("open_chain")
                "#,
        )
        .unwrap();

    let environment = game_environment(runtime.lua()).unwrap();
    // The point is equidistant from both children. sub_10085D364 updates
    // on strict less-than, so child zero wins.
    assert_eq!(environment.get::<f64>("tie_angle").unwrap(), 0.0);
    assert_eq!(
        environment.get::<f64>("vertical_angle").unwrap(),
        f64::from(std::f32::consts::FRAC_PI_2)
    );
    assert_eq!(
        environment.get::<f64>("precision_angle").unwrap(),
        f64::from((0.123456789_f64 as f32).atan2(3.0_f32))
    );
    // createTrack always builds a b2ChainShape with count-1 children;
    // openEnded changes track behavior, not the chain into a loop.
    assert_eq!(
        environment.get::<f64>("no_implicit_closing_angle").unwrap(),
        0.0
    );
    // sub_10003D650 returns zero only after a successful object lookup finds
    // no track pointer. Both track members use throwing getRenderObject first.
    assert_eq!(environment.get::<f64>("untracked_angle").unwrap(), 0.0);
    assert!(
        environment
            .get::<bool>("missing_track_angle_fails")
            .unwrap()
    );
    assert!(
        environment
            .get::<bool>("missing_destroy_track_fails")
            .unwrap()
    );
}

#[test]
fn destroy_track_obeys_native_world_lock_and_wakes_the_body() {
    let runtime = unlocked_test_runtime();
    runtime
        .execute_source(
            r#"
                createBox("body", "", 0, 0, 1, 1,
                    1, 0, 0, true, false, 1)
                createTrack({
                    points = { { x = 0, y = 0 }, { x = 4, y = 0 } },
                    blocks = { "body" },
                    openEnded = true,
                    rotateBlock = false
                })
                setSleeping("body", true)
            "#,
        )
        .unwrap();

    {
        let mut bridge = runtime.render.lock().unwrap();
        bridge.scene.get_mut("body").unwrap().sleep_time = 0.75;
        bridge.physics_world_locked = true;
    }
    runtime.execute_source(r#"destroyTrack("body")"#).unwrap();
    {
        let bridge = runtime.render.lock().unwrap();
        assert!(bridge.tracks.contains_key("body"));
        assert!(bridge.scene["body"].sleeping);
        assert_eq!(bridge.scene["body"].sleep_time, 0.75);
    }

    runtime.render.lock().unwrap().physics_world_locked = false;
    runtime.execute_source(r#"destroyTrack("body")"#).unwrap();
    let bridge = runtime.render.lock().unwrap();
    assert!(!bridge.tracks.contains_key("body"));
    assert!(!bridge.scene["body"].sleeping);
    assert_eq!(bridge.scene["body"].sleep_time, 0.0);
}

#[test]
fn object_track_overlap_uses_chain_distance_and_only_the_body_list_head() {
    let runtime = unlocked_test_runtime();
    runtime
        .execute_source(
            r#"
                createBox("box", "", 0, 0, 2, 2, 1, 0, 0, true, false, 1)
                within_skin = objectAndTrackOverlap("box", {
                    points = { { x = -3, y = 1.003 }, { x = 3, y = 1.003 } }
                })
                outside_skin = objectAndTrackOverlap("box", {
                    points = { { x = -3, y = 1.005 }, { x = 3, y = 1.005 } }
                })
                aabb_only = objectAndTrackOverlap("box", {
                    points = { { x = -2, y = 0.1 }, { x = -0.1, y = 2 } }
                })
                wrong_table_shape = objectAndTrackOverlap("box", {
                    { x = -3, y = 0 }, { x = 3, y = 0 }
                })
                createCircle("negative_circle", "", 0, 5, 1,
                    1, 0, 0, true, false, 1)
                native_resizeRadius("negative_circle", -1, 1, 0, 0)
                negative_core_overlap = objectAndTrackOverlap("negative_circle", {
                    points = { { x = -1, y = 5 }, { x = 1, y = 5 } }
                })
                negative_radius_separated = objectAndTrackOverlap("negative_circle", {
                    points = { { x = -1, y = 5.1 }, { x = 1, y = 5.1 } }
                })

                clearVertices()
                addVertex(0, -5); addVertex(3, -4); addVertex(5, -2)
                addVertex(5, 2); addVertex(3, 4); addVertex(0, 5)
                addVertex(-3, 4); addVertex(-5, 2); addVertex(-5, -2)
                addVertex(-3, -4)
                createPolygon("compound", "", 0, 0, 10, 10,
                    1, 0, 0, true, false, 1)
                "#,
        )
        .unwrap();

    let environment = game_environment(runtime.lua()).unwrap();
    assert!(environment.get::<bool>("within_skin").unwrap());
    assert!(!environment.get::<bool>("outside_skin").unwrap());
    assert!(!environment.get::<bool>("aabb_only").unwrap());
    assert!(!environment.get::<bool>("wrong_table_shape").unwrap());
    // b2Distance's use-radii branch returns zero when the two shape cores
    // overlap, even when native_resizeRadius installed a negative radius.
    assert!(environment.get::<bool>("negative_core_overlap").unwrap());
    assert!(
        !environment
            .get::<bool>("negative_radius_separated")
            .unwrap()
    );

    let (first_fixture, head_fixture) = {
        let bridge = runtime.render.lock().unwrap();
        let CollisionShape::Polygon { fixtures, .. } = &bridge.scene["compound"].collision_shape
        else {
            panic!("compound must have polygon fixtures");
        };
        assert!(fixtures.len() >= 2);
        (
            fixtures.first().unwrap().clone(),
            fixtures.last().unwrap().clone(),
        )
    };
    let sum = first_fixture
        .iter()
        .fold((0.0, 0.0), |sum, point| (sum.0 + point.0, sum.1 + point.1));
    let center = (
        sum.0 / first_fixture.len() as f64,
        sum.1 / first_fixture.len() as f64,
    );
    assert!(!polygon_contains_point(&head_fixture, center));

    let lua = runtime.lua();
    let points = lua.create_table().unwrap();
    for (index, x) in [center.0 - 0.01, center.0 + 0.01].into_iter().enumerate() {
        let point = lua.create_table().unwrap();
        point.set("x", x).unwrap();
        point.set("y", center.1).unwrap();
        points.raw_set(index + 1, point).unwrap();
    }
    let track = lua.create_table().unwrap();
    track.set("points", points).unwrap();
    let overlap = environment
        .get::<Function>("objectAndTrackOverlap")
        .unwrap();
    assert!(!overlap.call::<bool>(("compound", track.clone())).unwrap());

    // setPhysicsScale saves the old head-to-tail list and recreates it in
    // that order, making the original first-created fixture the new head.
    environment
        .get::<Function>("setPhysicsScale")
        .unwrap()
        .call::<()>(("compound", 1.0, 1.0))
        .unwrap();
    assert!(overlap.call::<bool>(("compound", track)).unwrap());
}

#[test]
fn track_joint_registration_matches_native_adapters_types_and_float32() {
    let runtime = unlocked_test_runtime();
    runtime
        .execute_source(
            r#"
                createBox("body", "", 0, 0, 2, 2, 1, 0, 0, true, false, 1)
                objects.world.body.blockCollisionEnabled = "lua-block"
                objects.world.body.ignoresScore = "lua-score"
                objects.world.body.keepOrientation = "lua-orientation"
                objects.world.body.recordVelocity = "lua-velocity"
                objects.world.body.revertGravity = "lua-gravity"
                native_setBlockCollisionEnabled("body", false)
                native_setIgnoresScore("body", true)
                native_setKeepOrientation("body", true)
                setRecordVelocity("body", true)
                setRevertGravity("body", true)

                flag_name_type_fails = not pcall(
                    native_setBlockCollisionEnabled, 1, true
                )
                flag_bool_type_fails = not pcall(
                    native_setBlockCollisionEnabled, "body", 1
                )
                overlap_name_type_fails = not pcall(
                    objectAndTrackOverlap, 1, { points = {} }
                )
                overlap_table_type_fails = not pcall(
                    objectAndTrackOverlap, "body", false
                )
                overlap_missing_object_fails = not pcall(
                    objectAndTrackOverlap, "missing", {
                        points = { { x = 0, y = 0 }, { x = 1, y = 0 } }
                    }
                )
                overlap_point_type_fails = not pcall(
                    objectAndTrackOverlap, "body", { points = { false } }
                )
                overlap_coordinate_type_fails = not pcall(
                    objectAndTrackOverlap, "body", {
                        points = { { x = false, y = 0 } }
                    }
                )
                vertices_name_type_fails = not pcall(getObjectVertices, 1)

                createBox("anchor", "", 0, 0, 1, 1, 0, 0, 0, true, false, 1)
                createBox("moving", "", 2, 0, 1, 1, 1, 0, 0, true, false, 1)
                createJoint({
                    name = "distance", end1 = "anchor", end2 = "moving",
                    type = 1, coordType = 2,
                    x1 = 0, y1 = 0, x2 = 0, y2 = 0
                })
                createJoint({
                    name = "revolute", end1 = "anchor", end2 = "moving",
                    type = 3, coordType = 2,
                    x1 = 0, y1 = 0, x2 = 0, y2 = 0
                })
                createJoint({
                    name = "weld", end1 = "anchor", end2 = "moving",
                    type = 2, coordType = 2,
                    x1 = 0, y1 = 0, x2 = 0, y2 = 0
                })
                objects.joints.distance.motorSpeed = 8
                objects.joints.revolute.frequency = 7
                objects.joints.weld.frequency = 6
                objects.joints.weld.motorSpeed = 5
                setJointParameters({
                    name = "distance", frequency = 0.99999999,
                    dampingRatio = 0.49999999, length = 3.9999999,
                    motorSpeed = 9
                })
                setJointParameters({
                    name = "revolute", motor = true,
                    motorSpeed = 0.99999999, maxTorque = 3.9999999,
                    limit = true, lowerLimit = -0.49999999,
                    upperLimit = 0.49999999, frequency = 9
                })
                setJointParameters({
                    name = "weld", frequency = 9, motorSpeed = 9
                })
                "#,
        )
        .unwrap();

    let environment = game_environment(runtime.lua()).unwrap();
    for field in [
        "flag_name_type_fails",
        "flag_bool_type_fails",
        "overlap_name_type_fails",
        "overlap_table_type_fails",
        "overlap_missing_object_fails",
        "overlap_point_type_fails",
        "overlap_coordinate_type_fails",
        "vertices_name_type_fails",
    ] {
        assert!(environment.get::<bool>(field).unwrap(), "{field}");
    }
    let world = environment
        .get::<mlua::Table>("objects")
        .unwrap()
        .get::<mlua::Table>("world")
        .unwrap();
    let body = world.get::<mlua::Table>("body").unwrap();
    assert_eq!(
        body.get::<String>("blockCollisionEnabled").unwrap(),
        "lua-block"
    );
    assert_eq!(body.get::<String>("ignoresScore").unwrap(), "lua-score");
    assert_eq!(
        body.get::<String>("keepOrientation").unwrap(),
        "lua-orientation"
    );
    assert_eq!(
        body.get::<String>("recordVelocity").unwrap(),
        "lua-velocity"
    );
    assert_eq!(body.get::<String>("revertGravity").unwrap(), "lua-gravity");

    let joints = environment
        .get::<mlua::Table>("objects")
        .unwrap()
        .get::<mlua::Table>("joints")
        .unwrap();
    let distance = joints.get::<mlua::Table>("distance").unwrap();
    assert_eq!(distance.get::<f64>("frequency").unwrap(), 1.0);
    assert_eq!(
        distance.get::<f64>("dampingRatio").unwrap(),
        f64::from(0.5_f32)
    );
    assert_eq!(
        distance.get::<f64>("length").unwrap(),
        f64::from(3.9999999_f32)
    );
    assert_eq!(distance.get::<f64>("motorSpeed").unwrap(), 8.0);
    let revolute = joints.get::<mlua::Table>("revolute").unwrap();
    assert_eq!(revolute.get::<f64>("motorSpeed").unwrap(), 1.0);
    assert_eq!(
        revolute.get::<f64>("maxTorque").unwrap(),
        f64::from(3.9999999_f32)
    );
    assert_eq!(revolute.get::<f64>("frequency").unwrap(), 7.0);
    let weld = joints.get::<mlua::Table>("weld").unwrap();
    assert_eq!(weld.get::<f64>("frequency").unwrap(), 6.0);
    assert_eq!(weld.get::<f64>("motorSpeed").unwrap(), 5.0);

    let bridge = runtime.render.lock().unwrap();
    let body = &bridge.scene["body"];
    assert!(!body.block_collision_enabled);
    assert!(body.ignores_score);
    assert!(body.keep_orientation);
    assert!(body.record_velocity);
    assert!(body.revert_gravity);
    let distance = &bridge.joints["distance"];
    assert_eq!(distance.frequency, 1.0);
    assert_eq!(distance.damping_ratio, f64::from(0.5_f32));
    assert_eq!(distance.rest_length, f64::from(3.9999999_f32));
    assert_eq!(distance.motor_speed, None);
    let revolute = &bridge.joints["revolute"];
    assert_eq!(revolute.motor_speed, Some(1.0));
    assert_eq!(revolute.frequency, 0.0);
    assert_eq!(bridge.joints["weld"].motor_speed, None);
}

#[test]
fn track_overlap_uses_native_fused_b2transform() {
    let runtime = unlocked_test_runtime();
    runtime
        .execute_source(
            r#"
                createBox("fma_track", "", 0, 0, 1, 1,
                    1, 0, 0, true, false, 1)
                "#,
        )
        .unwrap();

    let mut bridge = runtime.render.lock().unwrap();
    let object = bridge.scene.get_mut("fma_track").unwrap();
    object.x = 17_970_492.0;
    object.y = 3_920_512.25;
    object.angle = f64::from(0.169_156_95_f32);
    object.physics_scale_x = 1.0;
    object.physics_scale_y = 1.0;
    object.collision_shape = CollisionShape::Line {
        vertices: vec![
            (f64::from(-65.941_18_f32), f64::from(75.645_15_f32)),
            (f64::from(-65.941_18_f32), f64::from(76.645_15_f32)),
        ],
    };

    // The nested ARM FMLA sequence projects this edge to x=17,970,414.
    // Separate float32 multiply/add/subtract rounds it to 17,970,416, a
    // two-unit difference large enough to change b2Distance overlap.
    let native_edge = ((17_970_414.0, 3_920_575.75), (17_970_414.0, 3_920_576.75));
    let separately_rounded_edge = ((17_970_416.0, 3_920_575.75), (17_970_416.0, 3_920_576.75));
    assert!(object.head_fixture_overlaps_track_segment(native_edge));
    assert!(!object.head_fixture_overlaps_track_segment(separately_rounded_edge));
}

#[test]
fn recovered_object_transform_pivot_decoration_and_joint_motor_contracts() {
    let runtime = unlocked_test_runtime();
    register_test_sprite_sheet(
        &runtime,
        &["BASE_SPRITE", "DECORATION_SPRITE", "POST_DECORATION"],
    );
    runtime
        .execute_source(
            r#"
                blocks = {
                    decorated_definition = {
                        decorations = {
                            objects = {
                                amount = 3,
                                sprite = "DECORATION_SPRITE",
                                angleIncrement = 90,
                                scale = 0.5
                            }
                        }
                    }
                }
                createBox(
                    "decorated", "BASE_SPRITE", 4, 7, 1, 1,
                    0, 0, 0, true, false, 1
                )
                objects.world.decorated.definition = "decorated_definition"
                setAngle("decorated", math.pi / 2)
                setPivotOffset("decorated", 12, 18)
                setSensorGravityMask("decorated", 13)
                setDecorationObjects("decorated")
                native_setPostDrawFunction("decorated", function()
                    res.drawSprite("POST_DECORATION", 0, 0)
                end)
                world_x, world_y = getWorldPoint("decorated", 2, 3)
                local_x, local_y = getLocalPoint("decorated", world_x, world_y)

                createBox("motor_a", "", 0, 0, 1, 1, 1, 0, 0, true, false, 1)
                createBox("motor_b", "", 1, 0, 1, 1, 1, 0, 0, true, false, 1)
                createJoints({
                    motor = {
                        name = "motor", end1 = "motor_a", end2 = "motor_b",
                        type = 3, x1 = 0, y1 = 0, x2 = 0, y2 = 0
                    }
                })
                setRevoluteJointSpeed("motor", 2.5)
                drawGameNative()
                "#,
        )
        .unwrap();

    let environment = game_environment(runtime.lua()).unwrap();
    let angle = (std::f64::consts::PI / 2.0) as f32;
    let (sine, cosine) = angle.sin_cos();
    let expected_world_x = 4.0_f32 + 2.0_f32.mul_add(cosine, -(3.0_f32 * sine));
    let expected_world_y = 7.0_f32 + 3.0_f32.mul_add(cosine, 2.0_f32 * sine);
    assert_eq!(
        environment.get::<f64>("world_x").unwrap(),
        f64::from(expected_world_x)
    );
    assert_eq!(
        environment.get::<f64>("world_y").unwrap(),
        f64::from(expected_world_y)
    );
    let delta_x = expected_world_x - 4.0_f32;
    let delta_y = expected_world_y - 7.0_f32;
    assert_eq!(
        environment.get::<f64>("local_x").unwrap(),
        f64::from(delta_x.mul_add(cosine, delta_y * sine))
    );
    assert_eq!(
        environment.get::<f64>("local_y").unwrap(),
        f64::from(cosine.mul_add(delta_y, -(delta_x * sine)))
    );

    let bridge = runtime.render.lock().unwrap();
    let object = bridge.scene.get("decorated").unwrap();
    assert_eq!((object.pivot_offset_x, object.pivot_offset_y), (12.0, 18.0));
    assert_eq!(object.sensor_gravity_mask, 13);
    let decorations = bridge
        .commands
        .iter()
        .filter(|command| command.sprite == "DECORATION_SPRITE")
        .collect::<Vec<_>>();
    assert_eq!(decorations.len(), 3);
    assert!(decorations.iter().all(|command| !command.world_space));
    assert!((decorations[0].state.scale_x - 0.5).abs() < 1e-9);
    assert_eq!((decorations[0].x, decorations[0].y), (160.0, 280.0));
    let native_step =
        (90.0_f32 * f32::from_bits(0x4049_0FDB)).mul_add(f32::from_bits(0x3BB6_0B61), 0.0);
    let first_angle = object.angle as f32;
    let second_angle = native_step + first_angle;
    let third_angle = native_step + second_angle;
    assert_eq!(decorations[0].state.angle, first_angle);
    assert_eq!(decorations[1].state.angle, second_angle);
    assert_eq!(decorations[2].state.angle, third_angle);
    let post_decoration = bridge
        .commands
        .iter()
        .find(|command| command.sprite == "POST_DECORATION")
        .unwrap();
    assert!(!post_decoration.world_space);
    assert_eq!(
        (post_decoration.state.scale_x, post_decoration.state.scale_y),
        (0.5, 0.5)
    );
    assert_eq!(post_decoration.state.angle, third_angle);
    assert_eq!(bridge.joints["motor"].motor_speed, Some(2.5));
    assert!(bridge.scene["motor_a"].motion_started);
    assert!(bridge.scene["motor_b"].motion_started);
}

#[test]
fn recovered_object_feature_adapters_are_strict_f32_and_native_only() {
    let runtime = unlocked_test_runtime();
    runtime
        .execute_source(
            r#"
                blocks = {
                    valid_decoration = {
                        decorations = { objects = {
                            amount = -2,
                            sprite = "DECORATION",
                            angleIncrement = 0.99999999,
                            scale = 0.49999999
                        } }
                    },
                    invalid_decoration = {
                        decorations = { objects = {
                            sprite = "MISSING_AMOUNT",
                            angleIncrement = 0,
                            scale = 1
                        } }
                    }
                }
                createBox("body", "BODY", 0, 0, 2, 2, 1, 0, 0, true, false, 1)
                objects.world.body.definition = "valid_decoration"
                setObjectParameter("body", 6, 0.99999999)
                setObjectParameter("body", 20, 1)
                setObjectParameter("body", 22, 0)
                setObjectParameter("body", 22, 1)
                setObjectParameter("body", 33, 0)
                setObjectGravityCategory("body", 7.9999999)
                setSensorGravityMask("body", 5.9999999)
                setPivotOffset("body", 0.99999999, -0.99999999)
                setDecorationObjects("body")
                setFlashAnimation("body")
                removeFlashAnimation("body")

                createBox("joint_a", "", 0, 0, 1, 1, 1, 0, 0, true, false, 1)
                createBox("joint_b", "", 1, 0, 1, 1, 1, 0, 0, true, false, 1)
                createJoints({ motor = {
                    name = "motor", end1 = "joint_a", end2 = "joint_b",
                    type = 3, x1 = 0, y1 = 0, x2 = 0, y2 = 0
                } })
                setSleeping("joint_a", true)
                setSleeping("joint_b", true)
                setRevoluteJointSpeed("motor", 0.99999999)

                objects.world.lua_only = { definition = "valid_decoration" }
                pivot_unknown_fails = not pcall(
                    setPivotOffset, "lua_only", 0.99999999, -0.99999999
                )
                decoration_unknown_fails = not pcall(setDecorationObjects, "lua_only")
                objects.world.invalid = { definition = "invalid_decoration" }
                decoration_field_fails = not pcall(setDecorationObjects, "invalid")

                parameter_short_fails = not pcall(setObjectParameter, "body", 6)
                parameter_type_fails = not pcall(setObjectParameter, "body", 6, "1")
                parameter_unknown_fails = not pcall(setObjectParameter, "missing", 6, 1)
                category_type_fails = not pcall(setObjectGravityCategory, "body", true)
                category_unknown_fails = not pcall(setObjectGravityCategory, "missing", 1)
                mask_short_fails = not pcall(setSensorGravityMask, "body")
                mask_unknown_fails = not pcall(setSensorGravityMask, "missing", 1)
                pivot_short_fails = not pcall(setPivotOffset, "body", 1)
                decoration_type_fails = not pcall(setDecorationObjects, 1)
                joint_type_fails = not pcall(setRevoluteJointSpeed, "motor", true)
                flash_unknown_fails = not pcall(setFlashAnimation, "missing")
                "#,
        )
        .unwrap();

    let environment = game_environment(runtime.lua()).unwrap();
    for field in [
        "pivot_unknown_fails",
        "decoration_unknown_fails",
        "decoration_field_fails",
        "parameter_short_fails",
        "parameter_type_fails",
        "parameter_unknown_fails",
        "category_type_fails",
        "category_unknown_fails",
        "mask_short_fails",
        "mask_unknown_fails",
        "pivot_short_fails",
        "decoration_type_fails",
        "joint_type_fails",
        "flash_unknown_fails",
    ] {
        assert!(environment.get::<bool>(field).unwrap(), "{field}");
    }

    let world = object_world(runtime.lua()).unwrap();
    let body_table = world.get::<mlua::Table>("body").unwrap();
    assert!(matches!(
        body_table.get::<Value>("nativeParameters").unwrap(),
        Value::Nil
    ));
    assert!(matches!(
        body_table.get::<Value>("gravityCategory").unwrap(),
        Value::Nil
    ));
    assert!(matches!(
        body_table.get::<Value>("sensorGravityMask").unwrap(),
        Value::Nil
    ));
    assert!(matches!(
        body_table.get::<Value>("sensor").unwrap(),
        Value::Nil
    ));
    // Parameter 33 is a native byte store; the Lua constructor field remains
    // unchanged until the ordinary script path changes it.
    assert!(matches!(
        body_table.get::<Value>("visible").unwrap(),
        Value::Nil
    ));
    let lua_only = world.get::<mlua::Table>("lua_only").unwrap();
    assert_eq!(
        lua_only.get::<f64>("pivotOffsetX").unwrap(),
        f64::from(0.99999999_f32)
    );
    assert_eq!(
        lua_only.get::<f64>("pivotOffsetY").unwrap(),
        f64::from(-0.99999999_f32)
    );

    let bridge = runtime.render.lock().unwrap();
    let body = &bridge.scene["body"];
    assert_eq!(body.bounce_amplitude_multiplier, f64::from(0.99999999_f32));
    assert_eq!(body.gravity_category, 8);
    assert_eq!(body.sensor_gravity_mask, 6);
    assert_eq!(body.pivot_offset_x, f64::from(0.99999999_f32));
    assert_eq!(body.pivot_offset_y, f64::from(-0.99999999_f32));
    assert!(body.active);
    assert!(!body.sensor);
    assert!(!body.visible);
    assert!(!body.flash_animation);
    let decoration = body.decoration.as_ref().unwrap();
    assert_eq!(decoration.amount, -2);
    assert_eq!(decoration.angle_increment, f64::from(0.99999999_f32));
    assert_eq!(decoration.scale, f64::from(0.5_f32));
    assert_eq!(
        bridge.joints["motor"].motor_speed,
        Some(f64::from(0.99999999_f32))
    );
    assert!(!bridge.scene["joint_a"].sleeping);
    assert!(!bridge.scene["joint_b"].sleeping);
}
