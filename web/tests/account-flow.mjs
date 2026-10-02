// Linked production WASM and original startup, with test callbacks installed
// only in MEMFS. No provider, real credential, external URL or player save.
import { clearVirtualSave, snapshotSave } from "../storage.js";
import { withUtf8 } from "../account.js";
import { callEngine, readPacket } from "./frame-capture.mjs";
function require(value, message) { if (!value) throw new Error(message); }

export function installAccountFixture(module, register = false) {
  const original = "/runtime/data/scripts/browser-account-original.lua";
  if (!module.FS.analyzePath(original).exists) module.FS.writeFile(original, module.FS.readFile("/runtime/data/scripts/game.lua"));
  module._stella_shutdown(); clearVirtualSave(module.FS);
  module.FS.writeFile("/runtime/data/scripts/game.lua", `
    loadLuaFile("scripts/browser-account-original.lua")
    local startup = createStartUpAssets
    function createStartUpAssets()
      startup()
      local account_failures = 0
      _G.SkynestAccount.onLoginSuccess = function() error("Interactive UI invented login success") end
      _G.SkynestAccount.onLoginFailure = function(code, message)
        if code ~= "ERROR_USER_CANCELLED_LOGIN" then error("Wrong native cancellation code") end
        if message ~= "User cancelled login" then error("Wrong native cancellation message") end
        if _G.SkynestAccount.native_isLoginInProgress() then error("Cancellation retained login-in-progress") end
        account_failures = account_failures + 1
        if account_failures ~= 1 then error("Duplicate native cancellation callback") end
      end
      function update()
        if account_failures == 1 then setBGColor(17, 23, 45) else setBGColor(0, 0, 0) end
      end
      function draw()
        local target = "BROWSER_ACCOUNT_GAME_" .. screenWidth .. "x" .. screenHeight
        drawRect(0, 1, 0, 1, 0, 0, screenWidth, screenHeight, true)
        res.captureSprite(target)
        clearScreen()
        res.drawSprite(target, 0, 0)
      end
      _G.SkynestAccount.native_login(true, true, ${register})
    end
  `);
  callEngine(module, "_stella_set_locale", 0);
  callEngine(module, "_stella_init", 1024, 768);
}

export function runAccountSuite(module, onGameFrame = null) {
  const results = []; let now = 0, ui;
  const refresh = () => { callEngine(module, "_stella_account_frame", now); return ui = JSON.parse(module.UTF8ToString(module._stella_account_packet())); };
  const gameFrame = () => { callEngine(module, "_stella_frame", 1 / 60); const packet = readPacket(module); onGameFrame?.(packet); refresh(); return packet; };
  const boot = register => { now = 0; installAccountFixture(module, register); gameFrame(); return ui; };
  const control = (name, token = ui.token) => { withUtf8(module, name, pointer => callEngine(module, "_stella_account_control", token, pointer)); return refresh(); };
  const key = code => { callEngine(module, "_stella_account_key", ui.token, code, 0); return refresh(); };
  const edit = (field, value, start = value.length, end = start, backward = false) => {
    withUtf8(module, JSON.stringify({ value, start, end, backward }), pointer => callEngine(module, "_stella_account_edit", ui.token, field, pointer)); refresh();
  };
  const editor = field => JSON.parse(module.UTF8ToString(module._stella_account_editor(ui.token, field)));
  const has = name => ui?.controls.some(control => control.name === name);
  const view = expected => require(ui?.view === expected, `view ${ui?.view} != ${expected}`);
  function test(name, run) { run(); results.push({ name, pass: true }); }

  test("original artwork and complete control geometry above a running captured game", () => {
    boot(false); view("SignIn"); require(ui.image?.length === 1024 * 768 * 4, "missing native raster");
    require(ui.controls.length === 7 && has("registerLabel") && has("forgotPasswordLabel"), "missing native controls");
    const email = ui.controls.find(control => control.name === "emailTextField");
    require(JSON.stringify(email.rect) === "[321,276,376,38]", "original email geometry changed");
    require(JSON.parse(module.UTF8ToString(module._stella_audio_packet())).started, "account presentation paused native audio");
    const packet = gameFrame(); require(packet.operations.some(operation => operation.capture), "game capture unavailable while account visible");
  });
  test("required fields, native Return delegation and Unicode browser selection", () => {
    boot(false); control("signInButton"); require(has("emailErrorButton"), "required email bubble absent");
    edit(0, "A👩‍🚀e\u0301中", 2, 6); require(editor(0).start === 1 && editor(0).end === 6, "UTF-16 selection split a grapheme");
    key(1); require(ui.focus === 1 && ui.view === "SignIn", "email Return submitted rather than focusing password");
    edit(1, "abcdefgh"); key(1); require(ui.focus === null && ui.view === "SignIn", "password Return submitted rather than resigning");
    edit(0, ""); key(1); require(ui.focus === 0, "empty native Return was enabled");
  });
  test("password raster contains only masked text and credentials stay out of public/save packets", () => {
    boot(false); edit(1, "abcd"); require(ui.image, "password did not repaint");
    const first = module.HEAPU8.slice(ui.image.pointer, ui.image.pointer + ui.image.length);
    edit(1, "秘密密码"); require(ui.image, "replacement password did not repaint");
    const second = module.HEAPU8.subarray(ui.image.pointer, ui.image.pointer + ui.image.length);
    require(first.every((value, i) => value === second[i]), "same bullet count rendered different password pixels");
    const marker = "synthetic-browser-secret-20261002"; edit(1, marker);
    require(!module.UTF8ToString(module._stella_account_packet()).includes(marker), "password leaked to public UI packet");
    require(!module.UTF8ToString(module._stella_packet()).includes(marker), "password leaked to game packet");
    const save = snapshotSave(module.FS, null);
    require(save.files.every(file => !atob(file.data).includes(marker)), "password leaked to AppData");
  });
  test("help pages retain field contents, reject obsolete controls and return to SignIn", () => {
    boot(false); edit(0, "synthetic@example.invalid"); const obsolete = ui.token;
    control("questionButton"); view("Help1"); require(!has("emailTextField"), "old form remained interactive");
    control("registerLabel", obsolete); view("Help1");
    control("nextButton"); view("Help2"); control("nextButton"); view("Help3"); control("nextButton"); view("SignIn");
    require(editor(0).value === "synthetic@example.invalid", "native retained address was lost through help");
  });
  test("local validation uses elapsed host time and defers delivery while inactive", () => {
    boot(false); edit(1, "short"); now = 1.99; refresh(); require(!has("passwordErrorButton"), "password checked before two seconds");
    callEngine(module, "_stella_active", 0); now = 4; refresh(); require(!has("passwordErrorButton"), "inactive account delivered timer");
    callEngine(module, "_stella_active", 1); refresh(); require(has("passwordErrorButton"), "resume discarded elapsed deadline");
  });
  test("birthday selectors dismiss before Continue; Register2 validates then reports missing provider", () => {
    boot(true); view("Register1"); control("dayTextField"); require(has("pickerRow0"), "birthday picker absent");
    key(5); control("continueButton"); view("Register1"); require(!has("pickerRow0"), "Continue did not dismiss picker");
    for (const name of ["dayTextField", "monthTextField", "yearTextField"]) { control(name); key(5); key(1); }
    require(ui.controls.find(control => control.name === "yearTextField").label === "1900", "selected date is not exposed to browser accessibility");
    control("continueButton"); view("Register2"); control("registerButton"); require(has("emailErrorButton"), "registration required email missing");
    edit(0, "synthetic@example.invalid"); edit(1, "1234567"); control("registerButton"); view("Register2"); require(has("passwordErrorButton"), "registration short password accepted");
    edit(1, "synthetic-password"); control("registerButton");
    require(ui.busy, "registration request did not retain its native progress view");
    for (let turn = 0; turn < 4 && ui.busy; turn++) gameFrame();
    view("NoNetworkConnectivity");
  });
  test("reset uses its own blank field and cancellation posts the native failure before cached restart", () => {
    boot(false); edit(0, "synthetic@example.invalid"); control("forgotPasswordLabel"); view("ForgotPassword"); require(editor(0).value === "", "reset borrowed SignIn editor");
    edit(0, "   "); control("sendRequestButton"); view("ForgotPassword"); require(has("emailErrorButton"), "trim-empty reset submitted");
    edit(0, "synthetic@example.invalid"); control("sendRequestButton");
    for (let turn = 0; turn < 4 && ui.busy; turn++) gameFrame();
    view("NoNetworkConnectivity");
    const obsolete = ui.token; key(0); require(ui === null, "cancel did not hide immediately");
    require(JSON.stringify(gameFrame().background) === "[0,0,0]", "cancel callback delivered too early");
    require(JSON.stringify(gameFrame().background) === "[17,23,45]", "cancel failure was not delivered");
    boot(false); require(ui.token !== obsolete, "cached restart reused the old UI token"); control("closeButton", obsolete); view("SignIn");
  });
  test("private overlay and input rectangles resize independently and all original UI languages load", () => {
    boot(false); callEngine(module, "_stella_flush"); onGameFrame?.(readPacket(module));
    callEngine(module, "_stella_resize", 800, 600); refresh(); require(ui.image?.length === 800 * 600 * 4, "private image did not resize");
    require(JSON.stringify(ui.controls.find(control => control.name === "emailTextField").rect) === "[250.78125,215.625,293.75,29.6875]", "input rects did not follow native canvas transform");
    const labels = [];
    for (let locale = 0; locale < 11; locale++) {
      callEngine(module, "_stella_set_locale", locale); refresh(); view("SignIn");
      labels.push(ui.controls.find(control => control.name === "signInButton").label);
      require(ui.image, "language change did not repaint original account artwork");
    }
    require(new Set(labels).size >= 9, "native account locales fell back to one language");
    results.push({ name: "original UI locale labels", labels, pass: true });
  });
  module._stella_shutdown();
  return { pass: true, productionEngine: true, memoryOnlyFixture: true, cases: results };
}
