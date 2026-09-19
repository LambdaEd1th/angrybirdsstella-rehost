use super::*;

fn advance_clock(runtime: &StellaLua, clock: &mut usize, frames: usize, stage: &str) {
    advance(runtime, frames, stage);
    *clock += frames;
}

fn input(runtime: &StellaLua, frame: usize, x: f64, y: f64, down: bool) {
    eprintln!("[retry-input] frame={frame} x={x} y={y} down={down}");
    runtime.set_cursor(x, y, down).unwrap();
}

#[test]
fn shipped_finite_bird_failure_preserves_star_locks_and_restarts_through_ui() {
    let sandbox = ShippedDataSandbox::new("original-failure-retry");
    let runtime = StellaLua::new(&sandbox.data_root).unwrap();
    runtime.enable_local_services().unwrap();
    runtime.boot("scripts/game.lua").unwrap();
    let mut frame = 0;
    advance_clock(&runtime, &mut frame, 600, "retry startup");
    runtime
        .execute_source("LevelLoad.transitionToLevel('Chapter01',6)")
        .unwrap();
    advance_clock(&runtime, &mut frame, 360, "retry level entry");
    assert_scene(&runtime, "GameScene", Some("Chapter01_L06"));
    let environment = game_environment(runtime.lua()).unwrap();
    let snapshot = runtime
        .lua()
        .load(
            r#"return function()
        local count=0 for _ in pairs(levelGoals) do count=count+1 end
        return count,getRemainingBirdCount(),not not birdReady,currentBirdName or '',
            not not g_levelFailed,not not g_levelCompleted,flyingBird == nil,
            currentBirdName ~= nil and not objects.world[currentBirdName]:isLocked()
    end"#,
        )
        .set_environment(environment.clone())
        .eval::<Function>()
        .unwrap();
    type State = (u32, u32, bool, String, bool, bool, bool, bool);
    let initial: State = snapshot.call(()).unwrap();
    eprintln!("[retry] initial={initial:?}");
    assert!(initial.0 > 0 && initial.1 > 0);
    let finite:bool=runtime.lua().load("return not getLevelMetadata(levelName).infiniteBirds and not g_levelEndLogic.preventLevelFailing").set_environment(environment.clone()).eval().unwrap();
    assert!(finite, "selected level does not allow finite-bird failure");
    let available:u32=runtime.lua().load("local n=0 for _,b in pairs(birds) do if not b:isLocked() and not b.shot then n=n+1 end end return n").set_environment(environment.clone()).eval().unwrap();
    eprintln!(
        "[retry] available={available}, locked={}",
        initial.1 - available
    );
    assert!(available > 0 && available < initial.1);
    let mut fired = std::collections::HashSet::new();
    let mut next_shot = frame;
    let mut finished = false;
    while frame < 8000 {
        let s: State = snapshot.call(()).unwrap();
        if frame.is_multiple_of(120) {
            eprintln!("[retry] frame={frame} state={s:?}");
        }
        assert!(!s.5, "missed shots unexpectedly won");
        if s.4 {
            let (popup, done): (bool, bool) = runtime
                .lua()
                .load("local p=menuManager:getRoot():getChild('lastChancePopup'); return p ~= nil and p.visible,SettingsWrapper:getTimesLevelFailed('Chapter01_L06') == 1")
                .set_environment(environment.clone())
                .eval()
                .unwrap();
            // Authored GameHud does not offer a last-chance bird while unshot
            // star-locked birds remain. Let their refusal timer finish normally.
            assert!(
                !popup,
                "locked birds unexpectedly opened last-chance purchase UI"
            );
            if done {
                finished = true;
                break;
            }
        } else if s.2
            && s.6
            && s.7
            && !s.3.is_empty()
            && !fired.contains(&s.3)
            && frame >= next_shot
        {
            let (x, y): (f64, f64) = runtime
                .lua()
                .load("return physicsToScreenTransform(levelStartPosition.x,levelStartPosition.y)")
                .set_environment(environment.clone())
                .eval()
                .unwrap();
            if (0.0..904.0).contains(&x) && (0.0..748.0).contains(&y) {
                let before:(u32,f64,f64)=runtime.lua().load("local b=objects.world[currentBirdName]; local x,y=physicsToScreenTransform(b.x,b.y); return birdsShot,x,y").set_environment(environment.clone()).eval().unwrap();
                eprintln!("[retry] before press current bird={before:?} sling=({x},{y})");
                input(&runtime, frame, x, y, true);
                advance_clock(&runtime, &mut frame, 1, "miss sling press");
                let pressed:(bool,bool,String)=runtime.lua().load("return not not dragStarted,not not birdReady,selectedBird and selectedBird.name or ''").set_environment(environment.clone()).eval().unwrap();
                eprintln!("[retry] pressed={pressed:?}");
                input(&runtime, frame, x + 120.0, y + 20.0, true);
                advance_clock(&runtime, &mut frame, 60, "miss sling pull");
                input(&runtime, frame, x + 120.0, y + 20.0, false);
                advance_clock(&runtime, &mut frame, 1, "miss release consumed");
                let shot_count: u32 = runtime
                    .lua()
                    .load("return birdsShot")
                    .set_environment(environment.clone())
                    .eval()
                    .unwrap();
                assert_eq!(
                    shot_count,
                    before.0 + 1,
                    "host press/pull/release did not launch current bird"
                );
                fired.insert(s.3);
                next_shot = frame + 360;
            }
        }
        advance_clock(&runtime, &mut frame, 1, "finite bird flight");
    }
    let failed: State = snapshot.call(()).unwrap();
    eprintln!(
        "[retry] failed frame={frame} state={failed:?} shots={}",
        fired.len()
    );
    assert!(finished && failed.4 && failed.0 > 0);
    assert_eq!(fired.len(), available as usize);
    assert_eq!(failed.1, initial.1 - available);
    let refused: bool = runtime
        .lua()
        .load("return not not g_birdUnlockingRefused")
        .set_environment(environment.clone())
        .eval()
        .unwrap();
    assert!(refused, "authored locked-bird refusal did not finish");
    advance_clock(&runtime, &mut frame, 360, "failed screen animation");
    let (x,y):(f64,f64)=runtime.lua().load("local root=menuManager:getRoot(); local b=root:getChild('restartButton'); if not b or not b.visible then error('failed restart button absent') end; if root:getChild('nextButton').visible then error('failed level offered unearned next level') end; return b:getScreenPosition()").set_environment(environment.clone()).eval().unwrap();
    input(&runtime, frame, x, y, true);
    advance_clock(&runtime, &mut frame, 1, "failed restart press");
    input(&runtime, frame, x, y, false);
    advance_clock(&runtime, &mut frame, 360, "failed restart transition");
    assert_scene(&runtime, "GameScene", Some("Chapter01_L06"));
    let restarted: State = snapshot.call(()).unwrap();
    eprintln!("[retry] restarted frame={frame} state={restarted:?}");
    assert_eq!(restarted.0, initial.0);
    assert_eq!(restarted.1, initial.1);
    assert!(restarted.2 && !restarted.4 && !restarted.5);
    let reset: (u32, String) = runtime
        .lua()
        .load("return birdsShot,g_restartType")
        .set_environment(environment.clone())
        .eval()
        .unwrap();
    assert_eq!(reset, (0, "FAILED".to_owned()));
    assert!(runtime.render.lock().unwrap().physics_enabled);
    let progress:(u32,u32,bool)=runtime.lua().load("return SettingsWrapper:getTimesLevelCompleted('Chapter01_L06'),SettingsWrapper:getTimesLevelFailed('Chapter01_L06'),highscores.Chapter01_L06 == nil").set_environment(environment.clone()).eval().unwrap();
    assert_eq!(progress, (0, 1, true));
    // A retained legacy g_disableGameupdate value is not a verified native
    // gate. Prove that the rebuilt level accepts input and advances its body.
    let (bird, x, y): (String, f64, f64) = runtime.lua()
        .load("local b=objects.world[currentBirdName]; local x,y=physicsToScreenTransform(b.x,b.y); return b.name,x,y")
        .set_environment(environment.clone()).eval().unwrap();
    input(&runtime, frame, x, y, true);
    advance_clock(&runtime, &mut frame, 1, "retry sling press");
    input(&runtime, frame, x + 120.0, y + 20.0, true);
    advance_clock(&runtime, &mut frame, 60, "retry sling pull");
    input(&runtime, frame, x + 120.0, y + 20.0, false);
    advance_clock(&runtime, &mut frame, 1, "retry release consumed");
    let (bx, by): (f64, f64) = runtime
        .lua()
        .load("local b=objects.world[...]; return b.x,b.y")
        .set_environment(environment.clone())
        .call(bird.as_str())
        .unwrap();
    advance_clock(&runtime, &mut frame, 29, "retry flight");
    let (shots, after_x, after_y): (u32, f64, f64) = runtime
        .lua()
        .load("local name=...; local b=objects.world[name]; return birdsShot,b.x,b.y")
        .set_environment(environment)
        .call(bird)
        .unwrap();
    assert_eq!(shots, 1);
    assert!(
        (after_x - bx).hypot(after_y - by) > 0.1,
        "retried bird did not move"
    );
    eprintln!(
        "[retry] second flight frame={frame} displacement={}",
        (after_x - bx).hypot(after_y - by)
    );
    assert!(runtime.fallback_calls.lock().unwrap().is_empty());
    assert!(runtime.compatibility_bindings.lock().unwrap().is_empty());
}
