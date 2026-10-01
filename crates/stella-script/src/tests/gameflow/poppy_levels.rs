//! Original Poppy hold and free-hammer teaching flows, driven by host input.
use super::poppy_rewards::Rewards;
use super::stella_route::{launch, tick};
use super::*;
use mlua::Table;

pub(super) type Result = (f64, u32, u32, u32);

pub(super) fn saved_result(runtime: &StellaLua, level: &str) -> Result {
    runtime.lua().load(r#"
        local name=...
        local score=highscores[name] and highscores[name].score or 0
        return score,getLevelStars(name,score),SettingsWrapper:getTimesLevelCompleted(name),SettingsWrapper:getTimesLevelFailed(name)
    "#).set_environment(game_environment(runtime.lua()).unwrap()).call(level).unwrap()
}

pub(super) fn click(runtime: &StellaLua, frame: &mut usize, x: f64, y: f64, stage: &str) {
    assert!((0.0..1024.0).contains(&x) && (0.0..768.0).contains(&y));
    runtime.set_cursor(x, y, true).unwrap();
    tick(runtime, frame, 1, stage);
    runtime.set_cursor(x, y, false).unwrap();
    tick(runtime, frame, 1, stage);
}

pub(super) fn until(runtime: &StellaLua, frame: &mut usize, deadline: usize, stage: &str) {
    assert!(
        *frame <= deadline,
        "{stage}: input sequence exceeded its deadline"
    );
    tick(runtime, frame, deadline - *frame, stage);
}

pub(super) fn finish(
    runtime: &StellaLua,
    frame: &mut usize,
    level: &str,
    next_level: &str,
    before: Rewards,
    shots: u32,
) -> Result {
    finish_after_failures(runtime, frame, level, next_level, before, shots, 0)
}

pub(super) fn finish_after_failures(
    runtime: &StellaLua,
    frame: &mut usize,
    level: &str,
    next_level: &str,
    before: Rewards,
    shots: u32,
    expected_failures: u32,
) -> Result {
    let environment = game_environment(runtime.lua()).unwrap();
    let state: (bool, bool, u32, u32, u32) = runtime.lua().load(r#"
        local n=0 for _ in pairs(levelGoals) do n=n+1 end
        return not not g_levelCompleted,not not g_levelFailed,n,birdsShot,currentLevelStats.coinsCollected
    "#).set_environment(environment.clone()).eval().unwrap();
    if (state.0, state.1, state.2, state.3) != (true, false, 0, shots) {
        let diagnostic: String = runtime.lua().load(r#"
            local goals={}
            for name,g in pairs(levelGoals) do
                goals[#goals+1]=string.format('%s@(%.4f,%.4f)',name,g.x,g.y)
            end
            table.sort(goals)
            local b=flyingBird
            local bird=b and string.format('%s@(%.4f,%.4f) collided=%s disabled=%s',
                b.name,b.x,b.y,tostring(b.hasCollided),tostring(b.abilityDisabled)) or 'none'
            local root=menuManager:getRoot()
            return 'root='..tostring(root and root.name)..' goals='..table.concat(goals,', ')..' bird='..bird
        "#).set_environment(environment.clone()).eval().unwrap();
        eprintln!("[poppy-failure] level={level} frame={frame} state={state:?} {diagnostic}");
    }
    assert_eq!(
        (state.0, state.1, state.2, state.3),
        (true, false, 0, shots)
    );
    let result = saved_result(runtime, level);
    assert!(result.0 > 0.0);
    assert_eq!((result.2, result.3), (1, expected_failures));
    if matches!(
        level,
        "Chapter01_L13" | "Chapter01_L14" | "Chapter01_L15" | "Chapter01_L16"
    ) {
        assert!((1..=3).contains(&result.1));
    } else {
        assert_eq!(result.1, 3);
    }
    let earned = before.finish(runtime, result.1, state.4);
    eprintln!(
        "[poppy] level={level} result={result:?} shots={shots} collected={}",
        state.4
    );
    practice::click_next(runtime, frame);
    assert_scene(runtime, "GameScene", Some(next_level));
    assert_eq!(saved_result(runtime, level), result);
    assert_eq!(saved_result(runtime, next_level), (0.0, 0, 0, 0));
    earned.assert_current(runtime);
    assert!(runtime.fallback_calls.lock().unwrap().is_empty());
    assert!(runtime.compatibility_bindings.lock().unwrap().is_empty());
    result
}

pub(super) fn twelfth(runtime: &StellaLua, frame: &mut usize) -> Result {
    assert_scene(runtime, "GameScene", Some("Chapter01_L12"));
    assert_eq!(saved_result(runtime, "Chapter01_L12"), (0.0, 0, 0, 0));
    let before = Rewards::capture(runtime);
    tick(runtime, frame, 240, "Poppy tutorial camera settle");
    let start = *frame;
    launch(runtime, frame, -240.0, 100.0, 1, "twelfth");
    until(
        runtime,
        frame,
        start + 900,
        "original Poppy hold demonstration",
    );
    let (x,y): (f64,f64) = runtime.lua().load(r#"
        local a=menuManager:getRoot():getChild('tapArea')
        if not isGamePausedByTutorial() or not a or not a.visible or a.showFinger or not flyingBird then error('Poppy tutorial not ready for real hold') end
        return physicsToScreenTransform(flyingBird.x,flyingBird.y)
    "#).set_environment(game_environment(runtime.lua()).unwrap()).eval().unwrap();
    assert!((0.0..1024.0).contains(&x) && (0.0..768.0).contains(&y));
    runtime.set_cursor(x, y, true).unwrap();
    tick(runtime, frame, 130, "Poppy original tutorial hold");
    runtime.set_cursor(x, y, false).unwrap();
    tick(runtime, frame, 1, "Poppy original tutorial release");
    until(
        runtime,
        frame,
        start + 3300,
        "Poppy result and unlock animation",
    );
    finish(runtime, frame, "Chapter01_L12", "Chapter01_L13", before, 1)
}

pub(super) fn thirteenth(runtime: &StellaLua, frame: &mut usize) -> Result {
    assert_scene(runtime, "GameScene", Some("Chapter01_L13"));
    assert_eq!(saved_result(runtime, "Chapter01_L13"), (0.0, 0, 0, 0));
    let before = Rewards::capture(runtime);
    let environment = game_environment(runtime.lua()).unwrap();
    tick(runtime, frame, 240, "hammer tutorial camera settle");
    let start = *frame;
    let target_x: f64 = runtime
        .lua()
        .load("local x=math.huge for _,g in pairs(levelGoals) do x=math.min(x,g.x) end return x")
        .set_environment(environment.clone())
        .eval()
        .unwrap();
    assert!(target_x.is_finite());
    launch(runtime, frame, -240.0, 100.0, 1, "thirteenth");
    until(
        runtime,
        frame,
        start + 140,
        "Poppy approach above left goals",
    );
    loop {
        let ready: bool = runtime.lua().load(r#"
            local target=...
            local b=flyingBird
            if not b or b.hasCollided or b.abilityDisabled then error('Poppy drill approach obstructed') end
            local vx=getLinearVelocity(b.name)
            return b.x+vx*getDeltaTimeMultiplier()/60>=target
        "#).set_environment(environment.clone()).call(target_x).unwrap();
        if ready {
            break;
        }
        assert!(
            *frame < start + 280,
            "Poppy did not reach original goal column"
        );
        tick(runtime, frame, 1, "Poppy live horizontal approach");
    }
    click(
        runtime,
        frame,
        512.0,
        384.0,
        "Poppy normal screen tap and release",
    );
    until(
        runtime,
        frame,
        start + 2800,
        "original hammer teaching reveal",
    );
    let (x,y): (f64,f64) = runtime.lua().load(r#"
        local hud=getGameHud()
        local b=hud and hud:getChild('powerupButton')
        if not b or not b.visible or not b.enabled then error('original hammer button unavailable') end
        return b:getScreenPosition()
    "#).set_environment(environment.clone()).eval().unwrap();
    click(runtime, frame, x, y, "original free hammer button");
    until(
        runtime,
        frame,
        start + 3400,
        "original hammer target demonstration",
    );
    let (x,y): (f64,f64) = runtime.lua().load(r#"
        local f=PowerUp.Hammer.forcedTarget
        if not PowerUp.active or not f or not f.object or f.object.name~='pig_medium_8' then error('original hammer forced target missing') end
        return physicsToScreenTransform(f.object.x,f.object.y)
    "#).set_environment(environment.clone()).eval().unwrap();
    click(runtime, frame, x, y, "original hammer target click");
    until(
        runtime,
        frame,
        start + 5400,
        "hammer impact and original star reward",
    );
    let finished: bool = runtime
        .lua()
        .load(
            r#"
        local hud=getGameHud()
        return not PowerUp.active and PowerUp.useTimes==1 and not PowerUp.Hammer.forcedTarget
            and (not hud or not hud:getChild('powerUpTutorial'))
            and SettingsWrapper:getString('PowerUpAvailable')==nil
    "#,
        )
        .set_environment(environment)
        .eval()
        .unwrap();
    assert!(finished, "original hammer teaching did not finish");
    let shots = cleanup_thirteenth_after_hammer(runtime, frame);
    finish(
        runtime,
        frame,
        "Chapter01_L13",
        "Chapter01_L14",
        before,
        shots,
    )
}

fn cleanup_thirteenth_after_hammer(runtime: &StellaLua, frame: &mut usize) -> u32 {
    let environment = game_environment(runtime.lua()).unwrap();
    let mut shots = 1;
    for shot in 2..=3 {
        let target: Option<Table> = runtime
            .lua()
            .load(
                r#"
            local best=nil
            for name,g in pairs(levelGoals) do
                if not best or g.y>best.y or (g.y==best.y and name<best.name) then best=g end
            end
            return best and {best.name,best.x,best.y}
        "#,
            )
            .set_environment(environment.clone())
            .eval()
            .unwrap();
        let Some(target) = target else { break };
        let (name, x, y): (String, f64, f64) = (
            target.get(1).unwrap(),
            target.get(2).unwrap(),
            target.get(3).unwrap(),
        );
        let ready_deadline = *frame + 3600;
        let mut return_taps = 0;
        loop {
            let (ready, sling_x, sling_y, can_return): (bool, f64, f64, bool) = runtime
                .lua()
                .load(
                    r#"
                local x,y=physicsToScreenTransform(levelStartPosition.x,levelStartPosition.y)
                return not not birdReady,x,y,birdSpecialtyAvailable==false
                    and gameCamera.cameraAnimationSliderTarget==1
                    and gameCamera:getNumOfCameraTargetObjects()==0
            "#,
                )
                .set_environment(environment.clone())
                .eval()
                .unwrap();
            if ready && (16.0..1008.0).contains(&sling_x) && (16.0..752.0).contains(&sling_y) {
                break;
            }
            if ready && can_return && return_taps < 3 {
                return_taps += 1;
                click(runtime, frame, 512.0, 384.0, "hammer cleanup camera return");
                tick(runtime, frame, 120, "hammer cleanup camera settle");
                continue;
            }
            assert!(*frame < ready_deadline, "hammer cleanup bird not ready");
            tick(runtime, frame, 1, "hammer cleanup original bird readiness");
        }
        eprintln!("[thirteenth-cleanup] shot={shot} frame={frame} target={name}@({x},{y})");
        let shot_start = *frame;
        launch(
            runtime,
            frame,
            -240.0,
            if shot == 2 { 100.0 } else { 130.0 },
            shot,
            "thirteenth-cleanup",
        );
        shots = shot;
        until(
            runtime,
            frame,
            shot_start + 105,
            "hammer cleanup flight approach",
        );
        while *frame <= shot_start + 280 {
            let (available, crossed, bird_x, bird_y): (bool, bool, Option<f64>, Option<f64>) =
                runtime
                    .lua()
                    .load(
                        r#"
                local goal_x=...
                local b=flyingBird
                if not b or b.hasCollided or b.abilityDisabled then return false,false,nil,nil end
                local vx=getLinearVelocity(b.name)
                return true,b.x+vx*getDeltaTimeMultiplier()/60>=goal_x,b.x,b.y
            "#,
                    )
                    .set_environment(environment.clone())
                    .call(x)
                    .unwrap();
            if !available {
                break;
            }
            if crossed {
                eprintln!(
                    "[thirteenth-cleanup] tap frame={frame} target={name} bird=({bird_x:?},{bird_y:?})"
                );
                click(runtime, frame, 512.0, 384.0, "Poppy hammer cleanup drill");
                break;
            }
            tick(runtime, frame, 1, "Poppy hammer cleanup live column");
        }
        until(runtime, frame, shot_start + 2200, "hammer cleanup collapse");
    }
    shots
}

pub(super) fn fourteenth(runtime: &StellaLua, frame: &mut usize) -> Result {
    assert_scene(runtime, "GameScene", Some("Chapter01_L14"));
    assert_eq!(saved_result(runtime, "Chapter01_L14"), (0.0, 0, 0, 0));
    let before = Rewards::capture(runtime);
    let environment = game_environment(runtime.lua()).unwrap();
    tick(runtime, frame, 240, "ice tower camera settle");
    let start = *frame;
    let mut shots = 0;
    for shot in 1..=5 {
        let target: Option<(f64, f64)> = runtime
            .lua()
            .load(
                r#"
            local goal=nil
            for _,g in pairs(levelGoals) do if not goal or g.x<goal.x then goal=g end end
            if goal then return {goal.x,goal.y} end
        "#,
            )
            .set_environment(environment.clone())
            .eval::<Option<Table>>()
            .unwrap()
            .map(|g| (g.get(1).unwrap(), g.get(2).unwrap()));
        let Some((x, y)) = target else {
            break;
        };
        let shot_start = *frame;
        launch(runtime, frame, -240.0, 100.0, shot, "fourteenth");
        shots = shot;
        until(runtime, frame, shot_start + 80, "Poppy ice tower approach");
        while *frame <= shot_start + 300 {
            let (available, ready): (bool, bool) = runtime
                .lua()
                .load(
                    r#"
                local x,y=...
                local b=flyingBird
                if not b or b.hasCollided or b.abilityDisabled then return false,false end
                local vx=getLinearVelocity(b.name)
                return true,b.x+vx*getDeltaTimeMultiplier()/60>=x and b.y<=y-1
            "#,
                )
                .set_environment(environment.clone())
                .call((x, y))
                .unwrap();
            if !available {
                break;
            }
            if ready {
                click(
                    runtime,
                    frame,
                    512.0,
                    384.0,
                    "Poppy ice tower down-drill tap",
                );
                break;
            }
            tick(runtime, frame, 1, "Poppy live ice tower column search");
        }
        until(
            runtime,
            frame,
            shot_start + 2600,
            "ice tower collapse and camera return",
        );
    }
    until(
        runtime,
        frame,
        start + 13800,
        "ice tower final original result",
    );
    finish(
        runtime,
        frame,
        "Chapter01_L14",
        "Chapter01_L15",
        before,
        shots,
    )
}

pub(super) fn fifteenth(runtime: &StellaLua, frame: &mut usize) -> Result {
    assert_scene(runtime, "GameScene", Some("Chapter01_L15"));
    assert_eq!(saved_result(runtime, "Chapter01_L15"), (0.0, 0, 0, 0));
    let before = Rewards::capture(runtime);
    let environment = game_environment(runtime.lua()).unwrap();
    tick(runtime, frame, 240, "armored TNT tower camera settle");
    let start = *frame;
    let mut shots = 0;
    for shot in 1..=4 {
        let target: Option<(String, f64, f64)> = runtime
            .lua()
            .load(
                r#"
            local shot=...
            local goal=nil
            for _,g in pairs(levelGoals) do
                if not goal or (shot==3 and g.y>goal.y) or (shot~=3 and g.x<goal.x) then goal=g end
            end
            if shot<=2 then goal=objects.world.BLOCK_TNT_4X4_ARMORED_5 or objects.world.BLOCK_TNT_4X4_ARMORED_2 or goal end
            if next(levelGoals) and goal then return {goal.name,goal.x,goal.y} end
        "#,
            )
            .set_environment(environment.clone())
            .call::<Option<Table>>(shot)
            .unwrap()
            .map(|g| (g.get(1).unwrap(), g.get(2).unwrap(), g.get(3).unwrap()));
        let Some((name, x, y)) = target else {
            break;
        };
        eprintln!("[fifteenth-target] shot={shot} frame={frame} target={name}@({x},{y})");
        let shot_start = *frame;
        launch(runtime, frame, -240.0, 220.0, shot, "fifteenth");
        shots = shot;
        until(
            runtime,
            frame,
            shot_start + 80,
            "Poppy armored TNT tower approach",
        );
        while *frame <= shot_start + 380 {
            let (available, ready): (bool, bool) = runtime
                .lua()
                .load(
                    r#"
                local x,y=...
                local b=flyingBird
                if not b or b.hasCollided or b.abilityDisabled then return false,false end
                local vx=getLinearVelocity(b.name)
                return true,b.x+vx*getDeltaTimeMultiplier()/60>=x and b.y<=y-1
            "#,
                )
                .set_environment(environment.clone())
                .call((x, y))
                .unwrap();
            if !available {
                eprintln!("[fifteenth-drill] shot={shot} frame={frame} unavailable target={name}");
                break;
            }
            if ready {
                eprintln!("[fifteenth-drill] shot={shot} frame={frame} tap target={name}");
                click(
                    runtime,
                    frame,
                    512.0,
                    384.0,
                    "Poppy armored TNT tower down-drill tap",
                );
                break;
            }
            tick(
                runtime,
                frame,
                1,
                "Poppy live armored TNT tower column search",
            );
        }
        until(
            runtime,
            frame,
            shot_start + 2600,
            "armored TNT tower collapse and camera return",
        );
    }
    until(
        runtime,
        frame,
        start + 13800,
        "armored TNT tower final original result",
    );
    finish(
        runtime,
        frame,
        "Chapter01_L15",
        "Chapter01_L16",
        before,
        shots,
    )
}
