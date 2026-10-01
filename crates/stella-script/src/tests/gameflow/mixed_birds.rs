//! Mixed Poppy/Stella chapter inputs and original-path feedback.
use super::poppy_levels::{Result, click, finish_after_failures, saved_result, until};
use super::poppy_rewards::Rewards;
use super::stella_route::{launch, tick};
use super::*;
use mlua::Table;

enum SixteenthOutcome {
    Won(Result),
    Failed,
}

pub(super) fn sixteenth(runtime: &StellaLua, frame: &mut usize, initial_failures: u32) -> Result {
    for attempt in 0..4 {
        let expected_failures = initial_failures + attempt;
        eprintln!(
            "[sixteenth-attempt] start={} prior_failures={expected_failures}",
            attempt + 1
        );
        match sixteenth_attempt(runtime, frame, expected_failures) {
            SixteenthOutcome::Won(result) => return result,
            SixteenthOutcome::Failed if attempt < 3 => {
                retry_after_failure(runtime, frame, expected_failures + 1);
            }
            SixteenthOutcome::Failed => {
                panic!(
                    "L16 remained unearned after four authored attempts; failures={}",
                    expected_failures + 1
                );
            }
        }
    }
    unreachable!("four L16 attempts must return or panic")
}

fn retry_after_failure(runtime: &StellaLua, frame: &mut usize, failures: u32) {
    let environment = game_environment(runtime.lua()).unwrap();
    let mut button = None;
    for _ in 0..=6 {
        let observed: (bool, Option<f64>, Option<f64>) = runtime
            .lua()
            .load(
                r#"
            local root=menuManager:getRoot()
            local b=root:getChild('restartButton')
            local x,y=nil,nil
            if b and b.visible then x,y=b:getScreenPosition() end
            return root:getChild('nextButton').visible,x,y
        "#,
            )
            .set_environment(environment.clone())
            .eval()
            .unwrap();
        assert!(!observed.0, "failed L16 offered an unearned Next button");
        if let (Some(x), Some(y)) = (observed.1, observed.2) {
            button = Some((x, y));
            break;
        }
        tick(runtime, frame, 60, "L16 original failed-screen animation");
    }
    let (x, y) = button.expect("L16 failure did not offer its original retry button");
    click(runtime, frame, x, y, "L16 original retry button");
    tick(runtime, frame, 540, "L16 original failed restart");
    assert_scene(runtime, "GameScene", Some("Chapter01_L16"));
    assert_eq!(
        saved_result(runtime, "Chapter01_L16"),
        (0.0, 0, 0, failures)
    );
    let restarted: (u32, bool, bool, String) = runtime
        .lua()
        .load("return birdsShot,not not g_levelFailed,not not g_levelCompleted,g_restartType")
        .set_environment(environment)
        .eval()
        .unwrap();
    assert_eq!(restarted, (0, false, false, "FAILED".to_owned()));
    eprintln!("[sixteenth-attempt] authored retry ready frame={frame} failures={failures}");
}

fn sixteenth_attempt(
    runtime: &StellaLua,
    frame: &mut usize,
    expected_failures: u32,
) -> SixteenthOutcome {
    assert_scene(runtime, "GameScene", Some("Chapter01_L16"));
    assert_eq!(
        saved_result(runtime, "Chapter01_L16"),
        (0.0, 0, 0, expected_failures)
    );
    let before = Rewards::capture(runtime);
    let environment = game_environment(runtime.lua()).unwrap();
    let economy_before: (u32, u32, u32, u32) = runtime
        .lua()
        .load("return SettingsWrapper:getNumber('starsCollected'),Coins:getAmount(),settings.iap.gained.coins,settings.iap.used.coins")
        .set_environment(environment.clone())
        .eval()
        .unwrap();
    tick(runtime, frame, 240, "mixed birds camera settle");
    let start = *frame;
    let mut shots = 0;
    'shots: for (index, offset) in [0, 2600, 5600, 8800].into_iter().enumerate() {
        until(
            runtime,
            frame,
            (start + offset).max(*frame),
            "mixed birds collapse and camera return",
        );
        let ready_deadline = *frame + 3600;
        let mut return_attempts = 0;
        loop {
            let (done, failed, ready, x, y, can_return, tap_count): (bool, bool, bool, f64, f64, bool, f64) =
                runtime
                    .lua()
                    .load(
                        r#"
                local x,y=physicsToScreenTransform(levelStartPosition.x,levelStartPosition.y)
                local canReturn=birdSpecialtyAvailable==false
                    and gameCamera.cameraAnimationSliderTarget==1
                    and gameCamera:getNumOfCameraTargetObjects()==0
                return next(levelGoals)==nil,not not g_levelFailed,not not birdReady,x,y,canReturn,tapCount
            "#,
                    )
                    .set_environment(environment.clone())
                    .eval()
                    .unwrap();
            // Failed scenes reject slingshot input even when the retained
            // birdReady flag is true. Use the normal failed-screen accounting
            // and retry below instead of waiting for an impossible camera tap.
            if failed {
                break 'shots;
            }
            if done || (ready && (16.0..1008.0).contains(&x) && (16.0..752.0).contains(&y)) {
                break;
            }
            if ready && can_return && return_attempts < 3 {
                return_attempts += 1;
                eprintln!(
                    "[sixteenth-camera] request={return_attempts} original bird-camera return frame={frame} sling=({x},{y}) tapCount={tap_count}"
                );
                click(
                    runtime,
                    frame,
                    512.0,
                    if return_attempts == 1 { 384.0 } else { 128.0 },
                    "mixed birds original camera-return tap",
                );
                let tap: (f64, bool, bool, bool, Option<String>) = runtime
                    .lua()
                    .load("return tapCount,not not tapStarted,not not g_levelFailed,not not g_refuseDrags,currentEvent")
                    .set_environment(environment.clone())
                    .eval()
                    .unwrap();
                eprintln!("[sixteenth-camera] immediate tap feedback={tap:?}");
                tick(
                    runtime,
                    frame,
                    120,
                    "mixed birds original camera-return animation",
                );
                let after: (f64, bool, f64, f64, f64, f64, bool) = runtime
                    .lua()
                    .load(
                        r#"
                    local x=physicsToScreenTransform(levelStartPosition.x,levelStartPosition.y)
                    return tapCount,not not tapStarted,tapTimer,
                        gameCamera.cameraAnimationSlider,gameCamera.cameraAnimationSliderTarget,
                        x,not not PowerUp.tapProtection
                "#,
                    )
                    .set_environment(environment.clone())
                    .eval()
                    .unwrap();
                eprintln!(
                    "[sixteenth-camera] after request={return_attempts} frame={frame} tap/camera/sling={after:?}"
                );
                continue;
            }
            if *frame >= ready_deadline {
                let diagnostic: String = runtime.lua().load(r#"
                    local b=flyingBird
                    return string.format(
                        'bird=%s collided=%s disabled=%s specialty=%s allowReset=%s moving=%s resetTimer=%s targets=%s tap=%s/%s/%s slider=%s/%s protection=%s',
                        b and b.name or 'none',tostring(b and b.hasCollided),
                        tostring(b and b.abilityDisabled),tostring(birdSpecialtyAvailable),
                        tostring(allowResetToBirdCamera),tostring(hasMovingObjects),
                        tostring(cameraResetTimer),tostring(gameCamera:getNumOfCameraTargetObjects()),
                        tostring(tapCount),tostring(tapStarted),tostring(tapTimer),
                        tostring(gameCamera.cameraAnimationSlider),
                        tostring(gameCamera.cameraAnimationSliderTarget),
                        tostring(PowerUp.tapProtection))
                "#).set_environment(environment.clone()).eval().unwrap();
                panic!(
                    "mixed birds next launch not ready: frame={frame} ready={ready} sling=({x},{y}) {diagnostic}"
                );
            }
            tick(
                runtime,
                frame,
                1,
                "mixed birds original ready and camera return",
            );
        }
        let shot = index as u32 + 1;
        let target: Option<Table> = runtime
            .lua()
            .load(
                r#"
            local shot=...
            if next(levelGoals)==nil then return nil end
            local target=nil
            if shot==1 then target=objects.world.BOMB_FRUIT_1
            elseif shot==2 then target=objects.world.BOMB_FRUIT_2 end
            if not target then
                for name,g in pairs(levelGoals) do
                    if not target
                        or (shot==3 and (g.y>target.y or (g.y==target.y and name<target.name)))
                        or (shot~=3 and (g.x<target.x or (g.x==target.x and name<target.name))) then
                        target=g
                    end
                end
            end
            return {target.name,target.x,target.y}
        "#,
            )
            .set_environment(environment.clone())
            .call(shot)
            .unwrap();
        let Some(target) = target else {
            break;
        };
        let (name, x, y): (String, f64, f64) = (
            target.get(1).unwrap(),
            target.get(2).unwrap(),
            target.get(3).unwrap(),
        );
        eprintln!("[sixteenth-target] shot={shot} frame={frame} target={name}@({x},{y})");
        let shot_start = *frame;
        launch(runtime, frame, -240.0, 100.0, shot, "sixteenth");
        shots = shot;
        if shot == 1 || shot == 3 {
            until(
                runtime,
                frame,
                shot_start + 80,
                "Poppy fruit column approach",
            );
            while *frame <= shot_start + 380 {
                let (available, ready, bx, by): (bool, bool, Option<f64>, Option<f64>) = runtime
                    .lua()
                    .load(
                        r#"
                    local x,y,verticalGap=...
                    local b=flyingBird
                    if not b or b.hasCollided or b.abilityDisabled then return false,false,nil,nil end
                    local vx=getLinearVelocity(b.name)
                    return true,b.x+vx*getDeltaTimeMultiplier()/60>=x and b.y<=y-verticalGap,b.x,b.y
                "#,
                    )
                    .set_environment(environment.clone())
                    .call((x, y, if shot == 3 { 0.5 } else { 1.0 }))
                    .unwrap();
                if shot == 3 && (*frame - shot_start).is_multiple_of(20) {
                    eprintln!(
                        "[sixteenth-poppy] frame={frame} target={name} available={available} ready={ready} bird=({bx:?},{by:?})"
                    );
                }
                if !available {
                    break;
                }
                if ready {
                    eprintln!("[sixteenth-poppy] tap frame={frame} target={name}");
                    click(runtime, frame, 512.0, 384.0, "Poppy mixed level drill");
                    break;
                }
                tick(runtime, frame, 1, "Poppy live fruit column search");
            }
        } else if shot == 2 {
            until(
                runtime,
                frame,
                shot_start + 125,
                "Stella early fruit ray approach",
            );
            aim_stella(
                runtime,
                frame,
                StellaAim {
                    preferred: "BOMB_FRUIT_2",
                    goals_after: shot_start + 240,
                    deadline: shot_start + 340,
                    goals_first: false,
                    priority_goal: None,
                    support_after: None,
                },
            );
        } else {
            until(
                runtime,
                frame,
                shot_start + 65,
                "Stella early cleanup ray approach",
            );
            aim_stella(
                runtime,
                frame,
                StellaAim {
                    preferred: "BOMB_FRUIT_3",
                    goals_after: shot_start + 90,
                    deadline: shot_start + 300,
                    goals_first: true,
                    priority_goal: Some(&name),
                    support_after: Some(shot_start + 90),
                },
            );
        }
    }
    let result_frame = (start + 13800).max(*frame + 1800);
    until(runtime, frame, result_frame, "mixed birds original result");
    let (completed, failed, popup, recorded_shots, collected): (bool, bool, bool, u32, u32) =
        runtime
            .lua()
            .load(
                r#"
            local p=menuManager:getRoot():getChild('lastChancePopup')
            return not not g_levelCompleted,not not g_levelFailed,p~=nil and p.visible,
                birdsShot,currentLevelStats.coinsCollected
        "#,
            )
            .set_environment(environment.clone())
            .eval()
            .unwrap();
    assert_eq!(recorded_shots, shots);
    if completed {
        assert!(!failed && !popup);
        return SixteenthOutcome::Won(finish_after_failures(
            runtime,
            frame,
            "Chapter01_L16",
            "Chapter01_L17",
            before,
            shots,
            expected_failures,
        ));
    }
    assert!(
        failed || popup,
        "L16 shots ended without an original result"
    );
    if popup {
        let (x, y): (f64, f64) = runtime
            .lua()
            .load("local b=menuManager:getRoot():getChild('lastChancePopup'):getChild('giveUpButton');return b:getScreenPosition()")
            .set_environment(environment.clone())
            .eval()
            .unwrap();
        click(runtime, frame, x, y, "L16 original last-chance refusal");
        tick(runtime, frame, 720, "L16 original failed-screen transition");
    }
    assert_eq!(shots, 4, "L16 failed before all four normal birds fired");
    let loss_result = saved_result(runtime, "Chapter01_L16");
    assert_eq!(loss_result, (0.0, 0, 0, expected_failures + 1));
    let economy_after: (u32, u32, u32, u32) = runtime
        .lua()
        .load("return SettingsWrapper:getNumber('starsCollected'),Coins:getAmount(),settings.iap.gained.coins,settings.iap.used.coins")
        .set_environment(environment)
        .eval()
        .unwrap();
    assert_eq!(economy_after.0, economy_before.0);
    assert_eq!(economy_after.3, economy_before.3);
    assert_eq!(
        economy_after.1 as i64 - economy_before.1 as i64,
        economy_after.2 as i64 - economy_before.2 as i64
    );
    eprintln!(
        "[sixteenth-attempt] natural failure frame={frame} shots={shots} collected={collected} before={before:?} after={:?} saved={loss_result:?}",
        Rewards::capture(runtime)
    );
    SixteenthOutcome::Failed
}

struct StellaAim<'a> {
    preferred: &'a str,
    goals_after: usize,
    deadline: usize,
    goals_first: bool,
    priority_goal: Option<&'a str>,
    support_after: Option<usize>,
}

fn aim_stella(runtime: &StellaLua, frame: &mut usize, aim: StellaAim<'_>) {
    let StellaAim {
        preferred,
        goals_after,
        deadline,
        goals_first,
        priority_goal,
        support_after,
    } = aim;
    let environment = game_environment(runtime.lua()).unwrap();
    // Keep the original ray's direction but avoid top/bottom HUD controls.
    let source = format!(
        "local choose=(function() {} end)(); {}",
        include_str!("first_gate_aim.lua"),
        r#"
        return function(b,goal)
            local x,y,key,rank=choose(b,goal)
            if not x or rank<14000 then return nil end
            local vx,vy=getLinearVelocity(b.name)
            local dt=getDeltaTimeMultiplier()/60
            local bx,by=b.x+vx*dt,b.y+vy*dt
            local length=vLength(x-bx,y-by)
            if length==0 then return nil end
            local dx,dy=(x-bx)/length,(y-by)/length
            for distance=9,1,-0.5 do
                local tx,ty=bx+dx*distance,by+dy*distance
                local sx,sy=physicsToScreenTransform(tx,ty)
                if sx>=160 and sx<960 and sy>=120 and sy<640 then return tx,ty,key,rank end
            end
        end
        "#
    );
    let query: mlua::Function = runtime
        .lua()
        .load(&source)
        .set_environment(environment.clone())
        .eval()
        .unwrap();
    let mut held_pointer = None;
    let mut diagnosed_priority = false;
    while *frame <= deadline {
        let (bird, targets): (Option<Table>, Option<Table>) = runtime.lua().load(r#"
            local b=flyingBird
            if not b or b.hasCollided or b.abilityDisabled or next(levelGoals)==nil then return nil end
            local preferred,allowPigs,goalsFirst,priorityGoal=...
            local targets={};local fruit=objects.world[preferred]
            local names={}
            local priority=priorityGoal and levelGoals[priorityGoal]
            if priority then
                names[1]=priorityGoal
            elseif allowPigs or not fruit then
                for n in pairs(levelGoals) do names[#names+1]=n end
            end
            table.sort(names)
            if not priority and not goalsFirst and fruit then targets[#targets+1]=fruit end
            for _,n in ipairs(names) do targets[#targets+1]=levelGoals[n] end
            if not priority and goalsFirst and fruit then targets[#targets+1]=fruit end
            return b,targets
        "#).set_environment(environment.clone()).call((preferred, *frame >= goals_after, goals_first, priority_goal)).unwrap();
        let (Some(bird), Some(targets)) = (bird, targets) else {
            break;
        };
        let mut target_gone = false;
        for goal in targets.sequence_values::<Table>() {
            let goal = goal.unwrap();
            let name: String = goal.get("name").unwrap();
            let (x, y, key, rank): (Option<f64>, Option<f64>, Option<String>, Option<u32>) =
                query.call((bird.clone(), goal.clone())).unwrap();
            let (Some(mut x), Some(mut y), Some(key), Some(rank)) = (x, y, key, rank) else {
                if priority_goal == Some(name.as_str()) && !diagnosed_priority {
                    let diagnostic: String = runtime.lua().load(r#"
                        local b,goal=...
                        local cfg=getObjectDefinition(b.name).components.stella
                        local seen={}
                        local function record(hit)
                            local o=hit and hit.object
                            if not o or not o.x or not o.y then return end
                            local row=seen[o.name]
                            if not row then
                                row={distance=vLength(o.x-goal.x,o.y-goal.y),hits=0,
                                    material=tostring(o.material),x=o.x,y=o.y}
                                seen[o.name]=row
                            end
                            row.hits=row.hits+1
                        end
                        for i=-360,360 do
                            local angle=math.rad(i/2)
                            local dx,dy=math.cos(angle),math.sin(angle)
                            local first=raycast(b.x,b.y,b.x+dx*cfg.maxDistance,
                                b.y+dy*cfg.maxDistance,{b},function(o)return not o.ignoreStellaAimHit end)
                            record(first)
                            if first and first.object.material~='grabbable'
                                and not contains(cfg.stoppingMaterials,first.object.material) then
                                local nx,ny=first.rayCastNormalX,first.rayCastNormalY
                                if first.object.type=='circle' then
                                    local step=math.rad(cfg.circleAngleStep)
                                    nx,ny=vec2FromAngle(math.floor((math.atan2(ny,nx)+0.5*step)/step)*step)
                                end
                                local x=first.rayCastContactX+first.rayCastNormalX*b.radius
                                local y=first.rayCastContactY+first.rayCastNormalY*b.radius
                                local remaining=cfg.maxDistance-vLength(x-b.x,y-b.y)
                                local rx,ry=vec2Reflect(dx,dy,nx,ny)
                                if remaining>0 then
                                    record(raycast(x,y,x+rx*remaining,y+ry*remaining,{b},
                                        function(o)return not o.ignoreStellaAimHit end))
                                end
                            end
                        end
                        local rows={}
                        for object,row in pairs(seen) do
                            rows[#rows+1]=string.format('%06.3f:%s@%.3f,%.3f:%s:h%d',
                                row.distance,object,row.x,row.y,row.material,row.hits)
                        end
                        table.sort(rows)
                        while #rows>16 do table.remove(rows) end
                        return table.concat(rows,';')
                    "#).set_environment(environment.clone()).call((bird.clone(),goal.clone())).unwrap();
                    eprintln!("[sixteenth-priority-rays] frame={frame} target={name} {diagnostic}");
                    diagnosed_priority = true;
                }
                continue;
            };
            if *frame < support_after.unwrap_or(0) && rank < 20000 {
                continue;
            }
            let is_goal: bool = runtime
                .lua()
                .load("local goal=...;return levelGoals[goal.name]==goal")
                .set_environment(environment.clone())
                .call(goal.clone())
                .unwrap();
            let mut stable = 0;
            let mut active = false;
            for held in 0..24 {
                if *frame > deadline {
                    break;
                }
                if held > 0 && held % 8 == 0 {
                    let next: (Option<f64>, Option<f64>, Option<String>, Option<u32>) =
                        query.call((bird.clone(), goal.clone())).unwrap();
                    if let (Some(nx), Some(ny), _, Some(next_rank)) = next
                        && next_rank >= 14000
                    {
                        (x, y) = (nx, ny);
                    }
                }
                let pointer: (f64, f64) = runtime
                    .lua()
                    .load("return physicsToScreenTransform(...)")
                    .set_environment(environment.clone())
                    .call((x, y))
                    .unwrap();
                assert!(
                    (160.0..960.0).contains(&pointer.0) && (120.0..640.0).contains(&pointer.1),
                    "mixed aim moved over HUD"
                );
                runtime.set_cursor(pointer.0, pointer.1, true).unwrap();
                held_pointer = Some(pointer);
                tick(runtime, frame, 1, "Stella actual mixed-level path feedback");
                let (alive, valid, activated, path): (bool, bool, bool, String) = runtime
                    .lua()
                    .load(
                        r#"
                    local b,goal,isGoal,allowSupport=...;local rows={};local valid=false
                    local alive=(isGoal and levelGoals[goal.name]==goal)
                        or (not isGoal and objects.world[goal.name]==goal)
                    for _,p in ipairs(b.stellaAbility.jumpPath or {}) do
                        local o=p.object
                        if o and (o==goal or (allowSupport
                            and vLength(o.x-goal.x,o.y-goal.y)<0.7
                            and o.material~='decoration' and not string.find(o.name,'HOMETREE')
                            and not string.find(o.name,'STATIC'))) then valid=true end
                        rows[#rows+1]=tostring(p.x)..','..tostring(p.y)..':'..(o and o.name or '')
                    end
                    return alive,valid,not not b.stellaAbility.hasBeenActivated
                        or not not g_levelCompleted,table.concat(rows,';')
                "#,
                    )
                    .set_environment(environment.clone())
                    .call((bird.clone(), goal.clone(), is_goal, rank < 20000))
                    .unwrap();
                active = activated;
                stable = if valid { stable + 1 } else { 0 };
                if held == 0 || valid || active || !alive {
                    eprintln!(
                        "[sixteenth-feedback] frame={frame} target={name} alive={alive} valid={valid} active={active} path={path}"
                    );
                }
                if !alive {
                    target_gone = true;
                    break;
                }
                if stable >= 2 || active {
                    break;
                }
            }
            if stable >= 2 || active {
                let pointer = held_pointer.expect("verified aim must hold the pointer");
                eprintln!(
                    "[sixteenth-feedback] release frame={frame} predicted={key} rank={rank} verified_frames={stable}"
                );
                runtime.set_cursor(pointer.0, pointer.1, false).unwrap();
                tick(runtime, frame, 91, "Stella mixed-level verified release");
                let active: bool=runtime.lua().load("local b=...;return not not b.stellaAbility.hasBeenActivated or not not g_levelCompleted")
                    .set_environment(environment.clone()).call(bird).unwrap();
                assert!(
                    active,
                    "mixed-level verified input did not activate original ability"
                );
                return;
            }
            if target_gone {
                break;
            }
        }
        if target_gone {
            continue;
        }
        tick(runtime, frame, 1, "mixed-level read-only ray search");
    }
    if let Some(pointer) = held_pointer {
        runtime.set_cursor(pointer.0, pointer.1, false).unwrap();
        tick(runtime, frame, 1, "Stella mixed-level unverified release");
    }
    eprintln!(
        "[sixteenth-feedback] no usable ray for {preferred} by frame={frame}; ordinary flight continues"
    );
}
