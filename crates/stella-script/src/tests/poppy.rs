//! One Poppy shot: authored faces, drum reflection and subsequent ability input.
//! This regression ends before level completion and uses a private save directory.

use super::*;
use mlua::Table;

fn advance(runtime: &StellaLua, frames: usize) {
    for _ in 0..frames {
        runtime.update(1.0 / 60.0).unwrap();
        runtime.draw().unwrap();
        runtime.take_render_commands();
    }
}

fn assert_face(runtime: &StellaLua, bird: &str, sprite: &str) {
    runtime.draw().unwrap();
    let commands = runtime.take_render_commands();
    let animation = runtime._animation_runtime.lock().unwrap();
    let drawn = animation_render_commands(&animation, bird);
    let face = drawn
        .iter()
        .find(|command| command.sprite.as_str() == sprite)
        .unwrap_or_else(|| panic!("{bird} did not bind {sprite}"));
    assert!(face.state.alpha > 0.0);
    assert!(face.bound_region.is_some());
    assert!(commands.iter().any(|command| {
        command.sprite == face.sprite && command.x == face.x && command.y == face.y
    }));
    // SpriteManager's lookup is case-sensitive. The mixed-case editor guide
    // must remain unbound rather than cover every animated face with POPPY_IDLE.
    assert!(
        !drawn
            .iter()
            .any(|command| command.sprite.as_str() == "POPPY_IDLE")
    );
}

fn assert_skin_overlay(runtime: &StellaLua, bird: &str, skin: &str, overlays: &[&str]) {
    let animation = runtime._animation_runtime.lock().unwrap();
    assert_eq!(animation.skins.get(bird).map(String::as_str), Some(skin));
    let commands = animation_render_commands(&animation, bird);
    let submitted_overlays = commands
        .iter()
        .filter(|command| {
            command.sprite.contains("SHADES") || command.sprite.as_str() == "PIG_NOSE_POPPY"
        })
        .collect::<Vec<_>>();
    assert_eq!(submitted_overlays.len(), overlays.len());
    for sprite in overlays {
        let command = submitted_overlays
            .iter()
            .find(|command| command.sprite.as_str() == *sprite)
            .unwrap_or_else(|| panic!("{skin} did not bind {sprite}"));
        assert!(command.bound_region.is_some());
        assert!(command.state.alpha > 0.0);
    }
}

fn check_poppy_drum_faces(active_skin: Option<&str>, skin: &str, overlays: &[&str]) {
    let sandbox = ShippedDataSandbox::new(&format!("poppy-drum-faces-{skin}"));
    let runtime = StellaLua::new(&sandbox.data_root).unwrap();
    runtime.enable_local_services().unwrap();
    runtime.boot("scripts/game.lua").unwrap();
    advance(&runtime, 600);
    if let Some(active_skin) = active_skin {
        runtime
            .execute_source(&format!(
                "SettingsWrapper:setActiveBirdSkin('Poppy',{active_skin:?})"
            ))
            .unwrap();
    }
    runtime
        .execute_source("LevelLoad.transitionToLevel('Chapter01',19)")
        .unwrap();
    advance(&runtime, 360);
    let environment = game_environment(runtime.lua()).unwrap();
    assert_eq!(
        environment.get::<String>("levelName").unwrap(),
        "Chapter01_L18"
    );
    let bird_name: String = environment.get("currentBirdName").unwrap();
    let bird: Table = object_world(runtime.lua())
        .unwrap()
        .get(bird_name.as_str())
        .unwrap();
    assert_eq!(bird.get::<String>("definition").unwrap(), "Poppy");
    let check_face = |sprite| {
        assert_face(&runtime, &bird_name, sprite);
        assert_skin_overlay(&runtime, &bird_name, skin, overlays);
    };
    check_face("POPPY_BEAK_NORMAL");
    let (x, y): (f64, f64) = runtime
        .lua()
        .load("return physicsToScreenTransform(levelStartPosition.x,levelStartPosition.y)")
        .set_environment(environment.clone())
        .eval()
        .unwrap();
    runtime.set_cursor(x, y, true).unwrap();
    advance(&runtime, 1);
    runtime.set_cursor(x - 240.0, y + 115.0, true).unwrap();
    advance(&runtime, 60);
    check_face("POPPY_BEAK_ANGRY");
    runtime.set_cursor(x - 240.0, y + 115.0, false).unwrap();
    advance(&runtime, 40);
    assert!(bird.get::<bool>("shot").unwrap());
    check_face("POPPY_BODY_HAPPY_2");
    let eyebrow_before = runtime._animation_runtime.lock().unwrap().playback[&bird_name]
        .latched_targets["EYEBROW_LEFT"]
        .translation;
    advance(&runtime, 80);
    let eyebrow_after = runtime._animation_runtime.lock().unwrap().playback[&bird_name]
        .latched_targets["EYEBROW_LEFT"]
        .translation;
    assert_ne!(eyebrow_before, eyebrow_after, "flight seek froze the face");
    advance(&runtime, 20);
    let drum: Table = object_world(runtime.lua())
        .unwrap()
        .get("BLOCK_POPPYSHOUSE_BIGDRUM_DRUMSKIN_1_2")
        .unwrap();
    assert_eq!(
        drum.get::<Table>("drumBounceCounters")
            .unwrap()
            .get::<u32>(bird.clone())
            .unwrap(),
        1
    );
    let velocity_y = runtime.render.lock().unwrap().scene[&bird_name].velocity_y;
    assert!(velocity_y < 0.0, "drum failed to reflect downward flight");
    advance(&runtime, 60);
    // This exceeds PoppyAbility.onCollision's one-second loss delay after
    // the drum contact, so an accidental ordinary collision would fail here.
    assert!(
        !bird
            .get::<Option<bool>>("hasCollided")
            .unwrap()
            .unwrap_or(false)
    );
    assert!(
        !bird
            .get::<Option<bool>>("abilityDisabled")
            .unwrap()
            .unwrap_or(false)
    );
    runtime.set_cursor(600.0, 400.0, true).unwrap();
    advance(&runtime, 12);
    assert!(bird.get::<bool>("poppyAiming").unwrap());
    check_face("POPPY_BEAK_DIZZY_2");
    runtime.set_cursor(600.0, 400.0, false).unwrap();
    advance(&runtime, 1);
    assert_eq!(bird.get::<u32>("state").unwrap(), 1);
    advance(&runtime, 8);
    assert_eq!(bird.get::<u32>("state").unwrap(), 2);
    let sensor: String = bird.get("sensorName").unwrap();
    assert!(
        object_world(runtime.lua())
            .unwrap()
            .contains_key(sensor)
            .unwrap()
    );
    assert!(
        !environment
            .get::<Option<bool>>("g_levelCompleted")
            .unwrap()
            .unwrap_or(false)
    );
    assert!(runtime.fallback_calls.lock().unwrap().is_empty());
    assert!(runtime.compatibility_bindings.lock().unwrap().is_empty());
    eprintln!(
        "[poppy-costume] {active_skin:?}/{skin}: faces, overlay, drum, hold and drill passed"
    );
}

#[test]
fn poppy_authored_faces_switch_and_drum_bounce_preserves_ability_input() {
    check_poppy_drum_faces(None, "Normal", &[]);
}

#[test]
fn configured_normal_poppy_inherits_faces_and_preserves_drum_ability_input() {
    check_poppy_drum_faces(Some("Poppy"), "Normal", &[]);
}

#[test]
fn rockin_poppy_costume_inherits_faces_and_preserves_drum_ability_input() {
    check_poppy_drum_faces(
        Some("Rockin' Poppy"),
        "Rockin_Shades_Poppy",
        &["ROCKIN_POPPY_SHADES_LEFT", "ROCKIN_POPPY_SHADES_RIGHT"],
    );
}

#[test]
fn big_nose_poppy_costume_inherits_faces_and_preserves_drum_ability_input() {
    check_poppy_drum_faces(
        Some("Big Nose Poppy"),
        "Pig_Nose_Poppy",
        &["PIG_NOSE_POPPY"],
    );
}

#[test]
fn sweet_shades_poppy_costume_inherits_faces_and_preserves_drum_ability_input() {
    check_poppy_drum_faces(
        Some("Sweet Shades Poppy"),
        "Sweet_Shades_Poppy",
        &["SWEET_SHADES_POPPY_LEFT", "SWEET_SHADES_POPPY_RIGHT"],
    );
}

#[test]
fn pink_shades_poppy_costume_inherits_faces_and_preserves_drum_ability_input() {
    check_poppy_drum_faces(
        Some("Pink Shades Poppy"),
        "Pink_Shades_Poppy",
        &["PINK_SHADES_POPPY_LEFT", "PINK_SHADES_POPPY_RIGHT"],
    );
}

#[test]
fn next_poppy_updates_faces_after_a_missed_first_shot() {
    let sandbox = ShippedDataSandbox::new("next-poppy-faces");
    let runtime = StellaLua::new(&sandbox.data_root).unwrap();
    runtime.enable_local_services().unwrap();
    runtime.boot("scripts/game.lua").unwrap();
    advance(&runtime, 600);
    runtime
        .execute_source("LevelLoad.transitionToLevel('Chapter01',19)")
        .unwrap();
    advance(&runtime, 360);
    let environment = game_environment(runtime.lua()).unwrap();
    let first: String = environment.get("currentBirdName").unwrap();
    let (x, y): (f64, f64) = runtime
        .lua()
        .load("return physicsToScreenTransform(levelStartPosition.x,levelStartPosition.y)")
        .set_environment(environment.clone())
        .eval()
        .unwrap();
    runtime.set_cursor(x, y, true).unwrap();
    advance(&runtime, 1);
    runtime.set_cursor(x, y - 240.0, true).unwrap();
    advance(&runtime, 40);
    runtime.set_cursor(x, y - 240.0, false).unwrap();
    // Miss the structure, then let the original queue load the next bird.
    // No aim search or level completion is part of this regression.
    advance(&runtime, 600);
    let next: String = environment.get("currentBirdName").unwrap();
    assert_ne!(first, next);
    let bird: Table = object_world(runtime.lua())
        .unwrap()
        .get(next.as_str())
        .unwrap();
    assert_eq!(bird.get::<String>("definition").unwrap(), "Poppy");
    assert!(!bird.get::<Option<bool>>("shot").unwrap().unwrap_or(false));
    assert_face(&runtime, &next, "POPPY_BEAK_NORMAL");
    let (x, y): (f64, f64) = runtime
        .lua()
        .load("return physicsToScreenTransform(levelStartPosition.x,levelStartPosition.y)")
        .set_environment(environment.clone())
        .eval()
        .unwrap();
    runtime.set_cursor(x, y, true).unwrap();
    advance(&runtime, 1);
    runtime.set_cursor(x - 240.0, y + 115.0, true).unwrap();
    advance(&runtime, 40);
    assert_face(&runtime, &next, "POPPY_BEAK_ANGRY");
    runtime.set_cursor(x - 240.0, y + 115.0, false).unwrap();
    advance(&runtime, 40);
    assert!(bird.get::<bool>("shot").unwrap());
    assert_face(&runtime, &next, "POPPY_BODY_HAPPY_2");
    assert!(!environment.get::<bool>("g_levelCompleted").unwrap());
    assert!(runtime.fallback_calls().is_empty());
    assert!(runtime.compatibility_bindings().is_empty());
}
