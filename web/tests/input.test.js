import test from "node:test";
import assert from "node:assert/strict";
import { pathToFileURL } from "node:url";

const inputModule = process.env.STELLA_INPUT_MODULE
  ? pathToFileURL(process.env.STELLA_INPUT_MODULE) : new URL("../input.js", import.meta.url);
const { installInput } = await import(inputModule);

function host(accountDialog = { open: false }, ratingDialog = null, sharingDialog = null, gamerServicesDialog = null) {
  const document = new EventTarget(), window = new EventTarget(), canvas = new EventTarget();
  const calls = [], failures = [], captures = new Set();
  let nativeTouches = [];
  document.hidden = false; document.activeElement = canvas;
  canvas.width = 1000; canvas.height = 750;
  canvas.getBoundingClientRect = () => ({ left: 0, top: 0, width: 1000, height: 750 });
  canvas.focus = () => {
    if (document.activeElement === canvas) return;
    document.activeElement = canvas; canvas.dispatchEvent(new Event("focus"));
  };
  canvas.setPointerCapture = id => captures.add(id);
  canvas.hasPointerCapture = id => captures.has(id);
  canvas.releasePointerCapture = id => {
    captures.delete(id);
    pointer("lostpointercapture", id, 0, 0);
  };
  const module = {
    _stella_pointer: () => 0,
    _stella_touch: (phase, id, x, y) => {
      const point = [id, Math.trunc(x), Math.trunc(y)];
      if (phase === 0) nativeTouches.push(point);
      else if (phase === 1) {
        const index = nativeTouches.findIndex(touch => touch[0] === id);
        if (index >= 0) nativeTouches[index] = point;
      } else nativeTouches = nativeTouches.filter(touch => touch[0] !== id);
      return 0;
    },
    // Replay the previous shipping adapter without modifying the worktree.
    _stella_touches: (count, id1, x1, y1, id2, x2, y2) => {
      nativeTouches = [[id1, x1, y1], [id2, x2, y2]].slice(0, count);
      return 0;
    },
    _stella_active: () => { nativeTouches = []; return 0; },
    _stella_audio_packet: () => 1,
    UTF8ToString: () => '{"started":true,"generation":1,"playbacks":[]}',
    _stella_key: () => 0, _stella_wheel: () => 0,
  };
  const game = { module, failed: false, audio: { setActive() {}, sync() {} } };
  const cleanup = installInput(game, {
    canvas, document, window, accountDialog, ratingDialog, sharingDialog, gamerServicesDialog,
    engineCall: (module, name, ...args) => { calls.push([name, ...args]); assert.equal(module[name](...args), 0); },
    failGame: (_, error) => { failures.push(error); game.failed = true; },
    saveOrReport: () => {},
  });
  function pointer(type, id, x, y, pointerType = "touch", button = 0) {
    const event = new Event(type, { cancelable: true });
    Object.assign(event, { pointerId: id, pointerType, clientX: x, clientY: y, button });
    canvas.dispatchEvent(event);
    assert.deepEqual(failures, []);
    return event;
  }
  return { canvas, document, game, calls, captures, cleanup, pointer,
    touches: () => nativeTouches, cursorCalls: () => calls.filter(call => call[0] === "_stella_pointer") };
}

test("gamer controller keeps native lifetime active while suppressing captured game input", () => {
  const dialog = new EventTarget(), button = {};
  dialog.hidden = false; dialog.contains = element => element === button;
  const h = host(undefined, null, null, dialog); h.pointer("pointerdown", 11, 10, 20);
  h.game.gamerServicesVisible = true; h.game.resetInput(); const before = h.calls.length;
  const blur = new Event("blur"); Object.assign(blur, { relatedTarget: button }); h.canvas.dispatchEvent(blur);
  h.document.activeElement = button; dialog.dispatchEvent(new Event("focusin"));
  h.pointer("pointerup", 11, 10, 20); h.pointer("pointerdown", 12, 30, 40);
  assert.equal(h.game.lifecycle.active, true); assert.equal(h.calls.length, before); assert.equal(h.captures.size, 0);
  h.cleanup();
});

test("image preview owns focus without pausing and blocks underlying game input", () => {
  const preview = new EventTarget(), button = {};
  preview.hidden = false; preview.contains = element => element === preview || element === button;
  const h = host(undefined, null, preview); h.game.sharingVisible = true;
  const blur = new Event("blur"); Object.assign(blur, { relatedTarget: button }); h.canvas.dispatchEvent(blur);
  preview.dispatchEvent(new Event("focusin"));
  h.pointer("pointerdown", 9, 10, 20);
  const key = new Event("keydown", { cancelable: true }); Object.assign(key, { key: "Escape" }); h.canvas.dispatchEvent(key);
  const wheel = new Event("wheel", { cancelable: true }); Object.assign(wheel, { deltaY: 1 }); h.canvas.dispatchEvent(wheel);
  assert.equal(h.game.lifecycle.active, true); assert.equal(h.calls.length, 0); assert.equal(h.captures.size, 0);
  h.cleanup();
});

test("focus transfers into the native account form without application/audio pause", () => {
  const root = new EventTarget(), email = {};
  root.hidden = false; root.contains = element => element === root || element === email;
  const target = host(root);
  target.game.accountVisible = true;
  const blur = new Event("blur"); Object.assign(blur, { relatedTarget: email }); target.canvas.dispatchEvent(blur);
  target.document.activeElement = email; root.dispatchEvent(new Event("focusin"));
  assert.equal(target.game.lifecycle.active, true);
  assert.deepEqual(target.calls.filter(call => call[0] === "_stella_active"), []);
});

test("rating owns focus above the account and restores it without native activation changes", () => {
  const account = new EventTarget(), rating = new EventTarget(), email = {}, button = {};
  account.hidden = false; account.contains = element => element === email;
  rating.hidden = false; rating.contains = element => element === button;
  const target = host(account, rating); target.game.accountVisible = true; target.game.ratingVisible = true;
  const blur = new Event("blur"); Object.assign(blur, { relatedTarget: button }); target.canvas.dispatchEvent(blur);
  target.document.activeElement = button; rating.dispatchEvent(new Event("focusin"));
  const transfer = new Event("focusout"); Object.assign(transfer, { relatedTarget: email }); rating.dispatchEvent(transfer);
  target.document.activeElement = email; account.dispatchEvent(new Event("focusin"));
  rating.hidden = true; target.game.ratingVisible = false;
  assert.equal(target.game.lifecycle.active, true);
  assert.deepEqual(target.calls.filter(call => call[0] === "_stella_active"), []);
});

test("rating ownership drops captured game input and suppresses late pointers and keys", () => {
  const target = host(); target.pointer("pointerdown", 11, 10, 20);
  const count = target.calls.length;
  target.game.ratingVisible = true; target.game.resetInput();
  target.pointer("pointerup", 11, 10, 20); target.pointer("pointerdown", 12, 30, 40);
  target.pointer("pointermove", 12, 50, 60);
  const key = new Event("keydown", { cancelable: true }); Object.assign(key, { key: "Escape" }); target.canvas.dispatchEvent(key);
  assert.equal(target.calls.length, count); assert.equal(target.captures.size, 0);
});

test("opening account ownership drops captured game pointers and rejects late game input", () => {
  const target = host(); target.pointer("pointerdown", 11, 10, 20);
  const count = target.calls.length;
  target.game.accountVisible = true; target.game.resetInput();
  target.pointer("lostpointercapture", 11, 999, 999); target.pointer("pointerup", 11, 100, 200);
  target.pointer("pointerdown", 12, 10, 20); target.pointer("pointermove", 12, 30, 40);
  const key = new Event("keydown", { cancelable: true }); Object.assign(key, { key: "Escape" }); target.canvas.dispatchEvent(key);
  assert.equal(target.calls.length, count); assert.equal(target.captures.size, 0);
});

test("native account focus obeys hidden/outside pause and closing restores canvas ownership", () => {
  const root = new EventTarget(), email = {};
  root.hidden = false; root.contains = element => element === root || element === email;
  const target = host(root);
  target.document.activeElement = email;
  const blur = new Event("blur"); Object.assign(blur, { relatedTarget: email }); target.canvas.dispatchEvent(blur);
  root.dispatchEvent(new Event("focusin"));
  target.document.hidden = true; target.document.dispatchEvent(new Event("visibilitychange"));
  assert.equal(target.game.lifecycle.active, false);
  target.document.hidden = false; target.document.dispatchEvent(new Event("visibilitychange"));
  assert.equal(target.game.lifecycle.active, true);
  const outside = new Event("focusout"); Object.assign(outside, { relatedTarget: {} }); root.dispatchEvent(outside);
  assert.equal(target.game.lifecycle.active, false);
  root.hidden = true; target.document.activeElement = target.canvas; target.canvas.dispatchEvent(new Event("focus"));
  assert.equal(target.game.lifecycle.active, true);
  assert.deepEqual(target.calls.filter(call => call[0] === "_stella_active").map(call => call[1]), [0, 1, 0, 1]);
});

test("the native vector retains three fingers while Lua alone caps publication at two", () => {
  const target = host();
  target.pointer("pointerdown", 11, 10, 20);
  target.pointer("pointerdown", 12, 30, 40);
  target.pointer("pointerdown", 13, 50, 60);
  assert.deepEqual(target.touches(), [[11, 10, 20], [12, 30, 40], [13, 50, 60]]);
  target.pointer("pointermove", 13, 70, 80);
  target.pointer("pointerup", 12, 30, 40);
  assert.deepEqual(target.touches(), [[11, 10, 20], [13, 70, 80]]);
  assert.deepEqual(target.cursorCalls(), [["_stella_pointer", 10, 20, 1]]);
});

test("ending the primary finger does not move the cursor when a surviving finger moves", () => {
  const target = host();
  target.pointer("pointerdown", 11, 10, 20);
  target.pointer("pointerdown", 12, 30, 40);
  target.pointer("pointerup", 11, 15, 25);
  target.pointer("pointermove", 12, 300, 400);
  target.pointer("pointerup", 12, 310, 410);
  assert.deepEqual(target.cursorCalls(), [["_stella_pointer", 10, 20, 1], ["_stella_pointer", 15, 25, 0]]);
  assert.deepEqual(target.touches(), []);
});

test("a new begin after primary cancellation owns the cursor without promoting surviving fingers", () => {
  const target = host();
  target.pointer("pointerdown", 11, 10, 20);
  target.pointer("pointerdown", 12, 30, 40);
  target.pointer("pointercancel", 11, 15, 25);
  target.pointer("pointermove", 12, 300, 400);
  target.pointer("pointerdown", 13, 50, 60);
  target.pointer("pointermove", 13, 70, 80);
  assert.deepEqual(target.cursorCalls(), [["_stella_pointer", 10, 20, 1], ["_stella_pointer", 15, 25, 0],
    ["_stella_pointer", 50, 60, 1], ["_stella_pointer", 70, 80, 1]]);
});

test("lost capture releases at the pointer's last unclamped position", () => {
  const target = host();
  target.pointer("pointerdown", 11, -12.75, 900.25);
  target.pointer("pointermove", 11, 1300.5, -27.75);
  target.canvas.releasePointerCapture(11);
  assert.deepEqual(target.cursorCalls().at(-1), ["_stella_pointer", 1300.5, -27.75, 0]);
  assert.deepEqual(target.touches(), []);
  const calls = target.calls.length;
  target.pointer("pointerup", 11, 0, 0);
  assert.equal(target.calls.length, calls, "Duplicate events cannot release a reset pointer twice");
});

test("implicit capture loss after pointerup preserves surviving touch order without more callbacks", () => {
  const target = host();
  target.pointer("pointerdown", 11, 10, 20);
  target.pointer("pointerdown", 12, 30, 40);
  target.pointer("pointerup", 11, 15, 25);
  const calls = target.calls.length;
  target.canvas.releasePointerCapture(11);
  assert.equal(target.calls.length, calls);
  assert.deepEqual(target.touches(), [[12, 30, 40]]);
});

test("activation reset cancels capture without a release edge or late touch resurrection", () => {
  const target = host();
  for (const id of [11, 12, 13]) target.pointer("pointerdown", id, id, id);
  const cursorCalls = target.cursorCalls().length;
  target.document.activeElement = null; target.canvas.dispatchEvent(new Event("blur"));
  assert.equal(target.captures.size, 0);
  assert.deepEqual(target.touches(), []);
  target.canvas.focus();
  target.pointer("pointermove", 12, 300, 400);
  target.pointer("pointerup", 11, 300, 400);
  target.pointer("lostpointercapture", 13, 0, 0);
  assert.equal(target.cursorCalls().length, cursorCalls);
  assert.deepEqual(target.touches(), []);
});

test("mouse hover works without a button while unowned touch and pen motion are ignored", () => {
  const target = host();
  target.pointer("pointermove", 11, 300, 400, "touch");
  target.pointer("pointermove", 12, 500, 600, "pen");
  target.pointer("pointermove", 1, 30, 40, "mouse");
  target.pointer("pointerdown", 1, 50, 60, "mouse");
  target.pointer("pointermove", 1, 70, 80, "mouse");
  target.pointer("pointerup", 1, 90, 100, "mouse");
  assert.deepEqual(target.cursorCalls(), [["_stella_pointer", 30, 40, 0], ["_stella_pointer", 50, 60, 1],
    ["_stella_pointer", 70, 80, 1], ["_stella_pointer", 90, 100, 0]]);
  assert.deepEqual(target.touches(), []);
});

test("cleanup removes listeners and relinquishes capture without synthesizing a release", () => {
  const target = host();
  target.pointer("pointerdown", 11, 10, 20);
  const calls = target.calls.length;
  target.cleanup();
  assert.equal(target.captures.size, 0);
  target.pointer("pointerup", 11, 30, 40);
  assert.equal(target.calls.length, calls);
});

test("horizontal wheel motion retains a zero native vertical delta", () => {
  const target = host();
  for (const deltaY of [0, -0]) {
    const event = new Event("wheel", { cancelable: true });
    Object.assign(event, { deltaX: 120, deltaY, shiftKey: true, ctrlKey: false });
    target.canvas.dispatchEvent(event);
    assert.equal(event.defaultPrevented, true);
    assert.deepEqual(target.calls.at(-1), ["_stella_wheel", 0, 1, 0]);
  }
});
