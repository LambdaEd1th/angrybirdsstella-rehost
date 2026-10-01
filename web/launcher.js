import { SaveStore, emptySave, importFiles, restoreSave, snapshotSave, clearVirtualSave, MAX_BACKUP_BYTES } from "./storage.js";
import { GameRenderer } from "./renderer.js";
import { GameAudio } from "./audio.js";

const $ = id => document.getElementById(id);
const basePath = new URL(".", import.meta.url).pathname;
let store;
let storageError = "";
try { store = new SaveStore(window.localStorage, basePath); }
catch { storageError = "浏览器未允许本地存储。游戏仍可运行，请在退出前导出存档。"; }
const sessionSaves = new Map();
let selected = 1;
let running = null;
let busy = false;
let cachedModule = null;

const displaySizeKey = `stella-rehost:display:v1:${basePath}:size`;
const sizeControl = $("game-size");
const displaySizes = new Set([...sizeControl.options].map(option => option.value));
let displaySize = "auto";
try {
  const saved = window.localStorage.getItem(displaySizeKey);
  if (displaySizes.has(saved)) displaySize = saved;
} catch { /* Display sizing remains available when storage is disabled. */ }
sizeControl.value = displaySize;

function updateGameSize() {
  const area = $("canvas-area"), stage = $("canvas-stage"), canvas = $("canvas");
  area.dataset.sizeMode = displaySize === "auto" ? "fit" : "scaled";
  const style = getComputedStyle(area);
  const availableWidth = area.clientWidth - parseFloat(style.paddingLeft) - parseFloat(style.paddingRight);
  const availableHeight = area.clientHeight - parseFloat(style.paddingTop) - parseFloat(style.paddingBottom);
  if (availableWidth <= 0 || availableHeight <= 0) return;
  const scale = displaySize === "auto"
    ? Math.min(availableWidth / canvas.width, availableHeight / canvas.height)
    : Number(displaySize) / 100;
  const width = canvas.width * scale, height = canvas.height * scale;
  canvas.style.width = `${width}px`; canvas.style.height = `${height}px`;
  // A stage at least as large as the visible area keeps smaller frames centered
  // and larger frames reachable from their top-left corner via scrollbars.
  stage.style.width = `${Math.max(availableWidth, width)}px`;
  stage.style.height = `${Math.max(availableHeight, height)}px`;
}

sizeControl.addEventListener("change", () => {
  displaySize = displaySizes.has(sizeControl.value) ? sizeControl.value : "auto";
  try { window.localStorage.setItem(displaySizeKey, displaySize); }
  catch { /* Applying the selected size does not depend on saving the preference. */ }
  $("canvas-area").scrollTo(0, 0);
  updateGameSize();
});
const sizeObserver = new ResizeObserver(updateGameSize);
sizeObserver.observe($("canvas-area"));

function status(message, error = false) {
  $("status").textContent = message;
  $("status").classList.toggle("error", error);
}
function readSlot(slot) { return sessionSaves.get(slot) ?? store?.read(slot) ?? null; }
function writeSlot(slot, save) {
  // Retain the latest snapshot even if a browser quota write fails, so Export
  // can recover that progress. localStorage.setItem atomically keeps old data.
  sessionSaves.set(slot, save);
  if (!store) throw new Error(storageError);
  store.write(slot, save);
}
function refresh() {
  const slots = $("slots"); slots.replaceChildren();
  for (let slot = 1; slot <= 3; slot++) {
    let save = null, error = "";
    try { save = readSlot(slot); } catch (value) { error = value.message; }
    const button = document.createElement("button");
    button.className = `slot${slot === selected ? " selected" : ""}`;
    button.setAttribute("aria-pressed", String(slot === selected)); button.disabled = busy;
    const head = document.createElement("span"); head.className = "slot-head";
    const number = document.createElement("span"); number.className = "slot-number"; number.textContent = `SAVE 0${slot}`;
    const check = document.createElement("span"); check.className = "slot-check"; check.textContent = "✓"; check.setAttribute("aria-hidden", "true");
    head.append(number, check);
    const title = document.createElement("span"); title.className = "slot-title";
    const icon = document.createElement("span"); icon.className = "slot-icon"; icon.textContent = save?.files.length ? "❋" : "+"; icon.setAttribute("aria-hidden", "true");
    title.append(icon, document.createTextNode(error ? "存档读取异常" : save?.files.length ? `存档 ${slot}` : "新存档"));
    const date = document.createElement("span"); date.className = "slot-date";
    date.textContent = error ? "导入备份以恢复" : save?.files.length
      ? new Intl.DateTimeFormat("zh-CN", { month: "short", day: "numeric", hour: "2-digit", minute: "2-digit" }).format(new Date(save.updatedAt)) + " · 已保存"
      : "空存档 · 尚无进度";
    button.append(head, title, date); button.addEventListener("click", () => { selected = slot; status(storageError, !!storageError); refresh(); });
    slots.append(button);
    if (slot === selected) {
      $("start").replaceChildren(document.createTextNode(save?.files.length ? "继续游戏 " : "开始游戏 "));
      const arrow = document.createElement("span"); arrow.textContent = "↗"; arrow.setAttribute("aria-hidden", "true"); $("start").append(arrow);
      $("start").disabled = busy || !!error;
      $("export").disabled = busy || !save?.files.length;
      $("reset").disabled = busy || (!save && !error);
    }
  }
  $("import").disabled = busy;
}

async function confirmAction(title, message) {
  $("confirm-title").textContent = title; $("confirm-message").textContent = message;
  const dialog = $("confirm-dialog"); dialog.returnValue = "cancel";
  dialog.showModal();
  return new Promise(resolve => dialog.addEventListener("close", () => resolve(dialog.returnValue === "confirm"), { once: true }));
}

async function acquireSlot(slot) {
  if (!navigator.locks) return () => {};
  let release;
  const acquired = new Promise((resolve, reject) => {
    navigator.locks.request(`stella-save:${basePath}:${slot}`, { ifAvailable: true }, async lock => {
      if (!lock) { reject(new Error("这个存档正在另一个标签页使用，请先在那里保存并返回。")); return; }
      await new Promise(done => { release = done; resolve(); });
    }).catch(reject);
  });
  await acquired;
  return () => release();
}

async function receiveFiles(files) {
  if (busy || running || !files.length) return;
  const slot = selected;
  let release = () => {};
  try {
    const save = await importFiles(files);
    let occupied = false;
    try { occupied = !!readSlot(slot); } catch { occupied = true; }
    if (occupied && !await confirmAction("替换当前存档？", `导入的文件将替换存档 ${slot}。建议先导出当前存档作为备份。`)) return;
    release = await acquireSlot(slot); writeSlot(slot, save);
    status(`已导入到存档 ${slot}。点击“继续游戏”即可开始。`);
  } catch (error) { status(error.message, true); }
  finally { release(); refresh(); $("save-file").value = ""; }
}

function downloadSave(save, slot) {
  if (!save?.files.length) throw new Error("当前存档还没有可导出的进度。");
  const blob = new Blob([JSON.stringify(save, null, 2) + "\n"], { type: "application/json" });
  if (blob.size > MAX_BACKUP_BYTES) throw new Error("存档备份过大。");
  const url = URL.createObjectURL(blob); const link = document.createElement("a");
  link.href = url; link.download = `stella-save-${slot}-${new Date().toISOString().replace(/[:.]/g, "-")}.json`;
  document.body.append(link); link.click(); link.remove(); setTimeout(() => URL.revokeObjectURL(url), 1000);
}

$("import").addEventListener("click", () => $("save-file").click());
$("save-file").addEventListener("change", event => receiveFiles([...event.target.files]));
$("export").addEventListener("click", () => { try { downloadSave(readSlot(selected), selected); status("存档备份已导出。"); } catch (error) { status(error.message, true); } });
$("reset").addEventListener("click", async () => {
  const slot = selected;
  if (!await confirmAction("清空这个存档？", `存档 ${slot} 将被清空。请先导出备份；其他存档槽不受影响。`)) return;
  let release = () => {};
  try { release = await acquireSlot(slot); store?.clear(slot); sessionSaves.delete(slot); status("存档槽已清空，可以重新开始。"); }
  catch (error) { status(error.message, true); }
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
}

function persist(game, flush = true) {
  let flushError = null;
  if (flush) {
    try { engineCall(game.module, "_stella_save"); } catch (error) { flushError = error; }
  }
  const save = snapshotSave(game.module.FS, game.save);
  game.save = save;
  writeSlot(game.slot, save);
  $("game-save-status").textContent = `存档 ${game.slot} · 已保存 ${new Date().toLocaleTimeString("zh-CN", { hour: "2-digit", minute: "2-digit" })}`;
  game.saveFailure = "";
  if (flushError) throw new Error(`已备份现有文件，但未能刷新游戏进度：${flushError.message}`);
  return save;
}

function saveOrReport(game, flush = true) {
  try { return persist(game, flush); }
  catch (error) {
    game.saveFailure = error.message;
    $("game-save-status").textContent = "未能保存 · 请导出备份";
    $("game-error").textContent = error.message; $("game-error").hidden = false;
    return game.save;
  }
}

function failGame(game, error) {
  if (running !== game) return;
  game.failed = true; cancelAnimationFrame(game.animation); game.audio.stop();
  saveOrReport(game);
  $("game-error").textContent = `游戏暂停：${error.message}。可以导出存档，或保存并返回后重试。`;
  $("game-error").hidden = false;
}

async function startGame() {
  if (busy || running) return;
  busy = true; refresh(); status(""); $("loading").hidden = false;
  $("loading-text").textContent = "正在下载游戏资源，首次加载可能需要一点时间…";
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
      const { default: createStella } = await import("./engine/stella_web.js");
      module = await createStella({
      noInitialRun: true,
      locateFile: name => new URL(`./engine/${name}`, import.meta.url).href,
      preRun: [module => restoreSave(module.FS, save)],
      print: message => console.info("[Stella]", message),
      printErr: message => console.warn("[Stella]", message),
      setStatus: message => {
        if (!message) return;
        const progress = message.match(/\((\d+)\/(\d+)\)/);
        if (progress) {
          const percent = Math.max(3, Math.min(100, Number(progress[1]) / Number(progress[2]) * 100));
          $("progress").style.width = `${percent}%`;
          $("loading-text").textContent = `正在下载游戏资源… ${Math.floor(percent)}%`;
        }
      },
      });
      cachedModule = module;
    }
    $("loading-text").textContent = "正在启动游戏…";
    // Give the status one paint before the synchronous Lua startup.
    await new Promise(resolve => requestAnimationFrame(() => requestAnimationFrame(resolve)));
    engineCall(module, "_stella_init");
    const game = { module, renderer, audio, slot, save, release, animation: 0, last: performance.now(), lastSave: performance.now(), failed: false, saveFailure: "", cleanup: null };
    running = game;
    $("launcher").hidden = true; $("game").hidden = false; $("game-error").hidden = true;
    updateGameSize();
    $("canvas").focus();
    game.cleanup = installInput(game);
    // Always save generated device identity, including a completely new game.
    saveOrReport(game, false);
    function frame(now) {
      if (running !== game || game.failed) return;
      try {
        if (document.hidden || $("account-dialog").open) { game.last = now; game.animation = requestAnimationFrame(frame); return; }
        engineCall(module, "_stella_frame", Math.min((now - game.last) / 1000, 0.1)); game.last = now;
        const packet = JSON.parse(module.UTF8ToString(module._stella_packet()));
        renderer.render(module, packet); audio.sync(module, packet.audio);
        if (packet.account !== null && !$("account-dialog").open) {
          engineCall(module, "_stella_active", 0); saveOrReport(game, false); $("account-dialog").showModal();
        }
        if (now - game.lastSave > 15000) { saveOrReport(game, false); game.lastSave = now; }
        if (packet.exit) { returnHome(); return; }
        game.animation = requestAnimationFrame(frame);
      } catch (error) { failGame(game, error); }
    }
    game.animation = requestAnimationFrame(frame);
  } catch (error) {
    renderer?.dispose(); audio?.dispose(); release();
    cachedModule?._stella_shutdown();
    status(`无法启动游戏：${error.message}。请确认网页构建包含 engine 下的 JS、WASM 和 data 文件。`, true);
  } finally { busy = false; $("loading").hidden = true; refresh(); }
}

function installInput(game) {
  const canvas = $("canvas"), listeners = [], pointers = new Map();
  let primary = null;
  function listen(target, event, handler, options) { target.addEventListener(event, handler, options); listeners.push(() => target.removeEventListener(event, handler, options)); }
  function input(operation) { if (game.failed) return; try { operation(); } catch (error) { failGame(game, error); } }
  function point(event) {
    const rect = canvas.getBoundingClientRect();
    return [(event.clientX - rect.left) * 1024 / rect.width, (event.clientY - rect.top) * 768 / rect.height];
  }
  function touches() {
    const values = [...pointers].slice(0, 2).map(([id, [x, y]]) => [id, Math.trunc(x), Math.trunc(y)]);
    input(() => engineCall(game.module, "_stella_touches", values.length, ...(values[0] ?? [0, 0, 0]), ...(values[1] ?? [0, 0, 0])));
  }
  listen(canvas, "pointerdown", event => {
    if (event.button !== 0) return;
    event.preventDefault(); canvas.focus(); canvas.setPointerCapture(event.pointerId);
    if (event.pointerType !== "mouse") pointers.set(event.pointerId, point(event));
    touches();
    if (primary === null) primary = event.pointerId;
    if (primary === event.pointerId) input(() => engineCall(game.module, "_stella_pointer", ...point(event), 1));
  });
  listen(canvas, "pointermove", event => {
    if (pointers.has(event.pointerId)) { pointers.set(event.pointerId, point(event)); touches(); }
    if (primary !== null && primary !== event.pointerId) return;
    input(() => engineCall(game.module, "_stella_pointer", ...point(event), primary === event.pointerId ? 1 : 0));
  });
  for (const type of ["pointerup", "pointercancel", "lostpointercapture"]) listen(canvas, type, event => {
    pointers.delete(event.pointerId);
    touches();
    if (primary !== event.pointerId) return;
    primary = null; input(() => engineCall(game.module, "_stella_pointer", ...point(event), 0));
  });
  listen(canvas, "contextmenu", event => event.preventDefault());
  listen(canvas, "wheel", event => { event.preventDefault(); input(() => engineCall(game.module, "_stella_wheel", event.deltaY < 0 ? 1 : -1, +event.shiftKey, +event.ctrlKey)); }, { passive: false });
  const keys = { Escape: 0, m: 1, M: 1, "+": 2, "-": 3 };
  for (const type of ["keydown", "keyup"]) listen(canvas, type, event => {
    if (Object.hasOwn(keys, event.key)) { event.preventDefault(); input(() => engineCall(game.module, "_stella_key", keys[event.key], +(type === "keydown"))); }
  });
  listen(canvas, "blur", () => { primary = null; pointers.clear(); input(() => engineCall(game.module, "_stella_active", 0)); saveOrReport(game, false); });
  listen(canvas, "focus", () => { input(() => engineCall(game.module, "_stella_active", 1)); game.last = performance.now(); });
  listen(document, "visibilitychange", () => {
    input(() => engineCall(game.module, "_stella_active", document.hidden ? 0 : 1));
    if (document.hidden) saveOrReport(game, false);
    game.last = performance.now();
  });
  listen(window, "pagehide", () => { saveOrReport(game); });
  listen(canvas, "webglcontextlost", event => { event.preventDefault(); failGame(game, new Error("浏览器图形上下文已丢失")); });
  return () => listeners.forEach(remove => remove());
}

function returnHome() {
  const game = running; if (!game) return;
  saveOrReport(game);
  cancelAnimationFrame(game.animation); game.cleanup?.(); game.audio.dispose(); game.renderer.dispose(); game.module._stella_shutdown(); game.release();
  running = null; $("game").hidden = true; $("launcher").hidden = false;
  if (document.fullscreenElement) document.exitFullscreen().catch(() => {});
  status(game.saveFailure || "进度已保存。下次可在当前浏览器继续游戏。", !!game.saveFailure);
  refresh(); $("start").focus();
}

$("start").addEventListener("click", startGame);
$("home").addEventListener("click", returnHome);
$("fullscreen").addEventListener("click", async () => {
  try { if (document.fullscreenElement) await document.exitFullscreen(); else await $("game").requestFullscreen(); $("canvas").focus(); }
  catch { $("game-save-status").textContent = "此浏览器暂不支持全屏"; }
});
$("backup-game").addEventListener("click", () => {
  if (!running) return;
  try { downloadSave(saveOrReport(running), running.slot); }
  catch (error) { $("game-error").textContent = error.message; $("game-error").hidden = false; }
  $("canvas").focus();
});
$("account-dialog").addEventListener("close", () => {
  if (!running) return;
  try { engineCall(running.module, "_stella_cancel_account"); engineCall(running.module, "_stella_active", 1); $("canvas").focus(); }
  catch (error) { failGame(running, error); }
});

refresh(); status(storageError, !!storageError);
