use super::stella_route::{aim, aim_object, currency, launch, tick};
use super::*;

pub(super) fn saved_result(runtime: &StellaLua) -> (f64, u32, u32, u32) {
    runtime
        .lua()
        .load(
            r#"
        local h=highscores.Chapter01_L08
        local score=h and h.score or 0
        return score,getLevelStars('Chapter01_L08',score),
            SettingsWrapper:getTimesLevelCompleted('Chapter01_L08'),
            SettingsWrapper:getTimesLevelFailed('Chapter01_L08')
    "#,
        )
        .set_environment(game_environment(runtime.lua()).unwrap())
        .eval()
        .unwrap()
}

pub(super) fn complete(runtime: &StellaLua, frame: &mut usize) -> (f64, u32, u32, u32) {
    assert_scene(runtime, "GameScene", Some("Chapter01_L08"));
    assert_eq!(saved_result(runtime), (0.0, 0, 0, 0));
    let before = currency(runtime);
    let environment = game_environment(runtime.lua()).unwrap();
    let stars: u32 = runtime
        .lua()
        .load("return SettingsWrapper:getNumber('starsCollected')")
        .set_environment(environment.clone())
        .eval()
        .unwrap();
    eprintln!("[eighth] start frame={frame}");
    tick(runtime, frame, 240, "eighth initial camera settle");
    launch(runtime, frame, -120.0, 0.0, 1, "eighth");
    tick(runtime, frame, 109, "eighth flight under tree");
    aim_object(runtime, frame, "BURCKET_1", "eighth");
    tick(
        runtime,
        frame,
        1499,
        "eighth bucket and fruit chain reaction",
    );
    let remaining:u32=runtime.lua().load(r#"
        local n=0
        for name in pairs(levelGoals) do
            if name~='pig_medium_10' and name~='pig_medium_11' then error('eighth upper structure survived bucket attack') end
            n=n+1
        end
        return n
    "#).set_environment(environment.clone()).eval().unwrap();
    assert!(remaining <= 2);
    let mut shots = 1;
    // The chain reaction may also remove a lower pig. Aim only at the actual
    // surviving goals; never replay coordinates for a pig already eliminated.
    for shot in 2..=3 {
        let target:Option<String>=runtime.lua().load("if levelGoals.pig_medium_11 then return 'pig_medium_11' elseif levelGoals.pig_medium_10 then return 'pig_medium_10' end")
            .set_environment(environment.clone()).eval().unwrap();
        let Some(target) = target else {
            break;
        };
        launch(runtime, frame, -120.0, 20.0, shot, "eighth");
        shots = shot;
        tick(runtime, frame, 79, "eighth lower structure approach");
        aim(runtime, frame, &target, "eighth");
        tick(
            runtime,
            frame,
            1529,
            "eighth lower collapse and camera return",
        );
    }
    let deadline = *frame + 1800;
    loop {
        let (won, failed): (bool, bool) = runtime
            .lua()
            .load("return not not g_levelCompleted,not not g_levelFailed")
            .set_environment(environment.clone())
            .eval()
            .unwrap();
        assert!(!failed, "eighth natural route failed");
        if won {
            break;
        }
        assert!(
            *frame < deadline,
            "eighth route did not finish within available birds"
        );
        tick(runtime, frame, 1, "eighth completion transition");
    }
    tick(
        runtime,
        frame,
        1200,
        "eighth original bonus and reward animation",
    );
    let result = saved_result(runtime);
    assert!(result.0 > 0.0 && (1..=3).contains(&result.1));
    assert_eq!((result.2, result.3), (1, 0));
    let state:(u32,u32,u32,u32)=runtime.lua().load("local n=0 for _ in pairs(levelGoals) do n=n+1 end;return n,birdsShot,SettingsWrapper:getNumber('starsCollected'),currentLevelStats.coinsCollected")
        .set_environment(environment).eval().unwrap();
    assert_eq!((state.0, state.1, state.2), (0, shots, stars + result.1));
    assert_eq!(
        currency(runtime),
        (before.0 + state.3, before.1 + state.3, before.2)
    );
    eprintln!("[eighth] won frame={frame} result={result:?} state={state:?}");
    practice::next_level(runtime, frame, "Chapter01_L09");
    eprintln!("[eighth] next frame={frame}");
    assert!(runtime.fallback_calls.lock().unwrap().is_empty());
    assert!(runtime.compatibility_bindings.lock().unwrap().is_empty());
    result
}
