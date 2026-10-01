export const SAVE_FORMAT = "stella-rehost-save";
export const MAX_SAVE_BYTES = 3 * 1024 * 1024;
export const MAX_BACKUP_BYTES = 5 * 1024 * 1024;
export const SAVE_ROOT = "/runtime/appdata";

export function safePath(path) {
  if (typeof path !== "string" || !path || path.length > 240 || /[\\\x00-\x1f\x7f]/.test(path)
      || path.split("/").some(part => !part || part === "." || part === "..")) {
    throw new Error("存档含有无效的文件路径。");
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
    throw new Error("存档的文件数据不是有效的 Base64。");
  }
  return Uint8Array.from(atob(data), character => character.charCodeAt(0));
}

export function emptySave() {
  const now = new Date().toISOString();
  return { format: SAVE_FORMAT, version: 1, createdAt: now, updatedAt: now, files: [] };
}

export function validateSave(value) {
  if (!value || value.format !== SAVE_FORMAT || value.version !== 1 || !Array.isArray(value.files)
      || value.files.length > 2000) throw new Error("请选择 Stella Rehost 导出的 v1 JSON 存档。");
  for (const field of ["createdAt", "updatedAt"]) {
    if (typeof value[field] !== "string" || !Number.isFinite(Date.parse(value[field]))) {
      throw new Error("存档的保存时间无效。");
    }
  }
  let bytes = 0;
  const paths = new Set();
  const files = value.files.map(file => {
    const path = safePath(file?.path);
    if (paths.has(path)) throw new Error("存档中存在重复的文件路径。");
    paths.add(path);
    bytes += decodeBytes(file.data).byteLength;
    if (bytes > MAX_SAVE_BYTES) throw new Error("存档超过 3 MB，无法保存到 localStorage。");
    return { path, data: file.data };
  });
  return { format: SAVE_FORMAT, version: 1, createdAt: value.createdAt, updatedAt: value.updatedAt, files };
}

export async function importFiles(files) {
  if (!files.length) throw new Error("尚未选择存档文件。");
  if (files.reduce((sum, file) => sum + file.size, 0) > MAX_BACKUP_BYTES) {
    throw new Error("所选存档文件过大。");
  }
  if (files.length === 1 && files[0].name.toLowerCase().endsWith(".json")) {
    const text = await files[0].text();
    let value;
    try { value = JSON.parse(text); } catch { throw new Error("JSON 文件无法解析。"); }
    if (value?.format === SAVE_FORMAT) return validateSave(value);
    // A desktop service JSON is also a legitimate individual AppData file.
    if (!/^stella-[a-z-]+\.json$/.test(files[0].name)) {
      throw new Error("这个 JSON 不是 Stella 存档备份。");
    }
  }
  const save = emptySave();
  for (const file of files) {
    const path = safePath(file.name);
    if (!/\.(lua|json|dat|bin|txt|plist|registry)$/i.test(path) && !/^stella-[a-z-]+(?:\.registry\.lock)?$/.test(path)) {
      throw new Error("请选择存档备份，或桌面版 appdata 中的存档文件。");
    }
    save.files.push({ path, data: encodeBytes(new Uint8Array(await file.arrayBuffer())) });
  }
  return validateSave(save);
}

export class SaveStore {
  constructor(storage, basePath) {
    this.storage = storage;
    // GitHub project Pages share an origin. Isolate each repository's saves.
    this.prefix = `stella-rehost:v1:${basePath}:`;
  }
  key(slot) {
    if (![1, 2, 3].includes(slot)) throw new Error("无效的存档槽。");
    return `${this.prefix}slot:${slot}`;
  }
  read(slot) {
    const value = this.storage.getItem(this.key(slot));
    if (value === null) return null;
    try { return validateSave(JSON.parse(value)); }
    catch { throw new Error(`存档 ${slot} 无法读取，请导入备份或清空此存档槽。`); }
  }
  write(slot, save) {
    const value = JSON.stringify(validateSave(save));
    try { this.storage.setItem(this.key(slot), value); }
    catch { throw new Error("浏览器未能保存存档（空间不足或存储权限受限）。原存档已保留，请立即导出备份。"); }
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
      if (size > MAX_SAVE_BYTES) throw new Error("存档超过 3 MB，请先导出桌面版存档备份。");
      save.files.push({ path: logical, data: encodeBytes(bytes) });
    }
  }
  visit(SAVE_ROOT);
  return validateSave(save);
}
