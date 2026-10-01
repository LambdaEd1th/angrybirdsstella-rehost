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
fn shipped_earned_progression_handles_last_chance_gate_and_poppy_tutorials() {
    let sandbox = ShippedDataSandbox::new("original-first-five-progression");
    let runtime = StellaLua::new(&sandbox.data_root).unwrap();
    runtime.enable_local_services().unwrap();
    runtime.boot("scripts/game.lua").unwrap();
    let mut frame = 0;
    tick(&runtime, &mut frame, 600, "progression startup");
    runtime.execute_source("LevelLoad.transitionToLevel('cutscenes',getLevelNumberForLevelName('episode_1_cutscene_1_intro'))").unwrap();
    tick(
        &runtime,
        &mut frame,
        2400,
        "original chapter opening and map reveal",
    );
    assert_scene(&runtime, "GameScene", Some("Chapter01"));
    let opening: (u32,bool,Option<u32>,u32) = runtime.lua().load("return SettingsWrapper:getMapAreaProgress('Chapter01'),SettingsWrapper:hasSeenCutscene('episode_1_cutscene_1_intro'),SettingsWrapper:getNumber('starsCollected'),Coins:getAmount()")
        .set_environment(game_environment(runtime.lua()).unwrap()).eval().unwrap();
    assert_eq!(opening, (2, true, None, 0));

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
    last_chance::buy_play_and_decline_insufficient_coins(&runtime, &mut frame);
    assert_eq!(progress(&runtime), completed);
    let (sixth, sixth_collected) = sixth::complete(&runtime, &mut frame);
    let advanced = progress(&runtime);
    assert_eq!(advanced.0, 15 + sixth.1);
    assert_eq!((&advanced.1, &advanced.2), (&completed.1, &completed.2));
    runtime.set_application_active(false).unwrap();
    drop(runtime);
    let restored = StellaLua::new(&sandbox.data_root).unwrap();
    restored.enable_local_services().unwrap();
    restored.boot("scripts/game.lua").unwrap();
    advance(&restored, 600, "progression saved startup");
    assert_eq!(progress(&restored), advanced);
    assert_eq!(sixth::saved_result(&restored), sixth);
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
    assert_eq!(persisted, (72 + sixth_collected, 2));
    sixth::assert_currency(&restored, true, sixth_collected);
    restored.set_application_active(false).unwrap();
    drop(restored);
    let restored_again = StellaLua::new(&sandbox.data_root).unwrap();
    restored_again.enable_local_services().unwrap();
    restored_again.boot("scripts/game.lua").unwrap();
    advance(&restored_again, 600, "second orderly progression restart");
    sixth::assert_currency(&restored_again, true, sixth_collected);
    assert_eq!(progress(&restored_again), advanced);
    assert_eq!(sixth::saved_result(&restored_again), sixth);
    restored_again
        .execute_source("LevelLoad.transitionToLevel('Chapter01',7)")
        .unwrap();
    advance(&restored_again, 360, "seventh restored entry");
    sixth::assert_currency(&restored_again, true, sixth_collected);
    let mut seventh_frame = 960;
    let seventh = seventh::complete(&restored_again, &mut seventh_frame);
    let final_progress = progress(&restored_again);
    assert_eq!(final_progress.0, advanced.0 + seventh.1);
    assert_eq!(
        (&final_progress.1, &final_progress.2),
        (&advanced.1, &advanced.2)
    );
    assert_eq!(sixth::saved_result(&restored_again), sixth);
    let final_currency = stella_route::currency(&restored_again);
    restored_again.set_application_active(false).unwrap();
    drop(restored_again);
    let final_restore = StellaLua::new(&sandbox.data_root).unwrap();
    final_restore.enable_local_services().unwrap();
    final_restore.boot("scripts/game.lua").unwrap();
    advance(&final_restore, 600, "seven-level final restart");
    assert_eq!(progress(&final_restore), final_progress);
    assert_eq!(sixth::saved_result(&final_restore), sixth);
    assert_eq!(seventh::saved_result(&final_restore), seventh);
    assert_eq!(stella_route::currency(&final_restore), final_currency);
    final_restore
        .execute_source("LevelLoad.transitionToLevel('Chapter01',8)")
        .unwrap();
    advance(&final_restore, 360, "eighth restored entry");
    let mut eighth_frame = 960;
    let eighth = eighth::complete(&final_restore, &mut eighth_frame);
    let eight_progress = progress(&final_restore);
    assert_eq!(eight_progress.0, final_progress.0 + eighth.1);
    assert_eq!(
        (&eight_progress.1, &eight_progress.2),
        (&final_progress.1, &final_progress.2)
    );
    assert_eq!(sixth::saved_result(&final_restore), sixth);
    assert_eq!(seventh::saved_result(&final_restore), seventh);
    let eight_currency = stella_route::currency(&final_restore);
    final_restore.set_application_active(false).unwrap();
    drop(final_restore);
    let eight_restore = StellaLua::new(&sandbox.data_root).unwrap();
    eight_restore.enable_local_services().unwrap();
    eight_restore.boot("scripts/game.lua").unwrap();
    advance(&eight_restore, 600, "eight-level final restart");
    assert_eq!(progress(&eight_restore), eight_progress);
    assert_eq!(sixth::saved_result(&eight_restore), sixth);
    assert_eq!(seventh::saved_result(&eight_restore), seventh);
    assert_eq!(eighth::saved_result(&eight_restore), eighth);
    assert_eq!(stella_route::currency(&eight_restore), eight_currency);
    eight_restore
        .execute_source("LevelLoad.transitionToLevel('Chapter01',9)")
        .unwrap();
    advance(&eight_restore, 360, "ninth restored entry");
    let mut ninth_frame = 960;
    let ninth = ninth::complete(&eight_restore, &mut ninth_frame);
    let nine_progress = progress(&eight_restore);
    assert_eq!(nine_progress.0, eight_progress.0 + ninth.1);
    assert_eq!(
        (&nine_progress.1, &nine_progress.2),
        (&eight_progress.1, &eight_progress.2)
    );
    assert_eq!(sixth::saved_result(&eight_restore), sixth);
    assert_eq!(seventh::saved_result(&eight_restore), seventh);
    assert_eq!(eighth::saved_result(&eight_restore), eighth);
    let nine_currency = stella_route::currency(&eight_restore);
    eight_restore.set_application_active(false).unwrap();
    drop(eight_restore);
    let nine_restore = StellaLua::new(&sandbox.data_root).unwrap();
    nine_restore.enable_local_services().unwrap();
    nine_restore.boot("scripts/game.lua").unwrap();
    advance(&nine_restore, 600, "nine-level final restart");
    assert_eq!(progress(&nine_restore), nine_progress);
    assert_eq!(sixth::saved_result(&nine_restore), sixth);
    assert_eq!(seventh::saved_result(&nine_restore), seventh);
    assert_eq!(eighth::saved_result(&nine_restore), eighth);
    assert_eq!(ninth::saved_result(&nine_restore), ninth);
    assert_eq!(stella_route::currency(&nine_restore), nine_currency);
    nine_restore
        .execute_source("LevelLoad.transitionToLevel('Chapter01',10)")
        .unwrap();
    advance(&nine_restore, 360, "tenth restored entry");
    let mut tenth_frame = 960;
    let tenth = tenth::complete(&nine_restore, &mut tenth_frame);
    let ten_progress = progress(&nine_restore);
    assert_eq!(ten_progress.0, nine_progress.0 + tenth.1);
    assert_eq!(
        (&ten_progress.1, &ten_progress.2),
        (&nine_progress.1, &nine_progress.2)
    );
    assert_eq!(sixth::saved_result(&nine_restore), sixth);
    assert_eq!(seventh::saved_result(&nine_restore), seventh);
    assert_eq!(eighth::saved_result(&nine_restore), eighth);
    assert_eq!(ninth::saved_result(&nine_restore), ninth);
    let ten_currency = stella_route::currency(&nine_restore);
    nine_restore.set_application_active(false).unwrap();
    drop(nine_restore);
    let ten_restore = StellaLua::new(&sandbox.data_root).unwrap();
    ten_restore.enable_local_services().unwrap();
    ten_restore.boot("scripts/game.lua").unwrap();
    advance(&ten_restore, 600, "ten-level final restart");
    assert_eq!(progress(&ten_restore), ten_progress);
    assert_eq!(sixth::saved_result(&ten_restore), sixth);
    assert_eq!(seventh::saved_result(&ten_restore), seventh);
    assert_eq!(eighth::saved_result(&ten_restore), eighth);
    assert_eq!(ninth::saved_result(&ten_restore), ninth);
    assert_eq!(tenth::saved_result(&ten_restore), tenth);
    assert_eq!(stella_route::currency(&ten_restore), ten_currency);
    ten_restore
        .execute_source("LevelLoad.transitionToLevel('Chapter01',11)")
        .unwrap();
    advance(&ten_restore, 360, "eleventh restored entry");
    let mut eleventh_frame = 960;
    let eleventh = eleventh::complete(&ten_restore, &mut eleventh_frame);
    let eleven_progress = progress(&ten_restore);
    assert_eq!(eleven_progress.0, ten_progress.0 + eleventh.1);
    assert_eq!(
        (&eleven_progress.1, &eleven_progress.2),
        (&ten_progress.1, &ten_progress.2)
    );
    assert_eq!(sixth::saved_result(&ten_restore), sixth);
    assert_eq!(seventh::saved_result(&ten_restore), seventh);
    assert_eq!(eighth::saved_result(&ten_restore), eighth);
    assert_eq!(ninth::saved_result(&ten_restore), ninth);
    assert_eq!(tenth::saved_result(&ten_restore), tenth);
    let eleven_currency = stella_route::currency(&ten_restore);
    ten_restore.set_application_active(false).unwrap();
    drop(ten_restore);
    let eleven_restore = StellaLua::new(&sandbox.data_root).unwrap();
    eleven_restore.enable_local_services().unwrap();
    eleven_restore.boot("scripts/game.lua").unwrap();
    advance(&eleven_restore, 600, "eleven-level final restart");
    assert_eq!(progress(&eleven_restore), eleven_progress);
    assert_eq!(sixth::saved_result(&eleven_restore), sixth);
    assert_eq!(seventh::saved_result(&eleven_restore), seventh);
    assert_eq!(eighth::saved_result(&eleven_restore), eighth);
    assert_eq!(ninth::saved_result(&eleven_restore), ninth);
    assert_eq!(tenth::saved_result(&eleven_restore), tenth);
    assert_eq!(eleventh::saved_result(&eleven_restore), eleventh);
    assert_eq!(stella_route::currency(&eleven_restore), eleven_currency);
    let gate: (u32,bool,u32,String,u32,u32) = eleven_restore.lua().load("return SettingsWrapper:getMapAreaProgress('Chapter01'),SettingsWrapper:hasSeenCutscene('episode_1_generic_gate'),GateManager:getCurrentGateNumber(),GateManager:getGateState(1),SettingsWrapper:getTimesLevelCompleted('Chapter01_G01'),SettingsWrapper:getTimesLevelFailed('Chapter01_G01')")
        .set_environment(game_environment(eleven_restore.lua()).unwrap()).eval().unwrap();
    assert_eq!(gate, (3, true, 1, "opened".to_owned(), 0, 0));
    eleven_restore
        .execute_source("LevelLoad.transitionToLevel('Chapter01',12)")
        .unwrap();
    advance(&eleven_restore, 360, "first gate restored entry");
    let mut gate_frame = 960;
    let first_gate = first_gate::complete(&eleven_restore, &mut gate_frame);
    let gate_progress = progress(&eleven_restore);
    assert_eq!(gate_progress.0, eleven_progress.0 + first_gate.1);
    assert_eq!(
        (&gate_progress.1, &gate_progress.2),
        (&eleven_progress.1, &eleven_progress.2)
    );
    assert_eq!(sixth::saved_result(&eleven_restore), sixth);
    assert_eq!(seventh::saved_result(&eleven_restore), seventh);
    assert_eq!(eighth::saved_result(&eleven_restore), eighth);
    assert_eq!(ninth::saved_result(&eleven_restore), ninth);
    assert_eq!(tenth::saved_result(&eleven_restore), tenth);
    assert_eq!(eleventh::saved_result(&eleven_restore), eleventh);
    let gate_currency = stella_route::currency(&eleven_restore);
    eleven_restore.set_application_active(false).unwrap();
    drop(eleven_restore);
    for restart in 1..=2 {
        let restored = StellaLua::new(&sandbox.data_root).unwrap();
        restored.enable_local_services().unwrap();
        restored.boot("scripts/game.lua").unwrap();
        advance(&restored, 600, "gate reward orderly restart");
        first_gate::assert_persisted(&restored, first_gate, gate_currency);
        restored
            .execute_source("LevelLoad.transitionToMenu('Chapter01')")
            .unwrap();
        advance(&restored, 1200, "gate reward map revisit");
        first_gate::assert_persisted(&restored, first_gate, gate_currency);
        assert_eq!(progress(&restored), gate_progress);
        assert_eq!(sixth::saved_result(&restored), sixth);
        assert_eq!(seventh::saved_result(&restored), seventh);
        assert_eq!(eighth::saved_result(&restored), eighth);
        assert_eq!(ninth::saved_result(&restored), ninth);
        assert_eq!(tenth::saved_result(&restored), tenth);
        assert_eq!(eleventh::saved_result(&restored), eleventh);
        eprintln!("[first-gate] restart={restart} inventory={gate_currency:?}");
        restored.set_application_active(false).unwrap();
    }
    let poppy = StellaLua::new(&sandbox.data_root).unwrap();
    poppy.enable_local_services().unwrap();
    poppy.boot("scripts/game.lua").unwrap();
    advance(&poppy, 600, "Poppy earned-save restart");
    first_gate::assert_persisted(&poppy, first_gate, gate_currency);
    poppy
        .execute_source("LevelLoad.transitionToLevel('Chapter01',13)")
        .unwrap();
    advance(&poppy, 360, "earned L12 entry");
    let mut poppy_frame = 960;
    let twelfth = poppy_levels::twelfth(&poppy, &mut poppy_frame);
    let thirteenth = poppy_levels::thirteenth(&poppy, &mut poppy_frame);
    let fourteenth = poppy_levels::fourteenth(&poppy, &mut poppy_frame);
    let fifteenth = poppy_levels::fifteenth(&poppy, &mut poppy_frame);
    let sixteenth = mixed_birds::sixteenth(&poppy, &mut poppy_frame, 0);
    let poppy_progress = progress(&poppy);
    assert_eq!(
        poppy_progress.0,
        gate_progress.0 + twelfth.1 + thirteenth.1 + fourteenth.1 + fifteenth.1 + sixteenth.1
    );
    assert_eq!(
        (&poppy_progress.1, &poppy_progress.2),
        (&gate_progress.1, &gate_progress.2)
    );
    let poppy_currency = poppy_rewards::Rewards::capture(&poppy).after_startup_currency();
    poppy.set_application_active(false).unwrap();
    drop(poppy);
    for restart in 1..=2 {
        let restored = StellaLua::new(&sandbox.data_root).unwrap();
        restored.enable_local_services().unwrap();
        restored.boot("scripts/game.lua").unwrap();
        advance(&restored, 600, "Poppy reward orderly restart");
        assert_eq!(stella_route::currency(&restored), poppy_currency);
        restored
            .execute_source("LevelLoad.transitionToMenu('Chapter01')")
            .unwrap();
        advance(&restored, 1200, "Poppy reward map revisit");
        assert_eq!(stella_route::currency(&restored), poppy_currency);
        assert_eq!(progress(&restored), poppy_progress);
        assert_eq!(sixth::saved_result(&restored), sixth);
        assert_eq!(seventh::saved_result(&restored), seventh);
        assert_eq!(eighth::saved_result(&restored), eighth);
        assert_eq!(ninth::saved_result(&restored), ninth);
        assert_eq!(tenth::saved_result(&restored), tenth);
        assert_eq!(eleventh::saved_result(&restored), eleventh);
        assert_eq!(first_gate::saved_result(&restored), first_gate);
        assert_eq!(
            poppy_levels::saved_result(&restored, "Chapter01_L12"),
            twelfth
        );
        assert_eq!(
            poppy_levels::saved_result(&restored, "Chapter01_L13"),
            thirteenth
        );
        assert_eq!(
            poppy_levels::saved_result(&restored, "Chapter01_L14"),
            fourteenth
        );
        assert_eq!(
            poppy_levels::saved_result(&restored, "Chapter01_L15"),
            fifteenth
        );
        assert_eq!(
            poppy_levels::saved_result(&restored, "Chapter01_L16"),
            sixteenth
        );
        assert_eq!(
            poppy_levels::saved_result(&restored, "Chapter01_L17"),
            (0.0, 0, 0, 0)
        );
        let map: (u32, String, bool) = restored.lua().load("return SettingsWrapper:getMapAreaProgress('Chapter01'),GateManager:getGateState(1),next(SettingsWrapper:getPendingRewards('coins'))==nil")
            .set_environment(game_environment(restored.lua()).unwrap()).eval().unwrap();
        assert_eq!(map, (4, "destroyed".to_owned(), true));
        assert!(restored.fallback_calls.lock().unwrap().is_empty());
        assert!(restored.compatibility_bindings.lock().unwrap().is_empty());
        eprintln!("[poppy] restart={restart} inventory={poppy_currency:?}");
        restored.set_application_active(false).unwrap();
    }
}
