use super::stella_route::{aim, aim_object, currency, launch, tick};
use super::*;

pub(super) fn saved_result(runtime: &StellaLua) -> (f64, u32, u32, u32) {
    runtime
        .lua()
        .load(
            r#"
        local h=highscores.Chapter01_G01
        local score=h and h.score or 0
        return score,getLevelStars('Chapter01_G01',score),
            SettingsWrapper:getTimesLevelCompleted('Chapter01_G01'),
            SettingsWrapper:getTimesLevelFailed('Chapter01_G01')
    "#,
        )
        .set_environment(game_environment(runtime.lua()).unwrap())
        .eval()
        .unwrap()
}

fn target(runtime: &StellaLua) -> Option<(String, f64, f64)> {
    runtime.lua().load(r#"
        local chosen=nil
        for name,pig in pairs(levelGoals) do
            if not chosen or pig.x<levelGoals[chosen].x or (pig.x==levelGoals[chosen].x and name<chosen) then chosen=name end
        end
        if chosen then return {chosen,levelGoals[chosen].x,levelGoals[chosen].y} end
    "#).set_environment(game_environment(runtime.lua()).unwrap())
        .eval::<Option<mlua::Table>>().unwrap()
        .map(|t| (t.get(1).unwrap(),t.get(2).unwrap(),t.get(3).unwrap()))
}

// Adjust only held pointer input, using the original visible trajectory preview.
fn launch_right(runtime: &StellaLua, frame: &mut usize, shot: u32) {
    let environment = game_environment(runtime.lua()).unwrap();
    let (x, y): (f64, f64) = runtime
        .lua()
        .load("return physicsToScreenTransform(levelStartPosition.x,levelStartPosition.y)")
        .set_environment(environment.clone())
        .eval()
        .unwrap();
    let height: mlua::Function = runtime
        .lua()
        .load(
            r#"
        return function()
            local p=getSimulationTrajectoryPoints()
            if not p then return nil end
            for i=2,#p do
                if p[i-1].x<=5.85 and p[i].x>=5.85 and p[i].x>p[i-1].x then
                    return p[i-1].y+(p[i].y-p[i-1].y)*(5.85-p[i-1].x)/(p[i].x-p[i-1].x)
                end
            end
        end
    "#,
        )
        .set_environment(environment.clone())
        .eval()
        .unwrap();
    let (mut low, mut high, mut aim_y) = (y - 20.0, y + 80.0, y + 15.0);
    let mut aim_x = x - 240.0;
    let mut best: Option<(f64, f64)> = None;
    // Original cursor history lags input by about 0.1s. Sample each integer
    // horizontal candidate after 20 frames, then settle the best for 60 frames.
    for offset in 0..540 {
        if (120..=280).contains(&offset) && offset % 20 == 0 {
            let h: f64 = height
                .call(())
                .expect("original gap preview must reach calibration point");
            if (h - 2.12).abs() > 0.008 {
                if h > 2.12 {
                    low = aim_y;
                } else {
                    high = aim_y;
                }
                aim_y = (low + high) / 2.0;
            }
        }
        if (300..=480).contains(&offset) && offset % 20 == 0 {
            if offset > 300 {
                let h: f64 = height.call(()).unwrap();
                let error = (h - 2.12).abs();
                eprintln!("[first-gate-fine] shot={shot} x={aim_x} height={h}");
                if best.is_none_or(|(_, previous)| error < previous) {
                    best = Some((aim_x, error));
                }
            }
            aim_x = if offset == 480 {
                best.expect("original horizontal preview candidates").0
            } else {
                (x - 240.0).floor() - 4.0 + ((offset - 300) / 20) as f64
            };
        }
        if offset == 539 {
            let h: f64 = height.call(()).unwrap();
            eprintln!(
                "[first-gate-calibration] shot={shot} height={h} pointer_x={aim_x} pointer_y={aim_y}"
            );
            assert!(
                (h - 2.12).abs() <= 0.02,
                "original gap preview did not converge"
            );
        }
        let (px, py) = if offset < 60 {
            let t = f64::from(offset) / 300.0;
            (x - 240.0 * t, y + 15.0 * t)
        } else {
            (aim_x, aim_y)
        };
        runtime.set_cursor(px, py, true).unwrap();
        tick(runtime, frame, 1, "first gate preview-calibrated hold");
    }
    runtime.set_cursor(aim_x, aim_y, false).unwrap();
    tick(runtime, frame, 1, "first gate calibrated release");
    let count: u32 = runtime
        .lua()
        .load("return birdsShot")
        .set_environment(environment)
        .eval()
        .unwrap();
    assert_eq!(count, shot);
}

pub(super) fn aim_path(
    runtime: &StellaLua,
    frame: &mut usize,
    deadline: usize,
    query: &mlua::Function,
    minimum_rank: u32,
) {
    let environment = game_environment(runtime.lua()).unwrap();
    while *frame <= deadline {
        let (bird, goal): (Option<mlua::Table>, Option<mlua::Table>) = runtime.lua().load(r#"
            local b=flyingBird
            if not b or b.hasCollided or b.abilityDisabled or (b.stellaAbility and b.stellaAbility.hasBeenActivated) then return nil end
            local goal=nil
            for name,pig in pairs(levelGoals) do
                if not goal or pig.x<goal.x or (pig.x==goal.x and name<goal.name) then goal=pig end
            end
            return b,goal
        "#).set_environment(environment.clone()).eval().unwrap();
        let (Some(bird), Some(goal)) = (bird, goal) else {
            break;
        };
        let candidate: (Option<f64>, Option<f64>, Option<String>, Option<u32>) =
            query.call((bird.clone(), goal.clone())).unwrap();
        if let (Some(x), Some(y), Some(predicted), Some(rank)) = candidate
            && rank >= minimum_rank
        {
            let (mut world_x, mut world_y) = (x, y);
            let mut pointer = (0.0, 0.0);
            let mut stable = 0;
            let mut last_path = String::new();
            // Original aiming uses cursor lag and a moving bird/camera. A
            // predicted ray is only a proposed input; inspect the actual
            // authored jumpPath while holding before committing the release.
            for held in 0..90 {
                if held > 0 && held % 8 == 0 {
                    let next: (Option<f64>, Option<f64>, Option<String>, Option<u32>) =
                        query.call((bird.clone(), goal.clone())).unwrap();
                    if let (Some(x), Some(y), _, Some(rank)) = next
                        && rank >= minimum_rank
                    {
                        (world_x, world_y) = (x, y);
                    }
                }
                pointer = runtime
                    .lua()
                    .load("return physicsToScreenTransform(...)")
                    .set_environment(environment.clone())
                    .call((world_x, world_y))
                    .unwrap();
                assert!((0.0..1024.0).contains(&pointer.0) && (0.0..768.0).contains(&pointer.1));
                runtime.set_cursor(pointer.0, pointer.1, true).unwrap();
                tick(runtime, frame, 1, "first gate original aim path feedback");
                let (valid, active, path): (bool, bool, String) = runtime.lua().load(r#"
                    local b,goal,allowSupport=...
                    local valid=false local rows={}
                    for _,p in ipairs(b.stellaAbility.jumpPath or {}) do
                        local o=p.object
                        rows[#rows+1]=tostring(p.x)..','..tostring(p.y)..':'..(o and o.name or '')
                        if o and (o==goal or (allowSupport and o.name~='BOMB_FRUIT_1'
                            and vLength(o.x-goal.x,o.y-goal.y)<0.7
                            and o.material~='decoration' and not string.find(o.name,'HOMETREE')
                            and not string.find(o.name,'STATIC'))) then valid=true end
                    end
                    return valid,not not b.stellaAbility.hasBeenActivated or not not g_levelCompleted,table.concat(rows,';')
                "#).set_environment(environment.clone()).call((bird.clone(), goal.clone(), minimum_rank < 20000)).unwrap();
                if path != last_path && (held == 0 || valid || active) {
                    eprintln!(
                        "[first-gate-gap-feedback] frame={frame} held={held} valid={valid} active={active} path={path}"
                    );
                    last_path = path;
                }
                stable = if valid { stable + 1 } else { 0 };
                if stable >= 2 || active {
                    break;
                }
            }
            eprintln!(
                "[first-gate-gap] release frame={frame} predicted={predicted} rank={rank} verified_frames={stable}"
            );
            runtime.set_cursor(pointer.0, pointer.1, false).unwrap();
            tick(runtime, frame, 91, "first gate verified aim release");
            let activated: bool=runtime.lua().load("local b=...;return not not b.stellaAbility.hasBeenActivated or not not g_levelCompleted")
                    .set_environment(environment).call(bird).unwrap();
            if !activated {
                eprintln!(
                    "[first-gate-gap] original ability did not activate; ordinary flight continues"
                );
            }
            return;
        }
        tick(runtime, frame, 1, "first gate read-only gap search");
    }
    eprintln!("[first-gate-gap] no available path before frame={frame}; ordinary flight continues");
}

pub(super) fn complete(runtime: &StellaLua, frame: &mut usize) -> (f64, u32, u32, u32) {
    assert_scene(runtime, "GameScene", Some("Chapter01_G01"));
    assert_eq!(saved_result(runtime), (0.0, 0, 0, 0));
    let before = currency(runtime);
    let environment = game_environment(runtime.lua()).unwrap();
    let (stars,goals,birds,shots):(u32,u32,u32,u32)=runtime.lua().load("local n=0 for _ in pairs(levelGoals) do n=n+1 end;return SettingsWrapper:getNumber('starsCollected'),n,getRemainingBirdCount(),birdsShot")
        .set_environment(environment.clone()).eval().unwrap();
    assert_eq!((goals, birds, shots), (10, 6, 0));
    tick(runtime, frame, 240, "first gate camera settle");
    let gap_query: mlua::Function = runtime
        .lua()
        .load(include_str!("first_gate_aim.lua"))
        .set_environment(environment.clone())
        .eval()
        .unwrap();
    let mut shots = 0;
    for shot in 1..=6 {
        let Some((_, horizontal, height)) = target(runtime) else {
            break;
        };
        let right = shot > 1 && horizontal > 10.8 && height >= 1.8;
        let middle = shot > 1 && horizontal > 8.3 && horizontal < 10.8;
        let start = *frame;
        let (dy, flight) = if right {
            (15.0, 65)
        } else if middle {
            (100.0, 120)
        } else if shot == 1 || height < 1.8 || (horizontal > 8.3 && horizontal < 10.8) {
            (100.0, 180)
        } else {
            (10.0, 80)
        };
        if right {
            launch_right(runtime, frame, shot);
        } else {
            launch(runtime, frame, -240.0, dy, shot, "first-gate");
        }
        shots = shot;
        tick(runtime, frame, flight - 1, "first gate ability approach");
        if right {
            // The far-right pig can remain beyond the early gap window.
            // Continue observing the real flying bird until collision or
            // 300 flight frames; nearer paths still commit when first valid.
            aim_path(runtime, frame, start + 840, &gap_query, 14000);
        } else if middle {
            // A settled pig can remain shielded by its nearby supporting beam.
            // Prefer direct pig rays, but allow the same verified nearby
            // support path used by the right-hand cleanup route.
            aim_path(runtime, frame, start + 280, &gap_query, 14000);
        } else {
            let fruit = match shot {
                1 => Some("BOMB_FRUIT_3"),
                _ => None,
            };
            let fruit = fruit.filter(|name| {
                runtime
                    .lua()
                    .load("return objects.world[...]~=nil")
                    .set_environment(environment.clone())
                    .call::<bool>(*name)
                    .unwrap()
            });
            if !runtime
                .lua()
                .load(
                    "local b=flyingBird;return b~=nil and not b.hasCollided and not b.abilityDisabled",
                )
                .set_environment(environment.clone())
                .eval::<bool>()
                .unwrap()
            {
                // A real collision before the planned aim is still gameplay.
                // Preserve its outcome and require the same six-bird natural win.
                let state: (String, bool, bool) = runtime.lua().load("local b=flyingBird;return b and b.name or '',b and not not b.hasCollided or false,b and not not b.abilityDisabled or false")
                    .set_environment(environment.clone()).eval().unwrap();
                eprintln!("[first-gate] ordinary collision shot={shot} frame={frame} state={state:?}");
                tick(runtime, frame, 131, "first gate collided bird continuation");
            } else if let Some(name) = fruit {
                aim_object(runtime, frame, name, "first-gate");
            } else if let Some((name, _, _)) = target(runtime) {
                aim(runtime, frame, &name, "first-gate");
            } else {
                tick(
                    runtime,
                    frame,
                    131,
                    "first gate no remaining ability target",
                );
            }
        }

        tick(
            runtime,
            frame,
            start + (if shot == 2 { 2100 } else { 2000 }) - *frame,
            "first gate collapse and camera return",
        );
        let survivors: String = runtime.lua().load("local rows={} for name,pig in pairs(levelGoals) do rows[#rows+1]=string.format('%s@(%.3f,%.3f)',name,pig.x,pig.y) end table.sort(rows);return table.concat(rows,', ')")
            .set_environment(environment.clone()).eval().unwrap();
        eprintln!("[first-gate] after shot={shot} frame={frame} survivors={survivors}");
    }

    let deadline = *frame + 1800;
    loop {
        let (won, failed): (bool, bool) = runtime
            .lua()
            .load("return not not g_levelCompleted,not not g_levelFailed")
            .set_environment(environment.clone())
            .eval()
            .unwrap();
        assert!(
            !failed,
            "first gate exhausted original birds without winning"
        );
        if won {
            break;
        }
        assert!(*frame < deadline, "first gate did not complete naturally");
        tick(runtime, frame, 1, "first gate completion");
    }
    tick(runtime, frame, 1200, "first gate score and rewards");
    let result = saved_result(runtime);
    assert!(result.0 > 0.0 && (1..=3).contains(&result.1));
    assert_eq!((result.2, result.3), (1, 0));
    let state:(u32,u32,u32,u32)=runtime.lua().load("local n=0 for _ in pairs(levelGoals) do n=n+1 end;return n,birdsShot,SettingsWrapper:getNumber('starsCollected'),currentLevelStats.coinsCollected")
        .set_environment(environment.clone()).eval().unwrap();
    assert_eq!((state.0, state.1, state.2), (0, shots, stars + result.1));
    let earned = (before.0 + state.3, before.1 + state.3, before.2);
    assert_eq!(currency(runtime), earned);
    eprintln!(
        "[first-gate] won frame={frame} result={result:?} state={state:?} currency={earned:?}"
    );
    let reward: u32 = runtime
        .lua()
        .load("return g_economyParameters.gates[1].coinReward")
        .set_environment(environment.clone())
        .eval()
        .unwrap();
    assert_eq!(reward, 70);
    practice::click_next(runtime, frame);
    let deadline = *frame + 3600;
    loop {
        let ready:bool=runtime.lua().load("return levelName=='Chapter01_L12' and menuManager:getRoot().name=='GameScene' and notificationsFrame:getChild('levelLoadTransition')==nil")
            .set_environment(environment.clone()).eval().unwrap();
        if ready {
            break;
        }
        assert!(
            *frame < deadline,
            "first gate map reward/Poppy introduction did not reach L12"
        );
        tick(
            runtime,
            frame,
            30,
            "gate destruction reward and Poppy introduction",
        );
    }
    assert_scene(runtime, "GameScene", Some("Chapter01_L12"));
    assert_eq!(
        currency(runtime),
        (earned.0 + reward, earned.1 + reward, earned.2)
    );
    assert_persisted(
        runtime,
        result,
        (earned.0 + reward, earned.1 + reward, earned.2),
    );
    let untouched:bool=runtime.lua().load("return not highscores.Chapter01_L12 and SettingsWrapper:getTimesLevelCompleted('Chapter01_L12')==0 and SettingsWrapper:getTimesLevelFailed('Chapter01_L12')==0 and birdsShot==0 and not g_levelCompleted and not g_levelFailed")
        .set_environment(environment).eval().unwrap();
    assert!(
        untouched,
        "first gate transition granted unearned L12 progress"
    );
    eprintln!(
        "[first-gate] next frame={frame} currency={:?}",
        currency(runtime)
    );
    assert!(runtime.fallback_calls.lock().unwrap().is_empty());
    assert!(runtime.compatibility_bindings.lock().unwrap().is_empty());
    result
}

pub(super) fn assert_persisted(
    runtime: &StellaLua,
    result: (f64, u32, u32, u32),
    inventory: (u32, u32, u32),
) {
    assert_eq!(saved_result(runtime), result);
    assert_eq!(currency(runtime), inventory);
    let state:(u32,u32,String,bool,bool)=runtime.lua().load("return SettingsWrapper:getMapAreaProgress('Chapter01'),GateManager:getCurrentGateNumber(),GateManager:getGateState(1),SettingsWrapper:getFlag('gateShouldBeDestroyed'),SettingsWrapper:getFlag('shouldUnlockArea')")
        .set_environment(game_environment(runtime.lua()).unwrap()).eval().unwrap();
    assert_eq!(state, (4, 1, "destroyed".to_owned(), false, false));
}
