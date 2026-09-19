use super::*;
use mlua::Table;

fn tick(runtime: &StellaLua, frame: &mut usize, frames: usize, stage: &str) {
    advance(runtime, frames, stage);
    *frame += frames;
}

fn pointer(runtime: &StellaLua, frame: usize, x: f64, y: f64, down: bool) {
    eprintln!("[practice-input] frame={frame} x={x} y={y} down={down}");
    runtime.set_cursor(x, y, down).unwrap();
}

pub(super) fn next_level(runtime: &StellaLua, frame: &mut usize, expected: &str) {
    let deadline = *frame + 1800;
    let environment = game_environment(runtime.lua()).unwrap();
    let button=runtime.lua().load(r#"return function()
        local f=menuManager:getRoot():getChild('levelCompleted')
        local b=f and f:getChild('btnnextlevel')
        if not b or not b.visible or not b.enabled or (f.characterUnlockStarted and not f.characterUnlockFinished) then return nil end
        local x,y=b:getScreenPosition();if x<0 or x>=1024 or y<0 or y>=768 then return nil end;return {x=x,y=y}
    end"#).set_environment(environment).eval::<Function>().unwrap();
    let position = loop {
        if let Some(p) = button.call::<Option<Table>>(()).unwrap() {
            break p;
        }
        assert!(
            *frame < deadline,
            "completion next button did not become available"
        );
        tick(runtime, frame, 30, "completion animation");
    };
    let x = position.get("x").unwrap();
    let y = position.get("y").unwrap();
    pointer(runtime, *frame, x, y, true);
    tick(runtime, frame, 1, "next level press");
    pointer(runtime, *frame, x, y, false);
    tick(runtime, frame, 360, "next level transition");
    assert_scene(runtime, "GameScene", Some(expected));
}

#[test]
fn shipped_practice_levels_use_ability_and_unlock_stella() {
    let sandbox = ShippedDataSandbox::new("original-stella-practice");
    let runtime = StellaLua::new(&sandbox.data_root).unwrap();
    runtime.enable_local_services().unwrap();
    runtime.boot("scripts/game.lua").unwrap();
    let mut frame = 0;
    tick(&runtime, &mut frame, 600, "practice startup");
    runtime
        .execute_source("LevelLoad.transitionToLevel('Chapter01',4)")
        .unwrap();
    tick(&runtime, &mut frame, 360, "practice entry");
    complete_practice(&runtime, &mut frame, "Chapter01_L04");
    next_level(&runtime, &mut frame, "Chapter01_L05");
    complete_practice(&runtime, &mut frame, "Chapter01_L05");
    next_level(&runtime, &mut frame, "Chapter01_L06");
    let progress:(u32,bool)=runtime.lua().load("return SettingsWrapper:getNumber('starsCollected'),SettingsWrapper:isBirdSkinUnlocked('Stella')").set_environment(game_environment(runtime.lua()).unwrap()).eval().unwrap();
    eprintln!("[practice] progress={progress:?}");
    assert_eq!(progress, (6, true));
}

pub(super) fn complete_practice(runtime: &StellaLua, frame: &mut usize, level: &str) {
    assert_scene(runtime, "GameScene", Some(level));
    let environment = game_environment(runtime.lua()).unwrap();
    let snapshot = runtime
        .lua()
        .load(
            r#"return function()
        local b=flyingBird
        local n=0 local goal=nil local dist=math.huge
        for _,g in pairs(levelGoals) do
            n=n+1
            if b then
                local d=vLength(g.x-b.x,g.y-b.y)
                if d<dist then goal=g dist=d end
            end
        end
        local x,y=0,0
        if goal then
            local tx,ty=goal.x,goal.y
            if levelName=='Chapter01_L04' and goal.y < -6 then
                -- Aim at the visible right wall to reflect toward the upper pig.
                local wall=2.2077797651291
                tx=wall
                ty=b.y+(goal.y-b.y)*(wall-b.x)/(2*wall-goal.x-b.x)
            end
            x,y=physicsToScreenTransform(tx,ty)
        end
        return {goals=n,bird=b and b.name or '',ready=not not birdReady,
            active=b and b.stellaAbility and b.stellaAbility.hasBeenActivated or false,
            collided=b and b.hasCollided or false,disabled=b and b.abilityDisabled or false,
            reachable=b and goal and ((n==1 and goal.y < -6 and b.x > -0.2) or b.x > 1.2) or false,
            x=x,y=y,goal=goal and goal.name or '',won=not not g_levelCompleted}
    end"#,
        )
        .set_environment(environment.clone())
        .eval::<Function>()
        .unwrap();
    let deadline = *frame + 4800;
    let mut shots = 0;
    let mut targeted = std::collections::HashSet::new();
    let mut releasing = None;
    let mut won = false;
    while *frame < deadline {
        let s: Table = snapshot.call(()).unwrap();
        let bird: String = s.get("bird").unwrap();
        let goals: u32 = s.get("goals").unwrap();
        if frame.is_multiple_of(120) {
            eprintln!("[practice] level={level} frame={frame} goals={goals} bird={bird}");
        }
        if s.get::<bool>("won").unwrap() {
            won = true;
            break;
        }
        if let Some(at) = releasing {
            // Keep the pointer over the world target as the authored camera
            // settles during slow-motion aiming.
            let target: (f64, f64) = (s.get("x").unwrap(), s.get("y").unwrap());
            if *frame < at {
                pointer(runtime, *frame, target.0, target.1, true);
            }
            if *frame >= at {
                let path:String=runtime.lua().load("local a=flyingBird and flyingBird.stellaAbility; if not a then return 'no ability' end; local r={} for _,p in ipairs(a.jumpPath or {}) do r[#r+1]=tostring(p.x)..','..tostring(p.y)..':'..tostring(p.stopToAim) end; return 'tap='..tostring(a.tapPoint and a.tapPoint.x)..','..tostring(a.tapPoint and a.tapPoint.y)..' path='..table.concat(r,';')").set_environment(environment.clone()).eval().unwrap();
                eprintln!("[practice] releasing frame={frame} {path}");
                pointer(runtime, *frame, target.0, target.1, false);
                releasing = None;
            }
        } else if !bird.is_empty()
            && !s.get::<bool>("active").unwrap()
            && !s.get::<bool>("collided").unwrap()
            && !s.get::<bool>("disabled").unwrap()
            && s.get::<bool>("reachable").unwrap()
            && !targeted.contains(&bird)
        {
            let x: f64 = s.get("x").unwrap();
            let y: f64 = s.get("y").unwrap();
            if (0.0..1024.0).contains(&x) && (0.0..768.0).contains(&y) {
                eprintln!(
                    "[practice] ability goal={}",
                    s.get::<String>("goal").unwrap()
                );
                pointer(runtime, *frame, x, y, true);
                releasing = Some(*frame + 130);
                targeted.insert(bird);
            }
        } else if goals > 0 && bird.is_empty() && s.get::<bool>("ready").unwrap() && shots < 4 {
            if shots > 0 {
                // The camera can still be returning from the previous flight
                // when the next bird becomes ready. Read the sling after it
                // settles so the captured pointer replay also hits the bird.
                tick(runtime, frame, 180, "practice camera settling");
                let settled: Table = snapshot.call(()).unwrap();
                if settled.get::<u32>("goals").unwrap() == 0 || settled.get::<bool>("won").unwrap()
                {
                    continue;
                }
            }
            let (x, y): (f64, f64) = runtime
                .lua()
                .load("return physicsToScreenTransform(levelStartPosition.x,levelStartPosition.y)")
                .set_environment(environment.clone())
                .eval()
                .unwrap();
            if (120.0..1024.0).contains(&x) && (0.0..688.0).contains(&y) {
                pointer(runtime, *frame, x, y, true);
                tick(runtime, frame, 1, "practice sling press");
                pointer(runtime, *frame, x - 120.0, y + 40.0, true);
                tick(runtime, frame, 60, "practice sling pull");
                let mut low = (y - 40.0).max(0.0);
                let mut high = (y + 180.0).min(767.0);
                let mut aim_y = y + 40.0;
                for _ in 0..8 {
                    aim_y = (low + high) * 0.5;
                    pointer(runtime, *frame, x - 120.0, aim_y, true);
                    tick(runtime, frame, 30, "practice aim preview");
                    let points = runtime.render.lock().unwrap().trajectory_points.clone();
                    let height = points
                        .windows(2)
                        .find_map(|p| {
                            if p[0].0 <= -4.0 && p[1].0 >= -4.0 && p[1].0 > p[0].0 {
                                Some(
                                    p[0].1
                                        + (p[1].1 - p[0].1) * (-4.0 - p[0].0) / (p[1].0 - p[0].0),
                                )
                            } else {
                                None
                            }
                        })
                        .expect("practice preview does not reach corridor");
                    eprintln!("[practice] preview pointer_y={aim_y} height_at_x_minus4={height}");
                    if (height + 5.65).abs() < 0.1 {
                        break;
                    }
                    if height > -5.65 {
                        low = aim_y;
                    } else {
                        high = aim_y;
                    }
                }
                pointer(runtime, *frame, x - 120.0, aim_y, false);
                tick(runtime, frame, 1, "practice sling release");
                shots += 1;
            }
        }
        tick(runtime, frame, 1, "practice flight");
    }
    eprintln!(
        "[practice] end level={level} frame={frame} won={won} shots={shots} ability_inputs={}",
        targeted.len()
    );
    assert!(won, "practice level did not naturally win");
    tick(runtime, frame, 600, "practice ending and save");
    let result:(bool,u32,u32)=runtime.lua().load("local name=...; return highscores[name] ~= nil,SettingsWrapper:getTimesLevelCompleted(name),SettingsWrapper:getTimesLevelFailed(name)").set_environment(environment).call(level).unwrap();
    assert_eq!(result, (true, 1, 0));
    assert!(runtime.fallback_calls.lock().unwrap().is_empty());
    assert!(runtime.compatibility_bindings.lock().unwrap().is_empty());
}
