//! Shipped BirdRun variant stability coverage.

use super::*;

fn load_seeded_bird_run(label: &str) -> (ShippedDataSandbox, StellaLua) {
    let sandbox = ShippedDataSandbox::new(label);
    let runtime = StellaLua::new(&sandbox.data_root).unwrap();
    runtime.boot("scripts/game.lua").unwrap();
    runtime.execute_source("initializeEventSystem()").unwrap();
    runtime
        .execute_source(
            r#"
                PlayerState.getMysteryBoxItemsAvailableInCurrentLevel = function()
                    return {}
                end
                -- Purple's numeric interpretation of save seed 1725440811
                -- selects these eight variants. Force the recovered result
                -- so the regression does not depend on an unrelated blank
                -- sandbox profile having birds unlocked.
                g_isEditorPlayTest = true
                g_editorForceGroupVariants = { 1, 1, 5, 6, 8, 5, 8, 4 }
                SpriteSheetManager.useGroupSet('INGAME')
                currentFolder = 'BirdRun'
                currentPack = 'BirdRun'
                currentLevel = 9
                levelFolder = 'levels/BirdRun/'
                levelName = 'BirdRun_L09'
                loadLevelInternal(levelFolder .. levelName)
                g_isEditorPlayTest = false
                g_editorForceGroupVariants = nil
                score = 0
                blocks.BlockComponentManager.triggerGlobalEvent(blocks.events.EID_START)
                setPhysicsEnabled(true)
            "#,
        )
        .unwrap();
    (sandbox, runtime)
}

#[test]
fn bird_run_level09_seeded_round_blocks_keep_authored_mounts_and_settle() {
    let (_sandbox, runtime) = load_seeded_bird_run("bird-run-l09-round-blocks");

    let names = [
        "BLOCK_ROCK_ROUND_2X2_1_19",
        "BLOCK_ROCK_ROUND_2X2_1_20",
        "BLOCK_ROCK_ROUND_2X2_1_21",
        "BLOCK_ROCK_ROUND_2X2_1_22",
    ];
    let initial_positions = {
        let bridge = runtime.render.lock().unwrap();
        let positions = names.map(|name| {
            let object = bridge
                .scene
                .get(name)
                .unwrap_or_else(|| panic!("seeded BirdRun variant omitted {name}"));
            assert!(
                object.dynamic_body,
                "{name} was authored as a dynamic circle"
            );
            assert_eq!(object.native_shape_radius, f64::from(0.1_f32));
            (object.x, object.y)
        });
        for (joint_name, first, second, breakable) in [
            (
                "BLOCK_ROCK_ROUND_2X2_1_19BLOCK_WOOD_2X4_1_97",
                "BLOCK_ROCK_ROUND_2X2_1_19",
                "BLOCK_WOOD_2X4_1_97",
                false,
            ),
            (
                "BLOCK_WOOD_2X4_1_95BLOCK_ROCK_ROUND_2X2_1_20",
                "BLOCK_WOOD_2X4_1_95",
                "BLOCK_ROCK_ROUND_2X2_1_20",
                true,
            ),
            (
                "BLOCK_ROCK_ROUND_2X2_1_20BLOCK_LIGHT_TRIANGLE_R_4X4_1_6",
                "BLOCK_ROCK_ROUND_2X2_1_20",
                "BLOCK_LIGHT_TRIANGLE_R_4X4_1_6",
                false,
            ),
            (
                "BLOCK_ROCK_ROUND_2X2_1_21BLOCK_WOOD_2X4_1_96",
                "BLOCK_ROCK_ROUND_2X2_1_21",
                "BLOCK_WOOD_2X4_1_96",
                false,
            ),
            (
                "BLOCK_WOOD_2X4_1_98BLOCK_ROCK_ROUND_2X2_1_22",
                "BLOCK_WOOD_2X4_1_98",
                "BLOCK_ROCK_ROUND_2X2_1_22",
                true,
            ),
            (
                "BLOCK_ROCK_ROUND_2X2_1_22BLOCK_LIGHT_TRIANGLE_R_4X4_1_6",
                "BLOCK_ROCK_ROUND_2X2_1_22",
                "BLOCK_LIGHT_TRIANGLE_R_4X4_1_6",
                false,
            ),
        ] {
            let joint = bridge
                .joints
                .get(joint_name)
                .unwrap_or_else(|| panic!("missing authored wheel mount {joint_name}"));
            assert_eq!(
                (joint.first.as_str(), joint.second.as_str()),
                (first, second)
            );
            assert_eq!(joint.joint_type, 2);
            assert!(joint.is_physical);
            assert_eq!(joint.breakable, breakable);
        }
        positions
    };

    let mut maximum_settled_speed = 0.0_f64;
    for step in 0..300 {
        runtime.step_physics(1.0 / 30.0).unwrap();
        if step >= 150 {
            let bridge = runtime.render.lock().unwrap();
            for name in names {
                let object = bridge
                    .scene
                    .get(name)
                    .unwrap_or_else(|| panic!("{name} disappeared while the level idled"));
                maximum_settled_speed =
                    maximum_settled_speed.max(object.velocity_x.hypot(object.velocity_y));
            }
        }
    }

    let bridge = runtime.render.lock().unwrap();
    assert!(
        maximum_settled_speed < 0.01,
        "seeded round blocks failed to settle: speed={maximum_settled_speed}"
    );
    for (name, initial) in names.into_iter().zip(initial_positions) {
        let object = bridge
            .scene
            .get(name)
            .unwrap_or_else(|| panic!("{name} was destroyed without player input"));
        assert!(
            object.sleeping,
            "{name} remained awake after ten idle seconds"
        );
        assert!(
            (object.x - initial.0).hypot(object.y - initial.1) < 0.05,
            "{name} drifted away from its authored contact stack"
        );
    }
    drop(bridge);
    assert_eq!(
        game_environment(runtime.lua())
            .unwrap()
            .get::<i64>("score")
            .unwrap(),
        0
    );
}

#[test]
fn bird_run_level09_chainsaw_vehicle_is_an_authored_unpowered_dynamic_cart() {
    let (_sandbox, runtime) = load_seeded_bird_run("bird-run-l09-chainsaw-topology");
    let bridge = runtime.render.lock().unwrap();
    for name in [
        "BLOCK_JUNGLE_CHAINSAW_21",
        "BLOCK_JUNGLE_CHAINSAW_22",
        "BLOCK_WOOD_ROUND_4X4_1_10",
        "BLOCK_WOOD_ROUND_4X4_1_11",
        "BLOCK_ROCK_1X10_1_27",
        "pig_medium_15",
        "pig_medium_18",
    ] {
        assert!(
            bridge
                .scene
                .get(name)
                .unwrap_or_else(|| panic!("missing authored cart member {name}"))
                .dynamic_body,
            "{name} must remain gravity-driven"
        );
    }
    for (name, first, second) in [
        (
            "BLOCK_JUNGLE_CHAINSAW_21BLOCK_WOOD_2X4_1_138",
            "BLOCK_JUNGLE_CHAINSAW_21",
            "BLOCK_WOOD_2X4_1_138",
        ),
        (
            "BLOCK_JUNGLE_CHAINSAW_22BLOCK_WOOD_2X4_1_137",
            "BLOCK_JUNGLE_CHAINSAW_22",
            "BLOCK_WOOD_2X4_1_137",
        ),
    ] {
        let joint = bridge
            .joints
            .get(name)
            .unwrap_or_else(|| panic!("missing authored chainsaw weld {name}"));
        assert_eq!(
            (joint.first.as_str(), joint.second.as_str()),
            (first, second)
        );
        assert_eq!(joint.joint_type, 2);
        assert!(!joint.breakable);
        assert!(!joint.motor_enabled);
        assert_eq!(joint.motor_speed, None);
    }
    let wheel_joint_names = [
        "BLOCK_WOOD_ROUND_4X4_1_10BLOCK_ROCK_1X10_1_27",
        "BLOCK_WOOD_ROUND_4X4_1_11BLOCK_ROCK_1X10_1_27",
    ];
    for name in wheel_joint_names {
        let joint = bridge
            .joints
            .get(name)
            .unwrap_or_else(|| panic!("missing authored cart axle {name}"));
        assert_eq!(joint.joint_type, 3);
        assert!(!joint.breakable);
        assert!(!joint.motor_enabled);
        assert_eq!(joint.motor_speed, Some(0.0));
        assert!(!joint.limits_enabled);
    }
    assert!(bridge.joints.values().all(|joint| {
        !matches!(joint.first.as_str(), "pig_medium_15" | "pig_medium_18")
            && !matches!(joint.second.as_str(), "pig_medium_15" | "pig_medium_18")
    }));
    drop(bridge);

    let environment = game_environment(runtime.lua()).unwrap();
    let descriptors = environment
        .get::<mlua::Table>("objects")
        .unwrap()
        .get::<mlua::Table>("joints")
        .unwrap();
    for name in wheel_joint_names {
        let descriptor = descriptors.get::<mlua::Table>(name).unwrap();
        assert!(descriptor.get::<bool>("backAndForth").unwrap());
        assert!(!descriptor.get::<bool>("motor").unwrap());
        assert!(!descriptor.get::<bool>("limit").unwrap());
        assert_eq!(descriptor.get::<f64>("motorSpeed").unwrap(), 0.0);
    }
}

#[derive(Debug)]
struct BirdRunIdleSnapshot {
    blocks: std::collections::BTreeMap<String, (f64, f64, f64, bool)>,
    joints: std::collections::BTreeSet<String>,
    score: i64,
}

fn bird_run_idle_snapshot(runtime: &StellaLua) -> BirdRunIdleSnapshot {
    let bridge = runtime.render.lock().unwrap();
    let blocks = bridge
        .scene
        .iter()
        .filter(|(name, object)| name.starts_with("BLOCK_") && object.dynamic_body)
        .map(|(name, object)| {
            (
                name.clone(),
                (object.x, object.y, object.angle, object.sleeping),
            )
        })
        .collect();
    let joints = bridge.joints.keys().cloned().collect();
    drop(bridge);
    BirdRunIdleSnapshot {
        blocks,
        joints,
        score: game_environment(runtime.lua())
            .unwrap()
            .get("score")
            .unwrap(),
    }
}

fn assert_bird_run_remains_settled(
    trial: usize,
    seconds: u32,
    settled: &BirdRunIdleSnapshot,
    current: &BirdRunIdleSnapshot,
) {
    let removed_blocks = settled
        .blocks
        .keys()
        .filter(|name| !current.blocks.contains_key(*name))
        .cloned()
        .collect::<Vec<_>>();
    let added_blocks = current
        .blocks
        .keys()
        .filter(|name| !settled.blocks.contains_key(*name))
        .cloned()
        .collect::<Vec<_>>();
    let removed_joints = settled
        .joints
        .difference(&current.joints)
        .cloned()
        .collect::<Vec<_>>();
    let mut moved_blocks = Vec::new();
    let mut maximum_position_drift = 0.0_f64;
    let mut maximum_angle_drift = 0.0_f64;
    for (name, &(initial_x, initial_y, initial_angle, _)) in &settled.blocks {
        let Some(&(x, y, angle, _)) = current.blocks.get(name) else {
            continue;
        };
        let position_drift = (x - initial_x).hypot(y - initial_y);
        let angle_drift = (angle - initial_angle).abs();
        maximum_position_drift = maximum_position_drift.max(position_drift);
        maximum_angle_drift = maximum_angle_drift.max(angle_drift);
        if position_drift >= 0.05 || angle_drift >= 0.05 {
            moved_blocks.push(name.clone());
        }
    }
    let awake_blocks = current
        .blocks
        .iter()
        .filter_map(|(name, &(_, _, _, sleeping))| (!sleeping).then_some(name.clone()))
        .collect::<Vec<_>>();
    eprintln!(
        "BirdRun_L09 trial={trial} t={seconds}s blocks={} joints={} score={} removed_blocks={removed_blocks:?} added_blocks={added_blocks:?} removed_joints={removed_joints:?} moved_blocks={moved_blocks:?} awake_blocks={awake_blocks:?} max_position_drift={maximum_position_drift:.9} max_angle_drift={maximum_angle_drift:.9}",
        current.blocks.len(),
        current.joints.len(),
        current.score,
    );
    assert!(
        removed_blocks.is_empty(),
        "late block removal at {seconds}s"
    );
    assert!(added_blocks.is_empty(), "late block creation at {seconds}s");
    assert!(removed_joints.is_empty(), "late joint break at {seconds}s");
    assert!(
        moved_blocks.is_empty(),
        "late structural drift at {seconds}s"
    );
    assert!(
        awake_blocks.is_empty(),
        "building stayed awake at {seconds}s"
    );
    assert_eq!(current.score, settled.score, "idle score changed");
}

/// Deliberately excluded from the ordinary fast suite: this replays the exact
/// saved BirdRun L09 layout's dynamic Box2D buildings several times for one to
/// two simulated minutes. The full display-frame host additionally advances
/// authored mobile contraptions such as the chainsaw vehicle; those are not
/// stationary-building stability candidates.
#[test]
#[ignore = "long-duration BirdRun L09 idle stability audit"]
fn bird_run_level09_repeated_one_to_two_minute_idle_audit() {
    for trial in 1..=6 {
        let duration_seconds = if trial <= 3 { 60 } else { 120 };
        let (_sandbox, runtime) = load_seeded_bird_run(&format!("bird-run-l09-long-idle-{trial}"));
        for _ in 0..300 {
            runtime.step_physics(1.0 / 30.0).unwrap();
        }
        let settled = bird_run_idle_snapshot(&runtime);
        for _ in 300..(duration_seconds * 30) {
            runtime.step_physics(1.0 / 30.0).unwrap();
        }
        let current = bird_run_idle_snapshot(&runtime);
        assert_bird_run_remains_settled(trial as usize, duration_seconds, &settled, &current);
    }
}
