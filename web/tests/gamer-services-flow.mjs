// Original startup and native service calls run in the production WASM.
// The fixture uses isolated MEMFS only; it neither buys nor contacts GameKit.
import { clearVirtualSave } from "../storage.js";
import { withUtf8 } from "../account.js";
import { installRatingFixture } from "./rating-flow.mjs";
import { callEngine, readPacket } from "./frame-capture.mjs";

const original = "/runtime/data/scripts/browser-gamer-original.lua";
function require(value, message) { if (!value) throw new Error(message); }

export function installGamerFixture(module, fresh = true) {
  if (!module.FS.analyzePath(original).exists) module.FS.writeFile(original, module.FS.readFile("/runtime/data/scripts/game.lua"));
  module._stella_shutdown(); if (fresh) clearVirtualSave(module.FS);
  module.FS.writeFile("/runtime/data/scripts/game.lua", `
    loadLuaFile("scripts/browser-gamer-original.lua")
    local startup = createStartUpAssets
    function createStartUpAssets()
      startup()
      local gs = _G.FusionGamerServices
      gs.postAchievement("ACH_BROWSER_NATIVE")
      gs.postScore("<LEVEL&1> 🐦", 100.25)
      gs.postScore("<LEVEL&1> 🐦", 902.5)
      gs.postScore("<LEVEL&1> 🐦", 30.75)
      gs.showAchievements("ignored")
      gs.showLeaderboards("ignored")
      function update()
        gamerInput = gamelua.keyHold.LBUTTON or gamelua.keyPressed.KEY_BACK or gamelua.keyPressed.KEY_MENU
        setBGColor(gamerInput and 255 or 0, gamerInput and 0 or 255, 0)
      end
      function draw()
        drawRect(gamerInput and 1 or 0, gamerInput and 0 or 1, 0, 1, 0, 0, screenWidth, screenHeight, true)
        res.captureSprite("GAMER_PRIVATE_BOUNDARY")
        clearScreen()
        res.drawSprite("GAMER_PRIVATE_BOUNDARY", 0, 0)
      end
    end
  `);
  callEngine(module, "_stella_set_locale", 0); callEngine(module, "_stella_init", 1024, 768);
}

export function runGamerServicesSuite(module, { onGameFrame = null } = {}) {
  const cases = [], actions = () => JSON.parse(module.UTF8ToString(module._stella_platform_packet()));
  const frame = (label, expectedRGBA) => { callEngine(module, "_stella_frame", 1 / 60); const packet = readPacket(module); onGameFrame?.(packet, label, expectedRGBA); return packet; };
  const check = (name, run) => { run(); cases.push({ name, pass: true }); };
  const expected = [
    { kind: "gamerServices", view: "achievements", entries: [["ACH_BROWSER_NATIVE", "Unlocked"]], localProvider: true },
    { kind: "gamerServices", view: "leaderboards", entries: [["<LEVEL&1> 🐦", "902.5"]], localProvider: true },
  ];
  const matches = actual => actual.length === expected.length && actual.every((action, index) => {
    const wanted = expected[index];
    return action.kind === wanted.kind && action.view === wanted.view && action.localProvider === wanted.localProvider && JSON.stringify(action.entries) === JSON.stringify(wanted.entries);
  });
  const document = () => JSON.parse(module.FS.readFile("/runtime/appdata/stella-gamer-services.json", { encoding: "utf8" }));
  check("native presentation snapshots keep float high scores, Unicode IDs, provider scope and one-shot order", () => {
    installGamerFixture(module); require(matches(actions()), "native gamer presentation metadata missing or changed");
    require(actions().length === 0, "gamer presentations replayed");
    const stored = document(); require(stored.achievements.includes("ACH_BROWSER_NATIVE") && stored.scores["<LEVEL&1> 🐦"] === 902.5, "native local provider persistence changed");
  });
  check("controller blocks game ingress while audio and private capture continue", () => {
    callEngine(module, "_stella_pointer", 10, 20, 1);
    callEngine(module, "_stella_gamer_services_preview", 1);
    callEngine(module, "_stella_pointer", 10, 20, 1); callEngine(module, "_stella_key", 0, 1);
    callEngine(module, "_stella_key", 1, 1); callEngine(module, "_stella_touch", 0, 8, 10, 20);
    const packet = frame("game pixels beneath gamer controller");
    require(JSON.stringify(packet.background) === "[0,255,0]", "gamer controller did not clear/block original Lua input");
    require(packet.operations.some(operation => operation.capture), "private controller stopped game capture");
    require(!JSON.stringify(packet).includes("ACH_BROWSER_NATIVE"), "provider records entered game captures");
    require(JSON.parse(module.UTF8ToString(module._stella_audio_packet())).started, "controller stopped game audio");
    const before = JSON.stringify(document()); callEngine(module, "_stella_gamer_services_preview", 0);
    frame("private controller closed"); require(actions().length === 0 && JSON.stringify(document()) === before, "closing invented a post/authentication or mutated records");
    callEngine(module, "_stella_pointer", 10, 20, 1);
    require(JSON.stringify(frame("game input restored after controller", [255, 0, 0, 255]).background) === "[255,0,0]", "closing retained the native game input block");
    callEngine(module, "_stella_pointer", 10, 20, 0); frame("released game input after controller");
  });
  check("native high-score snapshots and records survive cached runtime restart", () => {
    installGamerFixture(module, false); require(matches(actions()), "cached restart lost native provider records");
  });
  const restoreEntry = () => module.FS.writeFile("/runtime/data/scripts/game.lua", module.FS.readFile(original));
  const rating = () => { callEngine(module, "_stella_rating_frame"); return JSON.parse(module.UTF8ToString(module._stella_rating_packet())); };
  const account = () => { callEngine(module, "_stella_account_frame", 0); return JSON.parse(module.UTF8ToString(module._stella_account_packet())); };
  for (const preview of ["_stella_gamer_services_preview", "_stella_share_preview"]) check(`${preview} rejects covered/stale rating and account ingress with independent preview ownership`, () => {
    restoreEntry(); installRatingFixture(module, { account: true }); frame();
    const initial = rating(), accountInitial = account();
    callEngine(module, preview, 1); const covered = rating();
    require(covered.token !== initial.token, "opening preview retained pressed native rating token");
    callEngine(module, "_stella_rating_choose", covered.token, 0); callEngine(module, "_stella_rating_choose", initial.token, 0);
    require(rating()?.token === covered.token && actions().length === 0, "covered/stale rating triggered Rate");
    withUtf8(module, "registerLabel", ptr => callEngine(module, "_stella_account_control", accountInitial.token, ptr));
    require(account()?.view === "SignIn", "covered account control changed its owner");
    callEngine(module, "_stella_pointer", 10, 20, 1); callEngine(module, "_stella_key", 0, 1); frame();
    const other = preview === "_stella_share_preview" ? "_stella_gamer_services_preview" : "_stella_share_preview";
    callEngine(module, other, 1); callEngine(module, preview, 0);
    callEngine(module, "_stella_rating_choose", rating().token, 0); require(rating() && actions().length === 0, "closing one preview unblocked the other");
    callEngine(module, other, 0); const current = rating();
    callEngine(module, "_stella_rating_choose", current.token, 2); require(rating() === null && actions().length === 0, "uncovered Later did not preserve native answer semantics");
    const accountVisible = account(); withUtf8(module, "registerLabel", ptr => callEngine(module, "_stella_account_control", accountVisible.token, ptr));
    require(account()?.view === "Register1", "uncovered account did not regain interaction");
  });
  restoreEntry(); module._stella_shutdown();
  return { pass: true, productionEngine: true, memoryOnlyFixture: true, externalProviders: 0, cases };
}
