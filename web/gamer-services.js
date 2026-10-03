// Game Center presentations consume immutable provider snapshots. Portable
// hosts expose local records; opening/closing never authenticates or posts.
import { onLanguageChange, t } from "./i18n.js";

export class BrowserGamerServicesUI {
  constructor(game, { root, gameCanvas, accountDialog, ratingDialog, engineCall, document = globalThis.document }) {
    Object.assign(this, { game, root, gameCanvas, accountDialog, ratingDialog, engineCall, document });
    this.queue = []; this.current = null; this.disposed = false;
    this.heading = document.createElement("h2"); this.scope = document.createElement("p");
    this.records = document.createElement("dl"); this.empty = document.createElement("p");
    this.closeButton = document.createElement("button"); this.closeButton.type = "button";
    root.replaceChildren(this.heading, this.scope, this.records, this.empty, this.closeButton);
    const close = () => this.close();
    const key = event => {
      if (!this.current || this.root.inert) return;
      if (event.key === "Escape") { event.preventDefault(); event.stopPropagation(); if (!event.repeat && !event.isComposing) this.close(); }
      else if (event.key === "Tab") { event.preventDefault(); this.focus(); }
    };
    this.closeButton.addEventListener("click", close); root.addEventListener("keydown", key);
    this.listeners = [() => this.closeButton.removeEventListener("click", close), () => root.removeEventListener("keydown", key), onLanguageChange(() => this.translate())];
    this.translate();
  }

  enqueue(action) {
    if (this.disposed || !["achievements", "leaderboards"].includes(action.view)) return;
    // Keep provider strings and float formatting intact; never interpret them
    // as HTML or read mutable save/provider state during a later presentation.
    this.queue.push({ view: action.view, localProvider: action.localProvider === true, entries: action.entries.map(entry => [...entry]) });
    if (!this.current) this.showNext();
  }

  showNext() {
    this.current = this.queue.shift() || null;
    if (!this.current) { this.hide(); return; }
    this.game.gamerServicesVisible = true; this.root.hidden = false;
    this.game.resetInput(); this.game.account.cancelPointer(); this.game.rating.cancelPointer();
    this.engineCall(this.game.module, "_stella_gamer_services_preview", 1);
    this.translate(); this.refresh(); this.focus();
  }

  translate() {
    const achievements = this.current?.view === "achievements";
    this.heading.textContent = t(achievements ? "achievements" : "leaderboards");
    this.root.setAttribute("aria-label", this.heading.textContent);
    this.scope.textContent = t("gamerServicesLocal"); this.scope.hidden = !this.current?.localProvider;
    this.records.replaceChildren();
    if (this.current?.localProvider) for (const [id, value] of this.current.entries) {
      const name = this.document.createElement("dt"), score = this.document.createElement("dd");
      name.textContent = id; score.textContent = achievements && value === "Unlocked" ? t("achievementUnlocked") : value;
      this.records.append(name, score);
    }
    this.records.hidden = !this.current?.localProvider || !this.current.entries.length;
    this.empty.hidden = !this.records.hidden;
    this.empty.textContent = t(!this.current?.localProvider ? "gamerServicesUnavailable" : achievements ? "noAchievements" : "noScores");
    this.closeButton.textContent = t("closePreview");
  }

  refresh() {
    if (!this.current) return;
    this.root.inert = !!this.game.sharingVisible;
    this.accountDialog.inert = true; this.ratingDialog.inert = true;
    if (this.accountDialog.contains(this.document.activeElement) || this.ratingDialog.contains(this.document.activeElement)) this.focus();
  }

  focus() {
    if (this.game.sharingVisible) this.game.sharing.focus();
    else this.closeButton.focus({ preventScroll: true });
  }

  close() {
    if (!this.current || this.disposed || this.game.sharingVisible) return;
    this.current = null; this.showNext();
  }

  hide(restoreFocus = true) {
    const focused = this.root.contains(this.document.activeElement), wasVisible = this.game.gamerServicesVisible;
    this.game.gamerServicesVisible = false; this.root.inert = !!this.game.sharingVisible;
    this.accountDialog.inert = !!(this.game.ratingVisible || this.game.sharingVisible);
    this.ratingDialog.inert = !!this.game.sharingVisible;
    if (wasVisible && !this.disposed) this.engineCall(this.game.module, "_stella_gamer_services_preview", 0);
    if (focused && restoreFocus) {
      if (this.game.sharingVisible) this.game.sharing.focus();
      else if (this.game.ratingVisible) this.game.rating.focus();
      else if (this.game.accountVisible) this.game.account.focus();
      else this.gameCanvas.focus({ preventScroll: true });
    }
    this.root.hidden = true;
  }

  dispose() {
    this.disposed = true; this.listeners.forEach(remove => remove());
    this.queue = []; this.current = null; this.hide(false); this.root.replaceChildren();
  }
}
