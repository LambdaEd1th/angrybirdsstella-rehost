use super::*;

const CHASSIS: &str = "BLOCK_ROCK_1X10_1_6";
const LEFT_WHEEL: &str = "BLOCK_WOOD_ROUND_4X4_1_11";
const RIGHT_WHEEL: &str = "BLOCK_WOOD_ROUND_4X4_1_12";
const PIG: &str = "pig_medium_8";

#[test]
fn chapter02_level56_upper_vehicle_rolls_briefly_then_sleeps_on_platform() {
    let sandbox = ShippedDataSandbox::new("chapter02-l56-upper-vehicle-settling");
    let runtime = StellaLua::new(&sandbox.data_root).unwrap();
    runtime.boot("scripts/game.lua").unwrap();
    runtime.execute_source("initializeEventSystem()").unwrap();
    runtime
        .execute_source(
            r#"
                SpriteSheetManager.useGroupSet('INGAME')
                currentFolder = 'Chapter02'
                currentPack = 'Chapter02'
                currentLevel = 56
                levelFolder = 'levels/Chapter02/'
                levelName = 'Chapter02_L56'
                loadLevelInternal(levelFolder .. levelName)
                blocks.BlockComponentManager.triggerGlobalEvent(blocks.events.EID_START)
                setPhysicsEnabled(true)
            "#,
        )
        .unwrap();

    let initial_x = {
        let bridge = runtime.render.lock().unwrap();
        for name in [CHASSIS, LEFT_WHEEL, RIGHT_WHEEL] {
            assert_eq!(bridge.scene[name].angular_damping, 1.0, "{name}");
            assert!(!bridge.scene[name].sleeping, "{name}");
        }
        bridge.scene[CHASSIS].x
    };

    let sleeping_frame = (1..=1_800).find(|_| {
        runtime.update(1.0 / 60.0).unwrap();
        let bridge = runtime.render.lock().unwrap();
        [CHASSIS, LEFT_WHEEL, RIGHT_WHEEL]
            .iter()
            .all(|name| bridge.scene[*name].sleeping)
    });
    let sleeping_frame = sleeping_frame.expect("upper vehicle never entered Box2D sleep");
    assert!(
        sleeping_frame > 30,
        "vehicle did not perform its authored initial roll"
    );

    let settled = {
        let bridge = runtime.render.lock().unwrap();
        assert!(bridge.scene.contains_key(PIG));
        let chassis = &bridge.scene[CHASSIS];
        assert!((0.1..0.5).contains(&(initial_x - chassis.x)));
        assert!(chassis.y < -6.0 && chassis.y > -6.2);
        assert!(bridge.scene[LEFT_WHEEL].x > 12.32);
        for name in [CHASSIS, LEFT_WHEEL, RIGHT_WHEEL] {
            let object = &bridge.scene[name];
            assert_eq!(object.velocity_x, 0.0, "{name}");
            assert_eq!(object.velocity_y, 0.0, "{name}");
            assert_eq!(object.angular_velocity, 0.0, "{name}");
        }
        [
            (chassis.x, chassis.y, chassis.angle),
            (
                bridge.scene[LEFT_WHEEL].x,
                bridge.scene[LEFT_WHEEL].y,
                bridge.scene[LEFT_WHEEL].angle,
            ),
            (
                bridge.scene[RIGHT_WHEEL].x,
                bridge.scene[RIGHT_WHEEL].y,
                bridge.scene[RIGHT_WHEEL].angle,
            ),
        ]
    };

    for _ in 0..600 {
        runtime.update(1.0 / 60.0).unwrap();
    }
    let bridge = runtime.render.lock().unwrap();
    assert!(bridge.scene.contains_key(PIG));
    for (index, name) in [CHASSIS, LEFT_WHEEL, RIGHT_WHEEL].iter().enumerate() {
        let object = &bridge.scene[*name];
        assert!(object.sleeping, "{name}");
        assert_eq!((object.x, object.y, object.angle), settled[index], "{name}");
    }
}
