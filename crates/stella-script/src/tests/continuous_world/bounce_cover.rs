//! Guard the drum-only eligibility exception through complete TOI selection.

use super::*;

fn check_swept_cover(
    cover_first: bool,
    angle: f32,
    start: (f32, f32),
    end: (f32, f32),
    flags: (bool, bool),
    controllable: bool,
    expect_cover_hit: bool,
) {
    let runtime = unlocked_test_runtime();
    let (sine, cosine) = angle.sin_cos();
    let world_point = |point: (f32, f32)| {
        (
            point.0.mul_add(cosine, -(point.1 * sine)),
            point.0.mul_add(sine, point.1 * cosine),
        )
    };
    let initial = world_point(start);
    let cover = "createBox('cover','',0,0,0.5,0.02,1,0,0,true,false,1)";
    let bird = format!(
        "createCircle('bird','',{},{},0.01,1,0,0,true,{controllable},1)",
        initial.0, initial.1
    );
    let constructors = if cover_first {
        format!("{cover}; {bird}")
    } else {
        format!("{bird}; {cover}")
    };
    runtime.execute_source(&constructors).unwrap();
    runtime
        .execute_source(&format!(
            "objects.world.cover.isDrum={}; objects.world.cover.ignoreCollision={}",
            flags.0, flags.1
        ))
        .unwrap();
    runtime.sync_native_collision_filter_state().unwrap();

    let mut bridge = runtime.render.lock().unwrap();
    bridge.set_object_active_state("bird", true);
    bridge.scene.get_mut("cover").unwrap().angle = f64::from(angle);
    bridge.sync_native_broad_phase();
    let sweep_starts = bridge
        .scene
        .iter()
        .map(|(name, object)| (name.clone(), NativeSweepStart::capture(object)))
        .collect();
    let delta = world_point((end.0 - start.0, end.1 - start.1));
    {
        let bird = bridge.scene.get_mut("bird").unwrap();
        bird.velocity_x = f64::from(delta.0 * 30.0);
        bird.velocity_y = f64::from(delta.1 * 30.0);
        bird.motion_started = true;
        bird.sleeping = false;
        bird.apply_native_position_delta(delta.0, delta.1, 0.0);
    }
    bridge.sync_native_broad_phase();
    let key = ("cover".to_owned(), "bird".to_owned(), 0, 0);
    assert!(bridge.broad_phase_contacts.contains(&key));
    assert!(bridge.scene["cover"].dynamic_body);
    assert!(bridge.scene["bird"].dynamic_body);
    assert!(!bridge.scene["cover"].bullet);
    assert!(!bridge.scene["bird"].bullet);
    // Every case overlaps at the ordinary step's end, including side/base
    // directions. Rejecting TOI must leave that ordinary contact available.
    assert!(
        bridge.scene["cover"]
            .collision_fixture_manifold(&bridge.scene["bird"], 0, 0)
            .is_some()
    );
    let final_center = bridge.scene["bird"].native_world_center();
    let pending = bridge.advance_continuous_tunneling(
        &sweep_starts,
        &BTreeMap::new(),
        &mut NativeToiStepState::default(),
    );
    if expect_cover_hit {
        let pending = pending.expect("front drum surface must stop the sweep early");
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].0.key, key);
        assert!(pending[0].0.alpha > 0.0 && pending[0].0.alpha < 1.0);
        assert_ne!(bridge.scene["bird"].native_world_center(), final_center);
    } else {
        assert!(pending.is_none(), "ordinary dynamic pair became continuous");
        assert_eq!(bridge.scene["bird"].native_world_center(), final_center);
        assert!(
            bridge.scene["cover"]
                .collision_fixture_manifold(&bridge.scene["bird"], 0, 0)
                .is_some()
        );
        // A real native bullet still admits this same geometric impact.
        // Thus the rejection above exercises policy, not a missed candidate.
        bridge.scene.get_mut("bird").unwrap().bullet = true;
        assert!(
            bridge
                .advance_continuous_tunneling(
                    &sweep_starts,
                    &BTreeMap::new(),
                    &mut NativeToiStepState::default()
                )
                .is_some()
        );
    }
}

#[test]
fn front_drum_sweep_works_for_rotated_covers_and_both_creation_orders() {
    for angle in [0.0, 0.63, 6.056_226] {
        for cover_first in [false, true] {
            check_swept_cover(
                cover_first,
                angle,
                (0.0, -0.07),
                (0.0, 0.0),
                (true, true),
                true,
                true,
            );
        }
    }
}

#[test]
fn drum_side_and_underside_keep_ordinary_dynamic_contact_policy() {
    for (start, end) in [((0.0, 0.07), (0.0, 0.0)), ((-0.29, 0.0), (-0.25, 0.0))] {
        check_swept_cover(true, 0.0, start, end, (true, true), true, false);
    }
}

#[test]
fn unmarked_boxes_and_non_birds_keep_ordinary_dynamic_contact_policy() {
    for flags in [(false, false), (true, false), (false, true)] {
        check_swept_cover(true, 0.0, (0.0, -0.07), (0.0, 0.0), flags, true, false);
    }
    check_swept_cover(
        true,
        0.0,
        (0.0, -0.07),
        (0.0, 0.0),
        (true, true),
        false,
        false,
    );
}
