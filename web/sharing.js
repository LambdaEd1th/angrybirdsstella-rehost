// Native screenshot shares preview an immutable PNG before choosing a target.
// Canvas2D encoders premultiply/round transparent RGB, so encode raw RGBA rows.
import { zlibSync } from "./vendor/fflate.js";
import { onLanguageChange, t } from "./i18n.js";

const crcTable = new Uint32Array(256);
for (let i = 0; i < 256; i++) {
  let value = i;
  for (let bit = 0; bit < 8; bit++) value = (value >>> 1) ^ (value & 1 ? 0xedb88320 : 0);
  crcTable[i] = value;
}
function chunk(type, data) {
  const bytes = new Uint8Array(data.length + 12), view = new DataView(bytes.buffer);
  view.setUint32(0, data.length);
  bytes.set([...type].map(char => char.charCodeAt(0)), 4); bytes.set(data, 8);
  let crc = 0xffffffff;
  for (const byte of bytes.subarray(4, bytes.length - 4)) crc = crcTable[(crc ^ byte) & 255] ^ (crc >>> 8);
  view.setUint32(bytes.length - 4, (crc ^ 0xffffffff) >>> 0);
  return bytes;
}
export function encodeScreenshotPNG({ width, height, rgba }) {
  if (!Number.isInteger(width) || !Number.isInteger(height) || width < 1 || height < 1 || width > 65535 || height > 65535 || rgba.length !== width * height * 4) throw new RangeError("Invalid screenshot extent");
  const header = new Uint8Array(13), view = new DataView(header.buffer);
  view.setUint32(0, width); view.setUint32(4, height); header.set([8, 6], 8);
  const rowBytes = width * 4, rows = new Uint8Array((rowBytes + 1) * height);
  for (let y = 0; y < height; y++) rows.set(rgba.subarray(y * rowBytes, (y + 1) * rowBytes), y * (rowBytes + 1) + 1);
  const chunks = [new Uint8Array([137, 80, 78, 71, 13, 10, 26, 10]), chunk("IHDR", header), chunk("IDAT", zlibSync(rows)), chunk("IEND", new Uint8Array())];
  const png = new Uint8Array(chunks.reduce((length, part) => length + part.length, 0));
  let offset = 0;
  for (const part of chunks) { png.set(part, offset); offset += part.length; }
  return png;
}

export class BrowserScreenshotUI {
  constructor(game, { root, gameCanvas, accountDialog, ratingDialog, engineCall, document = globalThis.document, navigator = globalThis.navigator, url = globalThis.URL, File = globalThis.File }) {
    Object.assign(this, { game, root, gameCanvas, accountDialog, ratingDialog, engineCall, document, navigator, url, File });
    this.queue = []; this.current = null; this.disposed = false;
    this.heading = document.createElement("h2");
    this.image = document.createElement("img"); this.image.draggable = false;
    this.actions = document.createElement("div"); this.actions.className = "screenshot-actions";
    this.shareButton = document.createElement("button"); this.shareButton.type = "button";
    this.download = document.createElement("a"); this.download.className = "screenshot-download";
    this.closeButton = document.createElement("button"); this.closeButton.type = "button";
    this.error = document.createElement("p"); this.error.setAttribute("role", "status"); this.error.hidden = true;
    this.actions.append(this.shareButton, this.download, this.closeButton);
    root.replaceChildren(this.heading, this.image, this.error, this.actions);
    this.listeners = [];
    const listen = (target, type, callback) => { target.addEventListener(type, callback); this.listeners.push(() => target.removeEventListener(type, callback)); };
    listen(this.shareButton, "click", () => this.share());
    listen(this.closeButton, "click", () => this.close());
    listen(this.download, "click", event => { if (this.current?.pending) event.preventDefault(); });
    listen(root, "keydown", event => {
      if (event.key === "Escape") { event.preventDefault(); event.stopPropagation(); this.close(); }
      else if (event.key === "Tab") {
        const controls = [this.shareButton, this.download, this.closeButton].filter(control => !control.hidden && !control.disabled);
        if (!controls.length || this.current?.pending) { event.preventDefault(); return; }
        const index = controls.indexOf(document.activeElement);
        const next = index < 0 ? (event.shiftKey ? controls.length - 1 : 0) : (index + (event.shiftKey ? -1 : 1) + controls.length) % controls.length;
        event.preventDefault(); controls[next].focus({ preventScroll: true });
      }
    });
    this.listeners.push(onLanguageChange(() => this.translate()));
    this.translate();
  }

  enqueue(screenshots) {
    if (this.disposed) return;
    for (const screenshot of screenshots) {
      const { request } = screenshot;
      const file = new this.File([encodeScreenshotPNG(screenshot)], request.filename, { type: "image/png" });
      this.queue.push({ request, file, url: this.url.createObjectURL(file), pending: false });
    }
    if (!this.current) this.showNext();
  }

  showNext() {
    this.current = this.queue.shift() || null;
    if (!this.current) { this.hide(); return; }
    const { request, file, url } = this.current;
    this.game.sharingVisible = true; this.root.hidden = false;
    this.game.resetInput(); this.game.account.cancelPointer(); this.game.rating.cancelPointer();
    this.engineCall(this.game.module, "_stella_share_preview", 1);
    this.image.src = url; this.download.href = url; this.download.download = request.filename;
    // File support is checked independently of title support. A URL/text-only
    // implementation must not silently share the page instead of the PNG.
    let supported = false;
    try { supported = typeof this.navigator?.share === "function" && !!this.navigator.canShare?.({ files: [file] }); } catch { /* Download remains available. */ }
    this.shareButton.hidden = !supported;
    this.error.hidden = true; this.current.error = null;
    this.setPending(false); this.translate(); this.refresh(); this.focus();
  }

  translate() {
    const title = this.current?.request.title || t("screenshotPreview");
    this.heading.textContent = title; this.root.setAttribute("aria-label", title); this.image.alt = title;
    this.shareButton.textContent = t("shareImage"); this.download.textContent = t("saveImage"); this.closeButton.textContent = t("closePreview");
    if (this.current?.error) this.error.textContent = t("shareImageFailed");
  }

  refresh() {
    if (!this.current) return;
    this.accountDialog.inert = true; this.ratingDialog.inert = true;
    if (this.game.gamerServices) this.game.gamerServices.root.inert = true;
    if (this.accountDialog.contains(this.document.activeElement) || this.ratingDialog.contains(this.document.activeElement)) this.focus();
  }

  focus() { (this.shareButton.hidden || this.current?.pending ? this.closeButton : this.shareButton).focus({ preventScroll: true }); }

  setPending(pending) {
    this.current.pending = pending;
    this.shareButton.disabled = pending; this.closeButton.disabled = pending;
    this.download.setAttribute("aria-disabled", String(pending)); this.download.tabIndex = pending ? -1 : 0;
  }

  async share() {
    const entry = this.current;
    if (!entry || entry.pending || this.shareButton.hidden || this.disposed) return;
    this.setPending(true); this.error.hidden = true;
    try {
      // No await before this call: Web Share requires the trusted click's
      // transient activation. Promise settlement does not close the preview.
      await this.navigator.share({ title: entry.request.title, files: [entry.file] });
    } catch (error) {
      if (!this.disposed && this.current === entry && error.name !== "AbortError") {
        entry.error = error; this.error.hidden = false; this.translate();
      }
    } finally {
      if (!this.disposed && this.current === entry) { this.setPending(false); this.focus(); }
    }
  }

  close() {
    if (!this.current || this.current.pending || this.disposed) return;
    this.url.revokeObjectURL(this.current.url); this.current = null;
    this.image.removeAttribute("src"); this.download.removeAttribute("href");
    this.showNext();
  }

  hide(restoreFocus = true) {
    const focused = this.root.contains(this.document.activeElement), wasVisible = this.game.sharingVisible;
    this.game.sharingVisible = false;
    this.accountDialog.inert = !!(this.game.ratingVisible || this.game.gamerServicesVisible); this.ratingDialog.inert = !!this.game.gamerServicesVisible;
    if (this.game.gamerServices) this.game.gamerServices.root.inert = false;
    if (wasVisible && !this.disposed) this.engineCall(this.game.module, "_stella_share_preview", 0);
    // Transfer owned focus before hiding to avoid a false application pause.
    if (focused && restoreFocus) {
      if (this.game.gamerServicesVisible) this.game.gamerServices.focus();
      else if (this.game.ratingVisible) this.game.rating.focus();
      else if (this.game.accountVisible) this.game.account.focus();
      else this.gameCanvas.focus({ preventScroll: true });
    }
    this.root.hidden = true;
  }

  dispose() {
    this.disposed = true; this.listeners.forEach(remove => remove());
    for (const entry of [...this.queue, this.current].filter(Boolean)) this.url.revokeObjectURL(entry.url);
    this.queue = []; this.current = null; this.hide(false); this.root.replaceChildren();
  }
}
