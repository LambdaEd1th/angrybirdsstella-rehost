//! Fixed shots in the BirdRun event identified from the player's save.
//! Only the event seed and unlocked birds are reproduced; player data stays private.

use super::*;
use mlua::Table;

fn advance(runtime: &StellaLua, frames: usize) {
    for _ in 0..frames {
        runtime.update(1.0 / 60.0).unwrap();
        runtime.draw().unwrap();
        runtime.take_render_commands();
    }
}

fn fixture(species: &str) -> (ShippedDataSandbox, StellaLua, String) {
    let sandbox = ShippedDataSandbox::new(&format!("bird-run-front-drum-{species}"));
    let runtime = StellaLua::new(&sandbox.data_root).unwrap();
    runtime.enable_local_services().unwrap();
    runtime.boot("scripts/game.lua").unwrap();
    advance(&runtime, 600);
    runtime
        .execute_source(
            "for _,bird in ipairs({'Stella','Poppy','Luca','Willow','Dahlia'}) do \
         SettingsWrapper:unlockBirdSkin(bird) end; \
         PlayerState:setFlag('islandFTUECompleted',true); \
         LevelLoad.transitionToMenu('IslandMap')",
        )
        .unwrap();
    advance(&runtime, 550);
    // Use BasicLevels.start, including its original variant generator,
    // metadata and per-turn game mode. No forced editor variants are used.
    runtime
        .execute_source(
            "IslandEvent.createFromEventData({type='levels',id='drum-regression', \
         seed='1191650966',perTurnBirdSelection=true, \
         levels={{levelName='BirdRun_L09',variantSeed='2667792133'}}}):start()",
        )
        .unwrap();
    advance(&runtime, 540);
    assert_eq!(
        game_environment(runtime.lua())
            .unwrap()
            .get::<String>("levelName")
            .unwrap(),
        "BirdRun_L09"
    );
    // The shipped selection handler calls these same two operations.
    runtime
        .execute_source(&format!(
            "g_currentGameMode:addBird({species:?}); \
         local selection=getGameHud():getChild('PerTurnBirdSelection'); \
         if selection then getGameHud():removeChild(selection) end"
        ))
        .unwrap();
    advance(&runtime, 410);
    let environment = game_environment(runtime.lua()).unwrap();
    let bird_name: String = environment.get("currentBirdName").unwrap();
    let bird: Table = object_world(runtime.lua())
        .unwrap()
        .get(bird_name.as_str())
        .unwrap();
    assert_eq!(bird.get::<String>("definition").unwrap(), species);
    runtime
        .execute_source(
            "drumContacts={}; local original=birdCollision; \
         birdCollision=function(a,b,...) \
         table.insert(drumContacts,{a=a,b=b,pressed=isKeyPressed('LBUTTON'), \
         held=isKeyHold('LBUTTON'),released=isKeyReleased('LBUTTON')}); \
         return original(a,b,...) end",
        )
        .unwrap();
    (sandbox, runtime, bird_name)
}

fn shoot(runtime: &StellaLua, y_offset: f64) {
    let environment = game_environment(runtime.lua()).unwrap();
    let (x, y): (f64, f64) = runtime
        .lua()
        .load("return physicsToScreenTransform(levelStartPosition.x,levelStartPosition.y)")
        .set_environment(environment.clone())
        .eval()
        .unwrap();
    // One fixed, shallow drag from the reproduced save case. Stop shortly
    // after the first bounce, before other obstacles or level completion.
    for frame in 0..60 {
        let progress = frame as f64 / 60.0;
        runtime
            .set_cursor(x - 240.0 * progress, y + y_offset * progress, true)
            .unwrap();
        advance(runtime, 1);
    }
    runtime.set_cursor(x - 240.0, y + y_offset, false).unwrap();
    advance(runtime, 110);
}

fn check_front_drum_bounce(species: &str) {
    let (_sandbox, runtime, bird_name) = fixture(species);
    let environment = game_environment(runtime.lua()).unwrap();
    shoot(&runtime, 24.0);
    let contacts: Table = environment.get("drumContacts").unwrap();
    let contacts = contacts
        .sequence_values::<Table>()
        .map(Result::unwrap)
        .collect::<Vec<_>>();
    let targets = contacts
        .iter()
        .map(|entry| entry.get::<String>("b").unwrap())
        .collect::<Vec<_>>();
    assert!(
        targets.iter().any(|name| name.contains("DRUMSKIN")),
        "shot missed the drum: {targets:?}"
    );
    assert!(
        targets.iter().all(|name| name.contains("DRUMSKIN")),
        "front bounce reached another collider: {targets:?}"
    );
    for entry in &contacts {
        assert_eq!(entry.get::<String>("a").unwrap(), bird_name);
        for key in ["pressed", "held", "released"] {
            assert!(
                !entry.get::<bool>(key).unwrap(),
                "unexpected {key} during drum contact"
            );
        }
    }
    let bird: Table = object_world(runtime.lua())
        .unwrap()
        .get(bird_name.as_str())
        .unwrap();
    for flag in [
        "hasCollided",
        "collisionTriggered",
        "abilityDisabled",
        "onGround",
    ] {
        assert!(
            !bird.get::<Option<bool>>(flag).unwrap().unwrap_or(false),
            "{species} acquired {flag} after drum bounce"
        );
    }
    assert!(
        bird.get::<Option<Table>>("willowAbility")
            .unwrap()
            .is_none()
    );
    assert!(
        bird.get::<Option<Table>>("dahliaAbility")
            .unwrap()
            .is_none()
    );
    assert!(bird.get::<Option<i64>>("state").unwrap().is_none());
    assert!(!environment.get::<bool>("g_levelCompleted").unwrap());
    assert!(!environment.get::<bool>("g_levelFailed").unwrap());
    assert!(runtime.fallback_calls().is_empty());
    assert!(runtime.compatibility_bindings().is_empty());
    let bridge = runtime.render.lock().unwrap();
    assert!(
        !bridge.scene[&bird_name].bullet,
        "ordinary bird bullet flags must remain unchanged"
    );
    assert!(
        bridge.scene[&bird_name].velocity_y < 0.0,
        "drum did not reflect the bird upward"
    );
}

#[test]
fn dahlia_ordinary_obstacle_still_activates_ability_without_input() {
    let (_sandbox, runtime, bird_name) = fixture("Dahlia");
    shoot(&runtime, 0.0);
    let environment = game_environment(runtime.lua()).unwrap();
    let contacts: Table = environment.get("drumContacts").unwrap();
    let first: Table = contacts.get(1).unwrap();
    assert_eq!(first.get::<String>("a").unwrap(), bird_name);
    assert_eq!(
        first.get::<String>("b").unwrap(),
        "BLOCK_HOMETREE_PLATFORM_1_46"
    );
    for key in ["pressed", "held", "released"] {
        assert!(!first.get::<bool>(key).unwrap());
    }
    let bird: Table = object_world(runtime.lua())
        .unwrap()
        .get(bird_name.as_str())
        .unwrap();
    assert!(bird.get::<bool>("hasCollided").unwrap());
    assert!(
        bird.get::<Option<Table>>("dahliaAbility")
            .unwrap()
            .is_some()
    );
    assert!(!environment.get::<bool>("g_levelCompleted").unwrap());
    assert!(runtime.fallback_calls().is_empty());
    assert!(runtime.compatibility_bindings().is_empty());
}

#[test]
fn dahlia_front_drum_bounce_does_not_activate_ability_without_input() {
    check_front_drum_bounce("Dahlia");
}

#[test]
fn willow_front_drum_bounce_does_not_activate_ability_without_input() {
    check_front_drum_bounce("Willow");
}

#[test]
fn poppy_front_drum_bounce_preserves_flight_and_ability() {
    check_front_drum_bounce("Poppy");
}

#[test]
fn luca_front_drum_bounce_preserves_flight_and_ability() {
    check_front_drum_bounce("Luca");
}

#[test]
fn stella_front_drum_bounce_does_not_start_last_chance_aim() {
    check_front_drum_bounce("Stella");
}
