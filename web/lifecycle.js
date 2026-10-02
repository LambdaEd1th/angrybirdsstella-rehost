// Canvas focus, page visibility and modal ownership all gate one native
// application lifetime. A visibility event alone cannot dismiss a pause.
export class GameLifecycle {
  constructor({ activate, resetInput, resetClock, focused = true, visible = true, modal = false }) {
    this.activate = activate;
    this.resetInput = resetInput;
    this.resetClock = resetClock;
    this.active = true; // stella_init activates the original runtime.
    this.update({ focused, visible, modal });
  }
  update({ focused = this.focused, visible = this.visible, modal = this.modal } = {}) {
    this.focused = focused; this.visible = visible; this.modal = modal;
    const active = focused && visible && !modal;
    if (this.active === active) return;
    // Drop platform ownership before releasePointerCapture dispatches any
    // events. The native activation reset creates no pointer-release edge.
    this.active = active;
    this.resetInput();
    this.resetClock();
    this.activate(active);
  }
}
