// Production WASM with original startup retained. Probe callbacks live only
// in MEMFS; no external launch, credential/provider or player save is used.
import { clearVirtualSave } from "../storage.js";
import { withUtf8 } from "../account.js";
import { callEngine, readPacket } from "./frame-capture.mjs";

function require(value, message) { if (!value) throw new Error(message); }
export const REVIEW_URL = "itms-apps://itunes.apple.com/WebObjects/MZStore.woa/wa/viewContentsUserReviews?onlyLatestVersion=true&pageNumber=0&sortOrdering=1&type=Purple+Software&id=875251011&mt=8&at=10lcoX";

export function installRatingFixture(module, { account = false, fresh = true, locale = 0 } = {}) {
  const original = "/runtime/data/scripts/browser-rating-original.lua";
  if (!module.FS.analyzePath(original).exists) module.FS.writeFile(original, module.FS.readFile("/runtime/data/scripts/game.lua"));
  module._stella_shutdown(); if (fresh) clearVirtualSave(module.FS);
  module.FS.writeFile("/runtime/data/scripts/game.lua", `
    loadLuaFile("scripts/browser-rating-original.lua")
    local startup = createStartUpAssets
    function createStartUpAssets()
      startup()
      function update()
        -- This fixture replaces the normal update wrapper which populates
        -- g_key* queries. Read the live native publication instead.
        if gamelua.keyHold.LBUTTON or gamelua.keyPressed.KEY_BACK or gamelua.keyPressed.KEY_MENU then
          error("Rating modal let game input through")
        end
      end
      function draw()
        drawRect(0, 1, 0, 1, 0, 0, screenWidth, screenHeight, true)
        res.captureSprite("BROWSER_RATING_CAPTURE")
        clearScreen()
        res.drawSprite("BROWSER_RATING_CAPTURE", 0, 0)
      end
      ${account ? "_G.SkynestAccount.native_login(true, false, false)" : ""}
      for i=1,6 do Apprater.showAlert(true, "browser rating probe") end
    end
  `);
  callEngine(module, "_stella_set_locale", locale);
  callEngine(module, "_stella_init", 1024, 768);
}

export function runRatingSuite(module, { ratingState = null, onGameFrame = null } = {}) {
  const cases = []; let ui;
  const refresh = () => {
    callEngine(module, "_stella_rating_frame");
    return ui = JSON.parse(module.UTF8ToString(module._stella_rating_packet()));
  };
  const gameFrame = label => {
    callEngine(module, "_stella_frame", 1 / 60);
    const packet = readPacket(module); if (label) onGameFrame?.(packet, label);
    refresh(); return packet;
  };
  const boot = options => { installRatingFixture(module, options); gameFrame("initial modal game capture"); return ui; };
  const actions = () => JSON.parse(module.UTF8ToString(module._stella_platform_packet()));
  const choose = (code, token = ui.token) => { callEngine(module, "_stella_rating_choose", token, code); return refresh(); };
  const center = code => { const rect = ui.buttons.find(b => b.choice === code).rect; return [rect[0] + rect[2] / 2, rect[1] + rect[3] / 2]; };
  const pointer = (phase, code, token = ui.token) => { const [x, y] = center(code); callEngine(module, "_stella_rating_pointer", token, phase, x, y); return refresh(); };
  const accountPacket = () => {
    callEngine(module, "_stella_account_frame", 0);
    return JSON.parse(module.UTF8ToString(module._stella_account_packet()));
  };
  function test(name, run) { run(); cases.push({ name, pass: true }); }
  function state(expected) {
    if (!ratingState) return;
    const actual = ratingState(module);
    for (const [key, value] of Object.entries(expected)) require(actual[key] === value, `native persisted ${key}: ${actual[key]} != ${value}`);
  }

  test("visible alert survives frames without an invented Later answer or store launch", () => {
    boot(); const token = ui.token;
    require(JSON.stringify(ui.buttons.map(b => b.choice)) === "[2,1,0]", "native later/decline/rate order changed");
    require(ui.image?.length === 1024 * 768 * 4, "private rating image missing");
    state({ promptCount: undefined, userPromptedLater: undefined, userHasDeclined: undefined, userHasRated: undefined });
    for (let frame = 0; frame < 40; frame++) gameFrame();
    require(ui?.token === token, "frame processing answered or replaced the retained alert");
    state({ promptCount: undefined, userPromptedLater: undefined, userHasDeclined: undefined, userHasRated: undefined });
    require(actions().length === 0, "unanswered alert launched an external action");
  });

  test("only explicit choices set native flags, launch Rate once and survive cached restart", () => {
    for (const [code, flag] of [[2, "userPromptedLater"], [1, "userHasDeclined"], [0, "userHasRated"]]) {
      boot(); const obsolete = ui.token;
      choose(code); require(ui === null, "answer did not close the retained alert");
      state({ promptCount: 1, [flag]: true, ...(code === 2 ? { tryCount: 0 } : {}) });
      const queued = actions();
      require(JSON.stringify(queued) === JSON.stringify(code === 0 ? [{ kind: "openUrl", url: REVIEW_URL }] : []), "native external launch selection or URL changed");
      require(actions().length === 0, "external action was delivered twice");
      choose(0, obsolete); require(actions().length === 0, "obsolete answer launched a second URL");
      installRatingFixture(module, { fresh: false }); gameFrame();
      require(ui === null, "answer suppression did not survive restart");
      state({ promptCount: 1, [flag]: true });
    }
  });

  test("cancel, wrong button, invalid choice and Escape retain an unanswered alert", () => {
    boot(); const token = ui.token;
    pointer(0, 0); pointer(2, 1); require(ui?.token === token, "release in another button chose an answer");
    pointer(0, 0); pointer(3, 0); pointer(2, 0); require(ui?.token === token, "canceled press chose an answer");
    choose(-1); choose(3); callEngine(module, "_stella_rating_key", token, 4, 0); refresh();
    require(ui?.token === token, "invalid code or Escape invented Later");
    state({ promptCount: undefined, userPromptedLater: undefined });
  });

  test("same-button pointer answer uses the retained owner and captures exclude private alert pixels", () => {
    boot(); const token = ui.token;
    callEngine(module, "_stella_pointer", 100, 100, 1); callEngine(module, "_stella_key", 0, 1);
    callEngine(module, "_stella_key", 1, 1); callEngine(module, "_stella_touch", 0, 11, 100, 100);
    const packet = gameFrame("modal blocks game input and private pixels");
    require(packet.rating !== null && packet.operations.some(op => op.capture), "modal replaced the game capture stream");
    require(!JSON.stringify(packet).includes(ui.message), "rating text leaked into game packets");
    require(JSON.parse(module.UTF8ToString(module._stella_audio_packet())).started, "modal deactivated native audio");
    pointer(0, 2); pointer(2, 2); require(ui === null, "same-button pointer did not answer Later");
    state({ promptCount: 1, userPromptedLater: true });
    gameFrame("game capture after closing private alert"); choose(0, token); require(actions().length === 0, "late pointer owner launched Rate");
  });

  test("resize clears a press and invalidates its token without changing the native owner", () => {
    boot(); const obsolete = ui.token; pointer(0, 0);
    callEngine(module, "_stella_flush"); callEngine(module, "_stella_resize", 800, 600); refresh();
    require(ui.image?.length === 800 * 600 * 4 && ui.token !== obsolete, "resize did not replace the private drawable/token");
    choose(0, obsolete); require(ui, "resize accepted an old control token");
    pointer(2, 0); require(ui, "resize retained an old button press");
    pointer(0, 1); pointer(2, 1); require(ui === null, "resized buttons are not interactive");
    state({ promptCount: 1, userHasDeclined: true });
  });

  test("activation cancels pressed input; inactive and previous-lifetime answers cannot launch", () => {
    boot(); const obsolete = ui.token; pointer(0, 0);
    callEngine(module, "_stella_active", 0); choose(0, obsolete); require(ui, "inactive answer closed the alert");
    require(!JSON.parse(module.UTF8ToString(module._stella_audio_packet())).started, "inactive audio stayed started");
    callEngine(module, "_stella_active", 1); refresh(); require(ui.token !== obsolete, "activation retained the old ingress lifetime");
    choose(0, obsolete); pointer(2, 0); require(ui, "activation resurrected an obsolete answer/press");
    require(actions().length === 0, "inactive/stale input queued a launch");
    pointer(0, 2); pointer(2, 2); state({ promptCount: 1, userPromptedLater: true });
  });

  test("keyboard navigation defaults to Later and wraps to the exact native Rate choice", () => {
    boot(); require(ui.focus === 2, "keyboard default is not Later");
    callEngine(module, "_stella_rating_key", ui.token, 0, 1); refresh(); require(ui.focus === 0, "reverse Tab did not wrap to Rate");
    callEngine(module, "_stella_rating_key", ui.token, 2, 0); refresh(); require(ui.focus === 2, "forward navigation did not wrap to Later");
    callEngine(module, "_stella_rating_focus", ui.token, 1); refresh(); require(ui.focus === 1, "accessible button focus did not reach Decline");
    callEngine(module, "_stella_rating_key", ui.token, 3, 0); refresh(); require(ui === null, "Enter did not answer the focused button");
    state({ promptCount: 1, userHasDeclined: true }); require(actions().length === 0, "Decline launched Rate");
  });

  test("rating intercepts account controls without clearing its owner, editor or raster", () => {
    boot({ account: true }); const account = accountPacket(); require(account.view === "SignIn", "underlying account absent");
    const pixels = module.HEAPU8.slice(account.image.pointer, account.image.pointer + account.image.length);
    withUtf8(module, "registerLabel", pointer => callEngine(module, "_stella_account_control", account.token, pointer));
    withUtf8(module, JSON.stringify({ value: "synthetic@example.invalid", start: 25, end: 25 }), pointer => callEngine(module, "_stella_account_edit", account.token, 0, pointer));
    callEngine(module, "_stella_account_key", account.token, 0, 0);
    const blocked = accountPacket(); require(blocked.view === "SignIn" && blocked.token === account.token, "account input escaped the top modal");
    require(module._stella_account_editor(account.token, 0) === 0, "top modal exposed the lower editor ingress");
    choose(2); const restored = accountPacket(); require(restored.view === "SignIn" && restored.token === account.token, "rating close lost the lower account owner");
    const editor = JSON.parse(module.UTF8ToString(module._stella_account_editor(account.token, 0)));
    require(editor.value === "", "rating allowed editing the lower account");
    require(!restored.image || pixels.every((p, i) => p === module.HEAPU8[restored.image.pointer + i]), "rating close changed the retained account pixels");
    gameFrame("account restored below closed rating");
  });

  test("new cached runtime rejects an old alert token even when native owner IDs restart", () => {
    boot(); const obsolete = ui.token;
    boot(); require(ui.token !== obsolete, "cached restart reused alert ingress");
    choose(0, obsolete); require(ui && actions().length === 0, "old runtime token answered the new alert");
  });

  test("all original locales supply the three native button labels and paint a private raster", () => {
    const labels = [];
    for (let locale = 0; locale < 11; locale++) {
      boot({ locale }); require(ui.image?.length === 1024 * 768 * 4 && ui.buttons.length === 3, "localized alert did not render");
      require(ui.message && ui.buttons.every(b => b.title), "localized text missing");
      labels.push({ locale, message: ui.message, buttons: ui.buttons.map(b => b.title) });
    }
    require(new Set(labels.map(row => row.buttons.join("|"))).size >= 9, "rating locales fell back to one language");
    cases.push({ name: "original localized rating messages/buttons", labels, pass: true });
  });

  module._stella_shutdown();
  return { pass: true, productionEngine: true, memoryOnlyFixture: true, externalLaunches: 0, independentlyDecodedRegistry: !!ratingState, cases };
}
