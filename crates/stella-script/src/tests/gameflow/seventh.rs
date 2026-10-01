use super::stella_route::{aim, currency, launch, tick};
use super::*;

pub(super) fn saved_result(runtime: &StellaLua) -> (f64, u32, u32, u32) {
    runtime
        .lua()
        .load(
            r#"
        local h=highscores.Chapter01_L07
        local score=h and h.score or 0
        return score,getLevelStars('Chapter01_L07',score),
            SettingsWrapper:getTimesLevelCompleted('Chapter01_L07'),
            SettingsWrapper:getTimesLevelFailed('Chapter01_L07')
    "#,
        )
        .set_environment(game_environment(runtime.lua()).unwrap())
        .eval()
        .unwrap()
}

pub(super) fn complete(runtime: &StellaLua, frame: &mut usize) -> (f64, u32, u32, u32) {
    assert_scene(runtime, "GameScene", Some("Chapter01_L07"));
    assert_eq!(saved_result(runtime), (0.0, 0, 0, 0));
    let environment = game_environment(runtime.lua()).unwrap();
    let stars: u32 = runtime
        .lua()
        .load("return SettingsWrapper:getNumber('starsCollected')")
        .set_environment(environment.clone())
        .eval()
        .unwrap();
    let before_currency = currency(runtime);
    eprintln!("[seventh] start frame={frame}");
    tick(runtime, frame, 240, "seventh camera settle");
    launch(runtime, frame, -120.0, 170.0, 1, "seventh");
    tick(runtime, frame, 209, "seventh left structure approach");
    aim(runtime, frame, "pig_medium_right_3", "seventh");
    tick(runtime, frame, 1399, "seventh collapse and camera return");
    let goals: (u32,bool)=runtime.lua().load("local n=0 for _ in pairs(levelGoals) do n=n+1 end;return n,levelGoals.pig_medium_right_4~=nil")
        .set_environment(environment.clone()).eval().unwrap();
    assert_eq!(goals, (1, true));
    launch(runtime, frame, -170.0, 220.0, 2, "seventh");
    tick(
        runtime,
        frame,
        149,
        "seventh high approach over right platform",
    );
    aim(runtime, frame, "pig_medium_right_4", "seventh");
    let deadline = *frame + 3600;
    loop {
        let (won, failed): (bool, bool) = runtime
            .lua()
            .load("return not not g_levelCompleted,not not g_levelFailed")
            .set_environment(environment.clone())
            .eval()
            .unwrap();
        assert!(!failed, "seventh two-bird route failed");
        if won {
            break;
        }
        assert!(*frame < deadline, "seventh did not complete naturally");
        tick(runtime, frame, 1, "seventh right structure collapse");
    }
    tick(runtime, frame, 1200, "seventh remaining bird bonuses");
    let result = saved_result(runtime);
    assert!(result.0 > 0.0 && (1..=3).contains(&result.1));
    assert_eq!((result.2, result.3), (1, 0));
    let state:(u32,u32,u32)=runtime.lua().load("local n=0 for _ in pairs(levelGoals) do n=n+1 end;return n,birdsShot,SettingsWrapper:getNumber('starsCollected')")
        .set_environment(environment).eval().unwrap();
    assert_eq!(state, (0, 2, stars + result.1));
    let coins = currency(runtime);
    // Coins.updateCollect increments the original level statistic when the
    // collection animation grants inventory. Account for that exact amount.
    let collected: u32 = runtime
        .lua()
        .load("return currentLevelStats.coinsCollected")
        .set_environment(game_environment(runtime.lua()).unwrap())
        .eval()
        .unwrap();
    assert_eq!(
        coins,
        (
            before_currency.0 + collected,
            before_currency.1 + collected,
            before_currency.2
        )
    );
    eprintln!("[seventh] won frame={frame} result={result:?} currency={coins:?}");
    practice::next_level(runtime, frame, "Chapter01_L08");
    eprintln!("[seventh] next frame={frame}");
    assert!(runtime.fallback_calls.lock().unwrap().is_empty());
    assert!(runtime.compatibility_bindings.lock().unwrap().is_empty());
    result
}
