use super::*;

fn tick(runtime: &StellaLua, frame: &mut usize, count: usize, stage: &str) {
    advance(runtime, count, stage);
    *frame += count;
}

fn pointer(runtime: &StellaLua, frame: usize, x: f64, y: f64, down: bool) {
    eprintln!("[last-chance-input] frame={frame} x={x} y={y} down={down}");
    runtime.set_cursor(x, y, down).unwrap();
}

fn click(runtime: &StellaLua, frame: &mut usize, expression: &str) {
    let (x, y): (f64, f64) = runtime.lua()
        .load(format!("local b={expression}; if not b or not b.visible then error('last-chance control missing') end; return b:getScreenPosition()"))
        .set_environment(game_environment(runtime.lua()).unwrap()).eval().unwrap();
    pointer(runtime, *frame, x, y, true);
    tick(runtime, frame, 1, "last chance button press");
    pointer(runtime, *frame, x, y, false);
    tick(runtime, frame, 1, "last chance button release");
}

/// Spend only the coins earned by the preceding natural first-five gameplay.
pub(super) fn buy_play_and_decline_insufficient_coins(runtime: &StellaLua, frame: &mut usize) {
    eprintln!("[last-chance] start frame={frame}");
    let (_, coins) = earned_retry::exhaust_to_popup(runtime, frame, "last-chance");
    let environment = game_environment(runtime.lua()).unwrap();
    let price: u32 = runtime
        .lua()
        .load("return getGameHud():getChild('lastChancePopup').price")
        .set_environment(environment.clone())
        .eval()
        .unwrap();
    assert_eq!((coins, price), (66, 60));
    eprintln!("[last-chance] offer frame={frame} coins={coins} price={price}");
    click(
        runtime,
        frame,
        "getGameHud():getChild('lastChancePopup'):getChild('btnBuy')",
    );
    let deadline = *frame + 1800;
    let bird = loop {
        let (ready, name): (bool, String) = runtime
            .lua()
            .load(
                r#"
            local b=currentBirdName and objects.world[currentBirdName]
            return b ~= nil and b.isExtraBird and not not birdReady
                and not g_levelFailed and not g_levelCompleted
                and not g_levelEndLogic.preventLevelFailing,
                currentBirdName or ''
        "#,
            )
            .set_environment(environment.clone())
            .eval()
            .unwrap();
        if ready {
            break name;
        }
        assert!(
            *frame < deadline,
            "coin-paid extra bird did not become playable"
        );
        tick(runtime, frame, 1, "coin-paid bird entrance");
    };
    tick(runtime, frame, 240, "extra bird camera settle");
    let bought: (u32, u32, u32, u32, u32, u32, u32, u32, bool) = runtime
        .lua()
        .load(
            r#"
        return Coins:getAmount(),settings.iap.gained.coins,settings.iap.used.coins,
            settings.iap.sync.coins,getRemainingBirdCount(),birdsShot,
            currentLevelStats.lastChanceBirdsUsed,Telepods.extraBirdPurchases or 0,
            getGameHud():getChild('lastChancePopup') == nil
    "#,
        )
        .set_environment(environment.clone())
        .eval()
        .unwrap();
    assert_eq!(bought, (6, 66, 60, 6, 1, 3, 1, 1, true));
    assert!(runtime.render.lock().unwrap().physics_enabled);
    eprintln!("[last-chance] bought frame={frame} bird={bird} state={bought:?}");
    let (x, y): (f64, f64) = runtime
        .lua()
        .load("return physicsToScreenTransform(levelStartPosition.x,levelStartPosition.y)")
        .set_environment(environment.clone())
        .eval()
        .unwrap();
    pointer(runtime, *frame, x, y, true);
    tick(runtime, frame, 1, "extra bird sling press");
    pointer(runtime, *frame, x + 120.0, y + 20.0, true);
    tick(runtime, frame, 60, "extra bird sling pull");
    pointer(runtime, *frame, x + 120.0, y + 20.0, false);
    tick(runtime, frame, 1, "extra bird release");
    let (bx, by): (f64, f64) = runtime
        .lua()
        .load("local b=objects.world[...];return b.x,b.y")
        .set_environment(environment.clone())
        .call(bird.as_str())
        .unwrap();
    tick(runtime, frame, 29, "extra bird free flight");
    let (shots, ax, ay): (u32, f64, f64) = runtime
        .lua()
        .load("local b=objects.world[...];return birdsShot,b.x,b.y")
        .set_environment(environment.clone())
        .call(bird.as_str())
        .unwrap();
    assert_eq!(shots, 4);
    assert!(
        (ax - bx).hypot(ay - by) > 0.1,
        "paid bird did not advance in physics"
    );
    eprintln!(
        "[last-chance] flight frame={frame} distance={}",
        (ax - bx).hypot(ay - by)
    );
    let deadline = *frame + 3600;
    loop {
        let popup:bool=runtime.lua().load("local h=getGameHud();local p=h and h:getChild('lastChancePopup');return p ~= nil and p.visible")
            .set_environment(environment.clone()).eval().unwrap();
        if popup {
            break;
        }
        assert!(
            *frame < deadline,
            "fourth missed bird did not reach last chance"
        );
        tick(runtime, frame, 1, "extra bird depletion");
    }
    tick(runtime, frame, 180, "second last chance entrance");
    click(
        runtime,
        frame,
        "getGameHud():getChild('lastChancePopup'):getChild('btnBuy')",
    );
    tick(runtime, frame, 180, "insufficient coins entrance");
    let shortfall: (u32, u32, u32, u32, u32, u32) = runtime
        .lua()
        .load(
            r#"
        local p=notificationsFrame:getChild('NotEnoughCoinsPopup')
        if not p or not p.visible then error('insufficient coins popup absent') end
        return p.neededCoins,Coins:getAmount(),settings.iap.used.coins,
            currentLevelStats.lastChanceBirdsUsed,Telepods.extraBirdPurchases or 0,
            getRemainingBirdCount()
    "#,
        )
        .set_environment(environment.clone())
        .eval()
        .unwrap();
    assert_eq!(shortfall, (54, 6, 60, 1, 1, 0));
    assert!(!runtime.render.lock().unwrap().physics_enabled);
    eprintln!("[last-chance] insufficient frame={frame} state={shortfall:?}");
    click(
        runtime,
        frame,
        "notificationsFrame:getChild('NotEnoughCoinsPopup'):getChild('btnClose')",
    );
    tick(runtime, frame, 120, "insufficient coins close");
    assert!(
        !runtime.render.lock().unwrap().physics_enabled,
        "closing the nested popup resumed physics under the last-chance popup"
    );
    click(
        runtime,
        frame,
        "getGameHud():getChild('lastChancePopup'):getChild('giveUpButton')",
    );
    tick(runtime, frame, 720, "paid bird failure screen");
    let failed: (u32,u32,u32,u32,bool) = runtime.lua().load(r#"
        return Coins:getAmount(),SettingsWrapper:getTimesLevelFailed('Chapter01_L06'),
            SettingsWrapper:getTimesLevelCompleted('Chapter01_L06'),currentLevelStats.lastChanceBirdsUsed,
            highscores.Chapter01_L06 == nil
    "#).set_environment(environment.clone()).eval().unwrap();
    assert_eq!(failed, (6, 2, 0, 1, true));
    click(
        runtime,
        frame,
        "menuManager:getRoot():getChild('restartButton')",
    );
    tick(runtime, frame, 540, "paid bird failure retry");
    let restarted: (u32,u32,u32,bool,bool,bool) = runtime.lua().load(r#"
        local n=0 for _,b in pairs(birds) do if not b.shot and not b:isLocked() then n=n+1 end end
        return Coins:getAmount(),n,birdsShot,not not birdReady,not not g_levelFailed,not not g_levelCompleted
    "#).set_environment(environment).eval().unwrap();
    assert_eq!(restarted, (6, 3, 0, true, false, false));
    assert_scene(runtime, "GameScene", Some("Chapter01_L06"));
    assert!(runtime.fallback_calls.lock().unwrap().is_empty());
    assert!(runtime.compatibility_bindings.lock().unwrap().is_empty());
    eprintln!("[last-chance] complete frame={frame} state={restarted:?}");
}
