import { test } from "node:test";
import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { createBackup, importFiles } from "../backup.js";
import { emptySave, encodeBytes, decodeBytes, MAX_SAVE_BYTES, MAX_BACKUP_BYTES } from "../storage.js";
import { zipSync, unzipSync } from "../vendor/fflate.js";

const rawFiles = {
  "settings.lua": new Uint8Array([0, 255, 128, 1, 2]),
  "stella-device-id": new TextEncoder().encode("00112233-4455-4677-8899-AABBCCDDEEFF"),
  "nested/进度.bin": new Uint8Array([10, 0, 255, 128]),
  "empty.dat": new Uint8Array(0),
};
function save() {
  return { ...emptySave(), files: Object.entries(rawFiles).map(([path, bytes]) => ({ path, data: encodeBytes(bytes) })) };
}
function bytesByPath(value) {
  return Object.fromEntries(value.files.map(file => [file.path, decodeBytes(file.data)]));
}
const importZIP = bytes => importFiles([new File([bytes], "save.zip")]);
const zip = (files, options) => zipSync(Object.fromEntries(Object.entries(files).map(([name, value]) => [name, value instanceof Uint8Array ? value : new TextEncoder().encode(value)])), options);

function entry(bytes, name) {
  const view = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength);
  let offset = view.getUint32(bytes.length - 6, true);
  while (view.getUint32(offset, true) === 0x02014b50) {
    const length = view.getUint16(offset + 28, true);
    if (new TextDecoder().decode(bytes.subarray(offset + 46, offset + 46 + length)) === name) {
      const local = view.getUint32(offset + 42, true);
      const body = local + 30 + view.getUint16(local + 26, true) + view.getUint16(local + 28, true);
      return { view, central: offset, local, body };
    }
    offset += 46 + length + view.getUint16(offset + 30, true) + view.getUint16(offset + 32, true);
  }
  throw new Error(`Missing test entry: ${name}`);
}

test("ZIP export contains only appdata and the original binary files", async () => {
  const archive = createBackup(save());
  const unpacked = unzipSync(archive);
  assert.deepEqual(Object.keys(unpacked).sort(), ["appdata/", ...Object.keys(rawFiles).map(path => "appdata/" + path)].sort());
  for (const [path, bytes] of Object.entries(rawFiles)) assert.deepEqual(unpacked["appdata/" + path], bytes);
  assert.deepEqual(bytesByPath(await importFiles([new File([archive], "SAVE.ZIP")])), rawFiles);
  const longPath = "a".repeat(236) + ".lua";
  const longSave = { ...emptySave(), files: [{ path: longPath, data: encodeBytes(rawFiles["settings.lua"]) }] };
  assert.deepEqual(bytesByPath(await importZIP(createBackup(longSave))), { [longPath]: rawFiles["settings.lua"] });
  assert.throws(() => createBackup(emptySave()), { code: "noProgress" });
});

test("manually replacing one file in the ZIP preserves every other file", async () => {
  const unpacked = unzipSync(createBackup(save()));
  const replacement = new Uint8Array([128, 0, 255, 10, 13, 0]);
  unpacked["appdata/settings.lua"] = replacement;
  const restored = bytesByPath(await importZIP(zipSync(unpacked)));
  assert.deepEqual(restored, { ...rawFiles, "settings.lua": replacement });
});

test("standard Python ZIP tools read exports and streamed DEFLATE imports", async () => {
  const result = spawnSync("python3", ["-c", `
import io, sys, zipfile
original = zipfile.ZipFile(io.BytesIO(sys.stdin.buffer.read()))
assert original.testzip() is None
assert set(original.namelist()) == {"appdata/", "appdata/settings.lua", "appdata/stella-device-id", "appdata/nested/进度.bin", "appdata/empty.dat"}
class Stream(io.BytesIO):
    def seekable(self): return False
    def seek(self, *args): raise io.UnsupportedOperation()
output = Stream()
with zipfile.ZipFile(output, "w", zipfile.ZIP_DEFLATED) as archive:
    for name in original.namelist(): archive.writestr(name, original.read(name))
sys.stdout.buffer.write(output.getvalue())
`], { input: createBackup(save()), maxBuffer: MAX_BACKUP_BYTES });
  assert.equal(result.status, 0, result.stderr?.toString() ?? result.error?.message);
  const archive = new Uint8Array(result.stdout);
  const { view, local } = entry(archive, "appdata/settings.lua");
  assert.ok(view.getUint16(local + 6, true) & 8, "Independent ZIP tool emits a data descriptor");
  assert.deepEqual(bytesByPath(await importZIP(archive)), rawFiles);
});

test("ZIP-only import rejects JSON backups, loose files and multiple archives", async () => {
  await assert.rejects(importFiles([]), { code: "noFiles" });
  for (const file of [new File([JSON.stringify(save())], "save.json"), new File([rawFiles["settings.lua"]], "settings.lua")]) {
    await assert.rejects(importFiles([file]), { code: "zipOnly" });
  }
  const file = new File([createBackup(save())], "save.zip");
  await assert.rejects(importFiles([file, file]), { code: "zipOnly" });
});

test("imports require appdata as the only top-level folder", async () => {
  for (const files of [{ "settings.lua": "x" }, { "backup/appdata/settings.lua": "x" }, { "appdata/settings.lua": "x", "manifest.json": "{}" }, { "appdata/settings.lua": "x", "__MACOSX/._settings.lua": "x" }]) {
    await assert.rejects(importZIP(zip(files)), { code: "archiveRoot" });
  }
  await assert.rejects(importZIP(zip({ "appdata/": "" })), { code: "emptyArchive" });
  assert.deepEqual(bytesByPath(await importZIP(zip({ "appdata/settings.lua": "x" }))), { "settings.lua": new Uint8Array([120]) }, "An explicit directory entry is optional");
});

test("reject traversal, duplicate paths and file/directory collisions", async () => {
  for (const path of ["appdata/../outside.lua", "appdata/a/../b.lua", "appdata/a\\b.lua", "appdata/a//b.lua", "appdata/./b.lua", "appdata//"]) {
    await assert.rejects(importZIP(zip({ [path]: "x" })), { code: "invalidPath" });
  }
  await assert.rejects(importZIP(zip({ "appdata/a": "x", "appdata/a/b": "y" })), { code: "duplicatePath" });
  await assert.rejects(importZIP(zip({ "appdata/a": "x", "appdata/a/": "" })), { code: "duplicatePath" });
  const duplicate = zip({ "appdata/a.lua": "x", "appdata/b.lua": "y" });
  const { central, local } = entry(duplicate, "appdata/b.lua");
  duplicate[central + 46 + "appdata/".length] = duplicate[local + 30 + "appdata/".length] = 97;
  await assert.rejects(importZIP(duplicate), { code: "duplicatePath" });
});

test("reject damaged ZIP headers, payloads and incorrect CRC32", async () => {
  const good = zip({ "appdata/settings.lua": "binary data" }, { level: 0 });
  for (const broken of [new Uint8Array(0), new Uint8Array([1, 2, 3]), good.slice(0, -1), good.slice(0, 30)]) {
    await assert.rejects(importZIP(broken), { code: "invalidZIP" });
  }
  const payload = good.slice(); payload[entry(payload, "appdata/settings.lua").body] ^= 255;
  await assert.rejects(importZIP(payload), { code: "invalidZIP" });
  const checksum = zip({ "appdata/settings.lua": "deflated data" }, { level: 6 });
  const item = entry(checksum, "appdata/settings.lua");
  item.view.setUint32(item.local + 14, 123, true); item.view.setUint32(item.central + 16, 123, true);
  await assert.rejects(importZIP(checksum), { code: "invalidZIP" });
  const mismatched = good.slice(); mismatched[entry(mismatched, "appdata/settings.lua").local + 30] = 98;
  await assert.rejects(importZIP(mismatched), { code: "invalidZIP" });
});

test("reject encryption, symlinks and unsupported compression", async () => {
  for (const alteration of ["encryption", "symlink", "compression"]) {
    const archive = zip({ "appdata/settings.lua": "data" });
    const { view, central, local } = entry(archive, "appdata/settings.lua");
    if (alteration === "encryption") { view.setUint16(central + 8, 1, true); view.setUint16(local + 6, 1, true); }
    if (alteration === "symlink") view.setUint32(central + 38, 0xa1ff0000, true);
    if (alteration === "compression") { view.setUint16(central + 10, 12, true); view.setUint16(local + 8, 12, true); }
    await assert.rejects(importZIP(archive), { code: "invalidZIP" });
  }
});

test("reject oversized archives and decompression beyond declared capacity", async () => {
  let read = false;
  await assert.rejects(importFiles([{ name: "save.zip", size: MAX_BACKUP_BYTES + 1, arrayBuffer() { read = true; } }]), { code: "filesTooLarge" });
  assert.equal(read, false, "Oversized files are rejected before reading into memory");
  await assert.rejects(importZIP(zip({ "appdata/large.bin": new Uint8Array(MAX_SAVE_BYTES + 1) })), { code: "saveTooLarge" });
  const dishonest = zip({ "appdata/large.bin": new Uint8Array(MAX_SAVE_BYTES) });
  const { view, local, central } = entry(dishonest, "appdata/large.bin");
  view.setUint32(local + 22, 1, true); view.setUint32(central + 24, 1, true);
  await assert.rejects(importZIP(dishonest), { code: "invalidZIP" });
});
