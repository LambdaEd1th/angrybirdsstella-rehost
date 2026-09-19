use super::*;

fn tick(runtime: &StellaLua, frame: &mut usize, count: usize, stage: &str) {
    advance(runtime, count, stage);
    *frame += count;
}

fn pointer(runtime: &StellaLua, frame: usize, x: f64, y: f64, down: bool) {
    eprintln!("[progression-input] frame={frame} x={x} y={y} down={down}");
    runtime.set_cursor(x, y, down).unwrap();
}

type Progress = (u32, bool, Vec<(f64, u32, u32)>);

fn progress(runtime: &StellaLua) -> Progress {
    let environment = game_environment(runtime.lua()).unwrap();
    let (stars, skin) = runtime.lua().load("return SettingsWrapper:getNumber('starsCollected'),SettingsWrapper:isBirdSkinUnlocked('Stella')")
        .set_environment(environment.clone()).eval().unwrap();
    let mut levels = Vec::new();
    for ordinal in 1..=5 {
        let name = format!("Chapter01_L{ordinal:02}");
        let state: (f64, u32, u32) = runtime.lua().load("local name=...; return highscores[name] and highscores[name].score or 0,SettingsWrapper:getTimesLevelCompleted(name),SettingsWrapper:getTimesLevelFailed(name)")
            .set_environment(environment.clone()).call(name).unwrap();
        assert!(state.0 > 0.0);
        assert_eq!((state.1, state.2), (1, 0));
        levels.push(state);
    }
    (stars, skin, levels)
}

fn assert_available_birds(runtime: &StellaLua) -> usize {
    assert_scene(runtime, "GameScene", Some("Chapter01_L06"));
    let diagnostics: String = runtime.lua().load("local r={} for _,b in pairs(birds) do local l=b.birdLock; r[#r+1]=b.name..':'..tostring(l and l.starLimit)..':'..tostring(l and l.shouldUnlock)..':'..tostring(l and l.locked) end; return tostring(SettingsWrapper:getNumber('starsCollected'))..' '..table.concat(r,',')")
        .set_environment(game_environment(runtime.lua()).unwrap()).eval().unwrap();
    eprintln!("[progression] lock entry {diagnostics}");
    let mut elapsed = 0;
    let (total, available, ready) = loop {
        let state: (u32, u32, bool) = runtime.lua().load("local n=0 for _,b in pairs(birds) do if not b.shot and not b:isLocked() then n=n+1 end end; return getRemainingBirdCount(),n,not not birdReady")
        .set_environment(game_environment(runtime.lua()).unwrap()).eval().unwrap();
        if state == (3, 3, true) || elapsed >= 1200 {
            break state;
        }
        advance(runtime, 30, "original bird unlock animations");
        elapsed += 30;
    };
    eprintln!("[progression] locks settled after {elapsed} frames: {total}/{available}/{ready}");
    assert_eq!((total, available, ready), (3, 3, true));
    assert!(runtime.fallback_calls.lock().unwrap().is_empty());
    assert!(runtime.compatibility_bindings.lock().unwrap().is_empty());
    elapsed
}

#[test]
fn shipped_first_five_levels_earn_stars_and_restore_unlocked_birds() {
    let sandbox = ShippedDataSandbox::new("original-first-five-progression");
    let runtime = StellaLua::new(&sandbox.data_root).unwrap();
    runtime.enable_local_services().unwrap();
    runtime.boot("scripts/game.lua").unwrap();
    let mut frame = 0;
    tick(&runtime, &mut frame, 600, "progression startup");
    runtime
        .execute_source("LevelLoad.transitionToLevel('Chapter01',1)")
        .unwrap();
    tick(&runtime, &mut frame, 360, "first level entry");
    let (x, y): (f64, f64) = runtime
        .lua()
        .load("return physicsToScreenTransform(levelStartPosition.x,levelStartPosition.y)")
        .set_environment(game_environment(runtime.lua()).unwrap())
        .eval()
        .unwrap();
    pointer(&runtime, frame, x, y, true);
    tick(&runtime, &mut frame, 1, "first sling press");
    pointer(&runtime, frame, x - 120.0, y + 20.0, true);
    tick(&runtime, &mut frame, 60, "first sling pull");
    pointer(&runtime, frame, x - 120.0, y + 20.0, false);
    tick(&runtime, &mut frame, 1560, "first flight and ending");
    let won: bool = runtime
        .lua()
        .load("return not not g_levelCompleted and not g_levelFailed")
        .set_environment(game_environment(runtime.lua()).unwrap())
        .eval()
        .unwrap();
    assert!(won, "first level did not naturally win");
    practice::next_level(&runtime, &mut frame, "Chapter01_L02");
    tap::complete_tap(&runtime, &mut frame);
    practice::next_level(&runtime, &mut frame, "Chapter01_L03");
    hold::complete_hold(&runtime, &mut frame);
    practice::next_level(&runtime, &mut frame, "Chapter01_L04");
    practice::complete_practice(&runtime, &mut frame, "Chapter01_L04");
    practice::next_level(&runtime, &mut frame, "Chapter01_L05");
    practice::complete_practice(&runtime, &mut frame, "Chapter01_L05");
    practice::next_level(&runtime, &mut frame, "Chapter01_L06");
    frame += assert_available_birds(&runtime);
    let completed = progress(&runtime);
    eprintln!("[progression] completed frame={frame} progress={completed:?}");
    assert_eq!((completed.0, completed.1), (15, true));
    earned_retry::exhaust_and_refuse(&runtime, &mut frame);
    assert_eq!(progress(&runtime), completed);
    runtime.set_application_active(false).unwrap();
    drop(runtime);
    let restored = StellaLua::new(&sandbox.data_root).unwrap();
    restored.enable_local_services().unwrap();
    restored.boot("scripts/game.lua").unwrap();
    advance(&restored, 600, "progression saved startup");
    assert_eq!(progress(&restored), completed);
    restored
        .execute_source("LevelLoad.transitionToLevel('Chapter01',6)")
        .unwrap();
    advance(&restored, 360, "restored unlocked level entry");
    assert_available_birds(&restored);
    let persisted: (u32, u32) = restored
        .lua()
        .load("return Coins:getAmount(),SettingsWrapper:getTimesLevelFailed('Chapter01_L06')")
        .set_environment(game_environment(restored.lua()).unwrap())
        .eval()
        .unwrap();
    assert_eq!(persisted, (66, 1));
}
