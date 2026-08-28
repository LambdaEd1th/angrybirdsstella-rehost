use super::*;

#[test]
fn polygon_max_separation_uses_native_centroid_seeded_directional_search() {
    // This valid convex pair has two local separation maxima. A full SAT
    // scan picks edge 2, but sub_10085FB84 seeds from the centroid direction
    // and climbs toward edge 4 without crossing the intervening minimum.
    let reference = [
        (0.967_828_45, 1.182_618_5),
        (0.532_282_1, 2.614_938_5),
        (-0.964_526_4, 2.643_320_6),
        (-1.454_058_5, 1.228_541_5),
        (-0.259_797_54, 0.325_778_04),
    ];
    let incident = [
        (2.444_748_4, -8.019_812),
        (6.982_384, 6.576_342),
        (-7.927_074, 3.207_973_2),
    ];
    let (separation, edge, _) = polygon_max_separation(&reference, &incident).unwrap();
    assert_eq!(edge, 4);
    assert_eq!(separation, f64::from(-6.751_747_6_f32));
}

#[test]
fn polygon_reference_face_uses_native_relative_and_absolute_tolerance() {
    // This shallow rotated overlap lies in the interval where the old
    // ad-hoc +0.0001 comparison selected polygon B, while
    // sub_10085F648's 0.98f*A+0.001f comparison retains polygon A.
    let first = [
        (-0.9645613337590933, -0.6554551345568048),
        (1.0322539816720482, -0.5426340547018801),
        (0.9645613337590933, 0.6554551345568048),
        (-1.0322539816720482, 0.5426340547018801),
    ];
    let second = [
        (-0.9494428060021692, 0.55545994581184),
        (1.0476783075740406, 0.6627318326607218),
        (0.9833151754647115, 1.8610045008064477),
        (-1.0138059381114983, 1.753732613957566),
    ];
    let (separation_a, _, _) = polygon_max_separation(&first, &second).unwrap();
    let (separation_b, _, _) = polygon_max_separation(&second, &first).unwrap();
    assert!(separation_b as f32 > separation_a as f32 + 0.0001_f32);
    assert!(separation_b as f32 <= (separation_a as f32).mul_add(0.98_f32, 0.001_f32));
    let manifold = polygon_manifold(&first, &second).expect("shallow polygon contact");
    assert!(matches!(
        manifold.manifold_type,
        ContactManifoldType::FaceFirst
    ));
}

#[test]
fn polygon_circle_uses_native_float_boundary_and_zero_feature_id() {
    let polygon = [(-1.0, -1.0), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)];
    let radius = 0.5_f32;
    let total_radius = radius + BOX2D_POLYGON_RADIUS as f32;
    let center = (0.0, f64::from(1.0_f32 + total_radius));
    let manifold = circle_polygon_manifold(center, f64::from(radius), &polygon, false)
        .expect("native <= radius boundary contact");
    assert!(matches!(
        manifold.manifold_type,
        ContactManifoldType::FaceFirst
    ));
    assert_eq!(manifold.feature_id, 0);
    assert_eq!((manifold.normal_x, manifold.normal_y), (0.0, 1.0));
    assert_eq!(manifold.penetration, 0.0);

    let outside = (0.0, f64::from((1.0_f32 + total_radius).next_up()));
    assert!(circle_polygon_manifold(outside, f64::from(radius), &polygon, false).is_none());
}

#[test]
fn circle_circle_uses_native_inclusive_boundary_and_small_delta_axis() {
    let touching = circle_circle_manifold((0.0, 0.0), 1.0, (2.0, 0.0), 1.0)
        .expect("native squared-radius boundary contact");
    assert_eq!(touching.feature_id, 0);
    assert_eq!(touching.penetration, 0.0);
    assert_eq!((touching.point_x, touching.point_y), (1.0, 0.0));
    assert!(
        circle_circle_manifold((0.0, 0.0), 1.0, (f64::from(2.0_f32.next_up()), 0.0), 1.0,)
            .is_none()
    );

    let tiny_y = f32::EPSILON * 0.5_f32;
    let near = circle_circle_manifold((0.0, 0.0), 1.0, (0.0, f64::from(tiny_y)), 1.0)
        .expect("sub-epsilon circle contact");
    assert_eq!((near.normal_x, near.normal_y), (1.0, 0.0));
    assert_eq!(near.penetration, 2.0);
    assert_eq!(near.point_y, f64::from(tiny_y * 0.5_f32));
}

#[test]
fn edge_circle_uses_native_regions_features_and_inclusive_radius() {
    let edge = ((-1.0, 0.0), (1.0, 0.0));
    let radius = 0.5_f32;
    let combined = radius + BOX2D_POLYGON_RADIUS as f32;

    let face = circle_segment_manifold((0.0, f64::from(combined)), f64::from(radius), edge, false)
        .expect("native edge face boundary contact");
    assert_eq!((face.normal_x, face.normal_y), (0.0, 1.0));
    assert_eq!(face.penetration, 0.0);
    assert_eq!(face.feature_id, contact_feature_id(0, 0, 1, 0));

    let vertex = circle_segment_manifold(
        (f64::from(-1.0_f32 - combined), 0.0),
        f64::from(radius),
        edge,
        false,
    )
    .expect("native edge vertex boundary contact");
    assert_eq!((vertex.normal_x, vertex.normal_y), (-1.0, 0.0));
    assert_eq!(vertex.penetration, 0.0);
    assert_eq!(vertex.feature_id, contact_feature_id(0, 0, 0, 0));

    let reversed =
        circle_segment_manifold((0.0, f64::from(combined)), f64::from(radius), edge, true)
            .expect("circle-first edge contact");
    assert_eq!((reversed.normal_x, reversed.normal_y), (0.0, -1.0));
    assert_eq!(reversed.feature_id, contact_feature_id(0, 0, 0, 1));
}

#[test]
fn edge_circle_face_distance_uses_native_fused_boundary_sequence() {
    let edge = (
        (f64::from(-11.723_374_f32), f64::from(21.018_469_f32)),
        (f64::from(-10.910_686_5_f32), f64::from(-7.053_440_6_f32)),
    );
    let center = (f64::from(-11.130_099_f32), f64::from(9.055_822_f32));
    let radius = f64::from(0.244_851_32_f32);

    // The former closest-point expression rounds distance² to
    // 0.0609355718 and fabricates a boundary contact. sub_10085E8AC's
    // FNMADD/FMADD sequence yields 0.0609355979, just outside the squared
    // combined radius 0.0609355755.
    assert!(circle_segment_manifold(center, radius, edge, false).is_none());
}

#[test]
fn recovered_two_point_manifold_and_block_solver_balance_face_contact() {
    let first = [(-0.5, 0.25), (0.5, 0.25), (0.5, 1.25), (-0.5, 1.25)];
    let second = [(0.1, -1.0), (1.1, -1.0), (1.1, 1.0), (0.1, 1.0)];
    let manifold = polygon_manifold(&first, &second).expect("overlapping box manifold");
    assert!(manifold.secondary.is_some());
    assert!((manifold.normal_x - 1.0).abs() < 1e-9);
    let feature_ids = [manifold.feature_id, manifold.secondary.unwrap().feature_id];
    assert_eq!(
        feature_ids,
        [
            contact_feature_id(1, 3, 1, 0),
            contact_feature_id(1, 3, 0, 1),
        ]
    );

    let runtime = StellaLua::new("/tmp").unwrap();
    runtime
        .execute_source(
            r#"
                createBox("mover", "", 0, 0.75, 1, 1, 1, 0, 0, true, false, 1)
                createBox("wall", "", 0.6, 0, 1, 2, 0, 0, 0, true, false, 1)
                setVelocity("mover", 6, 0)
                "#,
        )
        .unwrap();
    let mut bridge = runtime.render.lock().unwrap();
    bridge.solve_contacts();
    let pair = ("mover".to_owned(), "wall".to_owned(), 0, 0);
    let constraint = bridge.contact_velocity_constraints[&pair];
    assert_eq!(constraint.point_count, 2);
    assert!(constraint.normal_k.0 > 0.0);
    assert!(constraint.normal_k.2 > 0.0);
    assert!(
        [
            constraint.points[0].first_radius.0,
            constraint.points[0].first_radius.1,
            constraint.points[0].second_radius.0,
            constraint.points[0].second_radius.1,
            constraint.points[0].normal_mass,
            constraint.points[0].tangent_mass,
            constraint.points[0].velocity_bias,
            constraint.points[1].first_radius.0,
            constraint.points[1].first_radius.1,
            constraint.points[1].second_radius.0,
            constraint.points[1].second_radius.1,
            constraint.points[1].normal_mass,
            constraint.points[1].tangent_mass,
            constraint.points[1].velocity_bias,
            constraint.normal_mass.0,
            constraint.normal_mass.1,
            constraint.normal_mass.2,
            constraint.normal_k.0,
            constraint.normal_k.1,
            constraint.normal_k.2,
        ]
        .into_iter()
        .all(f32::is_finite)
    );
    let mover = &bridge.scene["mover"];
    assert!(mover.velocity_x < 6.0);
    assert!(
        mover.angular_velocity.abs() < 1e-9,
        "angular_velocity={}",
        mover.angular_velocity
    );
    let impulse = bridge
        .contact_impulses
        .get(&pair)
        .expect("cached face impulse");
    assert!(impulse.normal > 0.0);
    assert!(impulse.secondary_normal > 0.0);
}

#[test]
fn recovered_edge_polygon_manifold_clips_two_face_points() {
    let polygon = [(-0.5, -0.5), (0.5, -0.5), (0.5, 0.5), (-0.5, 0.5)];
    let manifold = polygon_segment_manifold(&polygon, ((-2.0, 0.501), (2.0, 0.501)), true)
        .expect("box/edge manifold");
    let second = manifold.secondary.expect("second clipped edge point");
    assert!(matches!(
        manifold.manifold_type,
        ContactManifoldType::FaceSecond
    ));
    assert!((manifold.normal_y - 1.0).abs() < 1e-9);
    assert_eq!(
        [manifold.feature_id, second.feature_id],
        [
            contact_feature_id(2, 0, 0, 1),
            contact_feature_id(3, 0, 0, 1),
        ]
    );
    let mut contact_x = [manifold.point_x, second.point_x];
    contact_x.sort_by(f64::total_cmp);
    assert!((contact_x[0] + 0.5).abs() < 1e-9);
    assert!((contact_x[1] - 0.5).abs() < 1e-9);

    let total_radius = (2.0 * BOX2D_POLYGON_RADIUS) as f32;
    let boundary_polygon = [(-0.5, -1.0), (0.5, -1.0), (0.5, 0.0), (-0.5, 0.0)];
    let touching = polygon_segment_manifold(
        &boundary_polygon,
        (
            (-2.0, f64::from(total_radius)),
            (2.0, f64::from(total_radius)),
        ),
        true,
    )
    .expect("native edge-polygon <= total-radius boundary");
    assert_eq!(touching.penetration, 0.0);
    assert!(
        polygon_segment_manifold(
            &boundary_polygon,
            (
                (-2.0, f64::from(total_radius.next_up())),
                (2.0, f64::from(total_radius.next_up())),
            ),
            true,
        )
        .is_none()
    );

    let polygon_axis = polygon_segment_manifold(&polygon, ((0.502, 0.502), (0.504, 0.504)), true)
        .expect("polygon-primary corner contact");
    assert!(matches!(
        polygon_axis.manifold_type,
        ContactManifoldType::FaceFirst
    ));
    assert_eq!((polygon_axis.normal_x, polygon_axis.normal_y), (1.0, 0.0));
}

#[test]
fn narrow_phase_rejects_aabb_only_circle_corner_overlap() {
    let runtime = StellaLua::new("/tmp").unwrap();
    runtime
        .execute_source(
            r#"
                createBox("box", "", 0, 0, 2, 2, 0, 0, 0, true, false, 1)
                createCircle("circle", "", 1.8, 1.8, 1, 1, 0, 0, true, false, 1)
                setVelocity("circle", 0.1, 0)
                "#,
        )
        .unwrap();

    let mut bridge = runtime.render.lock().unwrap();
    let box_aabb = bridge.scene["box"].collision_aabb().unwrap();
    let circle_aabb = bridge.scene["circle"].collision_aabb().unwrap();
    assert!(box_aabb.2 > circle_aabb.0 && box_aabb.3 > circle_aabb.1);
    assert!(bridge.solve_contacts().is_empty());
    assert!(bridge.active_contacts.is_empty());
}

#[test]
fn recovered_edge_fixtures_are_independent_two_sided_capsules() {
    let runtime = StellaLua::new("/tmp").unwrap();
    runtime
        .execute_source(
            r#"
                clearVertices()
                addVertex(-3, 0)
                addVertex(0, 0)
                addVertex(3, 0)
                createLineShape("ground", "", 0, 0, 6, 0, 0, 0, 0, true, false, 1)
                createCircle("above", "", -1, 0.9, 1, 1, 0, 0, true, false, 1)
                createCircle("below", "", 1, -0.9, 1, 1, 0, 0, true, false, 1)
                setVelocity("above", 0, -0.5)
                setVelocity("below", 0, 0.5)
                "#,
        )
        .unwrap();

    let mut bridge = runtime.render.lock().unwrap();
    assert_eq!(bridge.scene["ground"].collision_segments().len(), 2);
    let events = bridge.solve_contacts();
    assert!(events.iter().filter(|event| event.impulse > 0.0).count() >= 2);
    assert!(bridge.scene["above"].velocity_y.abs() < 1e-9);
    assert!(bridge.scene["below"].velocity_y.abs() < 1e-9);
}

#[test]
fn native_contact_factory_does_not_register_edge_edge_pairs() {
    let runtime = StellaLua::new("/tmp").unwrap();
    runtime
        .execute_source(
            r#"
                clearVertices()
                addVertex(-1, 0)
                addVertex(1, 0)
                createLineShape("horizontal", "", 0, 0, 2, 0, 0, 0, 0, true, false, 1)
                clearVertices()
                addVertex(0, -1)
                addVertex(0, 1)
                createLineShape("vertical", "", 0, 0, 0, 2, 1, 0, 0, true, false, 1)
                setWorldGravity(0, 0)
                "#,
        )
        .unwrap();

    let mut bridge = runtime.render.lock().unwrap();
    assert!(
        bridge.scene["horizontal"]
            .collision_fixture_manifolds(&bridge.scene["vertical"])
            .is_empty()
    );
    assert!(bridge.solve_contacts().is_empty());
    assert!(bridge.active_contacts.is_empty());
}

#[test]
fn native_shape_constructor_adapters_are_strict_and_narrow_to_float32() {
    let runtime = StellaLua::new("/tmp").unwrap();
    runtime
        .execute_source(
            r#"
                box_number_type_fails = not pcall(
                    createBox, "bad_box", "", "0", 0, 1, 1,
                    1, 0, 0, true, false, 1
                )
                box_short_fails = not pcall(
                    createBox, "short_box", "", 0, 0, 1, 1,
                    1, 0, 0, true, false
                )
                circle_name_type_fails = not pcall(
                    createCircle, 1, "", 0, 0, 1,
                    1, 0, 0, true, false, 1
                )
                circle_bool_type_fails = not pcall(
                    createCircle, "bad_circle", "", 0, 0, 1,
                    1, 0, 0, 1, false, 1
                )
                clearVertices(); addVertex(-1, 0); addVertex(1, 0)
                line_bool_type_fails = not pcall(
                    createLineShape, "bad_line", "", 0, 0, 2, 0,
                    0, 0, 0, true, 0, 1
                )
                polygon_number_type_fails = not pcall(
                    createPolygon, "bad_polygon", "", 0, 0, 2, 2,
                    1, 0, "0", true, false, 1
                )
                nonphysics_string_type_fails = not pcall(
                    createNonPhysicsObject, "bad_visual", 2, 0, 0, 1
                )
                add_vertex_missing_fails = not pcall(addVertex, 1)
                add_vertex_string_fails = not pcall(addVertex, "1", 2)
                clearVertices()
                addVertex(0.99999999, -0.99999999)
                createPolygon(
                    "rounded_polygon", "", 0, 0, 1, 1,
                    1, 0, 0, true, false, 1
                )

                createBox(
                    "rounded", "SPRITE", 0.99999999, -0.99999999,
                    2.9999999, 3.9999999, 4.9999999,
                    0.19999999, 0.29999999, true, false, 5.9999999
                )
                createNonPhysicsObject(
                    "rounded_visual", "SPRITE", 0.99999999,
                    -0.99999999, 5.9999999
                )
                "#,
        )
        .unwrap();

    let environment = game_environment(runtime.lua()).unwrap();
    for field in [
        "box_number_type_fails",
        "box_short_fails",
        "circle_name_type_fails",
        "circle_bool_type_fails",
        "line_bool_type_fails",
        "polygon_number_type_fails",
        "nonphysics_string_type_fails",
        "add_vertex_missing_fails",
        "add_vertex_string_fails",
    ] {
        assert!(environment.get::<bool>(field).unwrap(), "{field}");
    }
    let world = object_world(runtime.lua()).unwrap();
    for name in [
        "bad_box",
        "short_box",
        "bad_circle",
        "bad_line",
        "bad_polygon",
        "bad_visual",
    ] {
        assert!(matches!(world.raw_get::<Value>(name).unwrap(), Value::Nil));
    }

    let bridge = runtime.render.lock().unwrap();
    let rounded = &bridge.scene["rounded"];
    assert_eq!((rounded.x, rounded.y), (1.0, -1.0));
    assert_eq!(rounded.density, f64::from(4.9999999_f32));
    assert_eq!(rounded.friction, f64::from(0.19999999_f32));
    assert_eq!(rounded.restitution, f64::from(0.29999999_f32));
    assert_eq!(rounded.z_order, f64::from(5.9999999_f32));
    assert!(matches!(
        &bridge.scene["rounded_polygon"].collision_shape,
        CollisionShape::Polygon { vertices, .. }
            if vertices == &vec![(
                f64::from(0.99999999_f32),
                f64::from(-0.99999999_f32)
            )]
    ));
    assert!(matches!(
        rounded.collision_shape,
        CollisionShape::Box { width, height }
            if width == f64::from(2.9999999_f32)
                && height == f64::from(3.9999999_f32)
    ));
    let visual = &bridge.scene["rounded_visual"];
    assert_eq!(
        (visual.x, visual.y, visual.z_order),
        (1.0, -1.0, f64::from(5.9999999_f32))
    );
}
