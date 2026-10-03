// Rust owns the retained alert, pixels, hit map and native answer. Transparent
// DOM buttons provide browser accessibility without entering game captures.
import { straightAlpha } from "./account.js";

const keys = { Tab: 0, ArrowUp: 1, ArrowLeft: 1, ArrowDown: 2, ArrowRight: 2, Enter: 3, " ": 3, Escape: 4 };

export class BrowserRatingUI {
  constructor(game, { root, gameCanvas, accountDialog, engineCall, failGame, saveOrReport, dispatchActions, document = globalThis.document }) {
    Object.assign(this, { game, root, gameCanvas, accountDialog, engineCall, failGame, saveOrReport, dispatchActions, document });
    this.listeners = []; this.token = null; this.primary = null; this.buttons = new Map();
    this.canvas = document.createElement("canvas"); this.canvas.className = "account-pixels"; this.canvas.setAttribute("aria-hidden", "true");
    this.context = this.canvas.getContext("2d");
    this.controls = document.createElement("div"); this.controls.className = "account-controls";
    root.replaceChildren(this.canvas, this.controls);
    const listen = (type, handler, options) => { root.addEventListener(type, handler, options); this.listeners.push(() => root.removeEventListener(type, handler, options)); };
    listen("pointerdown", event => {
      if (event.button !== 0 || this.primary !== null || game.failed || game.sharingVisible || game.gamerServicesVisible || this.token === null) return;
      event.preventDefault();
      if (!game.lifecycle.active) {
        // Owned focus resumes the native lifetime and invalidates old input.
        // Read its new token before accepting this same trusted press.
        this.focus(); this.refresh();
      }
      if (!game.lifecycle.active || this.token === null) return;
      this.primary = event.pointerId; this.pointerToken = this.token; root.setPointerCapture(event.pointerId);
      this.pointer(0, event);
    });
    listen("pointermove", event => { if (this.primary === event.pointerId) { event.preventDefault(); this.pointer(1, event); } });
    const end = (event, canceled) => {
      if (this.primary !== event.pointerId) return;
      event.preventDefault(); this.primary = null;
      if (root.hasPointerCapture(event.pointerId)) root.releasePointerCapture(event.pointerId);
      this.pointer(canceled ? 3 : 2, event);
    };
    listen("pointerup", event => end(event, false));
    listen("pointercancel", event => end(event, true));
    listen("lostpointercapture", event => end(event, true));
    listen("click", event => {
      const button = event.target.closest?.("button[data-choice]");
      // Captured pointer release already follows Rust's same-button rule.
      if (button && event.detail === 0) this.run(() => engineCall(game.module, "_stella_rating_choose", this.token, Number(button.dataset.choice)));
    });
    listen("focusin", event => {
      if (!this.refreshing && event.target.dataset?.choice !== undefined) this.run(() => engineCall(game.module, "_stella_rating_focus", this.token, Number(event.target.dataset.choice)));
    });
    listen("keydown", event => {
      if (!Object.hasOwn(keys, event.key)) return;
      event.preventDefault();
      if (!event.repeat && !event.isComposing) this.run(() => engineCall(game.module, "_stella_rating_key", this.token, keys[event.key], +event.shiftKey));
    });
    listen("keyup", event => { if (Object.hasOwn(keys, event.key)) event.preventDefault(); });
    listen("focusout", event => { if (!root.contains(event.relatedTarget)) this.cancelPointer(); });
    for (const type of ["wheel", "touchstart", "touchmove"]) listen(type, event => { if (event.cancelable) event.preventDefault(); }, { passive: false });
    for (const type of ["contextmenu", "dragstart", "selectstart"]) listen(type, event => event.preventDefault());
  }

  run(operation) {
    if (this.game.failed || !this.game.lifecycle.active || this.game.sharingVisible || this.game.gamerServicesVisible || this.token === null) return;
    try {
      operation(); this.refresh();
      // Native flags are saved on close before the advisory OS launch. This
      // stays inside the trusted button callback, including keyboard/AT input.
      this.dispatchActions();
    } catch (error) { this.failGame(this.game, error); }
  }

  pointer(phase, event) {
    if (!this.packet) return;
    const rect = this.root.getBoundingClientRect(), [width, height] = this.packet.resolution;
    this.run(() => this.engineCall(this.game.module, "_stella_rating_pointer", this.pointerToken, phase, (event.clientX - rect.left) * width / rect.width, (event.clientY - rect.top) * height / rect.height));
  }

  cancelPointer() {
    const captured = this.primary; this.primary = null;
    if (captured !== null && this.root.hasPointerCapture(captured)) this.root.releasePointerCapture(captured);
  }

  refresh() {
    if (this.refreshing) return;
    this.refreshing = true;
    try {
      const { module } = this.game;
      this.engineCall(module, "_stella_rating_frame");
      const pointer = module._stella_rating_packet();
      if (!pointer) throw new Error(module.UTF8ToString(module._stella_error()));
      this.update(JSON.parse(module.UTF8ToString(pointer)));
    } finally { this.refreshing = false; }
  }

  update(packet) {
    if (!packet) { this.hide(); return; }
    const opened = this.root.hidden, ownedFocus = this.root.contains(this.document.activeElement);
    this.packet = packet; this.game.ratingVisible = true; this.root.hidden = false;
    this.root.setAttribute("aria-label", packet.message);
    const signature = JSON.stringify([packet.token, packet.resolution, packet.buttons]);
    if (this.signature !== signature) {
      // Keep focus in a visible owned element before replacing focused buttons.
      if (ownedFocus) this.root.focus({ preventScroll: true });
      this.cancelPointer(); this.controls.replaceChildren(); this.buttons.clear();
      for (const control of packet.buttons) {
        const button = this.document.createElement("button"); button.type = "button"; button.dataset.choice = String(control.choice);
        button.setAttribute("aria-label", control.title); button.textContent = control.title;
        const [x, y, width, height] = control.rect, [w, h] = packet.resolution;
        Object.assign(button.style, { left: `${x / w * 100}%`, top: `${y / h * 100}%`, width: `${width / w * 100}%`, height: `${height / h * 100}%` });
        this.controls.append(button); this.buttons.set(control.choice, button);
      }
      this.signature = signature;
    }
    this.token = packet.token;
    if (packet.image) {
      const { pointer, length, width, height } = packet.image;
      this.canvas.width = width; this.canvas.height = height;
      this.context.putImageData(new ImageData(straightAlpha(this.game.module.HEAPU8.subarray(pointer, pointer + length)), width, height), 0, 0);
    }
    if (opened) {
      this.game.resetInput(); this.game.account.cancelPointer();
      this.saveOrReport(this.game, false);
    }
    if (opened || ownedFocus) this.focus();
    // Rating owns input above an account form. Its timers/runtime remain live,
    // but its transparent editors must not enter browser focus traversal.
    this.accountDialog.inert = true;
  }

  focus() {
    if (this.game.sharingVisible) { this.game.sharing.focus(); return; }
    if (this.game.gamerServicesVisible) { this.game.gamerServices.focus(); return; }
    (this.buttons.get(this.packet?.focus) || this.root).focus({ preventScroll: true });
  }

  hide(restoreFocus = true) {
    if (this.root.hidden && this.token === null) return;
    const focused = this.root.contains(this.document.activeElement), wasVisible = this.game.ratingVisible;
    this.game.ratingVisible = false; this.accountDialog.inert = !!(this.game.sharingVisible || this.game.gamerServicesVisible);
    // Transfer focus before hiding; Chromium otherwise emits focusout(null)
    // and spuriously pauses/resumes the native game and audio.
    if (focused && restoreFocus) {
      if (this.game.accountVisible) this.game.account.focus();
      else this.gameCanvas.focus({ preventScroll: true });
    }
    this.cancelPointer(); this.controls.replaceChildren(); this.buttons.clear();
    this.canvas.width = 1; this.canvas.height = 1; this.root.hidden = true;
    this.packet = null; this.token = null; this.signature = null;
    if (wasVisible && restoreFocus) this.saveOrReport(this.game, false);
  }

  dispose() { this.listeners.forEach(remove => remove()); this.hide(false); this.root.replaceChildren(); }
}
