import test from "node:test";
import assert from "node:assert/strict";
import { BrowserRatingUI } from "../rating.js";
import { dispatchPlatformActions } from "../platform-actions.js";
import { GameLifecycle } from "../lifecycle.js";

test("the first rating press restores owned focus and uses the token created by native activation", () => {
  const handlers = new Map(), ingress = [], captured = [];
  const root = { replaceChildren() {}, addEventListener(type, handler) { handlers.set(type, handler); }, setPointerCapture(id) { captured.push(id); }, getBoundingClientRect: () => ({ left: 10, top: 20, width: 100, height: 100 }) };
  const document = { createElement: () => ({ getContext: () => ({}), setAttribute() {} }) };
  const game = { failed: false, lifecycle: { active: false }, module: {} };
  const ui = new BrowserRatingUI(game, { root, document, engineCall: (...args) => ingress.push(args.slice(1)), dispatchActions() {}, failGame() { assert.fail("Restoring focus should accept the press"); } });
  ui.token = 12; ui.packet = { resolution: [200, 200] };
  let nativeToken = 12, refreshes = 0;
  ui.focus = () => { game.lifecycle.active = true; nativeToken = 13; };
  ui.refresh = () => { refreshes++; ui.token = nativeToken; };
  handlers.get("pointerdown")({ button: 0, pointerId: 5, clientX: 30, clientY: 50, preventDefault() {} });
  assert.deepEqual(ingress, [["_stella_rating_pointer", 13, 0, 40, 60]]);
  assert.deepEqual(captured, [5]); assert.equal(ui.primary, 5); assert.equal(refreshes, 2);
});

test("closing rating restores a retained account before hidden focus removal and preserves native activation", () => {
  const transitions = [], lifecycle = new GameLifecycle({ activate: active => transitions.push(active), resetInput() {}, resetClock() {} });
  const ratingButton = {}, email = {}, document = { activeElement: ratingButton };
  const accountDialog = { inert: true }; let hidden = false, saves = 0;
  const root = {
    contains: element => element === ratingButton,
    get hidden() { return hidden; },
    set hidden(value) { hidden = value; if (value && document.activeElement === ratingButton) lifecycle.update({ focused: false }); },
  };
  const game = { ratingVisible: true, accountVisible: true, account: { focus() { assert.equal(accountDialog.inert, false); document.activeElement = email; lifecycle.update({ focused: true }); } } };
  BrowserRatingUI.prototype.hide.call({ root, document, accountDialog, game, token: 7, cancelPointer() {}, controls: { replaceChildren() {} }, buttons: new Map(), canvas: {}, saveOrReport() { saves++; } });
  assert.equal(document.activeElement, email); assert.equal(game.ratingVisible, false); assert.equal(hidden, true);
  assert.deepEqual(transitions, []); assert.equal(saves, 1);
});

test("unfocused or hidden rating rejects ingress and cannot dispatch an external launch", () => {
  let operations = 0, launches = 0;
  const ui = { game: { failed: false, lifecycle: { active: false } }, token: 12, refresh() {}, dispatchActions() { launches++; }, failGame() { assert.fail("No operation should run"); } };
  BrowserRatingUI.prototype.run.call(ui, () => operations++);
  ui.game.lifecycle.active = true; ui.token = null;
  BrowserRatingUI.prototype.run.call(ui, () => operations++);
  assert.equal(operations, 0); assert.equal(launches, 0);
});

test("external actions keep call order, native review strings and complete embedding payloads", () => {
  const trace = [], url = "itms-apps://itunes.apple.com/native?x=1&y=2";
  let packet = [ { kind: "openUrl", url }, { kind: "appStoreProduct", productId: "875251011", productType: 3 }, { kind: "video", path: "movies/intro.mp4" }, { kind: "gamerServices", view: "leaderboards", entries: [["level 1", "3000"]] } ];
  let transfer;
  const game = { module: { _stella_platform_packet() { transfer = JSON.stringify(packet); packet = []; return 1; }, UTF8ToString: () => transfer } };
  const window = { open: (...args) => { trace.push(args); return null; }, CustomEvent: class { constructor(type, options) { this.type = type; this.detail = options.detail; } }, dispatchEvent: event => trace.push([event.type, event.detail]) };
  dispatchPlatformActions(game, { window });
  assert.deepEqual(trace.slice(0, 2), [[url, "_blank", "noopener,noreferrer"], ["https://apps.apple.com/app/id875251011", "_blank", "noopener,noreferrer"]]);
  assert.deepEqual(trace.slice(2), [["stella-platform-action", { kind: "video", path: "movies/intro.mp4" }], ["stella-platform-action", { kind: "gamerServices", view: "leaderboards", entries: [["level 1", "3000"]] }]]);
  assert.deepEqual(dispatchPlatformActions(game, { window }), []); assert.equal(trace.length, 4);
});

test("advisory platform launch failure does not stop later actions or report a completed rating", () => {
  const launches = []; let transfer;
  const game = { module: { _stella_platform_packet: () => 1, UTF8ToString: () => transfer } };
  transfer = JSON.stringify([{ kind: "openUrl", url: "itms-apps://native-rating" }, { kind: "openUrl", url: "https://example.invalid/help" }]);
  const window = { open(url) { launches.push(url); if (url.startsWith("itms")) throw new Error("Synthetic OS launch failure"); } };
  const warn = console.warn; console.warn = () => {};
  try { assert.equal(dispatchPlatformActions(game, { window }).length, 2); }
  finally { console.warn = warn; }
  assert.deepEqual(launches, ["itms-apps://native-rating", "https://example.invalid/help"]);
});
