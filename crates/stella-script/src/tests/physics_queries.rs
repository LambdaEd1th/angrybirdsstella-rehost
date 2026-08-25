use super::*;

#[test]
fn polygon_constructor_consumes_native_vertex_buffer() {
    let runtime = unlocked_test_runtime();
    runtime
        .execute_source(
            r#"
                clearVertices()
                addVertex(-2, -1)
                addVertex(2, -1)
                addVertex(2, 1)
                addVertex(-2, 1)
                createPolygon("polygon", "", 10, 20, 4, 2, 3, 0, 0, true, false, 1)
                "#,
        )
        .unwrap();

    let world = object_world(runtime.lua()).unwrap();
    let polygon: mlua::Table = world.get("polygon").unwrap();
    assert!(matches!(
        polygon.get::<mlua::Value>("vertexCount").unwrap(),
        mlua::Value::Nil
    ));
    assert_eq!(polygon.get::<f64>("width").unwrap(), 4.0);
    assert_eq!(polygon.get::<f64>("height").unwrap(), 2.0);
    assert_eq!(polygon.get::<f64>("mass").unwrap(), 24.0);
    let bridge = runtime.render.lock().unwrap();
    assert!(matches!(
        &bridge.scene["polygon"].collision_shape,
        CollisionShape::Polygon { vertices, fixtures }
            if vertices.len() == 4 && fixtures.len() == 1
    ));
}

#[test]
fn polygon_constructor_uses_native_seven_vertex_fixture_limit() {
    let vertices = [
        (0.0, -5.0),
        (3.0, -4.0),
        (5.0, -2.0),
        (5.0, 2.0),
        (3.0, 4.0),
        (0.0, 5.0),
        (-3.0, 4.0),
        (-5.0, 2.0),
        (-5.0, -2.0),
        (-3.0, -4.0),
    ];
    let (_, fixtures) = native_polygon_fixtures(&vertices);
    assert!(fixtures.len() >= 2);
    assert!(
        fixtures
            .iter()
            .all(|fixture| (3..=8).contains(&fixture.len()))
    );
    let fixture_area = fixtures
        .iter()
        .map(|fixture| polygon_area(fixture))
        .sum::<f64>();
    assert!((fixture_area - polygon_area(&vertices)).abs() < 1.0e-6);

    let runtime = unlocked_test_runtime();
    runtime
        .execute_source(
            r#"
                clearVertices()
                addVertex(0, -5); addVertex(3, -4); addVertex(5, -2)
                addVertex(5, 2); addVertex(3, 4); addVertex(0, 5)
                addVertex(-3, 4); addVertex(-5, 2); addVertex(-5, -2)
                addVertex(-3, -4)
                createPolygon("wide", "", 0, 0, 10, 10, 1, 0, 0, true, false, 1)
                wide_fixtures = getObjectVertices("wide")
                "#,
        )
        .unwrap();
    let environment = game_environment(runtime.lua()).unwrap();
    let returned: mlua::Table = environment.get("wide_fixtures").unwrap();
    assert_eq!(returned.raw_len(), fixtures.len());
    for fixture in returned.sequence_values::<mlua::Table>() {
        assert!((3..=8).contains(&fixture.unwrap().raw_len()));
    }
}

#[test]
fn object_vertices_follow_native_fixture_list_and_resized_f32_vertices() {
    let source_vertices = vec![
        (0.0, -5.0),
        (3.0, -4.0),
        (5.0, -2.0),
        (5.0, 2.0),
        (3.0, 4.0),
        (0.0, 5.0),
        (-3.0, 4.0),
        (-5.0, 2.0),
        (-5.0, -2.0),
        (-3.0, -4.0),
    ];
    let (_, creation_order) = native_polygon_fixtures(&source_vertices);
    assert!(creation_order.len() >= 2);

    let runtime = unlocked_test_runtime();
    runtime
        .execute_source(
            r#"
                clearVertices()
                addVertex(0, -5); addVertex(3, -4); addVertex(5, -2)
                addVertex(5, 2); addVertex(3, 4); addVertex(0, 5)
                addVertex(-3, 4); addVertex(-5, 2); addVertex(-5, -2)
                addVertex(-3, -4)
                createPolygon("compound", "", 10, 20, 10, 10,
                    1, 0, 0, true, false, 1)
                setAngle("compound", 0.75)
                vertices_before_resize = getObjectVertices("compound")
                setPhysicsScale("compound", -2, 3)
                vertices_after_resize = getObjectVertices("compound")
                "#,
        )
        .unwrap();

    let environment = game_environment(runtime.lua()).unwrap();
    let before: mlua::Table = environment.get("vertices_before_resize").unwrap();
    let after: mlua::Table = environment.get("vertices_after_resize").unwrap();
    assert_eq!(before.raw_len(), creation_order.len());
    assert_eq!(after.raw_len(), creation_order.len());

    let assert_fixture =
        |actual: mlua::Table, expected: &[(f64, f64)], scale_x: f32, scale_y: f32| {
            assert_eq!(actual.raw_len(), expected.len());
            for (index, &(x, y)) in expected.iter().enumerate() {
                let point = actual.raw_get::<mlua::Table>(index + 1).unwrap();
                let expected_x = 10.0_f32 + (x as f32) * scale_x;
                let expected_y = 20.0_f32 + (y as f32) * scale_y;
                assert_eq!(point.get::<f32>("x").unwrap(), expected_x);
                assert_eq!(point.get::<f32>("y").unwrap(), expected_y);
            }
        };

    // Initial CreateFixture calls make the last-created fixture the body
    // list head. setPhysicsScale snapshots that list and recreates it in
    // the same iteration order, which reverses the visible list once.
    assert_fixture(
        before.raw_get::<mlua::Table>(1).unwrap(),
        creation_order.last().unwrap(),
        1.0,
        1.0,
    );
    assert_fixture(
        after.raw_get::<mlua::Table>(1).unwrap(),
        creation_order.first().unwrap(),
        -2.0,
        3.0,
    );

    let bridge = runtime.render.lock().unwrap();
    let CollisionShape::Polygon { fixtures, .. } = &bridge.scene["compound"].collision_shape else {
        panic!("compound must retain polygon fixtures");
    };
    assert_eq!(fixtures.first(), creation_order.last());
    assert_eq!(fixtures.last(), creation_order.first());
}

#[test]
fn native_ray_cast_returns_flat_six_value_records_in_hit_order() {
    let runtime = unlocked_test_runtime();
    runtime
        .execute_source(
            r#"
                createBox("far", "", 8, 0, 2, 2, 0, 0, 0, true, false, 1)
                createBox("near", "", 5, 0, 2, 2, 0, 0, 0, true, false, 1)
                ray_hits = getRayCastedObjects({ x1 = 0, y1 = 0, x2 = 10, y2 = 0 })
                ray_query_missing_fails = not pcall(getRayCastedObjects)
                ray_query_type_fails = not pcall(getRayCastedObjects, false)
                ray_query_top_type_fails = not pcall(
                    getRayCastedObjects,
                    { x1 = 0, y1 = 0, x2 = 10, y2 = 0 },
                    false
                )
                ray_query_field_missing_fails = not pcall(
                    getRayCastedObjects, { x1 = 0, y1 = 0, x2 = 10 }
                )
                ray_query_field_type_fails = not pcall(
                    getRayCastedObjects, { x1 = "0", y1 = 0, x2 = 10, y2 = 0 }
                )
                "#,
        )
        .unwrap();

    let environment = game_environment(runtime.lua()).unwrap();
    for field in [
        "ray_query_missing_fails",
        "ray_query_type_fails",
        "ray_query_top_type_fails",
        "ray_query_field_missing_fails",
        "ray_query_field_type_fails",
    ] {
        assert!(environment.get::<bool>(field).unwrap(), "{field}");
    }
    let hits: mlua::Table = environment.get("ray_hits").unwrap();
    assert_eq!(hits.raw_len(), 12);
    assert_eq!(hits.raw_get::<String>(1).unwrap(), "near");
    assert!((hits.raw_get::<f64>(2).unwrap() - 4.0).abs() < 1e-9);
    assert_eq!(hits.raw_get::<f64>(4).unwrap(), -1.0);
    assert_eq!(hits.raw_get::<f64>(6).unwrap(), f64::from(0.4_f32));
    assert_eq!(hits.raw_get::<String>(7).unwrap(), "far");
}

#[test]
fn native_ray_cast_skips_sensors_but_keeps_collision_disabled_fixtures() {
    let runtime = unlocked_test_runtime();
    runtime
        .execute_source(
            r#"
                createBox("sensor", "", 2, 0, 1, 1, 0, 0, 0, true, false, 1)
                setAsSensor("sensor", true)
                createBox("disabled", "", 4, 0, 1, 1, 0, 0, 0, true, false, 1)
                setCollisionEnabled("disabled", false)
                createBox("solid", "", 6, 0, 1, 1, 0, 0, 0, true, false, 1)
                filtered_ray_hits = getRayCastedObjects({ x1 = 0, y1 = 0, x2 = 8, y2 = 0 })
                inside_ray_hits = getRayCastedObjects({ x1 = 5.75, y1 = 0, x2 = 8, y2 = 0 })
                "#,
        )
        .unwrap();

    let environment = game_environment(runtime.lua()).unwrap();
    let hits: mlua::Table = environment.get("filtered_ray_hits").unwrap();
    assert_eq!(hits.raw_len(), 12);
    assert_eq!(hits.raw_get::<String>(1).unwrap(), "disabled");
    assert_eq!(hits.raw_get::<String>(7).unwrap(), "solid");
    let inside: mlua::Table = environment.get("inside_ray_hits").unwrap();
    assert_eq!(inside.raw_len(), 0);
}

#[test]
fn native_ray_cast_returns_each_concave_polygon_fixture_hit() {
    let runtime = unlocked_test_runtime();
    runtime
        .execute_source(
            r#"
                clearVertices()
                addVertex(2, -2); addVertex(8, -2); addVertex(8, 2)
                addVertex(6, 2); addVertex(6, 0); addVertex(4, 0)
                addVertex(4, 2); addVertex(2, 2)
                createPolygon("concave", "", 0, 0, 10, 10, 1, 0, 0, true, false, 1)
                fixture_ray_hits = getRayCastedObjects({ x1 = 0, y1 = 1, x2 = 10, y2 = 1 })
                "#,
        )
        .unwrap();

    let environment = game_environment(runtime.lua()).unwrap();
    let hits: mlua::Table = environment.get("fixture_ray_hits").unwrap();
    assert!(hits.raw_len() >= 12, "expected at least two fixture hits");
    for index in (1..=hits.raw_len()).step_by(6) {
        assert_eq!(hits.raw_get::<String>(index).unwrap(), "concave");
    }
}

#[test]
fn native_level_limits_reorder_corner_arguments_and_publish_out_of_bounds() {
    let runtime = unlocked_test_runtime();
    runtime
        .execute_source(
            r##"
                setLevelLimits(-10, -5, 10, 5)
                limit_x_min, limit_x_max, limit_y_min, limit_y_max =
                    native_getLevelLimits()
                createCircle("inside", "", 0, 0, 1, 1, 0, 0, true, false, 1)
                createCircle("outside", "", 0, 6, 1, 1, 0, 0, true, false, 1)
                setWorldGravity(0, 0)
                updatePhysics = function() end
                update = function()
                    inside_out = g_outOfBoundariesObjects.inside
                    outside_out = g_outOfBoundariesObjects.outside
                end
                "##,
        )
        .unwrap();

    runtime.update(1.0 / 60.0).unwrap();
    let environment = game_environment(runtime.lua()).unwrap();
    assert_eq!(environment.get::<f64>("limit_x_min").unwrap(), -10.0);
    assert_eq!(environment.get::<f64>("limit_x_max").unwrap(), 10.0);
    assert_eq!(environment.get::<f64>("limit_y_min").unwrap(), -5.0);
    assert_eq!(environment.get::<f64>("limit_y_max").unwrap(), 5.0);
    assert!(matches!(
        environment.get::<Value>("inside_out").unwrap(),
        Value::Nil
    ));
    assert!(environment.get::<bool>("outside_out").unwrap());
}

#[test]
fn native_out_of_bounds_reuses_table_and_interleaves_after_xy_before_velocity() {
    let runtime = unlocked_test_runtime();
    runtime
        .execute_source(
            r##"
                setLevelLimits(-10, -5, 10, 5)
                g_outOfBoundariesObjects = setmetatable(
                    { retained = true },
                    { __newindex = function(table, key, value)
                        boundary_key = key
                        boundary_x = objects.world[key].x
                        boundary_y = objects.world[key].y
                        boundary_velocity = objects.world[key].velocity
                        rawset(table, key, value)
                    end }
                )
                retained_out_table = g_outOfBoundariesObjects
                createCircle("outside", "", 0, 6, 1, 1, 0, 0, true, false, 1)
                setWorldGravity(0, 0)
                updatePhysics = function() end
                update = function()
                    out_table_identity =
                        rawequal(retained_out_table, g_outOfBoundariesObjects)
                end
                "##,
        )
        .unwrap();

    runtime.update(1.0 / 60.0).unwrap();
    let environment = game_environment(runtime.lua()).unwrap();
    assert!(environment.get::<bool>("out_table_identity").unwrap());
    assert_eq!(
        environment.get::<String>("boundary_key").unwrap(),
        "outside"
    );
    assert_eq!(environment.get::<f64>("boundary_x").unwrap(), 0.0);
    assert_eq!(environment.get::<f64>("boundary_y").unwrap(), 6.0);
    assert!(matches!(
        environment.get::<Value>("boundary_velocity").unwrap(),
        Value::Nil
    ));
    let out_table = environment
        .get::<mlua::Table>("g_outOfBoundariesObjects")
        .unwrap();
    assert!(out_table.get::<bool>("retained").unwrap());
    assert!(out_table.get::<bool>("outside").unwrap());
    assert_eq!(
        object_world(runtime.lua())
            .unwrap()
            .get::<mlua::Table>("outside")
            .unwrap()
            .get::<f64>("velocity")
            .unwrap(),
        0.0
    );

    runtime
        .execute_source("setPosition('outside', 0, 0)")
        .unwrap();
    runtime.update(1.0 / 60.0).unwrap();
    assert!(out_table.get::<bool>("outside").unwrap());
    assert!(out_table.get::<bool>("retained").unwrap());
}

#[test]
fn native_out_of_bounds_requires_table_only_for_a_nonempty_scene() {
    let empty_runtime = unlocked_test_runtime();
    empty_runtime
        .execute_source(
            "g_outOfBoundariesObjects = 7; updatePhysics = function() end; update = function() end",
        )
        .unwrap();
    empty_runtime.update(0.01).unwrap();

    let populated_runtime = unlocked_test_runtime();
    populated_runtime
        .execute_source(
            r#"
                g_outOfBoundariesObjects = 7
                createNonPhysicsObject("visual", "", 0, 0, 1)
                updatePhysics = function() end
                update = function() end
            "#,
        )
        .unwrap();
    assert!(populated_runtime.update(0.01).is_err());
}

#[test]
fn light_beam_object_plots_native_segment_path_and_disposes() {
    let runtime = unlocked_test_runtime();
    runtime
        .execute_source(
            r#"
                setLevelLimits(-20, -20, 20, 20)
                clearVertices()
                addVertex(0, -2)
                addVertex(0, 2)
                createLineShape("beam_chain", "", 1, 0, 0, 0, 0, 0, 0, true, false, 1)
                createBox("beam_sensor", "", 2, 0, 1, 1, 1, 0, 0, true, false, 1)
                setAsSensor("beam_sensor", true)
                createBox("beam_disabled", "", 3, 0, 1, 1, 1, 0, 0, true, false, 1)
                setCollisionEnabled("beam_disabled", false)
                createBox("beam_target", "", 5, 0, 2, 2, 1, 0, 0, true, false, 1)
                beam = makeLightBeam()
                beam_missing_query_fails = not pcall(function()
                    return beam:plotPath()
                end)
                beam_query_type_fails = not pcall(function()
                    return beam:plotPath(false)
                end)
                beam_angle_type_fails = not pcall(function()
                    return beam:plotPath({
                        startAngle = "0", startPoint = { x = 0, y = 0 }
                    })
                end)
                beam_point_type_fails = not pcall(function()
                    return beam:plotPath({ startAngle = 0, startPoint = false })
                end)
                beam_coordinate_type_fails = not pcall(function()
                    return beam:plotPath({
                        startAngle = 0, startPoint = { x = "0", y = 0 }
                    })
                end)
                beam_query = { startAngle = 0, startPoint = { x = 0, y = 0 } }
                beam_hit = beam:plotPath(beam_query)
                beam_same_target = beam:plotPath(beam_query)
                beam_target_is_object = beam_query.target == objects.world.beam_disabled
                beam_query.startAngle = math.pi
                beam_target_cleared = beam:plotPath(beam_query)
                beam_nil_unchanged = beam:plotPath(beam_query)
                beam_target_is_nil = beam_query.target == nil
                beam:dispose()
                beam_after_dispose = beam:plotPath(beam_query)
                "#,
        )
        .unwrap();

    let environment = game_environment(runtime.lua()).unwrap();
    for field in [
        "beam_missing_query_fails",
        "beam_query_type_fails",
        "beam_angle_type_fails",
        "beam_point_type_fails",
        "beam_coordinate_type_fails",
    ] {
        assert!(environment.get::<bool>(field).unwrap(), "{field}");
    }
    assert!(environment.get::<bool>("beam_hit").unwrap());
    assert!(!environment.get::<bool>("beam_same_target").unwrap());
    assert!(environment.get::<bool>("beam_target_is_object").unwrap());
    assert!(environment.get::<bool>("beam_target_cleared").unwrap());
    assert!(!environment.get::<bool>("beam_nil_unchanged").unwrap());
    assert!(environment.get::<bool>("beam_target_is_nil").unwrap());
    assert!(!environment.get::<bool>("beam_after_dispose").unwrap());
    let query: mlua::Table = environment.get("beam_query").unwrap();
    let path: mlua::Table = query.get("path").unwrap();
    assert_eq!(path.raw_len(), 4);
}

#[test]
fn light_beam_path_integrates_in_native_float32() {
    let runtime = unlocked_test_runtime();
    runtime
        .execute_source(
            r#"
                setLevelLimits(33554400, 33554439, -20, 20)
                beam = makeLightBeam()
                beam_query = {
                    startAngle = 0,
                    startPoint = { x = 33554432, y = 0 }
                }
                beam_changed = beam:plotPath(beam_query)
                "#,
        )
        .unwrap();

    let environment = game_environment(runtime.lua()).unwrap();
    assert!(!environment.get::<bool>("beam_changed").unwrap());
    let query: mlua::Table = environment.get("beam_query").unwrap();
    let path: mlua::Table = query.get("path").unwrap();
    assert_eq!(path.raw_len(), 2);
    let end: mlua::Table = path.raw_get(2).unwrap();
    assert_eq!(end.get::<f64>("x").unwrap(), 33_554_440.0);
}

#[test]
fn make_ray_replaces_named_body_draw_with_colored_fixture_polygon() {
    let runtime = unlocked_test_runtime();
    runtime
        .execute_source(
            r#"
                createBox("ray_body", "ORIGINAL_SPRITE", 4, 4, 2, 2, 1, 0, 0, true, false, 1)
                makeRay("ray_body", 0.123456789, 0.234567891, 0.345678912, 0.456789123)
                makeRay("ray_body", 1, 1, 1, 1)
                setPosition("ray_body", 40, 50)
                setRotation("ray_body", 1)
                ray_name_rejected = not pcall(makeRay, false, 1, 1, 1, 1)
                ray_red_rejected = not pcall(
                    makeRay, "ray_body", false, 1, 1, 1
                )
                ray_green_rejected = not pcall(
                    makeRay, "ray_body", 1, false, 1, 1
                )
                ray_blue_rejected = not pcall(
                    makeRay, "ray_body", 1, 1, false, 1
                )
                ray_alpha_rejected = not pcall(
                    makeRay, "ray_body", 1, 1, 1, false
                )
                ray_short_rejected = not pcall(
                    makeRay, "ray_body", 1, 1, 1
                )
                missing_ray_ok, missing_ray_error = pcall(
                    makeRay, "missing", 1, 1, 1, 1
                )
                missing_ray_error = tostring(missing_ray_error)
                drawGameNative()
                "#,
        )
        .unwrap();

    let environment = game_environment(runtime.lua()).unwrap();
    for field in [
        "ray_name_rejected",
        "ray_red_rejected",
        "ray_green_rejected",
        "ray_blue_rejected",
        "ray_alpha_rejected",
        "ray_short_rejected",
    ] {
        assert!(environment.get::<bool>(field).unwrap(), "{field}");
    }
    assert!(!environment.get::<bool>("missing_ray_ok").unwrap());
    assert!(
        environment
            .get::<String>("missing_ray_error")
            .unwrap()
            .contains("Missing object: missing")
    );
    let bridge = runtime.render.lock().unwrap();
    let ray = bridge.scene["ray_body"].ray.as_ref().unwrap();
    assert_eq!(
        ray.color,
        [
            f64::from(0.123456789_f64 as f32),
            f64::from(0.234567891_f64 as f32),
            f64::from(0.345678912_f64 as f32),
            f64::from(0.456789123_f64 as f32),
        ]
    );
    assert_eq!((ray.x, ray.y), (4.0, 4.0));
    assert_eq!(
        ray.vertices,
        vec![(-1.0, -1.0), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)]
    );
    assert!(
        bridge
            .commands
            .iter()
            .all(|command| command.sprite != "ORIGINAL_SPRITE")
    );
    assert_eq!(bridge.rect_commands.len(), 1);
    let mesh = &bridge.rect_commands[0];
    // drawGameNative installs +0xAC/+0xB0 in the live context before taking
    // the DrawablePolygon branch at 0x10004C14C. sub_10008D428 consumes that
    // basis, so changing the body rotation after makeRay rotates the retained
    // fixture polygon around its captured (4, 4) position.
    let (sine, cosine) = 1.0_f32.sin_cos();
    let minimum = (80.0_f32 - cosine * 20.0_f32) - sine * 20.0_f32;
    let maximum = (80.0_f32 + cosine * 20.0_f32) + sine * 20.0_f32;
    assert_eq!(
        (mesh.left, mesh.top, mesh.right, mesh.bottom),
        (
            f64::from(minimum),
            f64::from(minimum),
            f64::from(maximum),
            f64::from(maximum),
        )
    );
    assert_eq!(mesh.mesh_topology, ColorMeshTopology::TriangleList);
}
