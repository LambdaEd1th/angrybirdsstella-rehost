use super::*;

#[test]
fn contact_lifecycle_tracks_each_box2d_fixture_pair() {
    let runtime = StellaLua::new("/tmp").unwrap();
    runtime
        .execute_source(
            r#"
                clearVertices()
                addVertex(-2, 0)
                addVertex(0, 0)
                addVertex(2, 0)
                createLineShape("ground", "", 0, 0, 4, 0, 0, 0, 0, true, false, 1)
                createCircle("circle", "", 0, 0.9, 1, 1, 0, 0, true, false, 1)
                setVelocity("circle", 0, -0.5)
                "#,
        )
        .unwrap();

    let mut bridge = runtime.render.lock().unwrap();
    let events = bridge.solve_contacts();
    let begins = events
        .iter()
        .filter(|event| event.first == "circle" && event.second == "ground" && event.began)
        .count();
    assert_eq!(begins, 2);
    assert_eq!(bridge.active_contacts.len(), 2);
    assert!(
        bridge
            .active_contacts
            .contains_key(&("circle".to_owned(), "ground".to_owned(), 0, 0))
    );
    assert!(
        bridge
            .active_contacts
            .contains_key(&("circle".to_owned(), "ground".to_owned(), 0, 1))
    );
}

#[test]
fn fixture_pair_constraints_use_sequential_live_body_velocities() {
    let runtime = StellaLua::new("/tmp").unwrap();
    runtime
        .execute_source(
            r#"
                clearVertices()
                addVertex(-2, 0)
                addVertex(0, 0)
                addVertex(2, 0)
                createLineShape("ground", "", 0, 0, 4, 0, 0, 0, 0, true, false, 1)
                createCircle("circle", "", 0, 0.9, 1, 1, 0, 0, true, false, 1)
                setVelocity("circle", 0, -0.5)
                "#,
        )
        .unwrap();

    let mut bridge = runtime.render.lock().unwrap();
    let events = bridge.solve_contacts();
    assert_eq!(events.iter().filter(|event| event.began).count(), 2);
    assert!(bridge.scene["circle"].velocity_y.abs() < 1e-9);
    let impulses = bridge
        .contact_impulses
        .iter()
        .filter(|((first, second, _, _), _)| first == "circle" && second == "ground")
        .map(|(_, impulse)| impulse.normal)
        .collect::<Vec<_>>();
    assert_eq!(impulses.len(), 2);
    assert_eq!(
        impulses
            .iter()
            .filter(|impulse| **impulse > f64::from(f32::EPSILON))
            .count(),
        1
    );
}

#[test]
fn velocity_iterations_reuse_one_frozen_contact_manager_manifold() {
    let runtime = StellaLua::new("/tmp").unwrap();
    runtime
        .execute_source(
            r#"
                createCircle("mover", "", 0, 0, 1, 1, 0, 0, true, false, 1)
                createCircle("wall", "", 1.5, 0, 1, 0, 0, 0, true, false, 1)
                setWorldGravity(0, 0)
                setVelocity("mover", 1, 0)
                "#,
        )
        .unwrap();

    let mut bridge = runtime.render.lock().unwrap();
    let lifecycle = bridge.refresh_contacts();
    assert!(lifecycle.iter().any(|event| event.began));
    bridge.assemble_box2d_islands();
    bridge.seed_contact_velocity_constraints();
    bridge.begin_contact_step();
    let pair = ("mover".to_owned(), "wall".to_owned(), 0, 0);
    let constraint = bridge.contact_velocity_constraints[&pair];
    assert!(constraint.first.inverse_mass > 0.0);
    assert!(constraint.points[0].normal_mass > 0.0);

    // Moving a body here models a would-be narrow-phase rerun between
    // velocity passes. Clearing the live inverse mass also verifies that
    // every body write still uses the coefficient frozen in the native
    // 152-byte constraint record.
    bridge.scene.get_mut("wall").unwrap().x = 10.0;
    bridge.scene.get_mut("mover").unwrap().inverse_mass = 0.0;
    let impulses = bridge.solve_contact_velocity_constraints_once();
    assert!(impulses[&pair] > 0.0);
    assert!(bridge.scene["mover"].velocity_x.abs() < f64::from(f32::EPSILON));
    assert_eq!(
        bridge.contact_velocity_constraints[&pair].normal,
        constraint.normal
    );
    assert_eq!(
        bridge.contact_velocity_constraints[&pair].points[0].first_radius,
        constraint.points[0].first_radius
    );
    assert!(bridge.active_contacts.contains_key(&pair));

    let next_lifecycle = bridge.refresh_contacts();
    assert!(next_lifecycle.iter().any(|event| event.ended));
    assert!(!bridge.active_contacts.contains_key(&pair));
}

#[test]
fn contact_impulses_publish_only_after_native_store_impulses_member() {
    let runtime = StellaLua::new("/tmp").unwrap();
    runtime
        .execute_source(
            r#"
                createCircle("mover", "", 0, 0, 1, 1, 0, 0, true, false, 1)
                createCircle("wall", "", 1.5, 0, 1, 0, 0, 0, true, false, 1)
                setWorldGravity(0, 0)
                setVelocity("mover", 1, 0)
                "#,
        )
        .unwrap();

    let mut bridge = runtime.render.lock().unwrap();
    bridge.refresh_contacts();
    bridge.assemble_box2d_islands();
    bridge.seed_contact_velocity_constraints();
    bridge.begin_contact_step();
    let pair = ("mover".to_owned(), "wall".to_owned(), 0, 0);
    let manifold_cache_before_solve = bridge.contact_impulses[&pair];

    let solved = bridge.solve_contact_velocity_constraints_once();
    assert!(solved[&pair] > 0.0);
    assert!(bridge.solver_contact_impulses[&pair].normal > 0.0);
    assert_eq!(
        bridge.contact_impulses[&pair].normal,
        manifold_cache_before_solve.normal
    );

    bridge.store_island_contact_impulses(std::slice::from_ref(&pair));
    assert_eq!(
        bridge.contact_impulses[&pair].normal,
        bridge.solver_contact_impulses[&pair].normal
    );
}

#[test]
fn contact_update_aligns_cached_impulses_before_solver_initialization() {
    let runtime = StellaLua::new("/tmp").unwrap();
    runtime
        .execute_source(
            r#"
                createCircle("mover", "", 0, 0, 1, 1, 0, 0, true, false, 1)
                createCircle("wall", "", 1.5, 0, 1, 0, 0, 0, true, false, 1)
                setWorldGravity(0, 0)
                setVelocity("mover", 0.1, 0)
                "#,
        )
        .unwrap();

    let mut bridge = runtime.render.lock().unwrap();
    bridge.refresh_contacts();
    let pair = ("mover".to_owned(), "wall".to_owned(), 0, 0);
    let feature_id = bridge.contact_manifolds[&pair].feature_id;
    bridge.contact_impulses.insert(
        pair.clone(),
        CachedContactImpulse {
            normal: 4.0,
            tangent: -0.5,
            primary_feature_id: feature_id.wrapping_add(1),
            point_count: 1,
            ..CachedContactImpulse::default()
        },
    );

    bridge.scene.get_mut("mover").unwrap().wake();
    bridge.refresh_contacts();

    let aligned = bridge.contact_impulses[&pair];
    assert_eq!(aligned.primary_feature_id, feature_id);
    assert_eq!(aligned.point(0), (0.0, 0.0));
    assert!(bridge.solver_contact_impulses.is_empty());
}

#[test]
fn sensor_contact_update_discards_former_solid_impulses() {
    let runtime = StellaLua::new("/tmp").unwrap();
    runtime
        .execute_source(
            r#"
                createCircle("mover", "", 0, 0, 1, 1, 0, 0, true, false, 1)
                createCircle("wall", "", 1.5, 0, 1, 0, 0, 0, true, false, 1)
                setWorldGravity(0, 0)
                setVelocity("mover", 1, 0)
                "#,
        )
        .unwrap();

    let mut bridge = runtime.render.lock().unwrap();
    bridge.solve_contacts();
    let pair = ("mover".to_owned(), "wall".to_owned(), 0, 0);
    assert!(bridge.contact_impulses[&pair].normal > 0.0);

    bridge.scene.get_mut("wall").unwrap().sensor = true;
    bridge.scene.get_mut("mover").unwrap().wake();
    bridge.refresh_contacts();

    assert_eq!(bridge.active_contacts.get(&pair), Some(&true));
    assert!(!bridge.contact_manifolds.contains_key(&pair));
    assert!(!bridge.contact_impulses.contains_key(&pair));
}

#[test]
fn sleeping_box2d_contact_freezes_until_body_wakes_after_separation() {
    let runtime = StellaLua::new("/tmp").unwrap();
    runtime
        .execute_source(
            r#"
                createCircle("body", "", 0, 0, 1, 1, 0, 0, true, false, 1)
                createCircle("sensor", "", 1.5, 0, 1, 0, 0, 0, true, false, 1)
                setAsSensor("sensor", true)
                setVelocity("body", 0.1, 0)
                "#,
        )
        .unwrap();

    let mut bridge = runtime.render.lock().unwrap();
    let first_events = bridge.solve_contacts();
    assert!(first_events.iter().any(|event| event.began && event.sensor));
    assert_eq!(bridge.active_contacts.len(), 1);

    let body = bridge.scene.get_mut("body").unwrap();
    body.motion_started = false;
    body.sleeping = true;
    let sleeping_events = bridge.solve_contacts();
    assert!(sleeping_events.is_empty());
    assert_eq!(bridge.active_contacts.len(), 1);

    bridge.scene.get_mut("body").unwrap().x = 10.0;
    bridge.sync_native_broad_phase();
    let still_sleeping_events = bridge.solve_contacts();
    assert!(still_sleeping_events.is_empty());
    assert_eq!(bridge.active_contacts.len(), 1);

    let body = bridge.scene.get_mut("body").unwrap();
    body.motion_started = true;
    body.wake();
    let separated_events = bridge.solve_contacts();
    assert!(
        separated_events
            .iter()
            .any(|event| event.ended && event.sensor)
    );
    assert!(bridge.active_contacts.is_empty());
    assert!(!bridge.scene["body"].sleeping);
    assert_eq!(bridge.scene["body"].sleep_time, 0.0);
}

#[test]
fn awake_island_propagates_through_retained_sleeping_contact_chain() {
    let runtime = StellaLua::new("/tmp").unwrap();
    runtime
        .execute_source(
            r#"
                createCircle("a", "", 0, 0, 1, 1, 1, 0, true, false, 1)
                createCircle("b", "", 1.5, 0, 1, 1, 1, 0, true, false, 1)
                createCircle("c", "", 3.0, 0, 1, 1, 1, 0, true, false, 1)
                "#,
        )
        .unwrap();

    let mut bridge = runtime.render.lock().unwrap();
    bridge.refresh_contacts();
    bridge.assemble_box2d_islands();
    assert_eq!(bridge.active_contacts.len(), 2);

    bridge.scene.get_mut("a").unwrap().velocity_x = 1.0;
    bridge.scene.get_mut("b").unwrap().sleeping = true;
    bridge.scene.get_mut("c").unwrap().sleeping = true;
    bridge.refresh_contacts();
    bridge.assemble_box2d_islands();

    assert!(!bridge.scene["b"].sleeping);
    assert!(!bridge.scene["c"].sleeping);
    assert_eq!(bridge.solver_islands.len(), 1);
    assert_eq!(bridge.solver_islands[0].bodies.len(), 3);
    assert_eq!(bridge.velocity_contacts.len(), 2);
}

#[test]
fn shared_static_platform_terminates_and_reenters_native_island_dfs() {
    let runtime = StellaLua::new("/tmp").unwrap();
    runtime
        .execute_source(
            r#"
                createBox("ground", "", 0, -0.5, 8, 1, 0, 0, 0, true, false, 1)
                createCircle("left", "", -2, 0.45, 0.5, 1, 0, 0, true, false, 1)
                createCircle("right", "", 2, 0.45, 0.5, 1, 0, 0, true, false, 1)
                "#,
        )
        .unwrap();

    let mut bridge = runtime.render.lock().unwrap();
    bridge.refresh_contacts();
    bridge.assemble_box2d_islands();

    assert_eq!(bridge.solver_islands.len(), 2);
    // b2World::CreateBody inserts at the world-list head, so the body
    // created last is the first island seed (sub_10086DF90).
    assert_eq!(bridge.solver_islands[0].bodies[0], "right");
    assert_eq!(bridge.solver_islands[1].bodies[0], "left");
    for island in &bridge.solver_islands {
        assert_eq!(island.bodies.len(), 2);
        assert!(island.bodies.iter().any(|name| name == "ground"));
        assert_eq!(island.contacts.len(), 1);
        assert!(island.joints.is_empty());
    }
}
