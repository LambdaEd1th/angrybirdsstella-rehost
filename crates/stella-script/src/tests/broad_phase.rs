use super::*;

#[test]
fn line_proxy_aabb_matches_skinned_independent_edge_bounds() {
    let runtime = unlocked_test_runtime();
    runtime
        .execute_source(
            r#"
                clearVertices()
                addVertex(-1, 2)
                addVertex(3, -4)
                createLineShape("chain", "", 5, 7, 1, 1, 0, 0, 0, true, false, 1)
                "#,
        )
        .unwrap();

    let bridge = runtime.render.lock().unwrap();
    let skin = BOX2D_POLYGON_RADIUS as f32;
    assert_eq!(
        bridge.body_proxy_states["chain"].tight_aabbs,
        [(4.0 - skin, 3.0 - skin, 8.0 + skin, 9.0 + skin)]
    );
    let proxy_id = bridge.scene["chain"].fixture_proxy_ids[0].unwrap();
    assert_eq!(
        bridge.dynamic_tree.proxy_aabb(proxy_id).unwrap(),
        (
            (4.0 - skin) - 0.1,
            (3.0 - skin) - 0.1,
            (8.0 + skin) + 0.1,
            (9.0 + skin) + 0.1,
        )
    );
}

#[test]
fn negative_circle_radius_keeps_native_inverted_proxy_bounds() {
    let runtime = unlocked_test_runtime();
    runtime
        .execute_source(
            r#"
                createCircle("circle", "", 5, -3, 1, 1, 0, 0, true, false, 1)
                native_resizeRadius("circle", -2, 1, 0, 0)
                "#,
        )
        .unwrap();

    let bridge = runtime.render.lock().unwrap();
    assert_eq!(
        bridge.body_proxy_states["circle"].tight_aabbs,
        [(7.0, -1.0, 3.0, -5.0)]
    );
    let proxy_id = bridge.scene["circle"].fixture_proxy_ids[0].unwrap();
    assert_eq!(
        bridge.dynamic_tree.proxy_aabb(proxy_id).unwrap(),
        (6.9, -1.1, 3.1, -4.9)
    );
}

#[test]
fn dynamic_tree_proxy_ids_follow_native_leaf_allocation_and_reuse() {
    fn validate_tree(tree: &NativeDynamicTree, expected_leaves: usize) {
        fn validate_node(
            tree: &NativeDynamicTree,
            node_id: i32,
            expected_parent: i32,
        ) -> (i32, NativeAabb, usize) {
            let node = &tree.nodes[node_id as usize];
            assert_eq!(node.parent, expected_parent);
            assert!(node.height >= 0);
            if node.is_leaf() {
                assert_eq!(node.height, 0);
                assert!(node.user_data.is_some());
                return (0, node.aabb, 1);
            }
            assert!(node.user_data.is_none());
            let (first_height, first_aabb, first_leaves) =
                validate_node(tree, node.child1, node_id);
            let (second_height, second_aabb, second_leaves) =
                validate_node(tree, node.child2, node_id);
            assert_eq!(node.height, 1 + first_height.max(second_height));
            assert_eq!(node.aabb, native_aabb_combine(first_aabb, second_aabb));
            assert!((first_height - second_height).abs() <= 1);
            (node.height, node.aabb, first_leaves + second_leaves)
        }

        let (_, _, leaves) = validate_node(tree, tree.root, -1);
        assert_eq!(leaves, expected_leaves);
        assert_eq!(tree.node_count, expected_leaves * 2 - 1);
        assert_eq!(
            tree.nodes.iter().filter(|node| node.height >= 0).count(),
            tree.node_count
        );
        let mut free_nodes = BTreeSet::new();
        let mut free = tree.free_list;
        while free != -1 {
            assert!(free_nodes.insert(free), "cycle in native free list");
            let node = &tree.nodes[free as usize];
            assert_eq!(node.height, -1);
            free = node.next;
        }
        assert_eq!(free_nodes.len() + tree.node_count, tree.nodes.len());
    }

    let runtime = unlocked_test_runtime();
    runtime
        .execute_source(
            r#"
                createBox("a", "", -4, 0, 1, 1, 1, 0, 0, true, false, 1)
                createBox("b", "", -2, 0, 1, 1, 1, 0, 0, true, false, 1)
                createBox("c", "",  0, 0, 1, 1, 1, 0, 0, true, false, 1)
                "#,
        )
        .unwrap();
    {
        let bridge = runtime.render.lock().unwrap();
        assert_eq!(bridge.scene["a"].fixture_proxy_ids, vec![Some(0)]);
        assert_eq!(bridge.scene["b"].fixture_proxy_ids, vec![Some(1)]);
        assert_eq!(bridge.scene["c"].fixture_proxy_ids, vec![Some(3)]);
        validate_tree(&bridge.dynamic_tree, 3);
    }

    runtime
        .execute_source(
            r#"
                removeObject("b")
                createBox("d", "", 2, 0, 1, 1, 1, 0, 0, true, false, 1)
                "#,
        )
        .unwrap();
    let bridge = runtime.render.lock().unwrap();
    assert_eq!(bridge.scene["d"].fixture_proxy_ids, vec![Some(1)]);
    validate_tree(&bridge.dynamic_tree, 3);
    assert!(bridge.scene["d"].physics_creation_order > bridge.scene["c"].physics_creation_order);
}

#[test]
fn native_body_world_order_tracks_only_live_box2d_records() {
    let runtime = unlocked_test_runtime();
    runtime
        .execute_source(
            r#"
                createBox("older", "", -20, 0, 1, 1, 1, 0, 0, true, false, 1)
                createNonPhysicsObject("visual_only", "", 0, 0, 1)
                createBox("newer", "", 20, 0, 1, 1, 1, 0, 0, true, false, 1)
                "#,
        )
        .unwrap();

    {
        let mut bridge = runtime.render.lock().unwrap();
        assert_eq!(
            bridge
                .native_body_world_order
                .values()
                .rev()
                .map(String::as_str)
                .collect::<Vec<_>>(),
            ["newer", "older"]
        );
        bridge.assemble_box2d_islands();
        assert_eq!(bridge.solver_islands.len(), 2);
        assert_eq!(bridge.solver_islands[0].bodies, ["newer"]);
        assert_eq!(bridge.solver_islands[1].bodies, ["older"]);
        assert_eq!(bridge.solver_synchronized_bodies, ["newer", "older"]);
    }

    runtime.execute_source(r#"removeObject("newer")"#).unwrap();
    let bridge = runtime.render.lock().unwrap();
    assert_eq!(
        bridge
            .native_body_world_order
            .values()
            .map(String::as_str)
            .collect::<Vec<_>>(),
        ["older"]
    );
}

#[test]
fn dynamic_tree_grows_balances_queries_and_reuses_full_node_free_list() {
    let mut tree = NativeDynamicTree::default();
    let mut proxies = Vec::new();
    for index in 0..20 {
        let x = index as f32 * 2.0;
        proxies.push(tree.create_proxy((x, -0.5, x + 1.0, 0.5), (format!("body_{index}"), 0)));
    }
    assert_eq!(tree.node_count, 39);
    assert_eq!(tree.nodes.len(), 64);
    assert!(tree.nodes[tree.root as usize].height <= 6);
    let first_query = tree.query((-1.0, -1.0, 40.0, 1.0));
    assert_eq!(first_query.len(), 20);
    assert_eq!(
        first_query.iter().copied().collect::<BTreeSet<_>>().len(),
        20
    );

    for proxy_id in proxies.iter().copied().skip(1).step_by(2) {
        tree.destroy_proxy(proxy_id);
    }
    assert_eq!(tree.node_count, 19);
    let mut replacements = Vec::new();
    for index in 0..10 {
        let x = index as f32 * 2.0 + 0.25;
        replacements.push(tree.create_proxy(
            (x, -0.25, x + 0.5, 0.25),
            (format!("replacement_{index}"), 0),
        ));
    }
    assert_eq!(tree.node_count, 39);
    assert_eq!(tree.nodes.len(), 64);
    let second_query = tree.query((-1.0, -1.0, 40.0, 1.0));
    assert_eq!(second_query.len(), 20);
    assert_eq!(
        second_query.iter().copied().collect::<BTreeSet<_>>().len(),
        20
    );
    assert!(
        replacements
            .iter()
            .all(|proxy_id| tree.proxy_user_data(*proxy_id).is_some())
    );
}

#[test]
fn dynamic_tree_ray_cast_keeps_native_lifo_candidates_and_segment_culling() {
    let mut tree = NativeDynamicTree::default();
    let near = tree.create_proxy((4.0, -1.0, 6.0, 1.0), ("near".to_owned(), 0));
    let off_axis = tree.create_proxy((4.0, 3.0, 6.0, 5.0), ("off_axis".to_owned(), 0));
    let far = tree.create_proxy((7.0, -1.0, 9.0, 1.0), ("far".to_owned(), 0));

    let candidates = tree.ray_cast_candidates((0.0, 0.0), (10.0, 0.0), 1.0);
    assert_eq!(candidates, vec![far, near]);
    assert!(!candidates.contains(&off_axis));
}

#[test]
fn fat_aabb_creates_contact_before_narrow_phase_begin_contact() {
    let runtime = unlocked_test_runtime();
    runtime
        .execute_source(
            r#"
                createBox("a", "", 0, 0, 1, 1, 1, 0, 0, true, false, 1)
                createBox("b", "", 1.15, 0, 1, 1, 1, 0, 0, true, false, 1)
                "#,
        )
        .unwrap();

    let key = ("a".to_owned(), "b".to_owned(), 0, 0);
    let creation_order = {
        let mut bridge = runtime.render.lock().unwrap();
        let events = bridge.refresh_contacts();
        assert!(events.is_empty());
        assert!(bridge.active_contacts.is_empty());
        assert!(bridge.broad_phase_contacts.contains(&key));
        let order = bridge.contact_creation_order[&key];
        assert_eq!(bridge.native_contact_world_order.get(&order), Some(&key));
        let first = bridge.scene["a"].physics_creation_order;
        let second = bridge.scene["b"].physics_creation_order;
        assert_eq!(bridge.native_contact_body_orders[&order], (first, second));
        assert_eq!(bridge.native_body_contact_edges[&first], [order]);
        assert_eq!(bridge.native_body_contact_edges[&second], [order]);
        order
    };

    let mut bridge = runtime.render.lock().unwrap();
    bridge.scene.get_mut("b").unwrap().x = 1.0;
    bridge.sync_native_broad_phase();
    let events = bridge.refresh_contacts();
    assert!(events.iter().any(|event| event.began));
    assert_eq!(bridge.active_contacts.get(&key), Some(&false));
    assert_eq!(
        bridge.contact_creation_order.get(&key),
        Some(&creation_order)
    );
    assert_eq!(
        bridge.native_contact_world_order.get(&creation_order),
        Some(&key)
    );
}

#[test]
fn contact_manager_destroy_conditionally_wakes_only_touching_contact_endpoints() {
    let runtime = unlocked_test_runtime();
    runtime
        .execute_source(
            r#"
                createCircle("solid_sleeping", "", 0, 0, 1, 1, 0, 0, true, false, 1)
                createCircle("solid_awake", "", 1.5, 0, 1, 1, 0, 0, true, false, 1)

                createCircle("sensor_sleeping", "", 10, 0, 1, 1, 0, 0, true, false, 1)
                setAsSensor("sensor_sleeping", true)
                createCircle("sensor_awake", "", 11.5, 0, 1, 1, 0, 0, true, false, 1)

                -- These live circles are separated, but their expanded proxy
                -- AABBs still overlap and therefore own a non-touching node.
                createCircle("proxy_sleeping", "", 20, 0, 1, 1, 0, 0, true, false, 1)
                createCircle("proxy_awake", "", 22.15, 0, 1, 1, 0, 0, true, false, 1)
                setWorldGravity(0, 0)
            "#,
        )
        .unwrap();

    let mut bridge = runtime.render.lock().unwrap();
    let began = bridge.refresh_contacts();
    assert_eq!(began.iter().filter(|event| event.began).count(), 2);
    assert_eq!(bridge.active_contacts.len(), 2);
    assert_eq!(bridge.broad_phase_contacts.len(), 3);

    for name in ["solid_sleeping", "sensor_sleeping", "proxy_sleeping"] {
        let object = bridge.scene.get_mut(name).unwrap();
        object.sleeping = true;
        object.sleep_time = 0.5;
    }
    for (name, sleep_time) in [
        ("solid_awake", 0.71),
        ("sensor_awake", 0.72),
        ("proxy_awake", 0.73),
    ] {
        let object = bridge.scene.get_mut(name).unwrap();
        object.sleeping = false;
        object.sleep_time = sleep_time;
    }

    // Model the already-completed broad-phase proxy moves by publishing the
    // replacement fat AABBs directly. Collide then retires all three
    // persistent nodes through ContactManager::Destroy without UpdatePairs
    // introducing unrelated swept-path candidates into this focused test.
    for (name, x) in [
        ("solid_awake", 100.0),
        ("sensor_awake", 200.0),
        ("proxy_awake", 300.0),
    ] {
        bridge.body_proxy_states.get_mut(name).unwrap().fat_aabbs[0] =
            (x - 1.0, -1.0, x + 1.0, 1.0);
    }
    let ended = bridge.refresh_contacts();

    assert_eq!(ended.iter().filter(|event| event.ended).count(), 2);
    assert!(ended.iter().any(|event| event.ended && !event.sensor));
    assert!(ended.iter().any(|event| event.ended && event.sensor));
    assert!(bridge.active_contacts.is_empty());
    assert!(bridge.broad_phase_contacts.is_empty());

    // Purple's EndContact listener invokes SetAwake(true) for both touching
    // endpoints. It wakes sleeping bodies, but preserves an already-awake
    // body's accumulated sleep time. The non-touching proxy-only node never
    // reaches the listener at all.
    for name in ["solid_sleeping", "sensor_sleeping"] {
        assert!(!bridge.scene[name].sleeping, "{name}");
        assert_eq!(bridge.scene[name].sleep_time, 0.0, "{name}");
    }
    assert_eq!(bridge.scene["solid_awake"].sleep_time, 0.71);
    assert_eq!(bridge.scene["sensor_awake"].sleep_time, 0.72);
    assert!(bridge.scene["proxy_sleeping"].sleeping);
    assert_eq!(bridge.scene["proxy_sleeping"].sleep_time, 0.5);
    assert_eq!(bridge.scene["proxy_awake"].sleep_time, 0.73);
}

#[test]
fn body_transform_synchronizes_only_its_own_fixture_proxies() {
    let runtime = unlocked_test_runtime();
    runtime
        .execute_source(
            r#"
                createBox("target", "", -4, 0, 1, 1, 1, 0, 0, true, false, 1)
                createBox("unrelated", "", 4, 0, 1, 1, 1, 0, 0, true, false, 1)
                "#,
        )
        .unwrap();

    let mut bridge = runtime.render.lock().unwrap();
    let old_target = bridge.body_proxy_states["target"].tight_aabbs[0];
    let old_unrelated = bridge.body_proxy_states["unrelated"].tight_aabbs[0];
    bridge.scene.get_mut("target").unwrap().x = -3.0;
    bridge.scene.get_mut("unrelated").unwrap().x = 5.0;

    bridge.sync_native_body_broad_phase("target");

    assert_ne!(
        bridge.body_proxy_states["target"].tight_aabbs[0],
        old_target
    );
    assert_eq!(
        bridge.body_proxy_states["unrelated"].tight_aabbs[0],
        old_unrelated
    );
    bridge.sync_native_broad_phase();
    assert_ne!(
        bridge.body_proxy_states["unrelated"].tight_aabbs[0],
        old_unrelated
    );
}

#[test]
fn discrete_solve_synchronizes_only_non_static_bodies_visited_by_an_island() {
    let runtime = unlocked_test_runtime();
    runtime
        .execute_source(
            r#"
                createBox("awake", "", -8, 0, 1, 1, 1, 0, 0, true, false, 1)
                createBox("sleeping", "", 0, 0, 1, 1, 1, 0, 0, true, false, 1)
                createBox("static", "", 8, 0, 1, 1, 0, 0, 0, true, false, 1)
                setWorldGravity(0, 0)
                setVelocity("awake", 3, 0)
                setSleeping("sleeping", true)
                update = function() end
                updatePhysics = function() end
                "#,
        )
        .unwrap();

    let (old_awake, old_sleeping, old_static) = {
        let mut bridge = runtime.render.lock().unwrap();
        let bounds = (
            bridge.body_proxy_states["awake"].tight_aabbs[0],
            bridge.body_proxy_states["sleeping"].tight_aabbs[0],
            bridge.body_proxy_states["static"].tight_aabbs[0],
        );
        // Direct field mutation deliberately bypasses SetTransform. It makes
        // any accidental full-world post-solve synchronization observable.
        bridge.scene.get_mut("sleeping").unwrap().x = 2.0;
        bridge.scene.get_mut("static").unwrap().x = 10.0;
        bounds
    };

    runtime.update(1.0 / 30.0).unwrap();

    let bridge = runtime.render.lock().unwrap();
    assert_ne!(bridge.body_proxy_states["awake"].tight_aabbs[0], old_awake);
    assert_eq!(
        bridge.body_proxy_states["sleeping"].tight_aabbs[0],
        old_sleeping
    );
    assert_eq!(
        bridge.body_proxy_states["static"].tight_aabbs[0],
        old_static
    );
}

#[test]
fn broad_phase_rejects_pairs_without_a_dynamic_body() {
    let runtime = unlocked_test_runtime();
    runtime
        .execute_source(
            r#"
                createBox("static", "", 0, 0, 1, 1, 0, 0, 0, true, false, 1)
                createBox("kinematic", "", 0, 0, 1, 1, 1, 0, 0, true, false, 1)
                setObjectParameter("kinematic", 37, 1)
                "#,
        )
        .unwrap();

    let mut bridge = runtime.render.lock().unwrap();
    assert!(bridge.refresh_contacts().is_empty());
    assert!(bridge.broad_phase_contacts.is_empty());
    assert!(bridge.active_contacts.is_empty());
}

#[test]
fn contact_manager_updates_native_list_head_first() {
    let runtime = unlocked_test_runtime();
    runtime
        .execute_source(
            r#"
                createBox("ground", "", 0, 0, 6, 1, 0, 0, 0, true, false, 1)
                createCircle("a", "", -1.5, 0.8, 0.4, 1, 0, 0, true, false, 1)
                createCircle("b", "",  1.5, 0.8, 0.4, 1, 0, 0, true, false, 1)
                "#,
        )
        .unwrap();

    let mut bridge = runtime.render.lock().unwrap();
    let events = bridge.refresh_contacts();
    let began = events
        .iter()
        .filter(|event| event.began && !event.sensor)
        .collect::<Vec<_>>();
    assert_eq!(began.len(), 2);
    // UpdatePairs creates (groundProxy=0, aProxy=1) before (0, bProxy=3),
    // ContactFactory retains polygon/circle order, and AddPair pushes each
    // contact at the list head. Collide therefore updates ground/b before
    // ground/a.
    assert_eq!(
        (began[0].first.as_str(), began[0].second.as_str()),
        ("ground", "b")
    );
    assert_eq!(
        (began[1].first.as_str(), began[1].second.as_str()),
        ("ground", "a")
    );
}

#[test]
fn contact_factory_uses_proxy_order_and_native_shape_pair_direction() {
    let runtime = unlocked_test_runtime();
    runtime
        .execute_source(
            r#"
                createCircle("a_circle", "", 0, 0, 1, 1, 0, 0, true, false, 1)
                createBox("z_box", "", 0, 0, 1, 1, 1, 0, 0, true, false, 1)

                createBox("z_older_box", "", 10, 0, 1, 1, 1, 0, 0, true, false, 1)
                createBox("a_newer_box", "", 10, 0, 1, 1, 1, 0, 0, true, false, 1)

                createBox("a_edge_target", "", 20, 0, 1, 1, 1, 0, 0, true, false, 1)
                clearVertices()
                addVertex(-1, 0)
                addVertex(1, 0)
                createLineShape("z_edge", "", 20, 0, 2, 0, 0, 0, 0, true, false, 1)

                clearVertices()
                addVertex(-1, 0)
                addVertex(1, 0)
                createLineShape("dynamic_edge", "", 30, 0, 2, 0, 1, 0, 0, true, false, 1)
                clearVertices()
                addVertex(-1, 0)
                addVertex(1, 0)
                createLineShape("static_edge", "", 30, 0, 2, 0, 0, 0, 0, true, false, 1)
                "#,
        )
        .unwrap();

    let mut bridge = runtime.render.lock().unwrap();
    bridge.refresh_contacts();

    // Polygon/circle is registered only with the polygon as fixture A, even
    // though the circle owns the lower proxy id.
    assert!(bridge.broad_phase_contacts.contains(&(
        "z_box".to_owned(),
        "a_circle".to_owned(),
        0,
        0,
    )));
    // Same-type contacts preserve lower-proxy order rather than object-name
    // order.
    assert!(bridge.broad_phase_contacts.contains(&(
        "z_older_box".to_owned(),
        "a_newer_box".to_owned(),
        0,
        0,
    )));
    // Edge/polygon is registered only with the edge as fixture A, even when
    // the polygon was created first.
    assert!(bridge.broad_phase_contacts.contains(&(
        "z_edge".to_owned(),
        "a_edge_target".to_owned(),
        0,
        0,
    )));
    assert!(
        bridge
            .broad_phase_contacts
            .iter()
            .all(|key| key.0 != "dynamic_edge" && key.1 != "dynamic_edge"),
        "Purple's factory has no edge/edge contact registration"
    );
}

#[test]
fn add_pair_wakes_both_sleeping_endpoints_before_sensor_update() {
    let runtime = unlocked_test_runtime();
    runtime
        .execute_source(
            r#"
                createCircle("sleeping_sensor", "", 0, 0, 1, 1, 0, 0, true, false, 1)
                createCircle("sleeping_body", "", 0, 0, 1, 1, 0, 0, true, false, 1)
                setAsSensor("sleeping_sensor", true)
                setSleeping("sleeping_sensor", true)
                setSleeping("sleeping_body", true)

                createCircle("mixed_sensor", "", 10, 0, 1, 1, 0, 0, true, false, 1)
                createCircle("awake_body", "", 10, 0, 1, 1, 0, 0, true, false, 1)
                setAsSensor("mixed_sensor", true)
                setSleeping("mixed_sensor", true)
                "#,
        )
        .unwrap();

    let mut bridge = runtime.render.lock().unwrap();
    bridge.scene.get_mut("sleeping_sensor").unwrap().sleep_time = 0.5;
    bridge.scene.get_mut("sleeping_body").unwrap().sleep_time = 0.6;
    bridge.scene.get_mut("mixed_sensor").unwrap().sleep_time = 0.7;
    bridge.scene.get_mut("awake_body").unwrap().sleep_time = 0.75;
    let events = bridge.refresh_contacts();

    assert_eq!(
        events
            .iter()
            .filter(|event| event.began && event.sensor)
            .count(),
        2
    );
    for name in ["sleeping_sensor", "sleeping_body", "mixed_sensor"] {
        assert!(!bridge.scene[name].sleeping, "{name}");
        assert_eq!(bridge.scene[name].sleep_time, 0.0, "{name}");
    }
    assert!(!bridge.scene["awake_body"].sleeping);
    assert_eq!(bridge.scene["awake_body"].sleep_time, 0.75);
}

#[test]
fn recovered_edge_capsule_rejects_aabb_only_endpoint_overlap() {
    let runtime = unlocked_test_runtime();
    runtime
        .execute_source(
            r#"
                clearVertices()
                addVertex(-1, 0)
                addVertex(1, 0)
                createLineShape("ground", "", 0, 0, 2, 0, 0, 0, 0, true, false, 1)
                createCircle("circle", "", 1.9, 0.9, 1, 1, 0, 0, true, false, 1)
                setVelocity("circle", -0.1, 0)
                "#,
        )
        .unwrap();

    let mut bridge = runtime.render.lock().unwrap();
    let ground_aabb = bridge.scene["ground"].collision_aabb().unwrap();
    let circle_aabb = bridge.scene["circle"].collision_aabb().unwrap();
    assert!(ground_aabb.2 > circle_aabb.0 && ground_aabb.3 > circle_aabb.1);
    assert!(bridge.solve_contacts().is_empty());
}

#[test]
fn dynamic_zero_area_edge_body_uses_native_unit_mass_fallback() {
    let runtime = unlocked_test_runtime();
    runtime
        .execute_source(
            r#"
                clearVertices()
                addVertex(-1, 0)
                addVertex(1, 0)
                createLineShape("edge", "", 0, 0, 2, 0, 1, 0, 0, true, false, 1)
                "#,
        )
        .unwrap();

    let bridge = runtime.render.lock().unwrap();
    assert!(bridge.scene["edge"].dynamic_body);
    assert_eq!(bridge.scene["edge"].inverse_mass, 1.0);
    assert_eq!(bridge.scene["edge"].inverse_inertia(), 0.0);
}

#[test]
fn contact_restitution_uses_recovered_one_unit_velocity_threshold() {
    let runtime = unlocked_test_runtime();
    runtime
        .execute_source(
            r#"
                createCircle("mover", "", 0, 0, 1, 1, 0, 1, true, false, 1)
                createCircle("wall", "", 1.9, 0, 1, 0, 0, 1, true, false, 1)
                setVelocity("mover", 0.5, 0)
                "#,
        )
        .unwrap();

    let mut bridge = runtime.render.lock().unwrap();
    let events = bridge.solve_contacts();
    assert!(events.iter().any(|event| event.impulse > 0.0));
    // sub_100863BC4 only installs restitution bias below -1.0. A slow
    // contact therefore stops instead of bouncing back at restitution 1.
    assert!(bridge.scene["mover"].velocity_x.abs() < f64::from(f32::EPSILON));
}

#[test]
fn new_contact_gets_restitution_bias_before_first_warm_start() {
    let runtime = unlocked_test_runtime();
    runtime
        .execute_source(
            r#"
                createCircle("mover", "", 0, 0, 1, 1, 0, 1, true, false, 1)
                createCircle("wall", "", 1.9, 0, 1, 0, 0, 1, true, false, 1)
                setWorldGravity(0, 0)
                setVelocity("mover", 3, 0)
                update = function() end
                updatePhysics = function() end
                "#,
        )
        .unwrap();

    runtime.update(1.0 / 30.0).unwrap();
    let bridge = runtime.render.lock().unwrap();
    assert!(bridge.scene["mover"].velocity_x < -2.5);
    let pair = ("mover".to_owned(), "wall".to_owned(), 0, 0);
    assert!(bridge.active_contacts.contains_key(&pair));
    // The discrete solver used the restitution bias to produce the reflected
    // velocity above. SolveTOI then constructs a fresh no-warm-start solver;
    // its transient bias cache is not the discrete solver's retained output.
    assert_eq!(bridge.contact_velocity_bias[&pair][0], 0.0_f32);
}
