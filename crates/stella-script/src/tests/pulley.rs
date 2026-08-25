//! Shipped pulley-controller and rope-island regression coverage.

use super::*;

#[test]
fn chapter02_level44_pulley_island_settles_without_anchor_drift() {
    let sandbox = ShippedDataSandbox::new("chapter02-l44-pulley");
    let runtime = StellaLua::new(&sandbox.data_root).unwrap();
    runtime.boot("scripts/game.lua").unwrap();
    runtime.execute_source("initializeEventSystem()").unwrap();
    runtime
        .execute_source(
            r#"
                SpriteSheetManager.useGroupSet('INGAME')
                currentFolder = 'Chapter02'
                currentPack = 'Chapter02'
                currentLevel = 44
                levelFolder = 'levels/Chapter02/'
                levelName = 'Chapter02_L44'
                loadLevelInternal(levelFolder .. levelName)
                blocks.BlockComponentManager.triggerGlobalEvent(blocks.events.EID_START)
                setPhysicsEnabled(true)
            "#,
        )
        .unwrap();

    let world = object_world(runtime.lua()).unwrap();
    let expected_radius = |name: &str| {
        let pulley = world.get::<mlua::Table>(name).unwrap();
        let radius = pulley.get::<f32>("radius").unwrap();
        let scale_x = pulley.get::<f32>("scaleX").unwrap();
        let scale_y = pulley.get::<f32>("scaleY").unwrap();
        let fixture_scale = (scale_x.min(scale_y) / 0.1_f32).abs() + 0.0001_f32;
        f64::from(fixture_scale * radius)
    };
    let expected_pulley_7_radius = expected_radius("PULLEY_7");
    let expected_pulley_8_radius = expected_radius("PULLEY_8");

    let initial_positions = {
        let bridge = runtime.render.lock().unwrap();
        for name in [
            "PULLEY_7",
            "PULLEY_8",
            "RAIL_BLOCKWHEEL_1",
            "RAIL_BLOCKWHEEL_2",
            "BLOCK_WOOD_1X4_1_21",
        ] {
            assert!(bridge.scene.contains_key(name), "missing {name}");
        }
        assert_eq!(
            bridge.scene["PULLEY_7"].native_shape_radius,
            expected_pulley_7_radius
        );
        assert_eq!(
            bridge.scene["PULLEY_8"].native_shape_radius,
            expected_pulley_8_radius
        );
        assert!(bridge.scene["PULLEY_7"].body_mass > 0.004);
        assert!(bridge.scene["PULLEY_8"].body_mass > 0.003);
        assert!(bridge.joints.contains_key("PULLEY_7RAIL_BLOCKWHEEL_1"));
        assert!(bridge.joints.contains_key("PULLEY_8RAIL_BLOCKWHEEL_2"));
        [
            (bridge.scene["PULLEY_7"].x, bridge.scene["PULLEY_7"].y),
            (bridge.scene["PULLEY_8"].x, bridge.scene["PULLEY_8"].y),
        ]
    };

    let mut maximum_settled_pulley_speed = 0.0_f64;
    for frame in 0..600 {
        runtime.update(1.0 / 60.0).unwrap();
        if frame >= 300 {
            let bridge = runtime.render.lock().unwrap();
            for name in ["PULLEY_7", "PULLEY_8"] {
                let object = &bridge.scene[name];
                maximum_settled_pulley_speed =
                    maximum_settled_pulley_speed.max(object.velocity_x.hypot(object.velocity_y));
            }
        }
    }

    let bridge = runtime.render.lock().unwrap();
    assert!(
        maximum_settled_pulley_speed < 0.01,
        "undersized pulley fixtures kept spring-kicked velocity {maximum_settled_pulley_speed}"
    );
    for (name, initial) in [
        ("PULLEY_7", initial_positions[0]),
        ("PULLEY_8", initial_positions[1]),
    ] {
        let pulley = &bridge.scene[name];
        assert!(
            (pulley.x - initial.0).hypot(pulley.y - initial.1) < 0.003,
            "{name} drifted away from its welded authored pose"
        );
    }
    assert!((bridge.scene["PULLEY_7_rope_3_6"].y - bridge.scene["PULLEY_7"].y).abs() < 0.75);
    assert!((bridge.scene["PULLEY_8_rope_3_9"].y - bridge.scene["PULLEY_8"].y).abs() < 0.75);
}
