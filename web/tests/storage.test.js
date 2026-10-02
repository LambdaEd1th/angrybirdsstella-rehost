import { test } from "node:test";
import assert from "node:assert/strict";
import { SaveStore, emptySave, encodeBytes, decodeBytes, validateSave, safePath, restoreSave, snapshotSave, clearVirtualSave, SAVE_ROOT, MAX_SAVE_BYTES } from "../storage.js";
import { createBackup, importFiles } from "../backup.js";

function storage() {
  const values = new Map();
  return { getItem: key => values.get(key) ?? null, setItem: (key, value) => values.set(key, value), removeItem: key => values.delete(key) };
}
function fixture() {
  const value = emptySave();
  value.files = [{ path: "settings.lua", data: encodeBytes(new Uint8Array([0, 255, 128, 1, 2])) }, { path: "stella-device-id", data: encodeBytes(new TextEncoder().encode("00112233-4455-4677-8899-AABBCCDDEEFF")) }];
  return value;
}
function filesystem() {
  const directories = new Set(["/"]); const files = new Map();
  return {
    mkdirTree(path) { const parts = path.split("/").filter(Boolean); for (let count = 1; count <= parts.length; count++) directories.add("/" + parts.slice(0, count).join("/")); },
    writeFile(path, bytes) { files.set(path, new Uint8Array(bytes)); },
    readFile(path) { return files.get(path); },
    readdir(path) { return [".", "..", ...new Set([...directories, ...files.keys()].filter(value => value.startsWith(path + "/")).map(value => value.slice(path.length + 1).split("/")[0]))].filter(Boolean); },
    lstat(path) { return { mode: directories.has(path) ? 1 : 2 }; },
    rmdir(path) { directories.delete(path); }, unlink(path) { files.delete(path); },
    isDir(mode) { return mode === 1; }, isFile(mode) { return mode === 2; },
  };
}

test("binary files and stable device identity survive ZIP export/import and MEMFS restoration", async () => {
  const first = fixture(); const FS = filesystem(); restoreSave(FS, first);
  assert.deepEqual(FS.readFile(`${SAVE_ROOT}/settings.lua`), decodeBytes(first.files[0].data));
  FS.mkdirTree(`${SAVE_ROOT}/nested`); FS.writeFile(`${SAVE_ROOT}/nested/progress.bin`, new Uint8Array([10, 0, 255]));
  const save = snapshotSave(FS, first); assert.equal(save.createdAt, first.createdAt);
  const second = filesystem(); restoreSave(second, await importFiles([new File([createBackup(save)], "save.zip")]));
  assert.deepEqual(second.readFile(`${SAVE_ROOT}/nested/progress.bin`), new Uint8Array([10, 0, 255]));
  assert.deepEqual(second.readFile(`${SAVE_ROOT}/stella-device-id`), FS.readFile(`${SAVE_ROOT}/stella-device-id`));
});
test("switching slots clears only virtual AppData and preserves read-only resources", () => {
  const FS = filesystem(); restoreSave(FS, fixture()); FS.mkdirTree("/runtime/data"); FS.writeFile("/runtime/data/game.lua", new Uint8Array([1]));
  FS.mkdirTree(`${SAVE_ROOT}/nested`); FS.writeFile(`${SAVE_ROOT}/nested/old.lua`, new Uint8Array([2]));
  clearVirtualSave(FS); assert.equal(snapshotSave(FS, null).files.length, 0); assert.deepEqual(FS.readFile("/runtime/data/game.lua"), new Uint8Array([1]));
  restoreSave(FS, fixture()); assert.equal(snapshotSave(FS, null).files.length, 2);
});
test("repository paths and three slots do not overwrite each other", () => {
  const shared = storage(); const first = new SaveStore(shared, "/first/"); const second = new SaveStore(shared, "/second/");
  first.write(1, fixture()); first.write(2, emptySave()); second.write(1, emptySave());
  assert.equal(first.read(1).files.length, 2); assert.equal(first.read(2).files.length, 0); assert.equal(second.read(1).files.length, 0);
  first.clear(2); assert.equal(first.read(2), null); assert.equal(first.read(1).files.length, 2);
  assert.throws(() => first.read(4));
});
test("quota failure retains the previously stored snapshot", () => {
  const shared = storage(); const store = new SaveStore(shared, "/repo/"); store.write(1, fixture());
  shared.setItem = () => { throw new Error("QuotaExceededError"); };
  assert.throws(() => store.write(1, emptySave()), { code: "writeError" });
  assert.equal(store.read(1).files.length, 2);
});
test("reject malformed formats, traversal, duplicate paths and invalid Base64", () => {
  for (const path of ["/settings.lua", "../settings.lua", "a/../b", "a\\b", "a//b", "a\0b", "./b"]) assert.throws(() => safePath(path));
  assert.throws(() => validateSave({ ...fixture(), version: 2 }));
  assert.throws(() => validateSave({ ...fixture(), updatedAt: "broken" }));
  assert.throws(() => validateSave({ ...fixture(), files: [fixture().files[0], fixture().files[0]] }));
  for (const data of ["?===", "a", "a===", "AAAA\n"]) assert.throws(() => decodeBytes(data));
  assert.throws(() => validateSave({ ...fixture(), files: [{ path: "large.lua", data: encodeBytes(new Uint8Array(MAX_SAVE_BYTES + 1)) }] }), { code: "saveTooLarge" });
});
test("corrupt localStorage data is reported without silently overwriting it", () => {
  const shared = storage(); const store = new SaveStore(shared, "/repo/"); shared.setItem(store.key(1), "bad json");
  assert.throws(() => store.read(1), { code: "readError" }); assert.equal(shared.getItem(store.key(1)), "bad json");
});
