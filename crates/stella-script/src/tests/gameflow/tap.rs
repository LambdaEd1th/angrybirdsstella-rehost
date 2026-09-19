use super::*;

fn tick(runtime: &StellaLua, frame: &mut usize, frames: usize, stage: &str) {
    advance(runtime, frames, stage);
    *frame += frames;
}

fn pointer(runtime: &StellaLua, frame: usize, x: f64, y: f64, down: bool) {
    eprintln!("[tap-input] frame={frame} x={x} y={y} down={down}");
    runtime.set_cursor(x, y, down).unwrap();
}

fn sling_position(runtime: &StellaLua) -> (f64, f64) {
    runtime
        .lua()
        .load("return physicsToScreenTransform(levelStartPosition.x,levelStartPosition.y)")
        .set_environment(game_environment(runtime.lua()).unwrap())
        .eval()
        .unwrap()
}

fn launch(runtime: &StellaLua, frame: &mut usize, x: f64, y: f64) {
    pointer(runtime, *frame, x, y, true);
    tick(runtime, frame, 1, "tutorial sling press");
    pointer(runtime, *frame, x - 120.0, y + 60.0, true);
    tick(runtime, frame, 60, "tutorial sling pull");
    pointer(runtime, *frame, x - 120.0, y + 60.0, false);
    tick(runtime, frame, 1, "tutorial sling release");
}

#[test]
fn shipped_stella_tap_tutorial_accepts_host_ability_input() {
    let sandbox = ShippedDataSandbox::new("original-stella-tap");
    let runtime = StellaLua::new(&sandbox.data_root).unwrap();
    runtime.enable_local_services().unwrap();
    runtime.boot("scripts/game.lua").unwrap();
    let mut frame = 0;
    tick(&runtime, &mut frame, 600, "tap startup");
    runtime
        .execute_source("LevelLoad.transitionToLevel('Chapter01',2)")
        .unwrap();
    tick(&runtime, &mut frame, 360, "tap entry");
    complete_tap(&runtime, &mut frame);
}

pub(super) fn complete_tap(runtime: &StellaLua, clock: &mut usize) {
    let mut frame = *clock;
    assert_scene(runtime, "GameScene", Some("Chapter01_L02"));
    let environment = game_environment(runtime.lua()).unwrap();
    let snapshot = runtime
        .lua()
        .load(
            r#"return function()
        local t = g_tutorials.TUTORIAL_TYPE_TAP_STELLA
        local area = menuManager:getRoot():getChild('tapArea')
        local count = 0 for _ in pairs(levelGoals) do count = count + 1 end
        local gx,gy = 0,0
        if t.goal then gx,gy = physicsToScreenTransform(t.goal.x,t.goal.y) end
        local reachable = false
        if flyingBird and t.goal then
            local maxDistance = getObjectDefinition(flyingBird.name).components.stella.maxDistance
            reachable = vLength(flyingBird.x-t.goal.x,flyingBird.y-t.goal.y) < maxDistance * 0.8
        end
        return count,not not isGamePausedByTutorial(),area and area.visible or false,
            area and area.showFinger or false,gx,gy,flyingBird and flyingBird.name or '',
            not not g_levelCompleted,area and area.canInterruptFinger or false,not not birdReady,reachable
    end"#,
        )
        .set_environment(environment.clone())
        .eval::<Function>()
        .unwrap();
    type State = (
        u32,
        bool,
        bool,
        bool,
        f64,
        f64,
        String,
        bool,
        bool,
        bool,
        bool,
    );
    let initial: State = snapshot.call(()).unwrap();
    assert_eq!(initial.0, 2, "original tutorial goals changed");
    let (x, y) = sling_position(runtime);
    launch(runtime, &mut frame, x, y);
    let mut shots = 1;
    let mut targeted_birds = std::collections::HashSet::new();
    let mut release_at = None;
    let mut target = (0.0, 0.0);
    let mut saw_pause = false;
    let mut saw_demonstration = false;
    let mut saw_resume = false;
    let deadline = frame + 3840;
    while frame < deadline {
        let state: State = snapshot.call(()).unwrap();
        let (goals, paused, visible, finger, gx, gy, bird, won, interruptible, ready, reachable) =
            state;
        saw_pause |= paused;
        saw_demonstration |= paused && visible && finger;
        saw_resume |= saw_pause && !paused;
        if frame.is_multiple_of(120) {
            eprintln!(
                "[tap] frame={frame} goals={goals} paused={paused} visible={visible} finger={finger} bird={bird}"
            );
        }
        if won {
            assert_eq!(goals, 0);
            break;
        }
        if let Some(release) = release_at {
            if frame >= release {
                pointer(runtime, frame, target.0, target.1, false);
                release_at = None;
            }
        } else if visible
            && (!finger || interruptible)
            && !bird.is_empty()
            && (goals != 1 || reachable)
            && !targeted_birds.contains(&bird)
            && (0.0..1024.0).contains(&gx)
            && (0.0..768.0).contains(&gy)
        {
            // One action per bird: finish the authored hold demonstration,
            // then tap the final target when its demonstration is interruptible.
            target = (gx, gy);
            pointer(runtime, frame, gx, gy, true);
            release_at = Some(frame + if goals == 1 { 1 } else { 130 });
            targeted_birds.insert(bird);
        } else if bird.is_empty() && ready && shots < 4 {
            let (sx, sy) = sling_position(runtime);
            // Wait for the authored camera return before clicking a visible sling.
            if (120.0..900.0).contains(&sx) && (0.0..708.0).contains(&sy) {
                launch(runtime, &mut frame, sx, sy);
                shots += 1;
            }
        }
        tick(runtime, &mut frame, 1, "tap flight/tutorial");
    }
    let end: State = snapshot.call(()).unwrap();
    eprintln!("[tap] end frame={frame} state={end:?} shots={shots}");
    assert!(end.7, "tutorial did not naturally complete");
    assert!(saw_pause && saw_demonstration && saw_resume);
    assert!(targeted_birds.len() >= 2);
    let activations: (u32, u32) = runtime
        .lua()
        .load("local s=settings.birdActivationStats.Stella; return s.tap or 0,s.hold or 0")
        .set_environment(environment.clone())
        .eval()
        .unwrap();
    eprintln!("[tap] activations={activations:?}");
    assert!(activations.0 + activations.1 >= 2);
    tick(runtime, &mut frame, 600, "tap ending and save");
    let completed: (bool, u32, u32) = runtime
        .lua()
        .load("return highscores.Chapter01_L02 ~= nil,SettingsWrapper:getTimesLevelCompleted('Chapter01_L02'),SettingsWrapper:getTimesLevelFailed('Chapter01_L02')")
        .set_environment(environment)
        .eval()
        .unwrap();
    assert_eq!(completed, (true, 1, 0));
    assert!(runtime.fallback_calls.lock().unwrap().is_empty());
    assert!(runtime.compatibility_bindings.lock().unwrap().is_empty());
    *clock = frame;
}
