import test from "node:test";
import assert from "node:assert/strict";
import { BrowserGamerServicesUI } from "../gamer-services.js";
import { BrowserScreenshotUI } from "../sharing.js";
import { BrowserAccountUI } from "../account.js";
import { BrowserRatingUI } from "../rating.js";
import { dispatchPlatformActions } from "../platform-actions.js";
import { GameLifecycle } from "../lifecycle.js";
import { LOCALES, MESSAGES } from "../i18n.js";

function host() {
  const document = { activeElement: null }, calls = [], transitions = [];
  const lifecycle = new GameLifecycle({ activate: value => transitions.push(value), resetInput() {}, resetClock() {} });
  class Element extends EventTarget {
    constructor(tag) { super(); this.tag = tag; this.children = []; this.attributes = {}; this._hidden = false; }
    append(...children) { this.children.push(...children); }
    replaceChildren(...children) { this.children = children; }
    contains(element) { return element === this || this.children.some(child => child.contains(element)); }
    setAttribute(key, value) { this.attributes[key] = value; }
    removeAttribute(key) { delete this.attributes[key]; }
    focus() { if (this.inert) return; document.activeElement = this; lifecycle.update({ focused: true }); }
    get hidden() { return this._hidden; }
    set hidden(value) { this._hidden = value; if (value && this.contains(document.activeElement)) { lifecycle.update({ focused: false }); document.activeElement = null; } }
  }
  document.createElement = tag => new Element(tag);
  const root = new Element("div"), canvas = new Element("canvas"), account = new Element("div"), rating = new Element("div");
  root.hidden = true; canvas.focus();
  const engineCall = (_, name, ...args) => calls.push([name, ...args]);
  const game = { module: {}, lifecycle, resetInput() { calls.push(["resetInput"]); }, account: { cancelPointer() {}, focus() { account.focus(); } }, rating: { cancelPointer() {}, focus() { rating.focus(); } } };
  const ui = game.gamerServices = new BrowserGamerServicesUI(game, { root, gameCanvas: canvas, accountDialog: account, ratingDialog: rating, engineCall, document });
  const action = (view, entries = [], localProvider = true) => ({ kind: "gamerServices", view, entries, localProvider });
  return { ui, game, document, root, canvas, account, rating, calls, transitions, engineCall, action };
}

test("native gamer presentations drain once and retain independent provider strings in call order", () => {
  const h = host(), first = h.action("leaderboards", [["<LEVEL&1> 🐦", "902.5"]]), second = h.action("achievements", [["ACH_VIEW", "Unlocked"]]);
  let packet = [first, second], transfer;
  h.game.module = { _stella_platform_packet() { transfer = JSON.stringify(packet); packet = []; return 1; }, UTF8ToString: () => transfer };
  const window = { dispatchEvent() { assert.fail("The installed host must display its presentation"); } };
  assert.equal(dispatchPlatformActions(h.game, { window }).length, 2);
  assert.deepEqual(dispatchPlatformActions(h.game, { window }), []);
  first.entries[0][1] = "wrong";
  assert.equal(h.root.hidden, false); assert.equal(h.ui.records.children[0].textContent, "<LEVEL&1> 🐦");
  assert.equal(h.ui.records.children[1].textContent, "902.5"); assert.equal(h.ui.queue.length, 1);
  h.ui.close(); assert.equal(h.ui.records.children[0].textContent, "ACH_VIEW");
  assert.equal(h.game.gamerServicesVisible, true); h.ui.close();
  assert.equal(h.game.gamerServicesVisible, false); assert.equal(h.document.activeElement, h.canvas);
  assert.deepEqual(h.calls.filter(call => call[0] === "_stella_gamer_services_preview"), [["_stella_gamer_services_preview", 1], ["_stella_gamer_services_preview", 1], ["_stella_gamer_services_preview", 0]]);
  assert.deepEqual(h.transitions, []); h.ui.dispose();
});

test("signed-out presentation describes an unavailable provider without inventing empty authenticated records", () => {
  const h = host(); h.ui.enqueue(h.action("achievements", [["not a local record", "999"]], false));
  assert.equal(h.ui.scope.hidden, true); assert.equal(h.ui.records.hidden, true);
  assert.equal(h.ui.records.children.length, 0); assert.match(h.ui.empty.textContent, /unavailable/);
  h.ui.close(); assert.equal(h.calls.some(call => /auth|login|post|success|answer/.test(call[0])), false);
  h.ui.enqueue(h.action("leaderboards")); assert.equal(h.ui.records.hidden, true); assert.match(h.ui.empty.textContent, /No scores/);
  h.ui.dispose(); assert.equal(h.ui.current, null); assert.equal(h.ui.queue.length, 0);
  h.ui.enqueue(h.action("leaderboards")); assert.equal(h.root.hidden, true);
});

test("gamer controller owns covered input and restores rating/account focus without pausing", () => {
  const h = host(); h.game.accountVisible = true; h.game.ratingVisible = true;
  h.ui.enqueue(h.action("leaderboards")); assert.equal(h.account.inert, true); assert.equal(h.rating.inert, true);
  for (const prototype of [BrowserAccountUI.prototype, BrowserRatingUI.prototype]) {
    prototype.run.call({ game: h.game, token: 1 }, () => assert.fail("Covered native controls must not run"));
  }
  h.ui.close(); assert.equal(h.document.activeElement, h.rating); assert.equal(h.account.inert, true); assert.equal(h.rating.inert, false);
  h.game.ratingVisible = false; h.ui.enqueue(h.action("achievements")); h.ui.close();
  assert.equal(h.document.activeElement, h.account); assert.deepEqual(h.transitions, []); h.ui.dispose();
});

test("screenshot preview covers and restores the retained gamer controller independently", () => {
  const h = host(), preview = h.document.createElement("div"); preview.hidden = true;
  const sharing = h.game.sharing = new BrowserScreenshotUI(h.game, { root: preview, gameCanvas: h.canvas, accountDialog: h.account, ratingDialog: h.rating, document: h.document, engineCall: h.engineCall, navigator: {}, url: { createObjectURL: () => "blob:isolated", revokeObjectURL() {} } });
  h.ui.enqueue(h.action("leaderboards", [["real local score", "30.75"]]));
  sharing.enqueue([{ width: 1, height: 1, rgba: new Uint8Array([11, 13, 19, 255]), request: { filename: "Stella_Screenshot1.png", title: "native screenshot" } }]);
  assert.equal(h.root.inert, true); h.ui.close(); assert.equal(h.ui.current.entries[0][1], "30.75");
  h.ui.focus(); assert.equal(h.document.activeElement, sharing.closeButton);
  sharing.close(); assert.equal(h.root.inert, false); assert.equal(h.document.activeElement, h.ui.closeButton);
  assert.equal(h.game.gamerServicesVisible, true); assert.equal(h.rating.inert, true);
  h.ui.close(); assert.equal(h.document.activeElement, h.canvas); assert.deepEqual(h.transitions, []);
  sharing.dispose(); h.ui.dispose();
});

test("keyboard focus stays owned and repeated Escape cannot discard the next presentation", () => {
  const h = host(); h.ui.enqueue(h.action("achievements")); h.ui.enqueue(h.action("leaderboards"));
  const key = (name, options = {}) => { const event = new Event("keydown", { cancelable: true }); Object.assign(event, { key: name, ...options }); h.root.dispatchEvent(event); return event; };
  h.root.focus(); assert.equal(key("Tab", { shiftKey: true }).defaultPrevented, true); assert.equal(h.document.activeElement, h.ui.closeButton);
  key("Escape"); assert.equal(h.ui.current.view, "leaderboards");
  key("Escape", { repeat: true }); assert.equal(h.ui.current.view, "leaderboards");
  key("Escape"); assert.equal(h.root.hidden, true); assert.deepEqual(h.transitions, []); h.ui.dispose();
});

test("all original launcher languages cover gamer views and provider/record states", () => {
  for (const locale of LOCALES) for (const key of ["achievements", "leaderboards", "gamerServicesLocal", "gamerServicesUnavailable", "noAchievements", "noScores", "achievementUnlocked"]) assert.equal(typeof MESSAGES[locale.id][key], "string", `${locale.id}/${key}`);
});
