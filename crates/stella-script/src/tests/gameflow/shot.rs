use super::*;

#[test]
fn shipped_first_level_host_drag_wins_and_persists_progress() {
    let sandbox = ShippedDataSandbox::new("original-first-shot");
    let runtime = StellaLua::new(&sandbox.data_root).unwrap();
    runtime.enable_local_services().unwrap();
    runtime.boot("scripts/game.lua").unwrap();
    advance(&runtime, 600, "first shot startup");
    runtime
        .execute_source("LevelLoad.transitionToLevel('Chapter01', 1)")
        .unwrap();
    advance(&runtime, 360, "first shot entry");
    assert_scene(&runtime, "GameScene", Some("Chapter01_L01"));
    let environment = game_environment(runtime.lua()).unwrap();
    let snapshot = runtime
        .lua()
        .load(
            r#"return function()
        local b = currentBirdName and objects.world[currentBirdName]
        local x,y = physicsToScreenTransform(levelStartPosition.x, levelStartPosition.y)
        return x,y,tostring(currentBirdName),not not birdReady,not not g_firstBirdShot,
            not not dragStarted,not not selectedBird,score
    end"#,
        )
        .set_environment(environment.clone())
        .eval::<Function>()
        .unwrap();
    type ShotState = (f64, f64, String, bool, bool, bool, bool, f64);
    let before: ShotState = snapshot.call(()).unwrap();
    eprintln!("[first-shot] before={before:?}");
    runtime.set_cursor(before.0, before.1, true).unwrap();
    advance(&runtime, 1, "press sling");
    eprintln!(
        "[first-shot] pressed={:?}",
        snapshot.call::<ShotState>(()).unwrap()
    );
    runtime
        .set_cursor(before.0 - 120.0, before.1 + 20.0, true)
        .unwrap();
    advance(&runtime, 60, "pull sling");
    eprintln!(
        "[first-shot] pulled={:?}",
        snapshot.call::<ShotState>(()).unwrap()
    );
    runtime
        .set_cursor(before.0 - 120.0, before.1 + 20.0, false)
        .unwrap();
    advance(&runtime, 5, "release sling");
    let fired: ShotState = snapshot.call(()).unwrap();
    eprintln!("[first-shot] fired={fired:?}");
    assert!(fired.4, "host drag did not launch a bird");
    advance(&runtime, 600, "first shot flight");
    let end: (bool, bool, f64) = runtime
        .lua()
        .load("return not not g_levelCompleted,not not g_levelFailed,score")
        .set_environment(environment.clone())
        .eval()
        .unwrap();
    eprintln!("[first-shot] settled={end:?}");
    assert!(end.0 && !end.1 && end.2 > 0.0, "shot did not naturally win");
    let saved = runtime.lua().load(r#"return function()
        return highscores['Chapter01_L01'] ~= nil and SettingsWrapper:getTimesLevelCompleted('Chapter01_L01') == 1
    end"#).set_environment(environment.clone()).eval::<Function>().unwrap();
    let mut ending_frames = 0_usize;
    while !saved.call::<bool>(()).unwrap() && ending_frames < 1200 {
        if ending_frames.is_multiple_of(120) {
            let timers: (f64,f64,f64,bool) = runtime.lua().load("return g_levelEndLogic.levelEndTimer,g_levelEndLogic.birdBuffTimer,g_levelEndLogic.birdsLeftCounter,not not g_disableGameupdate").set_environment(environment.clone()).eval().unwrap();
            eprintln!("[first-shot] pending ending +{ending_frames}f timers={timers:?}");
        }
        advance(&runtime, 1, "original ending timers");
        ending_frames += 1;
    }
    assert!(
        saved.call::<bool>(()).unwrap(),
        "natural ending never saved its result"
    );
    drop(saved);
    let final_score: f64 = runtime
        .lua()
        .load("return score")
        .set_environment(environment.clone())
        .eval()
        .unwrap();
    eprintln!("[first-shot] ending_frames={ending_frames}; final_score={final_score}");
    let read_progress = r#"local name = 'Chapter01_L01'
        local progress = SettingsWrapper:getProgress(getEpisodeForLevel(name))
        return highscores[name] and highscores[name].score,
            SettingsWrapper:getTimesLevelCompleted(name),
            SettingsWrapper:getTimesLevelFailed(name),
            progress and progress.level, SettingsWrapper:getNumber('starsCollected')
    "#;
    type Progress = (f64, u32, u32, String, u32);
    let completed: Progress = runtime
        .lua()
        .load(read_progress)
        .set_environment(environment.clone())
        .eval()
        .unwrap();
    eprintln!("[first-shot] completed={completed:?}");
    assert_eq!(completed.0, (final_score * 0.1).floor() * 10.0);
    assert_eq!(completed.1, 1);
    assert_eq!(completed.2, 0);
    assert_eq!(completed.3, "Chapter01_L01");
    assert!((1..=3).contains(&completed.4));
    for name in ["settings.lua", "highscores.lua"] {
        let bytes = fs::read(sandbox.root.join("appdata").join(name)).unwrap();
        let clear = stella_assets::decrypt_persistent_lua(&bytes).unwrap();
        assert!(
            String::from_utf8(clear).unwrap().contains("Chapter01_L01"),
            "{name} omitted completed level"
        );
    }
    assert!(runtime.fallback_calls.lock().unwrap().is_empty());
    assert!(runtime.compatibility_bindings.lock().unwrap().is_empty());
    advance(&runtime, 360, "completed screen animation");
    let (next_x, next_y, expected_level): (f64, f64, String) = runtime
        .lua()
        .load(
            r#"
        local frame = menuManager:getRoot():getChild('levelCompleted')
        if not frame then error('completed frame absent') end
        local button = frame:getChild('btnnextlevel')
        if not button or not button.visible then error('next button absent') end
        local x,y = button:getScreenPosition()
        local metadata = getLevelMetadata(levelName)
        if shouldShowExitCutscene(metadata) then return x,y,metadata.exitCutscene end
        if isLastLevel() or metadata.returnToMap then return x,y,getMapScreenForLevel(levelName) end
        return x,y,actions.myLevels[currentFolder][getLevelNumberForLevelName(levelName)+1]
    "#,
        )
        .set_environment(environment.clone())
        .eval()
        .unwrap();
    eprintln!("[first-shot] next-button=({next_x},{next_y}); expected={expected_level}");
    runtime.set_cursor(next_x, next_y, true).unwrap();
    advance(&runtime, 1, "press completed next");
    runtime.set_cursor(next_x, next_y, false).unwrap();
    advance(&runtime, 360, "completed next transition");
    assert_scene(&runtime, "GameScene", Some(&expected_level));
    eprintln!("[first-shot] next-scene={expected_level}");
    drop(snapshot);
    drop(environment);
    drop(runtime);
    let restored = StellaLua::new(&sandbox.data_root).unwrap();
    restored.enable_local_services().unwrap();
    restored.boot("scripts/game.lua").unwrap();
    advance(&restored, 600, "saved progress startup");
    let recovered: Progress = restored
        .lua()
        .load(read_progress)
        .set_environment(game_environment(restored.lua()).unwrap())
        .eval()
        .unwrap();
    assert_eq!(recovered, completed, "new VM lost native saved progress");
    eprintln!("[first-shot] restored={recovered:?}");
    assert!(restored.fallback_calls.lock().unwrap().is_empty());
    assert!(restored.compatibility_bindings.lock().unwrap().is_empty());
}
