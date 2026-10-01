use super::*;

pub(super) fn tick(runtime: &StellaLua, frame: &mut usize, count: usize, stage: &str) {
    advance(runtime, count, stage);
    *frame += count;
}

fn pointer(runtime: &StellaLua, frame: usize, x: f64, y: f64, down: bool, tag: &str) {
    eprintln!("[{tag}-input] frame={frame} x={x} y={y} down={down}");
    runtime.set_cursor(x, y, down).unwrap();
}

pub(super) fn launch(
    runtime: &StellaLua,
    frame: &mut usize,
    dx: f64,
    dy: f64,
    shot: u32,
    tag: &str,
) {
    let environment = game_environment(runtime.lua()).unwrap();
    let (x, y): (f64, f64) = runtime
        .lua()
        .load("return physicsToScreenTransform(levelStartPosition.x,levelStartPosition.y)")
        .set_environment(environment.clone())
        .eval()
        .unwrap();
    // Match the successful real host drag, including camera updates during
    // the pull. Do not teleport the pointer to its endpoint for the hold.
    for offset in 0..60 {
        let t = f64::from(offset) / 60.0;
        pointer(runtime, *frame, x + dx * t, y + dy * t, true, tag);
        tick(runtime, frame, 1, "Stella route sling drag");
    }
    pointer(runtime, *frame, x + dx, y + dy, false, tag);
    tick(runtime, frame, 1, "Stella route release");
    let count: u32 = runtime
        .lua()
        .load("return birdsShot")
        .set_environment(environment)
        .eval()
        .unwrap();
    assert_eq!(count, shot);
}

pub(super) fn aim(runtime: &StellaLua, frame: &mut usize, target: &str, tag: &str) {
    aim_at(runtime, frame, target, tag, false);
}

pub(super) fn aim_object(runtime: &StellaLua, frame: &mut usize, target: &str, tag: &str) {
    aim_at(runtime, frame, target, tag, true);
}

fn aim_at(runtime: &StellaLua, frame: &mut usize, target: &str, tag: &str, world: bool) {
    let environment = game_environment(runtime.lua()).unwrap();
    let (x,y,name): (f64,f64,String)=runtime.lua().load(r#"
        local target,world=...
        local goal=(world and objects.world or levelGoals)[target]
        local b=flyingBird
        if not goal or not b or b.hasCollided or b.abilityDisabled then error('Stella route ability approach blocked') end
        if b.stellaAbility.hasBeenActivated then error('Stella route ability already used') end
        local x,y=physicsToScreenTransform(goal.x,goal.y)
        return x,y,b.name
    "#).set_environment(environment.clone()).call((target,world)).unwrap();
    assert!((0.0..1024.0).contains(&x) && (0.0..768.0).contains(&y));
    pointer(runtime, *frame, x, y, true, tag);
    tick(runtime, frame, 130, "Stella route ability aim");
    let path:String=runtime.lua().load("local a=objects.world[...].stellaAbility;local r={};for _,p in ipairs(a.jumpPath or {}) do r[#r+1]=tostring(p.x)..','..tostring(p.y)..':'..(p.object and p.object.name or '') end;return table.concat(r,';')")
        .set_environment(environment.clone()).call(name.as_str()).unwrap();
    eprintln!("[{tag}] ability frame={frame} target={target} path={path}");
    pointer(runtime, *frame, x, y, false, tag);
    tick(runtime, frame, 1, "Stella route ability release");
    let used: bool = runtime
        .lua()
        .load("return not not objects.world[...].stellaAbility.hasBeenActivated")
        .set_environment(environment)
        .call(name)
        .unwrap();
    assert!(used, "Stella route host aim did not activate Stella");
}

pub(super) fn currency(runtime: &StellaLua) -> (u32, u32, u32) {
    runtime.lua().load(r#"
        if next(SettingsWrapper:getPendingRewards('coins')) then error('route pending reward remains') end
        return Coins:getAmount(),settings.iap.gained.coins,settings.iap.used.coins
    "#).set_environment(game_environment(runtime.lua()).unwrap()).eval().unwrap()
}
