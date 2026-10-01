use super::stella_route::{aim, currency, launch, tick};
use super::*;

pub(super) fn saved_result(runtime: &StellaLua) -> (f64, u32, u32, u32) {
    runtime
        .lua()
        .load(
            r#"
        local h=highscores.Chapter01_L11
        local score=h and h.score or 0
        return score,getLevelStars('Chapter01_L11',score),
            SettingsWrapper:getTimesLevelCompleted('Chapter01_L11'),
            SettingsWrapper:getTimesLevelFailed('Chapter01_L11')
    "#,
        )
        .set_environment(game_environment(runtime.lua()).unwrap())
        .eval()
        .unwrap()
}

pub(super) fn complete(runtime: &StellaLua, frame: &mut usize) -> (f64, u32, u32, u32) {
    assert_scene(runtime, "GameScene", Some("Chapter01_L11"));
    assert_eq!(saved_result(runtime), (0.0, 0, 0, 0));
    let before = currency(runtime);
    let environment = game_environment(runtime.lua()).unwrap();
    let (stars,goals,bird_count,shot_count):(u32,u32,u32,u32)=runtime.lua().load("local n=0 for _ in pairs(levelGoals) do n=n+1 end;return SettingsWrapper:getNumber('starsCollected'),n,getRemainingBirdCount(),birdsShot")
        .set_environment(environment.clone()).eval().unwrap();
    assert_eq!((goals, bird_count, shot_count), (7, 3, 0));
    eprintln!("[eleventh] start frame={frame}");
    tick(runtime, frame, 240, "eleventh camera settle");
    launch(runtime, frame, -200.0, 240.0, 1, "eleventh");
    tick(runtime, frame, 129, "eleventh left tower approach");
    aim(runtime, frame, "pig_medium_right_2", "eleventh");
    tick(runtime, frame, 1479, "eleventh first two towers collapse");
    let remaining: Vec<String> = runtime.lua().load("local names={} for name in pairs(levelGoals) do names[#names+1]=name end;table.sort(names);return names")
        .set_environment(environment.clone()).eval().unwrap();
    assert!(
        remaining.iter().all(|name| name == "pig_medium_5"),
        "eleventh lower towers retained a pig: {remaining:?}"
    );
    let mut shots = 1;
    if !remaining.is_empty() {
        launch(runtime, frame, -120.0, 360.0, 2, "eleventh");
        tick(runtime, frame, 129, "eleventh skateboard tower approach");
        aim(runtime, frame, "pig_medium_5", "eleventh");
        shots = 2;
    }
    let deadline = *frame + 1800;
    loop {
        let (won, failed): (bool, bool) = runtime
            .lua()
            .load("return not not g_levelCompleted,not not g_levelFailed")
            .set_environment(environment.clone())
            .eval()
            .unwrap();
        assert!(!failed, "eleventh natural route failed");
        if won {
            break;
        }
        assert!(
            *frame < deadline,
            "eleventh did not finish within original birds"
        );
        tick(runtime, frame, 1, "eleventh completion transition");
    }
    tick(runtime, frame, 1200, "eleventh original result and rewards");
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
    eprintln!("[eleventh] won frame={frame} result={result:?} state={state:?}");
    let earned = currency(runtime);
    practice::click_next(runtime, frame);
    let deadline = *frame + 2400;
    loop {
        let ready: bool = runtime.lua().load("return levelName=='Chapter01_G01' and menuManager:getRoot().name=='GameScene' and notificationsFrame:getChild('levelLoadTransition')==nil")
            .set_environment(game_environment(runtime.lua()).unwrap()).eval().unwrap();
        if ready {
            break;
        }
        assert!(
            *frame < deadline,
            "eleventh exit comic/map did not reach first gate"
        );
        tick(
            runtime,
            frame,
            30,
            "eleventh exit comic and first gate reveal",
        );
    }
    assert_scene(runtime, "GameScene", Some("Chapter01_G01"));
    let gate: (u32,bool,u32,String,u32,u32,u32,u32,bool,bool) = runtime.lua().load("return SettingsWrapper:getMapAreaProgress('Chapter01'),SettingsWrapper:hasSeenCutscene('episode_1_generic_gate'),GateManager:getCurrentGateNumber(),GateManager:getGateState(1),SettingsWrapper:getTimesLevelCompleted('Chapter01_G01'),SettingsWrapper:getTimesLevelFailed('Chapter01_G01'),birdsShot,getRemainingBirdCount(),not not g_levelCompleted,not not g_levelFailed")
        .set_environment(game_environment(runtime.lua()).unwrap()).eval().unwrap();
    assert_eq!(
        gate,
        (3, true, 1, "opened".to_owned(), 0, 0, 0, 6, false, false)
    );
    assert_eq!(currency(runtime), earned);
    assert_eq!(saved_result(runtime), result);
    eprintln!("[eleventh] next frame={frame}");
    assert!(runtime.fallback_calls.lock().unwrap().is_empty());
    assert!(runtime.compatibility_bindings.lock().unwrap().is_empty());
    result
}
