use super::*;
use mlua::Table;

fn tick(runtime: &StellaLua, frame: &mut usize, frames: usize, stage: &str) {
    advance(runtime, frames, stage);
    *frame += frames;
}

fn pointer(runtime: &StellaLua, frame: usize, x: f64, y: f64, down: bool) {
    eprintln!("[hold-input] frame={frame} x={x} y={y} down={down}");
    runtime.set_cursor(x, y, down).unwrap();
}

#[test]
fn shipped_stella_hold_tutorial_aims_and_bounces_through_host_input() {
    let sandbox = ShippedDataSandbox::new("original-stella-hold");
    let runtime = StellaLua::new(&sandbox.data_root).unwrap();
    runtime.enable_local_services().unwrap();
    runtime.boot("scripts/game.lua").unwrap();
    let mut frame = 0;
    tick(&runtime, &mut frame, 600, "hold startup");
    runtime
        .execute_source("LevelLoad.transitionToLevel('Chapter01',3)")
        .unwrap();
    tick(&runtime, &mut frame, 360, "hold entry");
    complete_hold(&runtime, &mut frame);
}

pub(super) fn complete_hold(runtime: &StellaLua, frame: &mut usize) {
    assert_scene(runtime, "GameScene", Some("Chapter01_L03"));
    let environment = game_environment(runtime.lua()).unwrap();
    let snapshot = runtime.lua().load(r#"return function()
        local t=g_tutorials.TUTORIAL_TYPE_HOLD_STELLA
        local area=menuManager:getRoot():getChild('tapArea')
        local n=0 for _ in pairs(levelGoals) do n=n+1 end
        local x,y=physicsToScreenTransform(t.targetStart.x,t.targetStart.y)
        local b=flyingBird
        return {goals=n,paused=not not isGamePausedByTutorial(),visible=area and area.visible or false,
            finger=area and area.showFinger or false,hide=area and area.hideUI or false,
            x=x,y=y,bird=b and b.name or '',ready=not not birdReady,current=currentBirdName or '',
            won=not not g_levelCompleted,active=b and b.stellaAbility and b.stellaAbility.hasBeenActivated or false,
            jumps=b and b.stellaAbility and b.stellaAbility.jumps or 0}
    end"#).set_environment(environment.clone()).eval::<Function>().unwrap();
    let deadline = *frame + 4800;
    let mut shots = 0;
    let mut targeted = std::collections::HashSet::new();
    let mut releasing = None;
    let mut target = (0.0, 0.0);
    let mut saw_pause = false;
    let mut saw_active = false;
    let mut max_jumps = 0_u32;
    let mut won = false;
    while *frame < deadline {
        let s: Table = snapshot.call(()).unwrap();
        let goals: u32 = s.get("goals").unwrap();
        let paused: bool = s.get("paused").unwrap();
        let visible: bool = s.get("visible").unwrap();
        let finger: bool = s.get("finger").unwrap();
        let hide: bool = s.get("hide").unwrap();
        let bird: String = s.get("bird").unwrap();
        saw_pause |= paused;
        saw_active |= s.get::<bool>("active").unwrap();
        max_jumps = max_jumps.max(s.get("jumps").unwrap());
        if frame.is_multiple_of(120) {
            eprintln!(
                "[hold] frame={frame} goals={goals} paused={paused} visible={visible} finger={finger} hide={hide} bird={bird} jumps={max_jumps}"
            );
        }
        if s.get::<bool>("won").unwrap() {
            won = true;
            break;
        }
        if let Some(at) = releasing {
            if *frame >= at {
                pointer(runtime, *frame, target.0, target.1, false);
                releasing = None;
            }
        } else if visible && !finger && !hide && !bird.is_empty() && !targeted.contains(&bird) {
            let x: f64 = s.get("x").unwrap();
            let y: f64 = s.get("y").unwrap();
            assert!((0.0..1024.0).contains(&x) && (0.0..768.0).contains(&y));
            target = (x, y);
            pointer(runtime, *frame, x, y, true);
            releasing = Some(*frame + 130);
            targeted.insert(bird);
        } else if bird.is_empty() && s.get::<bool>("ready").unwrap() && shots < 3 {
            let (x, y): (f64, f64) = runtime
                .lua()
                .load("return physicsToScreenTransform(levelStartPosition.x,levelStartPosition.y)")
                .set_environment(environment.clone())
                .eval()
                .unwrap();
            if (120.0..1024.0).contains(&x) && (0.0..708.0).contains(&y) {
                pointer(runtime, *frame, x, y, true);
                tick(runtime, frame, 1, "hold sling press");
                pointer(runtime, *frame, x - 120.0, y + 40.0, true);
                tick(runtime, frame, 60, "hold sling pull");
                // Adjust the real held pointer using the same trajectory the
                // original aiming UI displays; never assign a launch velocity.
                let mut low = (y - 40.0).max(0.0);
                let mut high = (y + 180.0).min(767.0);
                let mut aim_y = y + 40.0;
                for _ in 0..8 {
                    aim_y = (low + high) * 0.5;
                    pointer(runtime, *frame, x - 120.0, aim_y, true);
                    tick(runtime, frame, 30, "hold aim preview");
                    let points = runtime.render.lock().unwrap().trajectory_points.clone();
                    let height = points
                        .windows(2)
                        .find_map(|p| {
                            if p[0].0 <= 1.0 && p[1].0 >= 1.0 && p[1].0 > p[0].0 {
                                Some(
                                    p[0].1 + (p[1].1 - p[0].1) * (1.0 - p[0].0) / (p[1].0 - p[0].0),
                                )
                            } else {
                                None
                            }
                        })
                        .expect("aim preview never reaches tutorial region");
                    eprintln!("[hold] preview pointer_y={aim_y} height_at_x1={height}");
                    if (height + 5.7).abs() < 0.2 {
                        break;
                    }
                    if height > -5.7 {
                        low = aim_y;
                    } else {
                        high = aim_y;
                    }
                }
                pointer(runtime, *frame, x - 120.0, aim_y, false);
                tick(runtime, frame, 1, "hold sling release");
                shots += 1;
            }
        }
        tick(runtime, frame, 1, "hold tutorial flight");
    }
    eprintln!(
        "[hold] end frame={frame} won={won} pause={saw_pause} active={saw_active} jumps={max_jumps} shots={shots}"
    );
    assert!(
        won && saw_pause && saw_active && max_jumps > 0,
        "hold tutorial did not aim, bounce and win"
    );
    tick(runtime, frame, 600, "hold completion persistence");
    let result:(bool,u32,u32)=runtime.lua().load("return highscores.Chapter01_L03 ~= nil,SettingsWrapper:getTimesLevelCompleted('Chapter01_L03'),SettingsWrapper:getTimesLevelFailed('Chapter01_L03')").set_environment(environment).eval().unwrap();
    assert_eq!(result, (true, 1, 0));
    assert!(runtime.fallback_calls.lock().unwrap().is_empty());
    assert!(runtime.compatibility_bindings.lock().unwrap().is_empty());
}
