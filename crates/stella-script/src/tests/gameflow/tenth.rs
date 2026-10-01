use super::stella_route::{aim, currency, launch, tick};
use super::*;

pub(super) fn saved_result(runtime: &StellaLua) -> (f64, u32, u32, u32) {
    runtime
        .lua()
        .load(
            r#"
        local h=highscores.Chapter01_L10
        local score=h and h.score or 0
        return score,getLevelStars('Chapter01_L10',score),
            SettingsWrapper:getTimesLevelCompleted('Chapter01_L10'),
            SettingsWrapper:getTimesLevelFailed('Chapter01_L10')
    "#,
        )
        .set_environment(game_environment(runtime.lua()).unwrap())
        .eval()
        .unwrap()
}

pub(super) fn complete(runtime: &StellaLua, frame: &mut usize) -> (f64, u32, u32, u32) {
    assert_scene(runtime, "GameScene", Some("Chapter01_L10"));
    assert_eq!(saved_result(runtime), (0.0, 0, 0, 0));
    let before = currency(runtime);
    let environment = game_environment(runtime.lua()).unwrap();
    let (stars,goals,bird_count,shot_count):(u32,u32,u32,u32)=runtime.lua().load("local n=0 for _ in pairs(levelGoals) do n=n+1 end;return SettingsWrapper:getNumber('starsCollected'),n,getRemainingBirdCount(),birdsShot")
        .set_environment(environment.clone()).eval().unwrap();
    assert_eq!((goals, bird_count, shot_count), (5, 4, 0));
    eprintln!("[tenth] start frame={frame}");
    tick(runtime, frame, 240, "tenth camera settle");
    launch(runtime, frame, -240.0, 0.0, 1, "tenth");
    tick(runtime, frame, 79, "tenth left structure approach");
    aim(runtime, frame, "pig_medium_right_2", "tenth");
    tick(runtime, frame, 1529, "tenth left structure collapse");
    let remaining: Vec<String> = runtime.lua().load("local names={} for name in pairs(levelGoals) do names[#names+1]=name end;table.sort(names);return names")
        .set_environment(environment.clone()).eval().unwrap();
    assert!(
        remaining
            .iter()
            .all(|name| matches!(name.as_str(), "pig_medium_right_5" | "pig_medium_right_6")),
        "tenth left structure retained a pig: {remaining:?}"
    );
    let mut shots = 1;
    if !remaining.is_empty() {
        let target = if remaining.iter().any(|name| name == "pig_medium_right_6") {
            "pig_medium_right_6"
        } else {
            "pig_medium_right_5"
        };
        launch(runtime, frame, -240.0, 220.0, 2, "tenth");
        tick(runtime, frame, 139, "tenth right tower approach");
        aim(runtime, frame, target, "tenth");
        shots = 2;
    }
    // The authored level supplies four birds. Let the second collapse settle,
    // then use remaining real birds if its reflected path left the tower alive.
    tick(runtime, frame, 1800, "tenth right tower collapse");
    let query: mlua::Function = runtime
        .lua()
        .load(include_str!("first_gate_aim.lua"))
        .set_environment(environment.clone())
        .eval()
        .unwrap();
    for shot in 3..=bird_count {
        let remaining: String = runtime
            .lua()
            .load(
                r#"
            local rows={}
            for name,g in pairs(levelGoals) do
                rows[#rows+1]=string.format('%s@(%.4f,%.4f)',name,g.x,g.y)
            end
            table.sort(rows);return table.concat(rows,', ')
        "#,
            )
            .set_environment(environment.clone())
            .eval()
            .unwrap();
        if remaining.is_empty() {
            break;
        }
        eprintln!("[tenth] cleanup shot={shot} frame={frame} remaining={remaining}");
        let start = *frame;
        launch(runtime, frame, -240.0, 220.0, shot, "tenth");
        tick(runtime, frame, 64, "tenth live cleanup approach");
        super::first_gate::aim_path(runtime, frame, start + 340, &query, 14000);
        shots = shot;
        tick(runtime, frame, 1800, "tenth cleanup collapse");
    }
    let deadline = *frame + 1800;
    loop {
        let (won, failed): (bool, bool) = runtime
            .lua()
            .load("return not not g_levelCompleted,not not g_levelFailed")
            .set_environment(environment.clone())
            .eval()
            .unwrap();
        assert!(!failed, "tenth natural route failed");
        if won {
            break;
        }
        assert!(
            *frame < deadline,
            "tenth did not finish within original birds"
        );
        tick(runtime, frame, 1, "tenth completion transition");
    }
    tick(runtime, frame, 1200, "tenth original result and rewards");
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
    eprintln!("[tenth] won frame={frame} result={result:?} state={state:?}");
    practice::next_level(runtime, frame, "Chapter01_L11");
    eprintln!("[tenth] next frame={frame}");
    assert!(runtime.fallback_calls.lock().unwrap().is_empty());
    assert!(runtime.compatibility_bindings.lock().unwrap().is_empty());
    result
}
