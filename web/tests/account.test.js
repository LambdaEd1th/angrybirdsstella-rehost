import test from "node:test";
import assert from "node:assert/strict";
import { straightAlpha, withUtf8, BrowserAccountUI } from "../account.js";
import { GameLifecycle } from "../lifecycle.js";

test("private premultiplied native account pixels are converted once for Canvas2D", () => {
  assert.deepEqual([...straightAlpha(new Uint8Array([0,0,0,128, 64,32,16,128, 1,2,3,255, 50,60,70,0]))], [0,0,0,128, 128,64,32,128, 1,2,3,255, 0,0,0,0]);
});

test("Unicode account ingress clears the temporary WASM buffer even after a callback fails", () => {
  const freed = [], module = { HEAPU8: new Uint8Array(128), _malloc: () => 16, _free: pointer => freed.push(pointer) };
  assert.throws(() => withUtf8(module, "synthetic-秘密👩‍🚀", pointer => {
    assert.equal(pointer, 16);
    const end = module.HEAPU8.indexOf(0, pointer);
    assert.equal(new TextDecoder().decode(module.HEAPU8.subarray(pointer, end)), "synthetic-秘密👩‍🚀");
    module.HEAPU8 = new Uint8Array(module.HEAPU8); // Emscripten can replace its memory view.
    throw new Error("synthetic failure");
  }), /synthetic failure/);
  assert.deepEqual(freed, [16]); assert.ok(module.HEAPU8.every(value => value === 0));
});

test("canceling a focused native account does not pause/resume the application or audio", () => {
  const transitions = [], lifecycle = new GameLifecycle({ activate: active => transitions.push(active), resetInput() {}, resetClock() {} });
  const editor = {}, document = { activeElement: editor }, canvas = {};
  const root = {
    contains: element => element === editor,
    set hidden(value) {
      // The browser removes focused hidden descendants before the next line.
      if (value && document.activeElement === editor) { document.activeElement = null; lifecycle.update({ focused: false }); }
    },
    get hidden() { return false; }, dataset: {}, hasPointerCapture: () => false,
  };
  canvas.focus = () => { document.activeElement = canvas; lifecycle.update({ focused: true }); };
  BrowserAccountUI.prototype.hide.call({ root, document, gameCanvas: canvas, game: { accountVisible: true }, primary: null, token: 1, clearInputs() {}, controls: { replaceChildren() {} }, canvas: {} });
  assert.equal(document.activeElement, canvas);
  assert.equal(lifecycle.active, true);
  assert.deepEqual(transitions, []);
});
