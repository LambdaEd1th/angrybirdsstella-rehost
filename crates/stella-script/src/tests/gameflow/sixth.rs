use super::stella_route::{aim, launch, tick};
use super::*;

pub(super) fn saved_result(runtime: &StellaLua) -> (f64, u32, u32, u32) {
    runtime
        .lua()
        .load(
            r#"
        local h=highscores.Chapter01_L06
        local score=h and h.score or 0
        return score,getLevelStars('Chapter01_L06',score),
            SettingsWrapper:getTimesLevelCompleted('Chapter01_L06'),
            SettingsWrapper:getTimesLevelFailed('Chapter01_L06')
    "#,
        )
        .set_environment(game_environment(runtime.lua()).unwrap())
        .eval()
        .unwrap()
}

// The shipped StarTrack.init numeric loop includes the previous star total.
// Completing a level from exactly 15 stars queues StarReward_15 again; startup
// grants that pending reward. Preserve this original script behavior.
pub(super) fn assert_currency(runtime: &StellaLua, restored: bool, collected: u32) {
    let state: (u32, u32, u32, u32, u32, String) = runtime
        .lua()
        .load(
            r#"
        local pending=SettingsWrapper:getPendingRewards('coins')
        local count=0
        for key,reward in pairs(pending) do
            if key~='StarReward_15' or reward.amount~=66 or reward.screen~='Level end' then
                error('unexpected sixth pending reward')
            end
            count=count+1
        end
        return Coins:getAmount(),settings.iap.gained.coins,settings.iap.used.coins,
            count,pending.StarReward_15 and pending.StarReward_15.amount or 0,
            pending.StarReward_15 and pending.StarReward_15.screen or ''
    "#,
        )
        .set_environment(game_environment(runtime.lua()).unwrap())
        .eval()
        .unwrap();
    let expected = if restored {
        (72 + collected, 132 + collected, 60, 0, 0, String::new())
    } else {
        (
            6 + collected,
            66 + collected,
            60,
            1,
            66,
            "Level end".to_owned(),
        )
    };
    assert_eq!(state, expected);
    eprintln!("[sixth] currency restored={restored} state={state:?}");
}

pub(super) fn complete(runtime: &StellaLua, frame: &mut usize) -> ((f64, u32, u32, u32), u32) {
    eprintln!("[sixth] start frame={frame}");
    assert_scene(runtime, "GameScene", Some("Chapter01_L06"));
    let before = saved_result(runtime);
    assert_eq!(before, (0.0, 0, 0, 2));
    tick(runtime, frame, 240, "sixth initial camera settle");
    launch(runtime, frame, -60.0, 190.0, 1, "sixth");
    tick(runtime, frame, 139, "sixth high approach above platform");
    aim(runtime, frame, "pig_medium_right_1", "sixth");
    tick(runtime, frame, 869, "sixth first bird and camera return");
    let goals: u32 = runtime
        .lua()
        .load("local n=0 for _ in pairs(levelGoals) do n=n+1 end;return n")
        .set_environment(game_environment(runtime.lua()).unwrap())
        .eval()
        .unwrap();
    assert_eq!(goals, 1, "high-platform attack did not remove its pig");
    launch(runtime, frame, -120.0, 170.0, 2, "sixth");
    tick(runtime, frame, 69, "sixth right platform approach");
    aim(runtime, frame, "pig_medium_1", "sixth");
    let environment = game_environment(runtime.lua()).unwrap();
    let deadline = *frame + 3600;
    loop {
        let (won, failed): (bool, bool) = runtime
            .lua()
            .load("return not not g_levelCompleted,not not g_levelFailed")
            .set_environment(environment.clone())
            .eval()
            .unwrap();
        assert!(!failed, "two-bird sixth-level route failed");
        if won {
            break;
        }
        assert!(*frame < deadline, "sixth level did not complete naturally");
        tick(runtime, frame, 1, "sixth structural collapse");
    }
    tick(
        runtime,
        frame,
        1200,
        "sixth unused bird bonus and result save",
    );
    let result = saved_result(runtime);
    assert!(result.0 > 0.0 && (1..=3).contains(&result.1));
    assert_eq!((result.2, result.3), (1, 2));
    let state:(u32,u32,u32,u32,u32)=runtime.lua().load("local n=0 for _ in pairs(levelGoals) do n=n+1 end;return n,birdsShot,Coins:getAmount(),SettingsWrapper:getNumber('starsCollected'),currentLevelStats.coinsCollected")
        .set_environment(environment).eval().unwrap();
    // Original Coins.updateCollect can grant a drop in L06 as well as later
    // levels. Keep that exact count separate from the pending 66-star reward.
    let collected = state.4;
    assert_eq!(state, (0, 2, 6 + collected, 15 + result.1, collected));
    assert_currency(runtime, false, collected);
    eprintln!("[sixth] won frame={frame} result={result:?} state={state:?}");
    practice::next_level(runtime, frame, "Chapter01_L07");
    eprintln!("[sixth] next frame={frame}");
    assert_currency(runtime, false, collected);
    assert!(runtime.fallback_calls.lock().unwrap().is_empty());
    assert!(runtime.compatibility_bindings.lock().unwrap().is_empty());
    (result, collected)
}
