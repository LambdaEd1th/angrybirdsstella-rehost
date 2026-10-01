use super::stella_route::{aim, aim_object, currency, launch, tick};
use super::*;

pub(super) fn saved_result(runtime: &StellaLua) -> (f64, u32, u32, u32) {
    runtime
        .lua()
        .load(
            r#"
        local h=highscores.Chapter01_L09
        local score=h and h.score or 0
        return score,getLevelStars('Chapter01_L09',score),
            SettingsWrapper:getTimesLevelCompleted('Chapter01_L09'),
            SettingsWrapper:getTimesLevelFailed('Chapter01_L09')
    "#,
        )
        .set_environment(game_environment(runtime.lua()).unwrap())
        .eval()
        .unwrap()
}

pub(super) fn complete(runtime: &StellaLua, frame: &mut usize) -> (f64, u32, u32, u32) {
    assert_scene(runtime, "GameScene", Some("Chapter01_L09"));
    assert_eq!(saved_result(runtime), (0.0, 0, 0, 0));
    let before = currency(runtime);
    let environment = game_environment(runtime.lua()).unwrap();
    let (stars,goals,bird_count,shot_count):(u32,u32,u32,u32)=runtime.lua().load("local n=0 for _ in pairs(levelGoals) do n=n+1 end;return SettingsWrapper:getNumber('starsCollected'),n,getRemainingBirdCount(),birdsShot")
        .set_environment(environment.clone()).eval().unwrap();
    assert_eq!((goals, bird_count, shot_count), (3, 4, 0));
    eprintln!("[ninth] start frame={frame}");
    tick(runtime, frame, 240, "ninth camera settle");
    launch(runtime, frame, -240.0, 90.0, 1, "ninth");
    tick(runtime, frame, 219, "ninth flight past tower");
    aim_object(runtime, frame, "BOMB_FRUIT_5", "ninth");
    tick(runtime, frame, 1389, "ninth fruit pile and tower collapse");
    let remaining:u32=runtime.lua().load(r#"
        if objects.world.BOMB_FRUIT_5 then error('ninth attacked fruit survived') end
        local n=0
        for name in pairs(levelGoals) do
            if name~='pig_medium_right_1' and name~='pig_medium_right_2' and name~='pig_medium_right_3' then error('ninth unexpected goal') end
            n=n+1
        end
        return n
    "#).set_environment(environment.clone()).eval().unwrap();
    assert!(remaining < 3, "ninth fruit attack removed no pigs");
    let mut shots = 1;
    for shot in 2..=4 {
        let target: Option<String> = runtime
            .lua()
            .load(
                r#"
            local chosen=nil
            for name,pig in pairs(levelGoals) do
                if not chosen or pig.x<levelGoals[chosen].x or
                    (pig.x==levelGoals[chosen].x and name<chosen) then chosen=name end
            end
            return chosen
        "#,
            )
            .set_environment(environment.clone())
            .eval()
            .unwrap();
        let Some(target) = target else {
            break;
        };
        launch(runtime, frame, -240.0, 10.0, shot, "ninth");
        shots = shot;
        tick(runtime, frame, 99, "ninth lower structure approach");
        aim(runtime, frame, &target, "ninth");
        tick(
            runtime,
            frame,
            1509,
            "ninth cleanup collapse and camera return",
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
        assert!(!failed, "ninth natural route failed");
        if won {
            break;
        }
        assert!(
            *frame < deadline,
            "ninth did not finish within original birds"
        );
        tick(runtime, frame, 1, "ninth completion transition");
    }
    tick(runtime, frame, 1200, "ninth original result and rewards");
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
    eprintln!("[ninth] won frame={frame} result={result:?} state={state:?}");
    practice::next_level(runtime, frame, "Chapter01_L10");
    eprintln!("[ninth] next frame={frame}");
    assert!(runtime.fallback_calls.lock().unwrap().is_empty());
    assert!(runtime.compatibility_bindings.lock().unwrap().is_empty());
    result
}
