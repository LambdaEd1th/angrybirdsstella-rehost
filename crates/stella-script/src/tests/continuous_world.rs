use super::*;

fn assert_same_manifold(actual: ContactManifold, expected: ContactManifold) {
    assert_eq!(
        std::mem::discriminant(&actual.manifold_type()),
        std::mem::discriminant(&expected.manifold_type())
    );
    assert_eq!(actual.normal_x.to_bits(), expected.normal_x.to_bits());
    assert_eq!(actual.normal_y.to_bits(), expected.normal_y.to_bits());
    assert_eq!(actual.penetration.to_bits(), expected.penetration.to_bits());
    assert_eq!(actual.point_x.to_bits(), expected.point_x.to_bits());
    assert_eq!(actual.point_y.to_bits(), expected.point_y.to_bits());
    assert_eq!(actual.feature_id, expected.feature_id);
    match (actual.secondary, expected.secondary) {
        (Some(actual), Some(expected)) => {
            assert_eq!(actual.penetration.to_bits(), expected.penetration.to_bits());
            assert_eq!(actual.point_x.to_bits(), expected.point_x.to_bits());
            assert_eq!(actual.point_y.to_bits(), expected.point_y.to_bits());
            assert_eq!(actual.feature_id, expected.feature_id);
        }
        (None, None) => {}
        pair => panic!("secondary contact mismatch: {pair:?}"),
    }
}

#[test]
fn toi_fixture_transform_matches_a_temporary_body_pose_without_cloning_it() {
    let runtime = unlocked_test_runtime();
    runtime
        .execute_source(
            r#"
                createBox("moving", "", 0, 0, 1, 1, 1, 0, 0, true, false, 1)
                createCircle("fixed", "", 0.58, 0.1, 0.22, 1, 0, 0, true, false, 1)
                "#,
        )
        .unwrap();

    let bridge = runtime.render.lock().unwrap();
    let moving = &bridge.scene["moving"];
    let fixed = &bridge.scene["fixed"];
    let center = (0.08_f32, -0.03_f32);
    let angle = 0.27_f32;

    // This is the old rehost boundary: cloning the complete scene object and
    // changing its sweep pose solely to obtain fixture geometry.
    let expected = {
        let mut temporary = moving.clone();
        temporary.set_native_sweep_transform(center, angle);
        temporary
            .collision_fixture_manifold(fixed, 0, 0)
            .expect("temporary pose must overlap")
    };
    let actual = moving
        .collision_fixture_manifold_at_transforms(
            fixed,
            0,
            0,
            moving.native_collision_transform_at_sweep(center, angle),
            fixed.native_collision_transform(),
        )
        .expect("compact transform must overlap");
    assert_same_manifold(actual, expected);
}

#[test]
fn continuous_step_stops_a_fast_circle_at_a_static_thin_edge() {
    let runtime = unlocked_test_runtime();
    runtime
        .execute_source(
            r#"
                clearVertices()
                addVertex(0, -1)
                addVertex(0, 1)
                createLineShape("wall", "", 0, 0, 0, 2, 0, 0, 0, true, false, 1)
                createCircle("body", "", -0.08, 0, 0.01, 1, 0, 0, true, false, 1)
                createCircle("unrelated", "", 10, 0, 0.01, 1, 0, 0, true, false, 1)
                "#,
        )
        .unwrap();

    let mut bridge = runtime.render.lock().unwrap();
    let sweep_starts = bridge
        .scene
        .iter()
        .map(|(name, object)| (name.clone(), NativeSweepStart::capture(object)))
        .collect();
    {
        let body = bridge.scene.get_mut("body").unwrap();
        body.velocity_x = f64::from(4.8_f32);
        body.velocity_y = 0.0;
        body.motion_started = true;
        body.sleeping = false;
        body.apply_native_position_delta(0.16_f32, 0.0, 0.0);
    }
    assert!(
        bridge.scene["body"]
            .collision_fixture_manifold(&bridge.scene["wall"], 0, 0)
            .is_none()
    );
    bridge.sync_native_broad_phase();
    let old_unrelated_aabb = bridge.body_proxy_states["unrelated"].tight_aabbs[0];
    // This body is active but is not part of the selected TOI island. Native
    // SolveTOI therefore leaves its proxy untouched in the post-island loop.
    bridge.scene.get_mut("unrelated").unwrap().x = 12.0;
    let key = ("body".to_owned(), "wall".to_owned(), 0, 0);
    assert!(bridge.broad_phase_contacts.contains(&key));

    let mut toi_state = NativeToiStepState::default();
    let pending = bridge
        .advance_continuous_tunneling(&sweep_starts, &BTreeMap::new(), &mut toi_state)
        .unwrap();
    assert_eq!(pending.len(), 1);
    assert_eq!(pending[0].0.key, key);
    assert_eq!(toi_state.counts.get(&key), Some(&1));
    assert!(toi_state.cached_world_alphas.contains_key(&key));
    assert!(pending[0].1.began);
    assert!(bridge.finish_toi_contact_update(&pending[0].0));
    // totalRadius=0.012 and Purple targets totalRadius-3*0.001,
    // so the core centre reaches 0.009 at alpha=(0.08-0.009)/0.16.
    assert!(
        (pending[0].0.alpha - 0.443_75_f32).abs() < 0.001_f32,
        "alpha={}",
        pending[0].0.alpha
    );
    let contacts = pending
        .iter()
        .map(|(contact, _)| contact.clone())
        .collect::<Vec<_>>();
    let (impulses, _, _) =
        bridge.finish_continuous_tunneling(&contacts, 1.0 / 30.0, 10, 0.16, 15_708.0 / 10_000.0);

    assert!(impulses[&key] > 0.0);
    assert!(bridge.solver_contact_impulses[&key].normal > 0.0);
    assert!(
        !bridge.contact_impulses.contains_key(&key),
        "b2Island::SolveTOI does not call StoreImpulses"
    );
    // SolveTOIPositionConstraints runs its 0.75 correction before the
    // velocity solve, moving the centre from -0.009 to about -0.010875.
    assert!((bridge.scene["body"].x + 0.010_875).abs() < 0.0002);
    assert!(bridge.scene["body"].velocity_x.abs() < 1e-6);
    assert!(bridge.active_contacts.contains_key(&key));
    assert_eq!(
        bridge.body_proxy_states["body"].tight_aabbs[0],
        bridge.scene["body"].collision_fixture_aabbs()[0]
    );
    assert_eq!(
        bridge.body_proxy_states["unrelated"].tight_aabbs[0],
        old_unrelated_aabb
    );
}

#[test]
fn solve_toi_completes_above_the_native_near_one_alpha_boundary() {
    let runtime = unlocked_test_runtime();
    runtime
        .execute_source(
            r#"
                clearVertices()
                addVertex(0, -1)
                addVertex(0, 1)
                createLineShape("wall", "", 0.15, 0, 0, 2, 0, 0, 0, true, false, 1)
                createCircle("body", "", 0, 0, 0.01, 1, 0, 0, true, false, 1)
                "#,
        )
        .unwrap();

    let mut bridge = runtime.render.lock().unwrap();
    let sweep_starts = bridge
        .scene
        .iter()
        .map(|(name, object)| (name.clone(), NativeSweepStart::capture(object)))
        .collect();
    {
        let body = bridge.scene.get_mut("body").unwrap();
        body.motion_started = true;
        body.sleeping = false;
        body.apply_native_position_delta(0.16_f32, 0.0, 0.0);
    }
    bridge.sync_native_broad_phase();
    let key = ("body".to_owned(), "wall".to_owned(), 0, 0);
    assert!(bridge.broad_phase_contacts.contains(&key));

    let mut toi_state = NativeToiStepState::default();
    toi_state
        .cached_world_alphas
        .insert(key.clone(), f32::from_bits(0x3F7F_FFED));
    assert!(
        bridge
            .advance_continuous_tunneling(&sweep_starts, &BTreeMap::new(), &mut toi_state)
            .is_none(),
        "Purple completes SolveTOI one float above its 0x3F7FFFEC boundary"
    );
    assert!(!toi_state.counts.contains_key(&key));
    assert_eq!(bridge.scene["body"].x, f64::from(0.16_f32));

    toi_state
        .cached_world_alphas
        .insert(key.clone(), f32::from_bits(0x3F7F_FFEC));
    let pending = bridge
        .advance_continuous_tunneling(&sweep_starts, &BTreeMap::new(), &mut toi_state)
        .expect("Purple processes an alpha exactly on its completion boundary");
    assert_eq!(pending[0].0.alpha.to_bits(), 0x3F7F_FFEC);
    assert_eq!(toi_state.counts.get(&key), Some(&1));
}

#[test]
fn selected_toi_contact_disabled_by_its_callback_restores_both_sweeps() {
    let runtime = unlocked_test_runtime();
    runtime
        .execute_source(
            r#"
                clearVertices()
                addVertex(0, -1)
                addVertex(0, 1)
                createLineShape("wall", "", 0.05, 0, 0, 2, 0, 0, 0, true, false, 1)
                createCircle("body", "", 0, 0, 0.01, 1, 0, 0, true, false, 1)
                setWorldGravity(0, 0)
                setVelocity("body", 4.8, 0)
                objects.world.body.strength = 100
                objects.world.body.defence = 0
                objects.world.wall.strength = 100
                objects.world.wall.defence = 0
                scoreTable = { blocks = { score = 0 } }
                worldAttributes = { scoreDamageMultiplier = 1 }
                toi_blocks = 0
                blockCollision = function()
                    toi_blocks = toi_blocks + 1
                    setVelocity("body", -4.8, 0)
                    setCollisionEnabled("body", false)
                end
                update = function() end
                updatePhysics = function() end
                "#,
        )
        .unwrap();

    runtime.update(1.0 / 30.0).unwrap();

    assert_eq!(
        game_environment(runtime.lua())
            .unwrap()
            .get::<i64>("toi_blocks")
            .unwrap(),
        1
    );
    let bridge = runtime.render.lock().unwrap();
    let body = &bridge.scene["body"];
    assert!(body.x > 0.15 && body.x < 0.17, "restored x={}", body.x);
    assert!((body.velocity_x + 4.8).abs() < 1e-6);
    assert!(!body.collision_enabled);
    assert!(bridge.active_contacts.is_empty());
}

#[test]
fn native_toi_advances_and_integrates_a_selected_kinematic_endpoint() {
    let runtime = unlocked_test_runtime();
    runtime
        .execute_source(
            r#"
                clearVertices()
                addVertex(0, -1)
                addVertex(0, 1)
                createLineShape("wall", "", 0, 0, 0, 2, 0, 0, 0, true, false, 1)
                setObjectParameter("wall", 37, 1)
                createCircle("body", "", -0.08, 0, 0.01, 1, 0, 0, true, false, 1)
                setWorldGravity(0, 0)
                setVelocity("wall", 0.03, 0)
                "#,
        )
        .unwrap();

    let mut bridge = runtime.render.lock().unwrap();
    assert!(bridge.scene["wall"].kinematic_body);
    let sweep_starts = bridge
        .scene
        .iter()
        .map(|(name, object)| (name.clone(), NativeSweepStart::capture(object)))
        .collect();
    {
        let body = bridge.scene.get_mut("body").unwrap();
        body.velocity_x = f64::from(4.8_f32);
        body.motion_started = true;
        body.sleeping = false;
        body.apply_native_position_delta(0.16_f32, 0.0, 0.0);
    }
    bridge.sync_native_broad_phase();
    assert!(
        bridge
            .broad_phase_contacts
            .iter()
            .any(|key| key.0 == "body" && key.1 == "wall"),
        "contacts={:?} body_proxy={:?} wall_proxy={:?}",
        bridge.broad_phase_contacts,
        bridge.body_proxy_states["body"],
        bridge.body_proxy_states["wall"]
    );

    let pending = bridge
        .advance_continuous_tunneling(
            &sweep_starts,
            &BTreeMap::new(),
            &mut NativeToiStepState::default(),
        )
        .expect("dynamic/kinematic pair must enter Purple's TOI candidate path");
    assert_eq!(
        pending[0].0.toi_bodies,
        ("body".to_owned(), "wall".to_owned())
    );
    assert!(bridge.finish_toi_contact_update(&pending[0].0));
    let wall_at_impact = bridge.scene["wall"].x;
    let wall_proxy_at_impact = bridge.body_proxy_states["wall"].tight_aabbs[0];
    let contacts = pending
        .iter()
        .map(|(contact, _)| contact.clone())
        .collect::<Vec<_>>();
    let (_, sweep_starts, cache_invalidation_bodies) =
        bridge.finish_continuous_tunneling(&contacts, 1.0 / 30.0, 10, 0.16, 15_708.0 / 10_000.0);

    assert!(sweep_starts.contains_key("wall"));
    assert_eq!(cache_invalidation_bodies, ["body"]);
    assert!(bridge.scene["wall"].x > wall_at_impact);
    assert_eq!(
        bridge.body_proxy_states["wall"].tight_aabbs[0], wall_proxy_at_impact,
        "Purple's post-TOI proxy loop synchronizes only dynamic bodies"
    );
}

#[test]
fn native_toi_accepts_a_bullet_dynamic_dynamic_pair() {
    let runtime = unlocked_test_runtime();
    runtime
        .execute_source(
            r#"
                createCircle("bullet", "", -0.08, 0, 0.01, 1, 0, 0, true, false, 1)
                createCircle("target", "", 0, 0, 0.01, 1, 0, 0, true, false, 1)
                setWorldGravity(0, 0)
                "#,
        )
        .unwrap();

    let mut bridge = runtime.render.lock().unwrap();
    bridge.scene.get_mut("bullet").unwrap().bullet = true;
    let sweep_starts = bridge
        .scene
        .iter()
        .map(|(name, object)| (name.clone(), NativeSweepStart::capture(object)))
        .collect();
    {
        let bullet = bridge.scene.get_mut("bullet").unwrap();
        bullet.velocity_x = f64::from(4.8_f32);
        bullet.motion_started = true;
        bullet.sleeping = false;
        bullet.apply_native_position_delta(0.16_f32, 0.0, 0.0);
    }
    bridge.sync_native_broad_phase();

    let pending = bridge
        .advance_continuous_tunneling(
            &sweep_starts,
            &BTreeMap::new(),
            &mut NativeToiStepState::default(),
        )
        .expect("a bullet is eligible for Purple's dynamic/dynamic TOI path");
    assert_eq!(pending.len(), 1);
    assert_eq!(
        pending[0].0.toi_bodies,
        ("bullet".to_owned(), "target".to_owned())
    );
    assert!(bridge.finish_toi_contact_update(&pending[0].0));
    assert!(bridge.scene["target"].motion_started);
    let contacts = pending
        .iter()
        .map(|(contact, _)| contact.clone())
        .collect::<Vec<_>>();
    let (_, next_sweeps, cache_invalidation_bodies) =
        bridge.finish_continuous_tunneling(&contacts, 1.0 / 30.0, 10, 0.16, 15_708.0 / 10_000.0);

    assert!(next_sweeps.contains_key("bullet"));
    assert!(next_sweeps.contains_key("target"));
    assert_eq!(cache_invalidation_bodies, ["bullet", "target"]);
    assert!(bridge.scene["target"].velocity_x > 0.0);
}

#[test]
fn toi_auxiliary_walk_rolls_back_a_kinematic_body_rejected_by_its_callback() {
    let runtime = unlocked_test_runtime();
    runtime
        .execute_source(
            r#"
                clearVertices()
                addVertex(0, -1)
                addVertex(0, 1)
                createLineShape("static", "", 0.05, 0, 0, 2, 0, 0, 0, true, false, 1)
                createLineShape("moving", "", 0.05, 0, 0, 2, 0, 0, 0, true, false, 1)
                setObjectParameter("moving", 37, 1)
                createCircle("body", "", 0, 0, 0.01, 1, 0, 0, true, false, 1)
                "#,
        )
        .unwrap();

    let mut bridge = runtime.render.lock().unwrap();
    let sweep_starts = bridge
        .scene
        .iter()
        .map(|(name, object)| (name.clone(), NativeSweepStart::capture(object)))
        .collect();
    bridge
        .scene
        .get_mut("body")
        .unwrap()
        .apply_native_position_delta(0.04_f32, 0.0, 0.0);
    bridge
        .scene
        .get_mut("moving")
        .unwrap()
        .apply_native_position_delta(-0.05_f32, 0.0, 0.0);
    bridge.sync_native_broad_phase();
    let selected_key = ("body".to_owned(), "static".to_owned(), 0, 0);
    let auxiliary = bridge
        .advance_next_toi_auxiliary_contact(
            ("body", "static"),
            0.25_f32,
            &[selected_key],
            &sweep_starts,
            &BTreeMap::new(),
        )
        .expect("the kinematic contact must be evaluated at the island alpha");

    assert_eq!(
        auxiliary.0.key,
        ("body".to_owned(), "moving".to_owned(), 0, 0)
    );
    assert!((bridge.scene["moving"].x - 0.0375).abs() < 1e-6);
    assert!(bridge.scene["moving"].motion_started);
    bridge.scene.get_mut("moving").unwrap().collision_enabled = false;
    assert!(!bridge.finish_toi_contact_update(&auxiliary.0));
    assert!(bridge.scene["moving"].x.abs() < 1e-6);
}

#[test]
fn toi_auxiliary_walk_restores_a_rejected_kinematic_body_sweep() {
    let runtime = unlocked_test_runtime();
    runtime
        .execute_source(
            r#"
                clearVertices()
                addVertex(0, -1)
                addVertex(0, 1)
                createLineShape("static", "", 0.05, 0, 0, 2, 0, 0, 0, true, false, 1)
                clearVertices()
                addVertex(-1, 0)
                addVertex(1, 0)
                createLineShape("moving", "", 0, 0.05, 2, 0, 0, 0, 0, true, false, 1)
                setObjectParameter("moving", 37, 1)
                createCircle("body", "", 0, 0, 0.01, 1, 0, 0, true, false, 1)
                "#,
        )
        .unwrap();

    let mut bridge = runtime.render.lock().unwrap();
    let sweep_starts = bridge
        .scene
        .iter()
        .map(|(name, object)| (name.clone(), NativeSweepStart::capture(object)))
        .collect();
    bridge
        .scene
        .get_mut("body")
        .unwrap()
        .apply_native_position_delta(0.04_f32, 0.0, 0.0);
    bridge
        .scene
        .get_mut("moving")
        .unwrap()
        .apply_native_position_delta(0.0, -0.1_f32, 0.0);
    bridge.sync_native_broad_phase();
    let rejected_end = NativeSweepStart::capture(&bridge.scene["moving"]);
    let selected_key = ("body".to_owned(), "static".to_owned(), 0, 0);
    assert!(
        bridge
            .advance_next_toi_auxiliary_contact(
                ("body", "static"),
                0.25_f32,
                &[selected_key],
                &sweep_starts,
                &BTreeMap::new(),
            )
            .is_none()
    );

    assert_eq!(
        NativeSweepStart::capture(&bridge.scene["moving"]).center,
        rejected_end.center
    );
}

#[test]
fn equal_toi_candidates_retain_the_native_contact_list_head() {
    let runtime = unlocked_test_runtime();
    runtime
        .execute_source(
            r#"
                clearVertices()
                addVertex(0, -1)
                addVertex(0, 1)
                createLineShape("a_old", "", 0, 0, 0, 2, 0, 0, 0, true, false, 1)
                clearVertices()
                addVertex(0, -1)
                addVertex(0, 1)
                createLineShape("z_new", "", 0, 0, 0, 2, 0, 0, 0, true, false, 1)
                createCircle("body", "", -0.08, 0, 0.01, 1, 0, 0, true, false, 1)
                "#,
        )
        .unwrap();

    let mut bridge = runtime.render.lock().unwrap();
    let sweep_starts = bridge
        .scene
        .iter()
        .map(|(name, object)| (name.clone(), NativeSweepStart::capture(object)))
        .collect();
    {
        let body = bridge.scene.get_mut("body").unwrap();
        body.motion_started = true;
        body.sleeping = false;
        body.apply_native_position_delta(0.16_f32, 0.0, 0.0);
    }
    bridge.sync_native_broad_phase();
    let expected = bridge
        .native_contact_world_order
        .iter()
        .rev()
        .map(|(_, key)| key)
        .find(|key| key.0 == "body" || key.1 == "body")
        .cloned()
        .expect("body must have a native contact-list head");
    assert!(expected.0 == "z_new" || expected.1 == "z_new");

    let pending = bridge
        .advance_continuous_tunneling(
            &sweep_starts,
            &BTreeMap::new(),
            &mut NativeToiStepState::default(),
        )
        .expect("coincident edges must produce one selected TOI contact");
    assert_eq!(pending[0].0.key, expected);
}

#[test]
fn toi_exactly_at_the_step_end_stays_in_the_discrete_solver() {
    let runtime = unlocked_test_runtime();
    runtime
        .execute_source(
            r#"
                clearVertices()
                addVertex(0, -1)
                addVertex(0, 1)
                createLineShape("wall", "", 0, 0, 0, 2, 0, 0, 0, true, false, 1)
                createCircle("body", "", -0.08, 0, 0.01, 1, 0, 0, true, false, 1)
                "#,
        )
        .unwrap();

    let mut bridge = runtime.render.lock().unwrap();
    let sweep_starts = bridge
        .scene
        .iter()
        .map(|(name, object)| (name.clone(), NativeSweepStart::capture(object)))
        .collect();
    {
        let body = bridge.scene.get_mut("body").unwrap();
        body.motion_started = true;
        body.sleeping = false;
        // The core separation at the endpoint lies in Purple's target band,
        // so b2TimeOfImpact returns exactly 1.0.
        body.apply_native_position_delta(0.070_9_f32, 0.0, 0.0);
    }
    bridge.sync_native_broad_phase();
    let pending = bridge.advance_continuous_tunneling(
        &sweep_starts,
        &BTreeMap::new(),
        &mut NativeToiStepState::default(),
    );
    assert!(pending.is_none());
}

#[test]
fn shallow_existing_contact_still_enters_native_toi_without_a_second_begin() {
    let runtime = unlocked_test_runtime();
    runtime
        .execute_source(
            r#"
                clearVertices()
                addVertex(0, -1)
                addVertex(0, 1)
                createLineShape("wall", "", 0, 0, 0, 2, 0, 0, 0, true, false, 1)
                createCircle("body", "", -0.0115, 0, 0.01, 1, 0, 0, true, false, 1)
                "#,
        )
        .unwrap();

    let mut bridge = runtime.render.lock().unwrap();
    bridge.sync_native_broad_phase();
    let key = ("body".to_owned(), "wall".to_owned(), 0, 0);
    let initial = bridge.refresh_contacts();
    assert!(initial.iter().any(|event| event.began));
    assert!(bridge.active_contacts.contains_key(&key));
    let sweep_starts = bridge
        .scene
        .iter()
        .map(|(name, object)| (name.clone(), NativeSweepStart::capture(object)))
        .collect();
    {
        let body = bridge.scene.get_mut("body").unwrap();
        body.motion_started = true;
        body.sleeping = false;
        body.apply_native_position_delta(0.01_f32, 0.0, 0.0);
    }
    bridge.sync_native_broad_phase();

    let pending = bridge
        .advance_continuous_tunneling(
            &sweep_starts,
            &BTreeMap::new(),
            &mut NativeToiStepState::default(),
        )
        .expect("shallow touching contact must still reach SolveTOI");
    assert_eq!(pending[0].0.key, key);
    assert!(!pending[0].1.began);
    assert!(pending[0].0.alpha > 0.0_f32);
    assert!(pending[0].0.alpha < 1.0_f32);
}

#[test]
fn toi_position_constraint_keeps_constructor_mass_cache() {
    let runtime = unlocked_test_runtime();
    runtime
        .execute_source(
            r#"
                clearVertices()
                addVertex(0, -1)
                addVertex(0, 1)
                createLineShape("wall", "", 0, 0, 0, 2, 0, 0, 0, true, false, 1)
                createCircle("body", "", -0.005, 0, 0.01, 1, 0, 0, true, false, 1)
                "#,
        )
        .unwrap();

    let mut bridge = runtime.render.lock().unwrap();
    let key = ("body".to_owned(), "wall".to_owned(), 0, 0);
    let manifold = bridge.scene["body"]
        .collision_fixture_manifold(&bridge.scene["wall"], 0, 0)
        .expect("overlapping circle/edge manifold");
    let constraint = PositionContactConstraint::from_manifold(
        &bridge.scene["body"],
        &bridge.scene["wall"],
        manifold,
    );
    assert!(constraint.first_inverse_mass > 0.0);
    let contact = NativeToiContact {
        key,
        toi_bodies: ("body".to_owned(), "wall".to_owned()),
        alpha: 0.5,
        manifold,
        rollback_poses: Vec::new(),
    };

    let before = bridge.scene["body"].native_world_center();
    bridge.scene.get_mut("body").unwrap().inverse_mass = 0.0;
    bridge.solve_toi_position_constraint(&contact, &constraint);
    let after = bridge.scene["body"].native_world_center();

    assert_ne!(after.0.to_bits(), before.0.to_bits());
}

#[test]
fn native_time_of_impact_uses_purple_target_for_point_separation() {
    let moving = NativeDistanceProxy {
        vertices: vec![(0.0, 0.0)],
        radius: 0.01,
    };
    let fixed = NativeDistanceProxy {
        vertices: vec![(0.0, 0.0)],
        radius: 0.02,
    };
    let moving_sweep = NativeSweep {
        local_center: (0.0, 0.0),
        center_0: (-0.1, 0.0),
        center: (0.1, 0.0),
        angle_0: 0.0,
        angle: 0.0,
    };
    let fixed_sweep = NativeSweep {
        local_center: (0.0, 0.0),
        center_0: (0.0, 0.0),
        center: (0.0, 0.0),
        angle_0: 0.0,
        angle: 0.0,
    };

    // target=max(0.001, 0.03-3*0.001)=0.027, hence
    // alpha=(0.1-0.027)/0.2=0.365 before the two shape radii overlap.
    let output = native_time_of_impact(&moving, moving_sweep, &fixed, fixed_sweep);
    assert_eq!(output.state, NativeToiState::Touching);
    assert!(
        (output.alpha - 0.365_f32).abs() < 0.001_f32,
        "alpha={}",
        output.alpha
    );

    let moving_away = NativeSweep {
        center: (-0.2, 0.0),
        ..moving_sweep
    };
    assert_eq!(
        native_time_of_impact(&moving, moving_away, &fixed, fixed_sweep).state,
        NativeToiState::Separated
    );
}

#[test]
fn continuous_world_rescans_after_toi_bounce_for_a_second_edge() {
    let runtime = unlocked_test_runtime();
    runtime
        .execute_source(
            r#"
                clearVertices()
                addVertex(0, -1)
                addVertex(0, 1)
                createLineShape("left", "", -0.05, 0, 0, 2, 0, 0, 1, true, false, 1)
                createLineShape("right", "", 0.05, 0, 0, 2, 0, 0, 1, true, false, 1)
                createCircle("body", "", 0, 0, 0.01, 1, 0, 1, true, false, 1)
                setWorldGravity(0, 0)
                setVelocity("body", 4.8, 0)
                update = function() end
                updatePhysics = function() end
                "#,
        )
        .unwrap();

    runtime.update(1.0 / 30.0).unwrap();
    let bridge = runtime.render.lock().unwrap();
    let left = ("body".to_owned(), "left".to_owned(), 0, 0);
    let right = ("body".to_owned(), "right".to_owned(), 0, 0);
    assert!(bridge.active_contacts.contains_key(&right));
    assert!(bridge.active_contacts.contains_key(&left));
    assert!(bridge.scene["body"].x > -0.041_f64);
    assert!(bridge.scene["body"].x < 0.041_f64);
    assert!(bridge.scene["body"].velocity_x > 0.0);
}

#[test]
fn toi_island_adds_both_static_contacts_at_a_simultaneous_corner() {
    let runtime = unlocked_test_runtime();
    runtime
        .execute_source(
            r#"
                clearVertices()
                addVertex(0, -1)
                addVertex(0, 1)
                createLineShape("vertical", "", 0.05, 0, 0, 2, 0, 0, 0, true, false, 1)
                clearVertices()
                addVertex(-1, 0)
                addVertex(1, 0)
                createLineShape("horizontal", "", 0, 0.05, 2, 0, 0, 0, 0, true, false, 1)
                createCircle("body", "", 0, 0, 0.01, 1, 0, 0, true, false, 1)
                "#,
        )
        .unwrap();

    let mut bridge = runtime.render.lock().unwrap();
    let sweep_starts = bridge
        .scene
        .iter()
        .map(|(name, object)| (name.clone(), NativeSweepStart::capture(object)))
        .collect();
    {
        let body = bridge.scene.get_mut("body").unwrap();
        body.velocity_x = f64::from(4.8_f32);
        body.velocity_y = f64::from(4.8_f32);
        body.motion_started = true;
        body.sleeping = false;
        body.apply_native_position_delta(0.16, 0.16, 0.0);
    }
    bridge.sync_native_broad_phase();
    let mut toi_state = NativeToiStepState::default();
    let mut pending = bridge
        .advance_continuous_tunneling(&sweep_starts, &BTreeMap::new(), &mut toi_state)
        .unwrap();
    let selected = pending[0].0.clone();
    while let Some(auxiliary) = bridge.advance_next_toi_auxiliary_contact(
        (&selected.toi_bodies.0, &selected.toi_bodies.1),
        selected.alpha,
        &pending
            .iter()
            .map(|(contact, _)| contact.key.clone())
            .collect::<Vec<_>>(),
        &sweep_starts,
        &BTreeMap::new(),
    ) {
        pending.push(auxiliary);
    }
    assert_eq!(pending.len(), 2);
    let keys = pending
        .iter()
        .map(|(contact, _)| contact.key.clone())
        .collect::<BTreeSet<_>>();
    assert!(keys.contains(&("body".to_owned(), "vertical".to_owned(), 0, 0)));
    assert!(keys.contains(&("body".to_owned(), "horizontal".to_owned(), 0, 0)));
    let contacts = pending
        .iter()
        .map(|(contact, _)| contact.clone())
        .collect::<Vec<_>>();
    bridge.finish_continuous_tunneling(&contacts, 1.0 / 30.0, 10, 0.16, 15_708.0 / 10_000.0);
    assert!(bridge.scene["body"].velocity_x.abs() < 1e-6);
    assert!(bridge.scene["body"].velocity_y.abs() < 1e-6);
}

#[test]
fn toi_island_includes_an_existing_touching_auxiliary_contact_once() {
    let runtime = unlocked_test_runtime();
    runtime
        .execute_source(
            r#"
                clearVertices()
                addVertex(0.05, -1)
                addVertex(0.05, 1)
                createLineShape("vertical", "", 0, 0, 0, 2, 0, 0, 0, true, false, 1)
                clearVertices()
                addVertex(-1, 0)
                addVertex(1, 0)
                createLineShape("horizontal", "", 0, 0, 2, 0, 0, 0, 0, true, false, 1)
                createCircle("body", "", 0, 0.0115, 0.01, 1, 0, 0, true, false, 1)
                "#,
        )
        .unwrap();

    let mut bridge = runtime.render.lock().unwrap();
    bridge.sync_native_broad_phase();
    let horizontal = ("body".to_owned(), "horizontal".to_owned(), 0, 0);
    assert!(
        bridge
            .refresh_contacts()
            .iter()
            .any(|event| { event.began && event.first == "body" && event.second == "horizontal" })
    );
    let horizontal_feature = bridge.contact_manifolds[&horizontal].feature_id;
    bridge.contact_impulses.insert(
        horizontal.clone(),
        CachedContactImpulse {
            normal: 0.25,
            primary_feature_id: horizontal_feature,
            point_count: 1,
            ..CachedContactImpulse::default()
        },
    );
    let sweep_starts = bridge
        .scene
        .iter()
        .map(|(name, object)| (name.clone(), NativeSweepStart::capture(object)))
        .collect();
    {
        let body = bridge.scene.get_mut("body").unwrap();
        body.motion_started = true;
        body.sleeping = false;
        body.apply_native_position_delta(0.08_f32, 0.0, 0.0);
    }
    bridge.sync_native_broad_phase();
    let mut pending = bridge
        .advance_continuous_tunneling(
            &sweep_starts,
            &BTreeMap::new(),
            &mut NativeToiStepState::default(),
        )
        .expect("vertical edge must select a TOI contact");
    assert!(
        pending[0].0.key.0 == "vertical" || pending[0].0.key.1 == "vertical",
        "selected={:?}",
        pending[0].0.key
    );
    let island_keys = pending
        .iter()
        .map(|(contact, _)| contact.key.clone())
        .collect::<Vec<_>>();
    let auxiliary = bridge
        .advance_next_toi_auxiliary_contact(
            (&pending[0].0.toi_bodies.0, &pending[0].0.toi_bodies.1),
            pending[0].0.alpha,
            &island_keys,
            &sweep_starts,
            &BTreeMap::new(),
        )
        .expect("existing horizontal contact must join the TOI island");
    assert_eq!(auxiliary.0.key, horizontal);
    assert!(!auxiliary.1.began);
    pending.push(auxiliary);
    let island_keys = pending
        .iter()
        .map(|(contact, _)| contact.key.clone())
        .collect::<Vec<_>>();
    assert!(
        bridge
            .advance_next_toi_auxiliary_contact(
                (&pending[0].0.toi_bodies.0, &pending[0].0.toi_bodies.1),
                pending[0].0.alpha,
                &island_keys,
                &sweep_starts,
                &BTreeMap::new(),
            )
            .is_none(),
        "native island flag must prevent a second insertion"
    );
    let contacts = pending
        .iter()
        .map(|(contact, _)| contact.clone())
        .collect::<Vec<_>>();
    bridge.finish_continuous_tunneling(&contacts, 1.0 / 30.0, 10, 0.16, 15_708.0 / 10_000.0);
    assert_eq!(
        bridge.contact_impulses[&horizontal].normal, 0.25,
        "SolveTOI must not overwrite the persistent warm-start manifold"
    );
    let saturated_island = (0..32)
        .map(|fixture| ("body".to_owned(), "horizontal".to_owned(), fixture, 0))
        .collect::<Vec<_>>();
    assert!(
        bridge
            .advance_next_toi_auxiliary_contact(
                (&pending[0].0.toi_bodies.0, &pending[0].0.toi_bodies.1),
                pending[0].0.alpha,
                &saturated_island,
                &sweep_starts,
                &BTreeMap::new(),
            )
            .is_none(),
        "native 32-contact TOI island capacity must stop edge expansion"
    );
}

#[test]
fn toi_auxiliary_contact_walk_observes_prior_begin_callback_mutation() {
    let runtime = unlocked_test_runtime();
    runtime
        .execute_source(
            r#"
                clearVertices()
                addVertex(0, -1)
                addVertex(0, 1)
                createLineShape("vertical", "", 0.05, 0, 0, 2, 0, 0, 0, true, false, 1)
                clearVertices()
                addVertex(-1, 0)
                addVertex(1, 0)
                createLineShape("horizontal", "", 0, 0.05, 2, 0, 0, 0, 0, true, false, 1)
                createCircle("body", "", 0, 0, 0.01, 1, 0, 0, true, false, 1)
                setWorldGravity(0, 0)
                setVelocity("body", 4.8, 4.8)
                objects.world.body.strength = 100
                objects.world.body.defence = 0
                objects.world.horizontal.strength = 100
                objects.world.horizontal.defence = 0
                objects.world.vertical.strength = 100
                objects.world.vertical.defence = 0
                scoreTable = { blocks = { score = 0 } }
                worldAttributes = { scoreDamageMultiplier = 1 }
                toi_blocks = 0
                blockCollision = function(first, second)
                    toi_blocks = toi_blocks + 1
                    if first == "horizontal" or second == "horizontal" then
                        setCollisionEnabled("vertical", false)
                    end
                end
                update = function() end
                updatePhysics = function() end
                "#,
        )
        .unwrap();

    runtime.update(1.0 / 30.0).unwrap();
    assert_eq!(
        game_environment(runtime.lua())
            .unwrap()
            .get::<i64>("toi_blocks")
            .unwrap(),
        1
    );
    let bridge = runtime.render.lock().unwrap();
    assert!(bridge.active_contacts.contains_key(&(
        "body".to_owned(),
        "horizontal".to_owned(),
        0,
        0
    )));
    assert!(!bridge.active_contacts.contains_key(&(
        "body".to_owned(),
        "vertical".to_owned(),
        0,
        0
    )));
}

#[test]
fn box2d_sweep_angle_remains_unwrapped_after_integration() {
    let runtime = unlocked_test_runtime();
    runtime
        .execute_source(
            r#"
                createBox("body", "", 0, 0, 1, 1, 1, 0, 0, false, false, 1)
                "#,
        )
        .unwrap();
    let mut bridge = runtime.render.lock().unwrap();
    let body = bridge.scene.get_mut("body").unwrap();
    body.angle = 6.2;
    body.angular_velocity = 1.0;
    body.sleeping = false;
    body.motion_started = true;
    bridge.integrate_positions(0.1, 100.0, 100.0);
    assert!(bridge.scene["body"].angle > std::f64::consts::TAU);
}

#[test]
fn solver_impulses_do_not_reset_box2d_sleep_timer() {
    let runtime = unlocked_test_runtime();
    runtime
        .execute_source(
            r##"
                clearVertices()
                addVertex(-4, 0)
                addVertex(4, 0)
                createLineShape("ground", "", 0, 0, 8, 0, 0, 1, 0, true, false, 1)
                createCircle("body", "", 0, 0.501, 0.5, 1, 1, 0, true, false, 1)
                setWorldGravity(0, -2)
                update = function() end
                updatePhysics = function() end
                "##,
        )
        .unwrap();

    for _ in 0..20 {
        runtime.update(1.0 / 30.0).unwrap();
    }
    let body = &runtime.render.lock().unwrap().scene["body"];
    assert!(
        body.sleeping,
        "resting contact must not wake itself in the solver"
    );
    assert_eq!(body.velocity_x, 0.0);
    assert_eq!(body.velocity_y, 0.0);
}
