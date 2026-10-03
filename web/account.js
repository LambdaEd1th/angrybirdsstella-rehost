// Original account pixels/controllers stay in Rust. These transparent controls
// supply browser accessibility, text input/IME and explicitly requested paste.
export function straightAlpha(premultiplied) {
  const pixels = new Uint8ClampedArray(premultiplied.length);
  for (let i = 0; i < pixels.length; i += 4) {
    const alpha = premultiplied[i + 3]; pixels[i + 3] = alpha;
    if (alpha) for (let c = 0; c < 3; c++) pixels[i + c] = Math.round(premultiplied[i + c] * 255 / alpha);
  }
  return pixels;
}

export function withUtf8(module, text, operation) {
  const bytes = new TextEncoder().encode(text), pointer = module._malloc(bytes.length + 1);
  if (!pointer) throw new Error("Account input allocation failed");
  try {
    module.HEAPU8.set(bytes, pointer); module.HEAPU8[pointer + bytes.length] = 0;
    return operation(pointer);
  } finally {
    module.HEAPU8.fill(0, pointer, pointer + bytes.length + 1); module._free(pointer);
  }
}

const names = { emailTextField: "Email address", passwordTextField: "Password", closeButton: "Close", backButton: "Back", questionButton: "Help", nextButton: "Next", okButton: "OK", dayTextField: "Day", monthTextField: "Month", yearTextField: "Year", emailErrorButton: "Email error", passwordErrorButton: "Password error", passwordTooltipButton: "Password help", gender_male_button: "Male", gender_female_button: "Female" };
const keys = { Escape: 0, Enter: 1, Tab: 2, ArrowUp: 3, ArrowDown: 4, Home: 5, End: 6 };
function fieldOf(element) { return element?.dataset?.field === undefined ? -1 : Number(element.dataset.field); }

export class BrowserAccountUI {
  constructor(game, { root, gameCanvas, engineCall, failGame, saveOrReport, document = globalThis.document, window = globalThis.window }) {
    Object.assign(this, { game, root, gameCanvas, engineCall, failGame, saveOrReport, document, window });
    this.listeners = []; this.inputs = new Map(); this.token = null; this.primary = null; this.composing = new Set();
    this.canvas = document.createElement("canvas"); this.canvas.className = "account-pixels"; this.canvas.setAttribute("aria-hidden", "true");
    this.context = this.canvas.getContext("2d");
    this.controls = document.createElement("div"); this.controls.className = "account-controls";
    root.replaceChildren(this.canvas, this.controls);
    const listen = (type, operation, options) => { root.addEventListener(type, operation, options); this.listeners.push(() => root.removeEventListener(type, operation, options)); };
    listen("pointerdown", event => {
      if (event.button !== 0 || this.primary !== null || game.failed || !game.lifecycle.active || game.ratingVisible) return;
      event.preventDefault(); this.primary = event.pointerId; this.pointerToken = this.token; root.setPointerCapture(event.pointerId);
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
      const button = event.target.closest?.("button[data-control]");
      // Pointer release already dispatches through the Rust hit map. Keyboard
      // and accessibility activation use the same named native controller.
      if (button && event.detail === 0) this.run(() => withUtf8(game.module, button.dataset.control, pointer => engineCall(game.module, "_stella_account_control", this.token, pointer)));
    });
    listen("focusin", event => {
      if (!this.refreshing) this.run(() => engineCall(game.module, "_stella_account_focus", this.token, fieldOf(event.target)));
    });
    const edit = element => {
      if (fieldOf(element) < 0 || this.refreshing) return;
      this.run(() => withUtf8(game.module, JSON.stringify({ value: element.value, start: element.selectionStart, end: element.selectionEnd, backward: element.selectionDirection === "backward" }), pointer => engineCall(game.module, "_stella_account_edit", this.token, fieldOf(element), pointer)));
    };
    listen("input", event => edit(event.target));
    listen("select", event => { if (document.activeElement === event.target) edit(event.target); });
    listen("keyup", event => { if (document.activeElement === event.target && fieldOf(event.target) >= 0) edit(event.target); });
    listen("compositionstart", event => this.composing.add(event.target));
    listen("compositionend", event => { this.composing.delete(event.target); edit(event.target); });
    listen("keydown", event => {
      if (event.isComposing || this.composing.has(event.target) || event.keyCode === 229) return;
      const field = fieldOf(event.target), picker = this.packet?.controls.some(control => control.name.startsWith("pickerRow"));
      const nativeKey = event.key === "Escape" || (event.key === "Enter" && !event.target.matches?.("button")) || (picker && ["ArrowUp", "ArrowDown", "Home", "End", "Enter"].includes(event.key));
      if (nativeKey) {
        event.preventDefault(); this.run(() => engineCall(game.module, "_stella_account_key", this.token, keys[event.key], +event.shiftKey));
      } else if (event.key === "Tab") {
        // Browser focus traversal includes the native links/buttons, with a
        // local trap; field Return still uses the recovered native delegate.
        const focusable = [...this.controls.children].filter(element => !element.disabled);
        const current = focusable.indexOf(document.activeElement);
        const next = event.shiftKey ? (current <= 0 ? focusable.at(-1) : focusable[current - 1]) : focusable[(current + 1) % focusable.length];
        if (next) { event.preventDefault(); next.focus({ preventScroll: true }); }
      } else if (field === 1 && (event.ctrlKey || event.metaKey) && ["c", "x"].includes(event.key.toLowerCase())) event.preventDefault();
    });
    for (const type of ["copy", "cut"]) listen(type, event => { if (fieldOf(event.target) === 1) event.preventDefault(); });
    listen("wheel", event => { event.preventDefault(); this.run(() => engineCall(game.module, "_stella_account_wheel", this.token, Math.round(event.deltaY / 24))); }, { passive: false });
    for (const type of ["contextmenu", "dragstart"]) listen(type, event => { if (fieldOf(event.target) === 1 || type === "dragstart") event.preventDefault(); });
  }

  run(operation) {
    if (this.game.failed || !this.game.lifecycle.active || this.game.ratingVisible || this.token === null) return;
    try { operation(); this.refresh(); }
    catch (error) { this.failGame(this.game, error); }
  }

  pointer(phase, event) {
    const rect = this.root.getBoundingClientRect(), [width, height] = this.packet.resolution;
    this.run(() => this.engineCall(this.game.module, "_stella_account_pointer", this.pointerToken, phase, (event.clientX - rect.left) * width / rect.width, (event.clientY - rect.top) * height / rect.height, +event.shiftKey));
  }

  refresh(now = performance.now()) {
    if (this.refreshing) return;
    this.refreshing = true;
    try {
      const { module } = this.game;
      this.engineCall(module, "_stella_account_frame", now / 1000);
      this.update(JSON.parse(module.UTF8ToString(module._stella_account_packet())));
    } finally { this.refreshing = false; }
  }

  update(packet) {
    if (!packet) { this.hide(); return; }
    const opened = this.root.hidden, ownedFocus = this.root.contains(this.document.activeElement);
    this.packet = packet; this.game.accountVisible = true; this.root.hidden = false; this.root.dataset.view = packet.view;
    this.root.setAttribute("aria-busy", String(packet.busy));
    if (opened) { this.game.resetInput(); this.saveOrReport(this.game, false); }
    const signature = JSON.stringify([packet.token, packet.busy, packet.resolution, packet.controls]);
    if (this.signature !== signature) {
      // Retain ownership while replacing a focused input. Removing it first
      // emits a focus loss and would spuriously pause/resume the game.
      if (ownedFocus) this.root.focus({ preventScroll: true });
      this.clearInputs(); this.controls.replaceChildren();
      for (const control of packet.controls) {
        const field = control.name === "emailTextField" ? 0 : control.name === "passwordTextField" ? 1 : -1;
        const element = this.document.createElement(field < 0 ? "button" : "input");
        element.dataset.control = control.name;
        if (field >= 0) {
          element.type = field === 1 ? "password" : "text"; element.inputMode = field === 1 ? "text" : "email";
          element.autocomplete = "off"; element.spellcheck = false; element.autocapitalize = "none"; element.dataset.field = String(field);
          this.inputs.set(field, element);
        } else element.type = "button";
        element.disabled = packet.busy; element.setAttribute("aria-label", control.label || names[control.name] || control.name);
        const [x, y, width, height] = control.rect, [w, h] = packet.resolution;
        Object.assign(element.style, { left: `${x / w * 100}%`, top: `${y / h * 100}%`, width: `${width / w * 100}%`, height: `${height / h * 100}%` });
        this.controls.append(element);
      }
      this.signature = signature;
    }
    this.token = packet.token;
    if (packet.image) {
      const { pointer, length, width, height } = packet.image;
      this.canvas.width = width; this.canvas.height = height;
      this.context.putImageData(new ImageData(straightAlpha(this.game.module.HEAPU8.subarray(pointer, pointer + length)), width, height), 0, 0);
    }
    for (const [field, input] of this.inputs) {
      if (this.composing.has(input)) continue;
      const pointer = this.game.module._stella_account_editor(this.token, field);
      if (!pointer) continue;
      const editor = JSON.parse(this.game.module.UTF8ToString(pointer));
      if (!editor) continue;
      if (input.value !== editor.value) input.value = editor.value;
      if (input.selectionStart !== editor.start || input.selectionEnd !== editor.end || (input.selectionDirection === "backward") !== editor.backward) input.setSelectionRange(editor.start, editor.end, editor.backward ? "backward" : "forward");
    }
    if (!this.game.ratingVisible && (opened || ownedFocus)) {
      const focus = this.inputs.get(packet.focus);
      if (focus && this.document.activeElement !== focus) focus.focus({ preventScroll: true });
      else if (!focus && (opened || fieldOf(this.document.activeElement) >= 0 || !this.root.contains(this.document.activeElement))) this.root.focus({ preventScroll: true });
    }
    if (packet.externalUrl) this.window.open(packet.externalUrl, "_blank", "noopener,noreferrer");
  }

  clearInputs() { for (const input of this.inputs.values()) input.value = ""; this.inputs.clear(); this.composing.clear(); }
  cancelPointer() {
    const captured = this.primary; this.primary = null;
    if (captured !== null && this.root.hasPointerCapture(captured)) this.root.releasePointerCapture(captured);
  }
  focus() {
    if (this.root.hidden) this.gameCanvas.focus({ preventScroll: true });
    else (this.inputs.get(this.packet?.focus) || this.root).focus({ preventScroll: true });
  }
  hide(restoreFocus = true) {
    if (this.root.hidden && this.token === null) return;
    const focused = this.root.contains(this.document.activeElement);
    // Hiding the focused dialog first makes Chromium send focusout(null),
    // incorrectly pausing/restarting native audio during cancellation.
    if (focused && restoreFocus) this.gameCanvas.focus({ preventScroll: true });
    const captured = this.primary; this.primary = null;
    if (captured !== null && this.root.hasPointerCapture(captured)) this.root.releasePointerCapture(captured);
    this.clearInputs(); this.controls.replaceChildren(); this.canvas.width = 1; this.canvas.height = 1;
    this.root.hidden = true; delete this.root.dataset.view; this.game.accountVisible = false; this.token = null; this.signature = null; this.packet = null;
  }
  dispose() { this.listeners.forEach(remove => remove()); this.hide(false); this.root.replaceChildren(); }
}
