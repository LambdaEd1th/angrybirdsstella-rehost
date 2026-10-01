import assert from "node:assert/strict";
import { resolve, join } from "node:path";
import { pathToFileURL } from "node:url";
import { readFile } from "node:fs/promises";
import { restoreSave, snapshotSave } from "../storage.js";
import { LOCALES } from "../locales.js";

const artifact = resolve(process.argv[2] ?? "dist/pages");
const engine = join(artifact, "engine");
const { default: createStella } = await import(pathToFileURL(join(engine, "stella_web.js")));
const data = await readFile(join(engine, "stella_web.data"));
async function boot(save) {
  const module = await createStella({
    noInitialRun: true, locateFile: name => join(engine, name),
    getPreloadedPackage: () => data.buffer.slice(data.byteOffset, data.byteOffset + data.byteLength),
    preRun: [module => restoreSave(module.FS, save)],
    print: () => {}, printErr: message => console.error(message),
  });
  function call(name, ...args) { assert.equal(module[name](...args), 0, module.UTF8ToString(module._stella_error())); }
  call("_stella_set_locale", 8); // Japanese startup, rather than an English-only boot.
  call("_stella_init");
  let packet;
  for (let frame = 0; frame < 600; frame++) {
    call("_stella_frame", 1 / 60); packet = JSON.parse(module.UTF8ToString(module._stella_packet()));
  }
  assert.ok(packet.vertices.length > 0, "Original game must submit actual render geometry");
  assert.ok(packet.operations.some(operation => operation.count > 0), "Original game must submit draws");
  assert.equal(packet.locale, "ja_JP");
  for (const [index, locale] of LOCALES.entries()) {
    call("_stella_set_locale", index);
    call("_stella_active", 0); call("_stella_active", 1);
    for (let frame = 0; frame < 4; frame++) call("_stella_frame", 1 / 60);
    packet = JSON.parse(module.UTF8ToString(module._stella_packet()));
    assert.equal(packet.locale, locale.game, "Language survives live switching and resume");
    assert.ok(packet.operations.some(operation => operation.count > 0), `${locale.game} renders`);
  }
  assert.equal(module._stella_set_locale(-1), -1);
  assert.equal(module._stella_set_locale(LOCALES.length), -1);
  call("_stella_pointer", 100, 100, 1); call("_stella_frame", 1 / 60);
  call("_stella_pointer", 100, 100, 0); call("_stella_wheel", 1, 0, 0);
  call("_stella_touches", 2, 1, 100, 100, 2, 180, 100); call("_stella_touches", 0, 0, 0, 0, 0, 0, 0);
  call("_stella_key", 0, 1); call("_stella_frame", 1 / 60); call("_stella_key", 0, 0);
  for (let resume = 0; resume < 8; resume++) {
    call("_stella_active", 0); call("_stella_active", 1); call("_stella_frame", 1 / 60); call("_stella_save");
  }
  return snapshotSave(module.FS, save);
}
const first = await boot(null);
assert.ok(first.files.some(file => file.path === "stella-device-id"));
assert.ok(first.files.some(file => file.path === "settings.lua"));
const second = await boot(first);
assert.equal(second.files.find(file => file.path === "stella-device-id").data, first.files.find(file => file.path === "stella-device-id").data);
console.log(`WebAssembly boot/render/input/save/restore and all ${LOCALES.length} game languages passed (${second.files.length} AppData files).`);
