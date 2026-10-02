import { Inflate, strFromU8, zipSync } from "./vendor/fflate.js";
import { emptySave, validateSave, safePath, encodeBytes, decodeBytes, MAX_SAVE_BYTES, MAX_BACKUP_BYTES } from "./storage.js";
import { LocalizedError } from "./i18n.js";

const ROOT = "appdata/";
const MAX_FILES = 2000;
const crcTable = new Uint32Array(256);
for (let index = 0; index < crcTable.length; index++) {
  let value = index;
  for (let bit = 0; bit < 8; bit++) value = (value >>> 1) ^ (value & 1 ? 0xedb88320 : 0);
  crcTable[index] = value;
}
function crc32(bytes) {
  let value = 0xffffffff;
  for (const byte of bytes) value = crcTable[(value ^ byte) & 255] ^ (value >>> 8);
  return (value ^ 0xffffffff) >>> 0;
}
function invalidZIP() { throw new LocalizedError("invalidZIP"); }

// Inspect the central directory before allocating any decompressed data.
// fflate supplies the codec; the host checks paths, duplicates and checksums.
function inspectArchive(bytes) {
  const view = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength);
  function range(offset, length, end = bytes.length) {
    if (offset < 0 || length < 0 || offset + length > end) invalidZIP();
  }
  const u16 = offset => { range(offset, 2); return view.getUint16(offset, true); };
  const u32 = offset => { range(offset, 4); return view.getUint32(offset, true); };
  let footer = bytes.length - 22;
  const earliest = Math.max(0, footer - 65535);
  for (; footer >= earliest; footer--) {
    if (u32(footer) === 0x06054b50 && footer + 22 + u16(footer + 20) === bytes.length) break;
  }
  if (footer < earliest) invalidZIP();
  const count = u16(footer + 10), start = u32(footer + 16), size = u32(footer + 12);
  // A save fits the ordinary, single-disk ZIP format; ZIP64/encryption and
  // other compression methods are not accepted by this browser host.
  if (u16(footer + 4) || u16(footer + 6) || u16(footer + 8) !== count
      || count > MAX_FILES * 2 + 1 || start + size !== footer) invalidZIP();
  const paths = new Map(), files = [], occupied = [];
  let offset = start, total = 0;
  for (let index = 0; index < count; index++) {
    range(offset, 46, footer);
    if (u32(offset) !== 0x02014b50) invalidZIP();
    const flags = u16(offset + 8), method = u16(offset + 10), checksum = u32(offset + 16);
    const compressed = u32(offset + 20), original = u32(offset + 24);
    const nameLength = u16(offset + 28), extraLength = u16(offset + 30), commentLength = u16(offset + 32);
    const local = u32(offset + 42);
    range(offset + 46, nameLength + extraLength + commentLength, footer);
    if ((flags & 0x2041) || ![0, 8].includes(method) || u16(offset + 34)
        || ((u32(offset + 38) >>> 16) & 0xf000) === 0xa000) invalidZIP();
    const rawName = bytes.subarray(offset + 46, offset + 46 + nameLength);
    const name = strFromU8(rawName, !(flags & 0x800));
    if (!name.startsWith(ROOT)) throw new LocalizedError("archiveRoot");
    const directory = name.endsWith("/");
    const path = name.slice(ROOT.length).replace(/\/$/, "");
    if (name !== ROOT) safePath(path);
    if (paths.has(path)) throw new LocalizedError("duplicatePath");
    paths.set(path, directory);
    if (directory && (original || checksum)) invalidZIP();
    if (!directory) {
      total += original;
      if (total > MAX_SAVE_BYTES) throw new LocalizedError("saveTooLarge");
      if (files.length >= MAX_FILES) invalidZIP();
    }
    range(local, 30, start);
    if (u32(local) !== 0x04034b50 || u16(local + 6) !== flags || u16(local + 8) !== method
        || u16(local + 26) !== nameLength) invalidZIP();
    const body = local + 30 + nameLength + u16(local + 28);
    range(local + 30, nameLength + u16(local + 28), start);
    range(body, compressed, start);
    for (let byte = 0; byte < nameLength; byte++) {
      if (bytes[local + 30 + byte] !== rawName[byte]) invalidZIP();
    }
    if (!(flags & 8) && (u32(local + 14) !== checksum || u32(local + 18) !== compressed || u32(local + 22) !== original)) invalidZIP();
    if (method === 0 && compressed !== original) invalidZIP();
    occupied.push([local, body + compressed]);
    if (!directory) files.push({ path, method, checksum, compressed, original, body });
    offset += 46 + nameLength + extraLength + commentLength;
  }
  if (offset !== footer) invalidZIP();
  occupied.sort((first, second) => first[0] - second[0]);
  for (let index = 1; index < occupied.length; index++) {
    if (occupied[index][0] < occupied[index - 1][1]) invalidZIP();
  }
  // A regular file cannot also be the parent directory of another entry.
  for (const path of paths.keys()) {
    const parts = path.split("/");
    for (let count = 1; count < parts.length; count++) {
      if (paths.get(parts.slice(0, count).join("/")) === false) throw new LocalizedError("duplicatePath");
    }
  }
  if (!files.length) throw new LocalizedError("emptyArchive");
  return files;
}

function extractFile(archive, entry) {
  let result;
  if (entry.method === 0) result = archive.slice(entry.body, entry.body + entry.compressed);
  else {
    result = new Uint8Array(entry.original);
    let written = 0;
    const inflater = new Inflate(chunk => {
      if (written + chunk.length > result.length) invalidZIP();
      result.set(chunk, written); written += chunk.length;
    });
    if (!entry.compressed) invalidZIP();
    // Small chunks bound temporary allocations even when a corrupt stream
    // claims a small size but expands beyond the declared save capacity.
    const end = entry.body + entry.compressed;
    for (let offset = entry.body; offset < end; offset += 1024) {
      const next = Math.min(offset + 1024, end);
      inflater.push(archive.subarray(offset, next), next === end);
    }
    if (written !== entry.original) invalidZIP();
  }
  if (result.length !== entry.original || crc32(result) !== entry.checksum) invalidZIP();
  return result;
}

export async function importFiles(files) {
  if (!files.length) throw new LocalizedError("noFiles");
  if (files.length !== 1 || !files[0].name.toLowerCase().endsWith(".zip")) throw new LocalizedError("zipOnly");
  if (files[0].size > MAX_BACKUP_BYTES) throw new LocalizedError("filesTooLarge");
  const archive = new Uint8Array(await files[0].arrayBuffer());
  if (archive.length > MAX_BACKUP_BYTES) throw new LocalizedError("filesTooLarge");
  try {
    const entries = inspectArchive(archive);
    const save = emptySave();
    save.files = entries.map(entry => ({ path: entry.path, data: encodeBytes(extractFile(archive, entry)) }));
    return validateSave(save);
  } catch (error) {
    if (error instanceof LocalizedError) throw error;
    invalidZIP();
  }
}

export function createBackup(value) {
  const save = validateSave(value);
  if (!save.files.length) throw new LocalizedError("noProgress");
  const files = { [ROOT]: new Uint8Array(0) };
  for (const file of [...save.files].sort((first, second) => first.path < second.path ? -1 : first.path > second.path ? 1 : 0)) {
    files[ROOT + file.path] = decodeBytes(file.data);
  }
  const archive = zipSync(files, { level: 6 });
  if (archive.length > MAX_BACKUP_BYTES) throw new LocalizedError("backupTooLarge");
  return archive;
}
