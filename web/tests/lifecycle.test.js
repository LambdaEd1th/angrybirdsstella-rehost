import test from "node:test";
import assert from "node:assert/strict";
import { GameLifecycle } from "../lifecycle.js";

function host(initial = {}) {
  const events = [], input = { primary: 11, touches: [11, 12], releases: [] };
  let lastFrame = 100, now = 100;
  const lifetime = new GameLifecycle({
    activate: value => events.push(value),
    resetInput: () => { input.primary = null; input.touches = []; },
    resetClock: () => { lastFrame = now; },
    ...initial,
  });
  return { lifetime, events, input, advance: seconds => { now += seconds; }, delta: () => now - lastFrame };
}

test("visibility does not resume a game whose canvas is still unfocused", () => {
  const { lifetime, events } = host();
  lifetime.update({ focused: false });
  lifetime.update({ visible: false }); lifetime.update({ visible: true });
  assert.equal(lifetime.active, false); assert.deepEqual(events, [false]);
  lifetime.update({ focused: true });
  assert.equal(lifetime.active, true); assert.deepEqual(events, [false, true]);
});

test("focus and visibility events cannot resume audio or gameplay behind an account modal", () => {
  const { lifetime, events } = host();
  lifetime.update({ modal: true }); lifetime.update({ focused: false });
  lifetime.update({ visible: false }); lifetime.update({ visible: true });
  lifetime.update({ focused: true });
  assert.equal(lifetime.active, false); assert.deepEqual(events, [false]);
  lifetime.update({ modal: false });
  assert.equal(lifetime.active, true); assert.deepEqual(events, [false, true]);
});

test("closing a modal while hidden keeps the game paused until visible and focused", () => {
  const { lifetime, events } = host();
  lifetime.update({ modal: true }); lifetime.update({ visible: false, focused: false });
  lifetime.update({ modal: false }); lifetime.update({ focused: true });
  assert.deepEqual(events, [false]);
  lifetime.update({ visible: true });
  assert.deepEqual(events, [false, true]);
});

test("duplicate notifications preserve one callback per transition and background time creates no frame debt", () => {
  const target = host();
  target.lifetime.update({ visible: false }); target.lifetime.update({ focused: false });
  target.lifetime.update({ visible: false }); target.advance(3600);
  target.lifetime.update({ visible: true }); target.lifetime.update({ focused: true });
  target.lifetime.update({ visible: true, focused: true });
  assert.deepEqual(target.events, [false, true]); assert.equal(target.delta(), 0);
  assert.equal(target.input.primary, null); assert.deepEqual(target.input.touches, []);
  assert.deepEqual(target.input.releases, []);
});

test("initial hidden or unfocused starts reconcile the already-active Rust host", () => {
  for (const initial of [{ visible: false }, { focused: false }, { modal: true }]) {
    const { lifetime, events } = host(initial);
    assert.equal(lifetime.active, false); assert.deepEqual(events, [false]);
    lifetime.update({ visible: true, focused: true, modal: false });
    assert.equal(lifetime.active, true); assert.deepEqual(events, [false, true]);
  }
});
