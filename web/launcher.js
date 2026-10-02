import { SaveStore, emptySave, restoreSave, snapshotSave, clearVirtualSave } from "./storage.js";
import { importFiles, createBackup } from "./backup.js";
import { GameRenderer } from "./renderer.js";
import { GameAudio } from "./audio.js";
import { installInput } from "./input.js";
import { BrowserAccountUI } from "./account.js";
import { ORIGINAL_SIZE, displayDimensions, drawableDimensions } from "./display.js";
import { LOCALES, configureLanguage, language, languageIndex, setLanguage, onLanguageChange, translateDocument, t, message, formatMessage, LocalizedError } from "./i18n.js";

const $ = id => document.getElementById(id);
document.documentElement.dataset.keyboardNavigation = "false";
document.addEventListener("keydown", event => { if (event.key === "Tab") document.documentElement.dataset.keyboardNavigation = "true"; }, true);
document.addEventListener("pointerdown", () => { document.documentElement.dataset.keyboardNavigation = "false"; }, true);
const basePath = new URL(".", import.meta.url).pathname;
// Filled by build.py so JavaScript, WebAssembly and preloaded data stay paired
// when an existing browser still caches the preceding Pages deployment.
const engineVersion = "__STELLA_ENGINE_VERSION__";
let store, browserStorage;
let storageError = "";
try { browserStorage = window.localStorage; store = new SaveStore(browserStorage, basePath); }
catch { storageError = new LocalizedError("storageDisabled"); }
configureLanguage({ storage: browserStorage, basePath, preferences: navigator.languages });
const sessionSaves = new Map();
let selected = 1;
let running = null;
let busy = false;
let cachedModule = null;
function focusGame() { if (running?.accountVisible) running.account.focus(); else $("canvas").focus({ preventScroll: true }); }
const localizedMessages = new Map();
function setText(id, value) { localizedMessages.set(id, value); $(id).textContent = formatMessage(value); }
const languageControls = [$("language"), $("game-language")];
for (const control of languageControls) {
  for (const locale of LOCALES) {
    const option = document.createElement("option"); option.value = locale.id; option.textContent = locale.name; option.lang = locale.id;
    control.append(option);
  }
  control.addEventListener("change", () => {
    try {
      if (running) engineCall(running.module, "_stella_set_locale", LOCALES.findIndex(locale => locale.id === control.value));
      setLanguage(control.value);
    } catch (error) { control.value = language(); status(error, true); }
    finally { if (running) focusGame(); }
  });
}
function refreshLanguage() {
  translateDocument();
  for (const control of languageControls) control.value = language();
  for (const [id, value] of localizedMessages) $(id).textContent = formatMessage(value);
  refresh(); updateGameSize();
}
onLanguageChange(refreshLanguage);

const displaySizeKey = `stella-rehost:display:v1:${basePath}:size`;
const customSizeKey = `stella-rehost:display:v1:${basePath}:dimensions`;
const sizeControl = $("game-size");
const widthControl = $("game-width"), heightControl = $("game-height");
const displaySizes = new Set([...sizeControl.options].map(option => option.value));
let displaySize = "auto";
let customSize = { ...ORIGINAL_SIZE };
let devicePixels = null;
function validDimension(value) { return Number.isInteger(value) && value >= 256 && value <= 8192; }
try {
  const saved = window.localStorage.getItem(displaySizeKey);
  if (displaySizes.has(saved)) displaySize = saved;
  const dimensions = JSON.parse(window.localStorage.getItem(customSizeKey));
  if (dimensions && validDimension(dimensions.width) && validDimension(dimensions.height)) customSize = dimensions;
} catch { /* Display sizing remains available when storage is disabled. */ }
sizeControl.value = displaySize;
widthControl.value = customSize.width; heightControl.value = customSize.height;

function targetResolution(renderer) {
  const rect = $("canvas").getBoundingClientRect();
  const pixelRatio = window.devicePixelRatio;
  const measured = devicePixels && devicePixels.cssWidth === rect.width && devicePixels.cssHeight === rect.height && devicePixels.ratio === pixelRatio
    ? devicePixels : undefined;
  return drawableDimensions(rect.width, rect.height, pixelRatio, renderer.drawableLimits, measured);
}

function syncGameResolution(game) {
  if (game.failed) return;
  const resolution = targetResolution(game.renderer);
  if (resolution.width === game.resolution.width && resolution.height === game.resolution.height) return;
  flushGameRendering(game);
  game.renderer.resize(resolution.width, resolution.height);
  engineCall(game.module, "_stella_resize", resolution.width, resolution.height);
  game.resolution = resolution;
}

function updateGameSize() {
  $("custom-game-size").hidden = displaySize !== "custom";
  if ($("game").hidden) return;
  const area = $("canvas-area"), stage = $("canvas-stage"), canvas = $("canvas");
  area.dataset.sizeMode = displaySize === "auto" ? "fit" : "sized";
  const style = getComputedStyle(area);
  const availableWidth = area.clientWidth - parseFloat(style.paddingLeft) - parseFloat(style.paddingRight);
  const availableHeight = area.clientHeight - parseFloat(style.paddingTop) - parseFloat(style.paddingBottom);
  if (availableWidth <= 0 || availableHeight <= 0) return;
  const { width, height } = displayDimensions(displaySize, availableWidth, availableHeight, customSize);
  canvas.style.width = `${width}px`; canvas.style.height = `${height}px`;
  // A stage at least as large as the visible area keeps smaller frames centered
  // and larger frames reachable from their top-left corner via scrollbars.
  stage.style.width = `${Math.max(availableWidth, width)}px`;
  stage.style.height = `${Math.max(availableHeight, height)}px`;
  if (running) {
    try { syncGameResolution(running); } catch (error) { failGame(running, error); }
  }
}

sizeControl.addEventListener("change", () => {
  displaySize = displaySizes.has(sizeControl.value) ? sizeControl.value : "auto";
  try { window.localStorage.setItem(displaySizeKey, displaySize); }
  catch { /* Applying the selected size does not depend on saving the preference. */ }
  $("canvas-area").scrollTo(0, 0);
  updateGameSize();
  if (running) focusGame();
});
function applyCustomSize() {
  const width = widthControl.valueAsNumber, height = heightControl.valueAsNumber;
  if (validDimension(width) && validDimension(height)) {
    customSize = { width, height };
    try { window.localStorage.setItem(customSizeKey, JSON.stringify(customSize)); } catch { /* Optional preference. */ }
    updateGameSize();
    return true;
  }
  return false;
}
for (const control of [widthControl, heightControl]) {
  control.addEventListener("input", applyCustomSize);
  control.addEventListener("change", () => {
    if (!applyCustomSize()) { widthControl.value = customSize.width; heightControl.value = customSize.height; }
  });
}
const sizeObserver = new ResizeObserver(updateGameSize);
sizeObserver.observe($("canvas-area"));
// Prefer the browser's measured physical pixels, including fractional desktop
// scaling. DPR remains the fallback on browsers without this observation box.
const pixelObserver = new ResizeObserver(entries => {
  const entry = entries[0], box = entry.devicePixelContentBoxSize?.[0];
  if (!box || !entry.contentRect.width || !entry.contentRect.height) return;
  devicePixels = { width: box.inlineSize, height: box.blockSize, cssWidth: entry.contentRect.width, cssHeight: entry.contentRect.height, ratio: window.devicePixelRatio };
  if (running) {
    try { syncGameResolution(running); } catch (error) { failGame(running, error); }
  }
});
try { pixelObserver.observe($("canvas"), { box: "device-pixel-content-box" }); }
catch { pixelObserver.observe($("canvas")); }
window.addEventListener("resize", updateGameSize);
function watchPixelRatio() {
  const query = window.matchMedia(`(resolution: ${window.devicePixelRatio}dppx)`);
  query.addEventListener("change", () => { devicePixels = null; updateGameSize(); watchPixelRatio(); }, { once: true });
}
watchPixelRatio();

function status(message, error = false) {
  setText("status", message);
  $("status").classList.toggle("error", error);
}
function readSlot(slot) { return sessionSaves.get(slot) ?? store?.read(slot) ?? null; }
function writeSlot(slot, save) {
  // Retain the latest snapshot even if a browser quota write fails, so Export
  // can recover that progress. localStorage.setItem atomically keeps old data.
  sessionSaves.set(slot, save);
  if (!store) throw storageError;
  store.write(slot, save);
}
function refresh() {
  const slots = $("slots"); slots.replaceChildren();
  for (let slot = 1; slot <= 3; slot++) {
    let save = null, error = "";
    try { save = readSlot(slot); } catch (value) { error = value; }
    const button = document.createElement("button");
    button.className = `slot${slot === selected ? " selected" : ""}`;
    button.setAttribute("aria-pressed", String(slot === selected)); button.disabled = busy;
    const head = document.createElement("span"); head.className = "slot-head";
    const number = document.createElement("span"); number.className = "slot-number"; number.textContent = t("slotNumber", { slot: `0${slot}` });
    const check = document.createElement("span"); check.className = "slot-check"; check.textContent = "✓"; check.setAttribute("aria-hidden", "true");
    head.append(number, check);
    const title = document.createElement("span"); title.className = "slot-title";
    const icon = document.createElement("span"); icon.className = "slot-icon"; icon.textContent = save?.files.length ? "❋" : "+"; icon.setAttribute("aria-hidden", "true");
    title.append(icon, document.createTextNode(error ? t("slotCorrupt") : save?.files.length ? t("slotTitle", { slot }) : t("newSave")));
    const date = document.createElement("span"); date.className = "slot-date";
    date.textContent = error ? t("restoreHint") : save?.files.length
      ? t("savedDate", { date: new Date(save.updatedAt) }) : t("emptySlot");
    button.append(head, title, date); button.addEventListener("click", () => { selected = slot; status(storageError, !!storageError); refresh(); });
    slots.append(button);
    if (slot === selected) {
      $("start").replaceChildren(document.createTextNode(t(save?.files.length ? "continue" : "start") + " "));
      const arrow = document.createElement("span"); arrow.textContent = "↗"; arrow.setAttribute("aria-hidden", "true"); $("start").append(arrow);
      $("start").disabled = busy || !!error;
      $("export").disabled = busy || !save?.files.length;
      $("reset").disabled = busy || (!save && !error);
    }
  }
  $("import").disabled = busy;
  for (const control of languageControls) control.disabled = busy;
}

async function confirmAction(title, message) {
  setText("confirm-title", title); setText("confirm-message", message);
  const dialog = $("confirm-dialog"); dialog.returnValue = "cancel";
  dialog.showModal();
  return new Promise(resolve => dialog.addEventListener("close", () => resolve(dialog.returnValue === "confirm"), { once: true }));
}

async function acquireSlot(slot) {
  if (!navigator.locks) return () => {};
  let release;
  const acquired = new Promise((resolve, reject) => {
    navigator.locks.request(`stella-save:${basePath}:${slot}`, { ifAvailable: true }, async lock => {
      if (!lock) { reject(new LocalizedError("slotBusy")); return; }
      await new Promise(done => { release = done; resolve(); });
    }).catch(reject);
  });
  await acquired;
  return () => release();
}

async function receiveFiles(files) {
  if (busy || running || !files.length) return;
  busy = true; refresh();
  const slot = selected;
  let release = () => {};
  try {
    const save = await importFiles(files);
    let occupied = false;
    try { occupied = !!readSlot(slot); } catch { occupied = true; }
    if (occupied && !await confirmAction(message("replaceTitle"), message("replaceText", { slot }))) return;
    release = await acquireSlot(slot); writeSlot(slot, save);
    status(message("imported", { slot }));
  } catch (error) { status(error, true); }
  finally { release(); busy = false; refresh(); $("save-file").value = ""; }
}

function downloadSave(save, slot) {
  if (!save?.files.length) throw new LocalizedError("noProgress");
  const blob = new Blob([createBackup(save)], { type: "application/zip" });
  const url = URL.createObjectURL(blob); const link = document.createElement("a");
  link.href = url; link.download = `stella-save-${slot}-${new Date().toISOString().replace(/[:.]/g, "-")}.zip`;
  document.body.append(link); link.click(); link.remove(); setTimeout(() => URL.revokeObjectURL(url), 1000);
}

$("import").addEventListener("click", () => $("save-file").click());
$("save-file").addEventListener("change", event => receiveFiles([...event.target.files]));
$("export").addEventListener("click", () => { try { downloadSave(readSlot(selected), selected); status(message("exported")); } catch (error) { status(error, true); } });
$("reset").addEventListener("click", async () => {
  const slot = selected;
  if (!await confirmAction(message("clearTitle"), message("clearText", { slot }))) return;
  let release = () => {};
  try { release = await acquireSlot(slot); store?.clear(slot); sessionSaves.delete(slot); status(message("cleared")); }
  catch (error) { status(error, true); }
  finally { release(); refresh(); }
});
for (const type of ["dragenter", "dragover"]) document.addEventListener(type, event => {
  if (running || busy || !event.dataTransfer.types.includes("Files")) return;
  event.preventDefault(); $("launcher").classList.add("dragging");
});
document.addEventListener("dragleave", event => { if (!event.relatedTarget) $("launcher").classList.remove("dragging"); });
document.addEventListener("drop", event => {
  event.preventDefault(); $("launcher").classList.remove("dragging"); receiveFiles([...event.dataTransfer.files]);
});
window.addEventListener("storage", event => {
  if (running || !store) return;
  if (event.key === null) { sessionSaves.clear(); refresh(); return; }
  if (event.key.startsWith(store.prefix)) {
    const slot = Number(event.key.slice(event.key.lastIndexOf(":") + 1));
    sessionSaves.delete(slot); refresh();
  }
});

function engineCall(module, name, ...args) {
  if (module[name](...args) !== 0) throw new Error(module.UTF8ToString(module._stella_error()));
  // Native platform/Lua callbacks may draw while the display link is paused.
  if (running?.module === module && !running.failed && ["_stella_active", "_stella_save", "_stella_cancel_account", "_stella_set_locale"].includes(name)) {
    flushGameRendering(running);
  }
}

function flushGameRendering(game) {
  engineCall(game.module, "_stella_flush");
  game.renderer.render(game.module, JSON.parse(game.module.UTF8ToString(game.module._stella_packet())));
}

function persist(game, flush = true) {
  let flushError = null;
  if (flush) {
    try { engineCall(game.module, "_stella_save"); } catch (error) { flushError = error; }
  }
  const save = snapshotSave(game.module.FS, game.save);
  game.save = save;
  writeSlot(game.slot, save);
  setText("game-save-status", message("savedStatus", { slot: game.slot, time: new Date() }));
  game.saveFailure = "";
  if (flushError) throw new LocalizedError("flushFailed", { error: flushError });
  return save;
}

function saveOrReport(game, flush = true) {
  try { return persist(game, flush); }
  catch (error) {
    game.saveFailure = error;
    setText("game-save-status", message("saveFailed"));
    setText("game-error", error); $("game-error").hidden = false;
    return game.save;
  }
}

function failGame(game, error) {
  if (running !== game) return;
  game.failed = true; cancelAnimationFrame(game.animation); game.audio.setActive(false); game.audio.stop();
  saveOrReport(game);
  setText("game-error", message("gamePaused", { error }));
  $("game-error").hidden = false;
}

async function startGame() {
  if (busy || running) return;
  busy = true; refresh(); status(""); $("loading").hidden = false;
  setText("loading-text", message("downloading"));
  const slot = selected;
  let release = () => {}, renderer, audio;
  try {
    release = await acquireSlot(slot);
    const save = readSlot(slot) ?? emptySave();
    renderer = new GameRenderer($("canvas")); audio = new GameAudio();
    let module = cachedModule;
    if (module) {
      clearVirtualSave(module.FS); restoreSave(module.FS, save);
    } else {
      const { default: createStella } = await import(`./engine/stella_web.js?v=${engineVersion}`);
      module = await createStella({
      noInitialRun: true,
      locateFile: name => new URL(`./engine/${name}?v=${engineVersion}`, import.meta.url).href,
      preRun: [module => restoreSave(module.FS, save)],
      print: message => console.info("[Stella]", message),
      printErr: message => console.warn("[Stella]", message),
      setStatus: rawStatus => {
        if (!rawStatus) return;
        const progress = rawStatus.match(/\((\d+)\/(\d+)\)/);
        if (progress) {
          const percent = Math.max(3, Math.min(100, Number(progress[1]) / Number(progress[2]) * 100));
          $("progress").style.width = `${percent}%`;
          setText("loading-text", message("downloadPercent", { percent: Math.floor(percent) }));
        }
      },
      });
      cachedModule = module;
    }
    setText("loading-text", message("starting"));
    // Give the status one paint before the synchronous Lua startup.
    await new Promise(resolve => requestAnimationFrame(() => requestAnimationFrame(resolve)));
    $("launcher").hidden = true; $("game").hidden = false; $("game-error").hidden = true;
    updateGameSize();
    const resolution = targetResolution(renderer);
    renderer.resize(resolution.width, resolution.height);
    engineCall(module, "_stella_set_locale", languageIndex());
    engineCall(module, "_stella_init", resolution.width, resolution.height);
    const game = { module, renderer, resolution, audio, slot, save, release, animation: 0, last: performance.now(), lastSave: performance.now(), failed: false, saveFailure: "", cleanup: null };
    running = game;
    game.account = new BrowserAccountUI(game, { root: $("account-dialog"), gameCanvas: $("canvas"), engineCall, failGame, saveOrReport });
    $("canvas").focus();
    game.cleanup = installInput(game, { canvas: $("canvas"), accountDialog: $("account-dialog"), engineCall, failGame, saveOrReport });
    // Always save generated device identity, including a completely new game.
    saveOrReport(game, false);
    function frame(now) {
      if (running !== game || game.failed) return;
      try {
        if (!game.lifecycle.active) { game.account.refresh(now); game.last = now; game.animation = requestAnimationFrame(frame); return; }
        syncGameResolution(game);
        engineCall(module, "_stella_frame", Math.min((now - game.last) / 1000, 0.1)); game.last = now;
        const packet = JSON.parse(module.UTF8ToString(module._stella_packet()));
        renderer.render(module, packet); audio.sync(module, packet.audio);
        game.account.refresh(now);
        if (now - game.lastSave > 15000) { saveOrReport(game, false); game.lastSave = now; }
        if (packet.exit) { returnHome(); return; }
        game.animation = requestAnimationFrame(frame);
      } catch (error) { failGame(game, error); }
    }
    game.animation = requestAnimationFrame(frame);
  } catch (error) {
    renderer?.dispose(); audio?.dispose(); release();
    cachedModule?._stella_shutdown();
    running = null; $("game").hidden = true; $("launcher").hidden = false;
    status(message("startFailed", { error }), true);
  } finally { busy = false; $("loading").hidden = true; refresh(); }
}


function returnHome() {
  const game = running; if (!game) return;
  saveOrReport(game);
  cancelAnimationFrame(game.animation); game.cleanup?.(); game.account.dispose(); game.audio.dispose(); game.renderer.dispose(); game.module._stella_shutdown(); game.release();
  running = null; $("game").hidden = true; $("launcher").hidden = false;
  if (document.fullscreenElement) document.exitFullscreen().catch(() => {});
  status(game.saveFailure || message("savedHome"), !!game.saveFailure);
  refresh(); $("start").focus();
}

$("start").addEventListener("click", startGame);
$("home").addEventListener("click", returnHome);
$("fullscreen").addEventListener("click", async () => {
  try { if (document.fullscreenElement) await document.exitFullscreen(); else await $("game").requestFullscreen(); focusGame(); }
  catch { setText("game-save-status", message("fullscreenUnavailable")); }
});
$("backup-game").addEventListener("click", () => {
  if (!running) return;
  try { downloadSave(saveOrReport(running), running.slot); }
  catch (error) { setText("game-error", error); $("game-error").hidden = false; }
  focusGame();
});
refreshLanguage(); status(storageError, !!storageError);
