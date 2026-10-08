//! Original bird components and assets, exercised without completing a level.

use super::*;
use mlua::Table;

mod input;

fn fixture(label: &str) -> (ShippedDataSandbox, StellaLua) {
    let sandbox = ShippedDataSandbox::new(label);
    let runtime = StellaLua::new(&sandbox.data_root).unwrap();
    runtime.enable_local_services().unwrap();
    runtime.boot("scripts/game.lua").unwrap();
    for _ in 0..600 {
        runtime.update(1.0 / 60.0).unwrap();
        runtime.draw().unwrap();
        runtime.take_render_commands();
    }
    runtime
        .execute_source("LevelLoad.transitionToLevel('Chapter01',19)")
        .unwrap();
    for _ in 0..360 {
        runtime.update(1.0 / 60.0).unwrap();
        runtime.draw().unwrap();
        runtime.take_render_commands();
    }
    assert_eq!(
        game_environment(runtime.lua())
            .unwrap()
            .get::<String>("levelName")
            .unwrap(),
        "Chapter01_L18"
    );
    (sandbox, runtime)
}

fn create_bird(runtime: &StellaLua, definition: &str) -> String {
    runtime.execute_source(&format!(
        "auditBirdName=createObject(blockTable,{definition:?},nil,levelStartPosition.x,levelStartPosition.y); \
         auditBird=objects.world[auditBirdName]; auditBird.damageDone=0; setVisible(auditBirdName,true)"
    )).unwrap();
    game_environment(runtime.lua())
        .unwrap()
        .get("auditBirdName")
        .unwrap()
}

fn draw_face(runtime: &StellaLua, bird: &str, action: &str, sprite: &str) -> Vec<(String, String)> {
    runtime.take_render_commands();
    runtime.draw().unwrap();
    let submitted = runtime.take_render_commands();
    let animation = runtime._animation_runtime.lock().unwrap();
    let playback = &animation.playback[bird];
    assert_eq!(playback.current_action, action);
    let drawn = animation_render_commands(&animation, bird);
    assert!(
        !drawn.is_empty(),
        "{bird} has no rendered parts in {action}"
    );
    let face = drawn
        .iter()
        .find(|command| command.sprite.as_str() == sprite)
        .unwrap_or_else(|| {
            panic!(
                "{bird}: {action} did not bind {sprite}; parts={:?}",
                drawn.iter().map(|c| c.sprite.as_str()).collect::<Vec<_>>()
            )
        });
    assert!(face.state.alpha > 0.0);
    assert!(face.bound_region.is_some());
    if action != "Willow_Spinning" {
        // Willow's spin atlas contains the entire face in one body sprite.
        // Other actions still submit separate eye/pupil parts.
        let eyes = drawn
            .iter()
            .filter(|command| {
                command.state.alpha > 0.0
                    && ["EYE", "PUPIL"]
                        .iter()
                        .any(|part| command.sprite.contains(part))
            })
            .collect::<Vec<_>>();
        assert!(!eyes.is_empty(), "{bird}: {action} did not draw eyes");
        for eye in eyes {
            assert!(eye.bound_region.is_some());
            assert!(
                submitted
                    .iter()
                    .any(|c| c.sprite == eye.sprite && c.x == eye.x && c.y == eye.y)
            );
        }
    }
    assert!(
        submitted
            .iter()
            .any(|c| c.sprite == face.sprite && c.x == face.x && c.y == face.y),
        "{bird}: {action} did not submit {sprite}; submitted={:?}",
        submitted
            .iter()
            .filter(|c| c.sprite == face.sprite)
            .map(|c| (c.x, c.y))
            .collect::<Vec<_>>()
    );
    let guide = format!("{}_IDLE", bird.split('_').next().unwrap().to_uppercase());
    assert!(
        !drawn.iter().any(|c| c.sprite.as_str() == guide),
        "{bird} bound an editor guide"
    );
    let transform = animation.transforms[bird];
    assert_eq!(
        transform.angle,
        runtime.render.lock().unwrap().scene[bird].render_angle
    );
    let signature = playback
        .latched_targets
        .iter()
        .filter(|(name, _)| {
            ["EYE", "PUPIL", "BEAK", "BROW"]
                .iter()
                .any(|part| name.contains(part))
        })
        .map(|(name, state)| (name.clone(), format!("{state:?}")))
        .collect::<Vec<_>>();
    assert!(!signature.is_empty());
    signature
}

fn pulse(runtime: &StellaLua, source: &str, elapsed: f64) {
    runtime.execute_source(&format!(
        "local b=auditBird.birdAnimation; {source}; b:update(1/60,20); AnimationWrapperNative.update({elapsed})"
    )).unwrap();
}

#[test]
fn five_birds_original_states_bind_faces_and_paused_flight_seeks_change_expression() {
    let (_sandbox, runtime) = fixture("five-bird-faces");
    for (bird, ready, ready_beak, flight_beak, power, power_sprite, power_trigger) in [
        (
            "Stella",
            "Stella_ready",
            "STELLA_BEAK_OPEN",
            "STELLA_BEAK_OPEN",
            "Ability",
            "STELLA_BEAK_ANGRY",
            "b:trigger('stellaAim',0.25)",
        ),
        (
            "Poppy",
            "Poppy_ready",
            "POPPY_BEAK_ANGRY",
            "POPPY_BODY_HAPPY_2",
            "Poppy_Power",
            "POPPY_BEAK_DIZZY_2",
            "startPoppyAim(auditBird)",
        ),
        (
            "Luca",
            "Luca_Ready",
            "LUCA_BEAK_GRUMPY",
            "LUCA_BEAK_HAPPY",
            "Luca_ability",
            "LUCA_BEAK_SCREAM",
            "b:trigger('lucaScream',0.25,0.5)",
        ),
        (
            "Willow",
            "Willow_Ready",
            "WILLOW_BEAK_ANGRY",
            "WILLOW_BEAK_HAPPY",
            "Willow_Spinning",
            "WILLOW_SPIN",
            "startWillowAbility(auditBird); b:trigger('willowSpinning')",
        ),
        (
            "Dahlia",
            "Dahlia_Ready",
            "DAHLIA_BEAK_GRIN",
            "DAHLIA_BEAK_HAPPY",
            "Dahlia_Spinning",
            "DAHLIA_BEAK_SCREAM",
            "b:trigger('kick',0.3)",
        ),
    ] {
        let name = create_bird(&runtime, bird);
        pulse(
            &runtime,
            "b:clearState(); selectedBird=nil; b:trigger('onSling')",
            0.0,
        );
        let idle_beak = if bird == "Stella" {
            "STELLA_BEAK_CLOSED".to_owned()
        } else {
            format!("{}_BEAK_NORMAL", bird.to_uppercase())
        };
        let idle = draw_face(&runtime, &name, &format!("{bird}_Idle"), &idle_beak);
        pulse(
            &runtime,
            "b:clearState(); selectedBird=auditBird; b:trigger('onSlingAiming')",
            0.5,
        );
        let aiming = draw_face(&runtime, &name, ready, ready_beak);
        assert_ne!(idle, aiming, "{bird}: ready did not replace idle face");
        pulse(
            &runtime,
            "b:clearState(); selectedBird=nil; setVelocity(auditBirdName,4,-3); b:trigger('inFlight')",
            0.0,
        );
        let ascending = draw_face(&runtime, &name, &format!("{bird}_Flying"), flight_beak);
        pulse(
            &runtime,
            "setVelocity(auditBirdName,4,3); b:trigger('inFlight')",
            0.0,
        );
        let descending = draw_face(&runtime, &name, &format!("{bird}_Flying"), flight_beak);
        assert_ne!(aiming, ascending, "{bird}: ready face persisted in flight");
        assert_ne!(
            ascending, descending,
            "{bird}: velocity-driven seek froze face targets"
        );
        pulse(&runtime, &format!("b:clearState(); {power_trigger}"), 0.1);
        let activated = draw_face(&runtime, &name, power, power_sprite);
        assert_ne!(
            descending, activated,
            "{bird}: ability did not replace flight face"
        );
    }
    assert!(runtime.fallback_calls.lock().unwrap().is_empty());
    assert!(runtime.compatibility_bindings.lock().unwrap().is_empty());
    assert!(
        !game_environment(runtime.lua())
            .unwrap()
            .get::<Option<bool>>("g_levelCompleted")
            .unwrap()
            .unwrap_or(false)
    );
}

fn collision(runtime: &StellaLua, bird: &str, target: &str) {
    // A component boundary fixture, with the native callback argument order
    // and post-Lua timer commit. The existing Poppy shot covers real TOI contact.
    dispatch_native_contact_callbacks(
        runtime.lua(),
        &runtime.render,
        vec![NativeContactCallback::Bird {
            first: bird.to_owned(),
            second: target.to_owned(),
            arm_collision_timer: true,
            force: 10.0,
            damage: 0.0,
            point_x: 0.0,
            point_y: 0.0,
            normal_x: 0.0,
            normal_y: -1.0,
        }],
    )
    .unwrap();
}

fn expire_collision_delays(runtime: &StellaLua) {
    // The shipped scheduler transfers newly queued calls on its first pass.
    runtime
        .execute_source(
            "g_realDt=1.25; updateDelayedCallbacks(0,20); updateDelayedCallbacks(1.25,21.25)",
        )
        .unwrap();
}

#[test]
fn five_birds_drum_callbacks_skip_ordinary_ability_loss_but_base_contacts_do_not() {
    let (_sandbox, runtime) = fixture("five-bird-drum");
    let drum = "BLOCK_POPPYSHOUSE_BIGDRUM_DRUMSKIN_1_2";
    let mut birds = Vec::new();
    for definition in ["Stella", "Poppy", "Luca", "Willow", "Dahlia"] {
        let name = create_bird(&runtime, definition);
        runtime
            .execute_source("auditBird.shot=true; setVelocity(auditBirdName,0,5)")
            .unwrap();
        collision(&runtime, &name, drum);
        assert!(
            runtime.render.lock().unwrap().scene[&name].velocity_y < 0.0,
            "{definition}: drum did not reflect velocity"
        );
        let world = object_world(runtime.lua()).unwrap();
        let bird = world.get::<Table>(name.as_str()).unwrap();
        let cover = world.get::<Table>(drum).unwrap();
        assert_eq!(
            cover
                .get::<Table>("drumBounceCounters")
                .unwrap()
                .get::<u32>(bird)
                .unwrap(),
            1
        );
        birds.push(name);
    }
    expire_collision_delays(&runtime);
    for name in &birds {
        let bird = object_world(runtime.lua())
            .unwrap()
            .get::<Table>(name.as_str())
            .unwrap();
        for flag in [
            "hasCollided",
            "abilityDisabled",
            "collisionTriggered",
            "pausingForAim",
            "delayedActivationTriggered",
        ] {
            assert!(
                !bird.get::<Option<bool>>(flag).unwrap().unwrap_or(false),
                "{name}: drum set {flag}"
            );
        }
        assert!(
            bird.get::<Option<Table>>("willowAbility")
                .unwrap()
                .is_none()
        );
    }
    runtime.execute_source(
        "for name,obj in pairs(objects.world) do \
         if obj.definition and string.find(obj.definition,'BIGDRUM') and not obj.isDrum then \
         auditDrumBase=name; break end end; if not auditDrumBase then error('missing authored drum base') end"
    ).unwrap();
    let base: String = game_environment(runtime.lua())
        .unwrap()
        .get("auditDrumBase")
        .unwrap();
    for name in [&birds[1], &birds[2]] {
        collision(&runtime, name, &base);
        let bird = object_world(runtime.lua())
            .unwrap()
            .get::<Table>(name.as_str())
            .unwrap();
        assert!(
            bird.get::<bool>("collisionTriggered").unwrap(),
            "{name}: base was treated as a drum cover"
        );
    }
    expire_collision_delays(&runtime);
    for name in [&birds[1], &birds[2]] {
        let bird = object_world(runtime.lua())
            .unwrap()
            .get::<Table>(name.as_str())
            .unwrap();
        assert!(
            bird.get::<bool>("abilityDisabled").unwrap(),
            "{name}: ordinary collision lost its original delay"
        );
    }
    assert!(runtime.fallback_calls.lock().unwrap().is_empty());
    assert!(runtime.compatibility_bindings.lock().unwrap().is_empty());
}
