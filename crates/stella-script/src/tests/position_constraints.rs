use super::*;

#[test]
fn contact_solver_warm_starts_cached_normal_impulse() {
    let runtime = StellaLua::new("/tmp").unwrap();
    runtime
        .execute_source(
            r#"
                clearVertices()
                addVertex(-2, 0)
                addVertex(2, 0)
                createLineShape("ground", "", 0, 0, 4, 0, 0, 0, 0, true, false, 1)
                createCircle("circle", "", 0, 0.9, 1, 1, 0, 0, true, false, 1)
                setVelocity("circle", 0, -0.5)
                "#,
        )
        .unwrap();

    let mut bridge = runtime.render.lock().unwrap();
    bridge.solve_contacts();
    let pair = ("circle".to_owned(), "ground".to_owned(), 0, 0);
    assert!(bridge.contact_impulses[&pair].normal > 0.0);
    bridge.scene.get_mut("circle").unwrap().velocity_y = -0.2;
    bridge.begin_contact_step();
    assert!(bridge.scene["circle"].velocity_y > 0.2);
}

#[test]
fn contact_restitution_bias_uses_one_pre_warm_island_snapshot() {
    let runtime = StellaLua::new("/tmp").unwrap();
    runtime
        .execute_source(
            r#"
                createCircle("a", "", 0, 0, 1, 1, 0, 0, true, false, 1)
                createCircle("b", "", 1.5, 0, 1, 1, 0, 0, true, false, 1)
                createCircle("c", "", 3.0, 0, 1, 1, 0, 0, true, false, 1)
                "#,
        )
        .unwrap();

    let mut bridge = runtime.render.lock().unwrap();
    let first_pair = ("a".to_owned(), "b".to_owned(), 0, 0);
    let second_pair = ("b".to_owned(), "c".to_owned(), 0, 0);
    bridge.refresh_contacts();
    bridge.assemble_box2d_islands();
    let feature_id = contact_feature_id(0, 0, 0, 0);
    bridge.contact_impulses.insert(
        first_pair,
        CachedContactImpulse {
            normal: 20.0,
            primary_feature_id: feature_id,
            point_count: 1,
            ..CachedContactImpulse::default()
        },
    );
    bridge.contact_impulses.insert(
        second_pair.clone(),
        CachedContactImpulse {
            normal: 0.001,
            primary_feature_id: feature_id,
            point_count: 1,
            ..CachedContactImpulse::default()
        },
    );

    bridge.begin_contact_step();
    assert_eq!(bridge.contact_velocity_bias[&second_pair][0], 0.0);
    assert!(bridge.scene["b"].velocity_x > 1.0);
}

#[test]
fn velocity_constraint_drops_ill_conditioned_second_point_before_warm_start() {
    let runtime = StellaLua::new("/tmp").unwrap();
    runtime
        .execute_source(
            r#"
                createBox("a", "", 0, 0, 1, 1, 1, 0, 0, true, false, 1)
                createBox("b", "", 0.5, 0, 1, 1, 1, 0, 0, true, false, 1)
                "#,
        )
        .unwrap();
    let mut bridge = runtime.render.lock().unwrap();
    let point = ContactPoint {
        penetration: 0.1,
        point_x: 0.25,
        point_y: 0.0,
        feature_id: contact_feature_id(0, 0, 1, 0),
    };
    let manifold = ContactManifold {
        manifold_type: ContactManifoldType::FaceFirst,
        normal_x: 1.0,
        normal_y: 0.0,
        penetration: point.penetration,
        point_x: point.point_x,
        point_y: point.point_y,
        feature_id: point.feature_id,
        secondary: Some(ContactPoint {
            feature_id: contact_feature_id(0, 1, 1, 0),
            ..point
        }),
    };
    assert_eq!(
        velocity_contact_points(&bridge.scene["a"], &bridge.scene["b"], manifold).len(),
        1
    );
    let pair = ("a".to_owned(), "b".to_owned(), 0, 0);
    bridge.velocity_contacts.insert(pair.clone(), manifold);
    bridge.contact_impulses.insert(
        pair.clone(),
        CachedContactImpulse {
            normal: 1.0,
            secondary_normal: 2.0,
            primary_feature_id: point.feature_id,
            secondary_feature_id: manifold.secondary.unwrap().feature_id,
            point_count: 2,
            ..CachedContactImpulse::default()
        },
    );
    bridge.begin_contact_step();
    assert_eq!(bridge.contact_velocity_constraints[&pair].point_count, 1);
    assert_eq!(bridge.solver_contact_impulses[&pair].point_count, 1);
    assert_eq!(bridge.solver_contact_impulses[&pair].secondary_normal, 0.0);
}

#[test]
fn polygon_circle_position_constraint_uses_native_circle_center_clip_point() {
    let runtime = StellaLua::new("/tmp").unwrap();
    runtime
        .execute_source(
            r#"
                createBox("box", "", 0, 0, 2, 2, 0, 0, 0, true, false, 1)
                createCircle("circle", "", 0, 1.9, 1, 1, 0, 0, true, false, 1)
                "#,
        )
        .unwrap();
    let bridge = runtime.render.lock().unwrap();
    let first = &bridge.scene["box"];
    let second = &bridge.scene["circle"];
    let manifold = first
        .collision_fixture_manifold(second, 0, 0)
        .expect("box/circle contact");
    let constraint = PositionContactConstraint::from_manifold(first, second, manifold);
    assert_eq!(constraint.point_count(), 1);
    let point = constraint.world_point(first, second, 0).unwrap();
    let circle_center = second.collision_circle().unwrap().0;
    assert!((point.point.0 - circle_center.0 as f32).abs() < 1e-6_f32);
    assert!((point.point.1 - circle_center.1 as f32).abs() < 1e-6_f32);
    assert!((point.separation + manifold.penetration as f32).abs() < 1e-6_f32);
}

#[test]
fn scaled_fixture_witnesses_round_trip_through_the_unscaled_body_transform() {
    let runtime = StellaLua::new("/tmp").unwrap();
    runtime
        .execute_source(
            r#"
                createBox("box", "", 0, 0, 1, 1, 0, 0, 0, true, false, 1)
                setPhysicsScale("box", 2, 2)
                createCircle("circle", "", 1.4, 0, 0.5, 1, 0, 0, true, false, 1)
            "#,
        )
        .unwrap();
    let bridge = runtime.render.lock().unwrap();
    let first = &bridge.scene["box"];
    let second = &bridge.scene["circle"];
    let manifold = first
        .collision_fixture_manifold(second, 0, 0)
        .expect("scaled box/circle contact");
    let constraint = PositionContactConstraint::from_manifold(first, second, manifold);
    let point = constraint.world_point(first, second, 0).unwrap();
    assert!((point.separation + manifold.penetration as f32).abs() < 1.0e-5_f32);
    assert!((point.point.0 - second.x as f32).abs() < 1.0e-6_f32);
}

#[test]
fn coincident_circle_position_manifold_keeps_box2d_zero_normal() {
    let runtime = StellaLua::new("/tmp").unwrap();
    runtime
        .execute_source(
            r#"
                createCircle("a", "", 0, 0, 1, 1, 0, 0, true, false, 1)
                createCircle("b", "", 0, 0, 1, 1, 0, 0, true, false, 1)
                "#,
        )
        .unwrap();
    let bridge = runtime.render.lock().unwrap();
    let first = &bridge.scene["a"];
    let second = &bridge.scene["b"];
    let manifold = first
        .collision_fixture_manifold(second, 0, 0)
        .expect("coincident circle contact");
    let constraint = PositionContactConstraint::from_manifold(first, second, manifold);
    let point = constraint.world_point(first, second, 0).unwrap();
    assert_eq!(point.normal, (0.0_f32, 0.0_f32));
    assert_eq!(point.point, (0.0_f32, 0.0_f32));
    assert_eq!(point.separation, -2.0_f32);
}

#[test]
fn two_point_position_constraint_rebuilds_transform_after_first_impulse() {
    let runtime = StellaLua::new("/tmp").unwrap();
    runtime
        .execute_source(
            r#"
                createBox("mover", "", 0, 0.45, 2, 1, 1, 0, 0, true, false, 1)
                createBox("ground", "", 0, -0.5, 4, 1, 0, 0, 0, true, false, 1)
                setWorldGravity(0, 0)
                setVelocity("mover", 0, -0.1)
                "#,
        )
        .unwrap();

    let mut bridge = runtime.render.lock().unwrap();
    bridge.solve_contacts();
    let pair = ("ground".to_owned(), "mover".to_owned(), 0, 0);
    assert_eq!(bridge.position_contacts[&pair].point_count(), 2);
    bridge.solve_contact_positions();
    assert!(bridge.scene["mover"].angle.abs() > 1e-8);
}

#[test]
fn position_constraint_keeps_constructor_mass_cache_after_live_body_mutation() {
    let runtime = StellaLua::new("/tmp").unwrap();
    runtime
        .execute_source(
            r#"
                createBox("mover", "", 0, 0.45, 2, 1, 1, 0, 0, true, false, 1)
                createBox("ground", "", 0, -0.5, 4, 1, 0, 0, 0, true, false, 1)
                setWorldGravity(0, 0)
                "#,
        )
        .unwrap();

    let mut bridge = runtime.render.lock().unwrap();
    bridge.refresh_contacts();
    bridge.assemble_box2d_islands();
    let pair = bridge
        .position_contacts
        .keys()
        .find(|key| key.0 == "mover" || key.1 == "mover")
        .cloned()
        .expect("mover/ground position constraint");
    let cached = &bridge.position_contacts[&pair];
    assert!(cached.first_inverse_mass > 0.0 || cached.second_inverse_mass > 0.0);

    let before = bridge.scene["mover"].native_world_center();
    bridge.scene.get_mut("mover").unwrap().inverse_mass = 0.0;
    bridge.solve_island_contact_positions(std::slice::from_ref(&pair));
    let after = bridge.scene["mover"].native_world_center();

    assert_ne!(after.1.to_bits(), before.1.to_bits());
}

#[test]
fn position_constraint_reconstructs_transforms_with_cached_local_centers() {
    let runtime = StellaLua::new("/tmp").unwrap();
    runtime
        .execute_source(
            r#"
                createCircle("a", "", 0, 0, 1, 1, 0, 0, true, false, 1)
                createCircle("b", "", 1.5, 0, 1, 1, 0, 0, true, false, 1)
                "#,
        )
        .unwrap();

    let mut bridge = runtime.render.lock().unwrap();
    let manifold = bridge.scene["a"]
        .collision_fixture_manifold(&bridge.scene["b"], 0, 0)
        .expect("circle contact");
    let constraint =
        PositionContactConstraint::from_manifold(&bridge.scene["a"], &bridge.scene["b"], manifold);
    let before = constraint
        .world_point(&bridge.scene["a"], &bridge.scene["b"], 0)
        .unwrap();

    let body = bridge.scene.get_mut("b").unwrap();
    let center = body.native_world_center();
    let angle = body.angle as f32;
    body.fixture_mass_data.1 = (0.25, -0.125);
    body.set_native_sweep_transform(center, angle);
    let after = constraint
        .world_point(&bridge.scene["a"], &bridge.scene["b"], 0)
        .unwrap();

    assert_eq!(after.normal.0.to_bits(), before.normal.0.to_bits());
    assert_eq!(after.normal.1.to_bits(), before.normal.1.to_bits());
    assert_eq!(after.point.0.to_bits(), before.point.0.to_bits());
    assert_eq!(after.point.1.to_bits(), before.point.1.to_bits());
    assert_eq!(after.separation.to_bits(), before.separation.to_bits());
}

#[test]
fn contact_warm_start_resets_impulse_when_box2d_feature_changes() {
    let face = ContactPoint {
        penetration: 0.01,
        point_x: 0.0,
        point_y: 0.0,
        feature_id: contact_feature_id(2, 0, 1, 0),
    };
    let vertex = ContactPoint {
        penetration: 0.01,
        point_x: 0.0,
        point_y: 0.0,
        feature_id: contact_feature_id(3, 0, 0, 0),
    };
    let cached = CachedContactImpulse {
        normal: 4.0,
        tangent: -0.5,
        primary_feature_id: face.feature_id,
        point_count: 1,
        ..CachedContactImpulse::default()
    };

    let retained = cached.aligned_to(&[face]);
    assert_eq!(retained.point(0), (4.0, -0.5));
    let reset = cached.aligned_to(&[vertex]);
    assert_eq!(reset.point(0), (0.0, 0.0));
    assert_eq!(reset.primary_feature_id, vertex.feature_id);
}
