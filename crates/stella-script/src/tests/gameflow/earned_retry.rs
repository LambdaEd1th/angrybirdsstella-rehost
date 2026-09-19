use super::*;

fn tick(runtime: &StellaLua, frame: &mut usize, count: usize, stage: &str) {
    advance(runtime, count, stage);
    *frame += count;
}

fn pointer(runtime: &StellaLua, frame: usize, x: f64, y: f64, down: bool) {
    eprintln!("[earned-retry-input] frame={frame} x={x} y={y} down={down}");
    runtime.set_cursor(x, y, down).unwrap();
}

/// Continue the actual first-five earned progression, without seeded unlocks.
pub(super) fn exhaust_and_refuse(runtime: &StellaLua, frame: &mut usize) {
    let environment = game_environment(runtime.lua()).unwrap();
    let snapshot = runtime.lua().load(r#"return function()
        local goals=0 for _ in pairs(levelGoals) do goals=goals+1 end
        local available=0 for _,b in pairs(birds) do if not b.shot and not b:isLocked() then available=available+1 end end
        local popup=getGameHud():getChild('lastChancePopup')
        return goals,getRemainingBirdCount(),available,birdsShot,
            not not birdReady,flyingBird == nil,currentBirdName or '',
            not not g_levelFailed,not not g_levelCompleted,popup ~= nil and popup.visible
    end"#).set_environment(environment.clone()).eval::<Function>().unwrap();
    type State = (u32, u32, u32, u32, bool, bool, String, bool, bool, bool);
    let initial: State = snapshot.call(()).unwrap();
    assert!(initial.0 > 0);
    assert_eq!((initial.1, initial.2, initial.3), (3, 3, 0));
    let coins: u32 = runtime
        .lua()
        .load("return Coins:getAmount()")
        .set_environment(environment.clone())
        .eval()
        .unwrap();
    assert_eq!(coins, 66);
    let mut fired = std::collections::HashSet::new();
    let deadline = *frame + 9000;
    let mut next_shot = *frame;
    loop {
        let state: State = snapshot.call(()).unwrap();
        if (*frame).is_multiple_of(120) {
            eprintln!("[earned-retry] frame={frame} state={state:?}");
        }
        assert!(!state.8, "deliberate misses unexpectedly completed L06");
        assert!(
            *frame < deadline,
            "earned birds did not reach last chance: {state:?}"
        );
        if state.9 {
            assert!(state.7 && state.0 > 0);
            assert_eq!((state.1, state.2, state.3, fired.len()), (0, 0, 3, 3));
            break;
        }
        if !state.7
            && state.4
            && state.5
            && !state.6.is_empty()
            && !fired.contains(&state.6)
            && *frame >= next_shot
        {
            // Let the flight-follow camera return before aiming the next bird.
            // A ready sling alone can still be hundreds of pixels off its
            // resting screen position during this authored transition.
            tick(runtime, frame, 240, "earned sling camera settle");
            let (x, y): (f64, f64) = runtime
                .lua()
                .load("return physicsToScreenTransform(levelStartPosition.x,levelStartPosition.y)")
                .set_environment(environment.clone())
                .eval()
                .unwrap();
            if (0.0..904.0).contains(&x) && (0.0..748.0).contains(&y) {
                pointer(runtime, *frame, x, y, true);
                tick(runtime, frame, 1, "earned miss press");
                pointer(runtime, *frame, x + 120.0, y + 20.0, true);
                tick(runtime, frame, 60, "earned miss pull");
                pointer(runtime, *frame, x + 120.0, y + 20.0, false);
                tick(runtime, frame, 1, "earned miss release");
                let after: State = snapshot.call(()).unwrap();
                assert_eq!(
                    after.3,
                    state.3 + 1,
                    "host input failed to launch earned bird"
                );
                fired.insert(state.6);
                next_shot = *frame + 360;
            }
        }
        tick(runtime, frame, 1, "earned bird exhaustion");
    }
    tick(runtime, frame, 180, "last chance entrance");
    let (x, y, price, value): (f64, f64, u32, String) = runtime
        .lua()
        .load(
            r#"
        local p=getGameHud():getChild('lastChancePopup')
        if not p or not p.visible then error('last chance popup missing') end
        local b=p:getChild('giveUpButton');local x,y=b:getScreenPosition()
        return x,y,p.price,b.returnValue
    "#,
        )
        .set_environment(environment.clone())
        .eval()
        .unwrap();
    assert_eq!(value, "GIVE_UP");
    assert!(price > 0);
    eprintln!("[earned-retry] popup frame={frame} giveup=({x},{y}) price={price} coins={coins}");
    pointer(runtime, *frame, x, y, true);
    tick(runtime, frame, 1, "last chance refusal press");
    pointer(runtime, *frame, x, y, false);
    tick(runtime, frame, 720, "last chance refusal and failed screen");
    let (x,y,failed,completed,amount,used): (f64,f64,u32,u32,u32,u32)=runtime.lua().load(r#"
        local root=menuManager:getRoot()
        if root:getChild('lastChancePopup') then error('refused popup retained') end
        local b=root:getChild('restartButton')
        if not b or not b.visible then error('failed restart button missing') end
        if root:getChild('nextButton').visible then error('failed level offered next') end
        local x,y=b:getScreenPosition()
        return x,y,SettingsWrapper:getTimesLevelFailed('Chapter01_L06'),SettingsWrapper:getTimesLevelCompleted('Chapter01_L06'),Coins:getAmount(),currentLevelStats.lastChanceBirdsUsed
    "#).set_environment(environment.clone()).eval().unwrap();
    assert_eq!((failed, completed, amount, used), (1, 0, coins, 0));
    eprintln!("[earned-retry] failed frame={frame} restart=({x},{y})");
    pointer(runtime, *frame, x, y, true);
    tick(runtime, frame, 1, "earned failed restart press");
    pointer(runtime, *frame, x, y, false);
    tick(runtime, frame, 540, "earned failed restart");
    assert_scene(runtime, "GameScene", Some("Chapter01_L06"));
    let state: State = snapshot.call(()).unwrap();
    assert_eq!((state.0, state.1, state.2, state.3), (initial.0, 3, 3, 0));
    assert!(state.4 && !state.7 && !state.8 && !state.9);
    let result: (String,u32,u32,bool)=runtime.lua().load("return g_restartType,Coins:getAmount(),SettingsWrapper:getTimesLevelFailed('Chapter01_L06'),highscores.Chapter01_L06 == nil")
        .set_environment(environment).eval().unwrap();
    assert_eq!(result, ("FAILED".to_owned(), coins, 1, true));
    eprintln!("[earned-retry] complete frame={frame} state={state:?}");
    assert!(runtime.fallback_calls.lock().unwrap().is_empty());
    assert!(runtime.compatibility_bindings.lock().unwrap().is_empty());
}
