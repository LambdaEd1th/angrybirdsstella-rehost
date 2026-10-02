import { LocalizedError } from "./i18n.js";

export const SAVE_FORMAT = "stella-rehost-save";
export const MAX_SAVE_BYTES = 3 * 1024 * 1024;
export const MAX_BACKUP_BYTES = 5 * 1024 * 1024;
export const SAVE_ROOT = "/runtime/appdata";

export function safePath(path) {
  if (typeof path !== "string" || !path || path.length > 240 || /[\\\x00-\x1f\x7f]/.test(path)
      || path.split("/").some(part => !part || part === "." || part === "..")) {
    throw new LocalizedError("invalidPath");
  }
  return path;
}

export function encodeBytes(bytes) {
  let binary = "";
  for (let index = 0; index < bytes.length; index += 8192) {
    binary += String.fromCharCode(...bytes.subarray(index, index + 8192));
  }
  return btoa(binary);
}

export function decodeBytes(data) {
  if (typeof data !== "string" || data.length > MAX_BACKUP_BYTES || data.length % 4 !== 0
      || !/^(?:[A-Za-z0-9+/]{4})*(?:[A-Za-z0-9+/]{2}==|[A-Za-z0-9+/]{3}=)?$/.test(data)) {
    throw new LocalizedError("invalidBase64");
  }
  return Uint8Array.from(atob(data), character => character.charCodeAt(0));
}

export function emptySave() {
  const now = new Date().toISOString();
  return { format: SAVE_FORMAT, version: 1, createdAt: now, updatedAt: now, files: [] };
}

export function validateSave(value) {
  if (!value || value.format !== SAVE_FORMAT || value.version !== 1 || !Array.isArray(value.files)
      || value.files.length > 2000) throw new LocalizedError("invalidFormat");
  for (const field of ["createdAt", "updatedAt"]) {
    if (typeof value[field] !== "string" || !Number.isFinite(Date.parse(value[field]))) {
      throw new LocalizedError("invalidTime");
    }
  }
  let bytes = 0;
  const paths = new Set();
  const files = value.files.map(file => {
    const path = safePath(file?.path);
    if (paths.has(path)) throw new LocalizedError("duplicatePath");
    paths.add(path);
    bytes += decodeBytes(file.data).byteLength;
    if (bytes > MAX_SAVE_BYTES) throw new LocalizedError("saveTooLarge");
    return { path, data: file.data };
  });
  return { format: SAVE_FORMAT, version: 1, createdAt: value.createdAt, updatedAt: value.updatedAt, files };
}

export class SaveStore {
  constructor(storage, basePath) {
    this.storage = storage;
    // GitHub project Pages share an origin. Isolate each repository's saves.
    this.prefix = `stella-rehost:v1:${basePath}:`;
  }
  key(slot) {
    if (![1, 2, 3].includes(slot)) throw new LocalizedError("invalidSlot");
    return `${this.prefix}slot:${slot}`;
  }
  read(slot) {
    const key = this.key(slot);
    try {
      const value = this.storage.getItem(key);
      return value === null ? null : validateSave(JSON.parse(value));
    }
    catch { throw new LocalizedError("readError", { slot }); }
  }
  write(slot, save) {
    const value = JSON.stringify(validateSave(save));
    try { this.storage.setItem(this.key(slot), value); }
    catch { throw new LocalizedError("writeError"); }
  }
  clear(slot) { this.storage.removeItem(this.key(slot)); }
}

export function restoreSave(FS, save) {
  FS.mkdirTree(SAVE_ROOT);
  if (!save) return;
  for (const file of validateSave(save).files) {
    const path = `${SAVE_ROOT}/${file.path}`;
    FS.mkdirTree(path.slice(0, path.lastIndexOf("/")));
    FS.writeFile(path, decodeBytes(file.data));
  }
}

export function clearVirtualSave(FS) {
  function remove(directory) {
    for (const name of FS.readdir(directory)) {
      if (name === "." || name === "..") continue;
      const path = `${directory}/${name}`;
      if (FS.isDir(FS.lstat(path).mode)) { remove(path); FS.rmdir(path); }
      else FS.unlink(path);
    }
  }
  FS.mkdirTree(SAVE_ROOT);
  remove(SAVE_ROOT);
}

export function snapshotSave(FS, previous) {
  const save = emptySave();
  save.createdAt = previous?.createdAt ?? save.createdAt;
  let size = 0;
  function visit(directory, relative = "") {
    for (const name of FS.readdir(directory).sort()) {
      if (name === "." || name === "..") continue;
      const path = `${directory}/${name}`;
      const logical = safePath(relative + name);
      const stat = FS.lstat(path);
      if (FS.isDir(stat.mode)) { visit(path, logical + "/"); continue; }
      if (!FS.isFile(stat.mode)) continue;
      const bytes = FS.readFile(path);
      size += bytes.byteLength;
      if (size > MAX_SAVE_BYTES) throw new LocalizedError("snapshotTooLarge");
      save.files.push({ path: logical, data: encodeBytes(bytes) });
    }
  }
  visit(SAVE_ROOT);
  return validateSave(save);
}
