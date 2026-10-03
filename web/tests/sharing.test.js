import test from "node:test";
import assert from "node:assert/strict";
import { inflateSync } from "node:zlib";
import { BrowserScreenshotUI, encodeScreenshotPNG } from "../sharing.js";
import { GameLifecycle } from "../lifecycle.js";
import { LOCALES, MESSAGES } from "../i18n.js";

function decodePNG(bytes) {
  assert.deepEqual([...bytes.subarray(0, 8)], [137, 80, 78, 71, 13, 10, 26, 10]);
  const view = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength), idat = [];
  let width, height;
  for (let offset = 8; offset < bytes.length;) {
    const length = view.getUint32(offset), type = String.fromCharCode(...bytes.subarray(offset + 4, offset + 8));
    const data = bytes.subarray(offset + 8, offset + 8 + length);
    if (type === "IHDR") { width = view.getUint32(offset + 8); height = view.getUint32(offset + 12); assert.deepEqual([...data.subarray(8)], [8, 6, 0, 0, 0]); }
    if (type === "IDAT") idat.push(data);
    offset += length + 12;
  }
  const rows = inflateSync(Buffer.concat(idat)), rgba = new Uint8Array(width * height * 4);
  for (let row = 0; row < height; row++) {
    const start = row * (width * 4 + 1); assert.equal(rows[start], 0);
    rgba.set(rows.subarray(start + 1, start + 1 + width * 4), row * width * 4);
  }
  return { width, height, rgba };
}

test("PNG shares preserve top-down rows, fractional alpha and RGB under zero alpha", () => {
  const screenshot = { width: 2, height: 2, rgba: new Uint8Array([255, 0, 0, 64, 0, 255, 0, 0, 13, 31, 127, 1, 17, 33, 65, 255]) };
  assert.deepEqual(decodePNG(encodeScreenshotPNG(screenshot)), screenshot);
  assert.throws(() => encodeScreenshotPNG({ ...screenshot, rgba: new Uint8Array(15) }), /extent/);
});

function host(navigator = {}) {
  const document = { activeElement: null }, transitions = [], calls = [], revoked = [], files = new Map();
  const lifecycle = new GameLifecycle({ activate: value => transitions.push(value), resetInput() {}, resetClock() {} });
  class Element extends EventTarget {
    constructor(tag) { super(); this.tag = tag; this.children = []; this.attributes = {}; this._hidden = false; }
    append(...children) { this.children.push(...children); }
    replaceChildren(...children) { this.children = children; }
    contains(element) { return element === this || this.children.some(child => child.contains(element)); }
    setAttribute(key, value) { this.attributes[key] = value; }
    removeAttribute(key) { delete this.attributes[key]; if (key === "src" || key === "href") delete this[key]; }
    focus() { if (this.disabled) return; document.activeElement = this; lifecycle.update({ focused: true }); }
    get hidden() { return this._hidden; }
    set hidden(value) { this._hidden = value; if (value && this.contains(document.activeElement)) { lifecycle.update({ focused: false }); document.activeElement = null; } }
  }
  document.createElement = tag => new Element(tag);
  const root = new Element("div"), canvas = new Element("canvas"), accountDialog = new Element("div"), ratingDialog = new Element("div");
  root.hidden = true; canvas.focus();
  const game = { module: {}, lifecycle, resetInput() { calls.push(["resetInput"]); }, account: { cancelPointer() {}, focus() { accountDialog.focus(); } }, rating: { cancelPointer() {}, focus() { ratingDialog.focus(); } } };
  const ui = new BrowserScreenshotUI(game, { root, gameCanvas: canvas, accountDialog, ratingDialog, document, navigator,
    engineCall: (_, name, ...args) => calls.push([name, ...args]),
    url: { createObjectURL(file) { const name = `blob:owned-${files.size}`; files.set(name, file); return name; }, revokeObjectURL(url) { revoked.push(url); } } });
  const screenshot = (sequence, color) => ({ width: 1, height: 1, rgba: new Uint8Array(color), request: { sequence, filename: `Stella_Screenshot${sequence}.png`, title: "native <title> 🐦" } });
  return { ui, game, root, canvas, accountDialog, ratingDialog, document, transitions, calls, revoked, files, screenshot };
}

test("preview retains independent native PNGs until each Done, including signed filenames", async () => {
  const h = host();
  const first = h.screenshot(-2147483648, [255, 0, 0, 64]), second = h.screenshot(0, [0, 255, 0, 0]);
  h.ui.enqueue([first, second]); first.rgba.fill(0);
  assert.equal(h.game.sharingVisible, true); assert.equal(h.root.hidden, false); assert.equal(h.ui.shareButton.hidden, true);
  assert.equal(h.ui.heading.textContent, "native <title> 🐦");
  assert.equal(h.ui.download.download, "Stella_Screenshot-2147483648.png");
  const file = h.files.get(h.ui.download.href);
  assert.equal(file.name, h.ui.download.download); assert.equal(file.type, "image/png");
  assert.deepEqual([...decodePNG(new Uint8Array(await file.arrayBuffer())).rgba], [255, 0, 0, 64]);
  assert.equal(h.accountDialog.inert, true); assert.equal(h.ratingDialog.inert, true);
  h.ui.close();
  assert.deepEqual(h.revoked, ["blob:owned-0"]); assert.equal(h.ui.download.download, "Stella_Screenshot0.png");
  h.ui.close();
  assert.deepEqual(h.revoked, ["blob:owned-0", "blob:owned-1"]);
  assert.equal(h.game.sharingVisible, false); assert.equal(h.root.hidden, true); assert.equal(h.document.activeElement, h.canvas);
  assert.deepEqual(h.transitions, []); assert.deepEqual(h.calls.filter(c => c[0] === "_stella_share_preview"), [["_stella_share_preview", 1], ["_stella_share_preview", 1], ["_stella_share_preview", 0]]);
  h.ui.dispose(); assert.equal(h.revoked.length, 2);
});

test("a trusted share forwards the PNG immediately and cancellation keeps the preview", async () => {
  let reject, forwarded;
  const h = host({ canShare: data => { assert.equal(Object.keys(data).join(), "files"); return true; }, share: data => { forwarded = data; return new Promise((_, no) => { reject = no; }); } });
  h.ui.enqueue([h.screenshot(12, [17, 33, 65, 255])]);
  const pending = h.ui.share();
  assert.equal(forwarded.title, "native <title> 🐦"); assert.equal(forwarded.files[0].name, "Stella_Screenshot12.png");
  assert.equal(h.ui.current.pending, true); h.ui.close(); assert.equal(h.root.hidden, false); assert.equal(h.revoked.length, 0);
  reject(new DOMException("Canceled", "AbortError")); await pending;
  assert.equal(h.ui.current.pending, false); assert.equal(h.ui.error.hidden, true); assert.equal(h.revoked.length, 0);
  h.ui.dispose(); assert.deepEqual(h.revoked, ["blob:owned-0"]);
});

test("share settlement after shutdown cannot restore disposed UI or release a new preview", async () => {
  let resolve;
  const h = host({ canShare: () => true, share: () => new Promise(done => { resolve = done; }) });
  h.ui.enqueue([h.screenshot(1, [0, 0, 0, 255])]);
  const pending = h.ui.share(); h.ui.dispose(); resolve(); await pending;
  assert.equal(h.ui.current, null); assert.equal(h.root.hidden, true); assert.equal(h.game.sharingVisible, false);
  assert.deepEqual(h.revoked, ["blob:owned-0"]);
  h.ui.enqueue([h.screenshot(2, [255, 255, 255, 255])]); // Disposed hosts cannot retain new resources.
  assert.equal(h.files.size, 1);
});

test("host share rejection keeps the image and download usable without reporting a Lua result", async () => {
  const h = host({ canShare: () => true, share: () => Promise.reject(new DOMException("Host unavailable", "NotAllowedError")) });
  h.ui.enqueue([h.screenshot(1, [0, 0, 0, 255])]); await h.ui.share();
  assert.equal(h.ui.error.hidden, false); assert.equal(h.ui.closeButton.disabled, false); assert.equal(h.ui.download.attributes["aria-disabled"], "false");
  assert.equal(h.revoked.length, 0); assert.equal(h.calls.some(call => /result|answer|success/.test(call[0])), false);
  h.game.accountVisible = true; h.game.ratingVisible = true; h.ui.close();
  assert.equal(h.document.activeElement, h.ratingDialog); assert.equal(h.accountDialog.inert, true); assert.deepEqual(h.transitions, []);
  h.ui.dispose();
});

test("all original launcher locales provide screenshot actions and retry text", () => {
  for (const locale of LOCALES) for (const key of ["screenshotPreview", "shareImage", "saveImage", "closePreview", "shareImageFailed"]) assert.equal(typeof MESSAGES[locale.id][key], "string", `${locale.id}/${key}`);
});

test("preview traps keyboard focus in both directions and Escape releases its PNG", () => {
  const h = host({ canShare: () => true, share: () => Promise.resolve() });
  h.ui.enqueue([h.screenshot(1, [0, 0, 0, 255])]); h.root.focus();
  function key(name, shiftKey = false) {
    const event = new Event("keydown", { cancelable: true }); Object.assign(event, { key: name, shiftKey }); h.root.dispatchEvent(event); assert.equal(event.defaultPrevented, true);
  }
  key("Tab", true); assert.equal(h.document.activeElement, h.ui.closeButton);
  key("Tab"); assert.equal(h.document.activeElement, h.ui.shareButton);
  key("Tab"); assert.equal(h.document.activeElement, h.ui.download);
  key("Escape"); assert.equal(h.root.hidden, true); assert.equal(h.document.activeElement, h.canvas);
  assert.deepEqual(h.revoked, ["blob:owned-0"]); assert.deepEqual(h.transitions, []); h.ui.dispose();
});
