import { GameLifecycle } from "./lifecycle.js";
import { canvasPoint } from "./display.js";
import { LocalizedError } from "./i18n.js";

export function installInput(game, { canvas, accountDialog, ratingDialog = null, sharingDialog = null, gamerServicesDialog = null, engineCall, failGame, saveOrReport, document = globalThis.document, window = globalThis.window }) {
  const listeners = [], pointers = new Map();
  let primary = null;
  function listen(target, event, handler, options) { target.addEventListener(event, handler, options); listeners.push(() => target.removeEventListener(event, handler, options)); }
  const modalVisible = () => game.accountVisible || game.ratingVisible || game.sharingVisible || game.gamerServicesVisible;
  function input(operation) { if (game.failed || !game.lifecycle.active || modalVisible()) return; try { operation(); } catch (error) { failGame(game, error); } }
  function resetPointers() {
    const captured = [...pointers.keys()];
    primary = null; pointers.clear();
    for (const id of captured) if (canvas.hasPointerCapture(id)) canvas.releasePointerCapture(id);
  }
  game.resetInput = resetPointers;
  const dialogs = [accountDialog, ratingDialog, sharingDialog, gamerServicesDialog].filter(Boolean);
  function ownsFocus(element) { return element === canvas || dialogs.some(dialog => !dialog.hidden && dialog.contains?.(element)); }
  game.lifecycle = new GameLifecycle({
    focused: ownsFocus(document.activeElement), visible: !document.hidden, modal: false,
    resetInput: resetPointers,
    resetClock: () => { game.last = performance.now(); },
    activate: active => {
      // Suspend before Lua can throw, then reconcile the state changed by the
      // original pause/resume callbacks before allowing physical audio again.
      if (!active) game.audio.setActive(false);
      engineCall(game.module, "_stella_active", +active);
      const pointer = game.module._stella_audio_packet();
      if (!pointer) throw new Error(game.module.UTF8ToString(game.module._stella_error()));
      game.audio.sync(game.module, JSON.parse(game.module.UTF8ToString(pointer)));
      game.audio.setActive(active);
    },
  });
  function lifecycle(state) {
    if (game.failed) return;
    try { game.lifecycle.update(state); } catch (error) { failGame(game, error); }
  }
  function point(event) {
    return canvasPoint(canvas, event);
  }
  function touch(phase, id, position) {
    input(() => engineCall(game.module, "_stella_touch", phase, id, ...position));
  }
  listen(canvas, "pointerdown", event => {
    if (event.button !== 0 || game.failed || modalVisible()) return;
    event.preventDefault(); canvas.focus();
    if (!game.lifecycle.active) return;
    canvas.setPointerCapture(event.pointerId);
    const position = point(event);
    pointers.set(event.pointerId, { type: event.pointerType, position });
    if (event.pointerType !== "mouse") touch(0, event.pointerId, position);
    if (primary === null) primary = event.pointerId;
    if (primary === event.pointerId) input(() => engineCall(game.module, "_stella_pointer", ...position, 1));
  });
  listen(canvas, "pointermove", event => {
    if (game.failed || !game.lifecycle.active || modalVisible()) return;
    const position = point(event), tracked = pointers.get(event.pointerId);
    if (tracked) {
      tracked.position = position;
      if (tracked.type !== "mouse") touch(1, event.pointerId, position);
    }
    if (primary === event.pointerId) input(() => engineCall(game.module, "_stella_pointer", ...position, 1));
    // UIKit never promotes a surviving finger or publishes its hover position.
    // A mouse can still hover when there is no active primary pointer.
    else if (primary === null && event.pointerType === "mouse") input(() => engineCall(game.module, "_stella_pointer", ...position, 0));
  });
  function endPointer(event, lostCapture = false) {
    const tracked = pointers.get(event.pointerId);
    if (!tracked) return; // Ignore duplicate releases and events from a reset lifetime.
    const position = lostCapture ? tracked.position : point(event);
    pointers.delete(event.pointerId);
    if (tracked.type !== "mouse") touch(2, event.pointerId, position);
    if (primary !== event.pointerId) return;
    primary = null; input(() => engineCall(game.module, "_stella_pointer", ...position, 0));
  }
  for (const type of ["pointerup", "pointercancel"]) listen(canvas, type, endPointer);
  // lostpointercapture has no reliable release location. Use the last position
  // delivered by this pointer, and ignore the implicit event after pointerup.
  listen(canvas, "lostpointercapture", event => endPointer(event, true));
  for (const type of ["contextmenu", "selectstart", "dragstart"]) listen(canvas, type, event => event.preventDefault());
  // iOS Safari can start a selection/callout even when pointerdown is canceled.
  // Keep Pointer Events as the single game input source; these listeners only
  // suppress browser gestures, including while two fingers control the game.
  for (const type of ["touchstart", "touchmove"]) listen(canvas, type, event => {
    if (event.cancelable) event.preventDefault();
  }, { passive: false });
  listen(canvas, "wheel", event => { event.preventDefault(); input(() => engineCall(game.module, "_stella_wheel", event.deltaY === 0 ? 0 : event.deltaY < 0 ? 1 : -1, +event.shiftKey, +event.ctrlKey)); }, { passive: false });
  const keys = { Escape: 0, m: 1, M: 1, "+": 2, "-": 3 };
  for (const type of ["keydown", "keyup"]) listen(canvas, type, event => {
    if (Object.hasOwn(keys, event.key)) { event.preventDefault(); input(() => engineCall(game.module, "_stella_key", keys[event.key], +(type === "keydown"))); }
  });
  listen(canvas, "blur", event => { lifecycle({ focused: !!ownsFocus(event.relatedTarget) }); saveOrReport(game, false); });
  listen(canvas, "focus", () => { lifecycle({ focused: true }); });
  for (const dialog of dialogs) if (dialog.addEventListener) {
    listen(dialog, "focusin", () => lifecycle({ focused: true }));
    listen(dialog, "focusout", event => lifecycle({ focused: !!ownsFocus(event.relatedTarget) }));
  }
  listen(document, "visibilitychange", () => {
    lifecycle({ visible: !document.hidden });
    if (document.hidden) saveOrReport(game, false);
  });
  listen(window, "pagehide", () => { lifecycle({ visible: false }); saveOrReport(game); });
  listen(window, "pageshow", () => { lifecycle({ visible: !document.hidden, focused: !!ownsFocus(document.activeElement) }); });
  listen(canvas, "webglcontextlost", event => { event.preventDefault(); failGame(game, new LocalizedError("graphicsLost")); });
  return () => { resetPointers(); listeners.forEach(remove => remove()); };
}
