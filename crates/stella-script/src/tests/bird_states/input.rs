//! Fixed host input through the shipped sling and ability components.

use super::*;

fn advance(runtime: &StellaLua, frames: usize) {
    for _ in 0..frames {
        runtime.update(1.0 / 60.0).unwrap();
        runtime.draw().unwrap();
        runtime.take_render_commands();
    }
}

fn flag(runtime: &StellaLua, expression: &str) -> bool {
    runtime
        .lua()
        .load(format!("return not not ({expression})"))
        .set_environment(game_environment(runtime.lua()).unwrap())
        .eval()
        .unwrap()
}

type BirdInputCase = (
    &'static str,
    &'static str,
    &'static str,
    &'static str,
    &'static str,
    &'static str,
    &'static str,
    &'static str,
);

const CASES: [BirdInputCase; 5] = [
    (
        "Stella",
        "Stella_ready",
        "STELLA_BEAK_OPEN",
        "STELLA_BEAK_OPEN",
        "Ability",
        "STELLA_BEAK_ANGRY",
        "auditBird.stellaAbility.aiming",
        "auditBird.stellaAbility.hasBeenActivated and not auditBird.stellaAbility.aiming",
    ),
    (
        "Poppy",
        "Poppy_ready",
        "POPPY_BEAK_ANGRY",
        "POPPY_BODY_HAPPY_2",
        "Poppy_Power",
        "POPPY_BEAK_DIZZY_2",
        "auditBird.poppyAiming",
        "auditBird.state == 1 and not auditBird.poppyAiming",
    ),
    (
        "Luca",
        "Luca_Ready",
        "LUCA_BEAK_GRUMPY",
        "LUCA_BEAK_HAPPY",
        "Luca_ability",
        "LUCA_BEAK_SCREAM",
        "auditBird.lucaAiming",
        "auditBird.lucaScreamAbilityUsed and not auditBird.lucaAiming",
    ),
    (
        "Willow",
        "Willow_Ready",
        "WILLOW_BEAK_ANGRY",
        "WILLOW_BEAK_HAPPY",
        "Willow_Spinning",
        "WILLOW_SPIN",
        "auditBird.willowAbility and not auditBird.willowAbility.released",
        "auditBird.willowAbility.released and not auditBird.willowAbility.holding",
    ),
    (
        "Dahlia",
        "Dahlia_Ready",
        "DAHLIA_BEAK_GRIN",
        "DAHLIA_BEAK_HAPPY",
        "Dahlia_Side_Collision",
        "DAHLIA_BEAK_ANGRY",
        "auditBird.dahliaAbility.aiming and auditBird.dahliaAbility.ghost",
        // Dahlia's shipped aim expires on its timer, independently of release.
        "auditBird.dahliaAbility.dashing",
    ),
];

fn check_host_input(
    (bird, ready, ready_sprite, flight_sprite, held, held_sprite, aiming, activated): BirdInputCase,
) {
    let (_sandbox, runtime) = fixture(&format!("{bird}-skill-input"));
    // The same shipped creation entry is used by extra birds/Telepods.
    // It establishes the real sling state; no animation or ability state
    // is injected, and all later changes come from normal host input.
    runtime
        .execute_source(&format!(
            "auditBird=g_slingshot:createNewBirdToSlingshot({bird:?}); auditBirdName=auditBird.name"
        ))
        .unwrap();
    advance(&runtime, 1);
    let environment = game_environment(runtime.lua()).unwrap();
    let name: String = environment.get("auditBirdName").unwrap();
    let idle_sprite = if bird == "Stella" {
        "STELLA_BEAK_CLOSED".to_owned()
    } else {
        format!("{}_BEAK_NORMAL", bird.to_uppercase())
    };
    let idle_face = draw_face(&runtime, &name, &format!("{bird}_Idle"), &idle_sprite);
    let (x, y): (f64, f64) = runtime
        .lua()
        .load("return physicsToScreenTransform(levelStartPosition.x,levelStartPosition.y)")
        .set_environment(environment.clone())
        .eval()
        .unwrap();
    runtime.set_cursor(x, y, true).unwrap();
    advance(&runtime, 1);
    runtime.set_cursor(x - 240.0, y + 115.0, true).unwrap();
    advance(&runtime, 30);
    let sling_face = draw_face(&runtime, &name, ready, ready_sprite);
    assert_ne!(idle_face, sling_face, "{bird}: sling input froze the face");
    runtime.set_cursor(x - 240.0, y + 115.0, false).unwrap();
    advance(&runtime, 40);
    assert!(flag(&runtime, "auditBird.shot and flyingBird == auditBird"));
    assert!(!flag(
        &runtime,
        "auditBird.hasCollided or auditBird.abilityDisabled"
    ));
    let flight_face = draw_face(&runtime, &name, &format!("{bird}_Flying"), flight_sprite);
    assert_ne!(sling_face, flight_face, "{bird}: launch froze the face");
    runtime.set_cursor(600.0, 400.0, true).unwrap();
    advance(&runtime, 12);
    assert!(flag(&runtime, aiming), "{bird}: skill press/hold was lost");
    let held_face = draw_face(&runtime, &name, held, held_sprite);
    assert_ne!(flight_face, held_face, "{bird}: skill input froze the face");
    runtime.set_cursor(600.0, 400.0, false).unwrap();
    advance(&runtime, 1);
    if bird == "Dahlia" {
        for _ in 0..120 {
            if flag(&runtime, activated) {
                break;
            }
            advance(&runtime, 1);
        }
    }
    assert!(
        flag(&runtime, activated),
        "{bird}: original activation did not finish"
    );
    if bird == "Poppy" {
        advance(&runtime, 8);
        assert!(flag(
            &runtime,
            "auditBird.state == 2 and objects.world[auditBird.sensorName]"
        ));
    } else if bird == "Dahlia" {
        // The default shipped bird does not enable canCancelDash.
        // A further press/release must leave its dash running; its own
        // dash timer subsequently starts the kick without a state write.
        assert!(!flag(&runtime, "auditBird.dahliaAbility.canCancelDash"));
        runtime.set_cursor(600.0, 400.0, true).unwrap();
        advance(&runtime, 1);
        runtime.set_cursor(600.0, 400.0, false).unwrap();
        advance(&runtime, 1);
        assert!(flag(&runtime, "auditBird.dahliaAbility.dashing"));
        for _ in 0..120 {
            if flag(&runtime, "auditBird.dahliaAbility.kicking") {
                break;
            }
            advance(&runtime, 1);
        }
        assert!(flag(&runtime, "auditBird.dahliaAbility.kicking"));
        // The ability queues the kick after this frame's BirdAnimation
        // update. The next original update consumes that queued state.
        advance(&runtime, 1);
        draw_face(&runtime, &name, "Dahlia_Spinning", "DAHLIA_BEAK_SCREAM");
    }
    assert!(!flag(&runtime, "g_levelCompleted or g_levelFailed"));
    assert!(runtime.fallback_calls.lock().unwrap().is_empty());
    assert!(runtime.compatibility_bindings.lock().unwrap().is_empty());
    eprintln!("[bird-input] {bird}: sling, flight, skill hold and activation passed");
}

#[test]
fn stella_host_input_switches_faces_and_activates_ability() {
    check_host_input(CASES[0]);
}

#[test]
fn poppy_host_input_switches_faces_and_creates_drill_sensor() {
    check_host_input(CASES[1]);
}

#[test]
fn luca_host_input_switches_faces_and_activates_scream() {
    check_host_input(CASES[2]);
}

#[test]
fn willow_host_input_switches_faces_and_releases_roll() {
    check_host_input(CASES[3]);
}

#[test]
fn dahlia_host_input_switches_faces_and_reaches_timed_kick() {
    check_host_input(CASES[4]);
}
