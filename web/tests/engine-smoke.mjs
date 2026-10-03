import assert from "node:assert/strict";
import { resolve, join } from "node:path";
import { pathToFileURL } from "node:url";
import { readFile } from "node:fs/promises";
import { restoreSave, snapshotSave } from "../storage.js";
import { createBackup, importFiles } from "../backup.js";
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
  call("_stella_init", 1280, 720);
  function audio() {
    const pointer = module._stella_audio_packet();
    assert.ok(pointer, module.UTF8ToString(module._stella_error()));
    return JSON.parse(module.UTF8ToString(pointer));
  }
  assert.equal(audio().started, true);
  let packet;
  for (let frame = 0; frame < 600; frame++) {
    call("_stella_frame", 1 / 60); packet = JSON.parse(module.UTF8ToString(module._stella_packet()));
  }
  assert.ok(packet.vertices.length > 0, "Original game must submit actual render geometry");
  assert.ok(packet.operations.some(operation => operation.count > 0), "Original game must submit draws");
  assert.ok(packet.beforeClear, "Scheduler/update stream is consumed before Lua draw resets its queues");
  assert.deepEqual(packet.clearScissor, [0, 0, 1280, 720], "Host clear uses the native resolved scissor");
  assert.equal(packet.locale, "ja_JP");
  assert.deepEqual(packet.resolution, [1280, 720], "Boot uses the actual drawable extent");
  for (const [width, height] of [[1429, 768], [768, 1024], [2560, 1440], [1002, 752], [1024, 768]]) {
    call("_stella_flush"); // Old-target calls must finish before the drawable changes.
    call("_stella_resize", width, height);
    call("_stella_resize", width, height); // Duplicate observations must be harmless.
    for (let frame = 0; frame < 4; frame++) call("_stella_frame", 1 / 60);
    packet = JSON.parse(module.UTF8ToString(module._stella_packet()));
    assert.deepEqual(packet.resolution, [width, height]);
    // null disables scissoring and clears the entire resized drawable.
    assert.deepEqual(packet.clearClip ?? [0, 0, width, height], [0, 0, width, height], "Clear clipping follows the new drawable");
    assert.deepEqual(packet.clearScissor, [0, 0, width, height]);
    assert.ok(packet.operations.some(operation => operation.count > 0), "Arbitrary aspect changes keep drawing the original game");
  }
  assert.equal(module._stella_resize(0, 768), -1);
  assert.equal(module._stella_resize(1024, 0), -1);
  assert.equal(module._stella_resize(-1, 768), -1);
  assert.equal(module._stella_resize(65536, 768), -1);
  for (const [index, locale] of LOCALES.entries()) {
    call("_stella_set_locale", index);
    call("_stella_active", 0); call("_stella_active", 1);
    for (let frame = 0; frame < 4; frame++) call("_stella_frame", 1 / 60);
    packet = JSON.parse(module.UTF8ToString(module._stella_packet()));
    assert.equal(packet.locale, locale.game, "Language survives live switching and resume");
    assert.ok(packet.operations.some(operation => operation.count > 0), `${locale.game} renders`);
  }
  // The real resume path may now retain the native rating alert. Supply an
  // explicit test-user choice so subsequent pointer checks target the game.
  call("_stella_rating_frame");
  const rating = JSON.parse(module.UTF8ToString(module._stella_rating_packet()));
  if (rating) {
    assert.deepEqual(rating.buttons.map(button => button.choice), [2, 1, 0]);
    for (let frame = 0; frame < 8; frame++) call("_stella_frame", 1 / 60);
    call("_stella_rating_frame");
    assert.equal(JSON.parse(module.UTF8ToString(module._stella_rating_packet())).token, rating.token, "Display frames must not invent a rating answer");
    call("_stella_rating_choose", rating.token, 2);
    call("_stella_rating_frame");
    assert.equal(JSON.parse(module.UTF8ToString(module._stella_rating_packet())), null);
  }
  assert.equal(module._stella_set_locale(-1), -1);
  assert.equal(module._stella_set_locale(LOCALES.length), -1);
  call("_stella_pointer", 100, 100, 1); call("_stella_frame", 1 / 60);
  call("_stella_pointer", 100, 100, 0); call("_stella_wheel", 1, 0, 0);
  call("_stella_touches", 2, 1, 100, 100, 2, 180, 100); call("_stella_touches", 0, 0, 0, 0, 0, 0, 0);
  // The shipped Pointer Events host now forwards every touch event. Exercise
  // three fingers and a return to two in the linked WebAssembly/original Lua.
  call("_stella_touch", 0, 11, 100.75, 100.25);
  call("_stella_touch", 0, 12, 180.75, 100.25);
  call("_stella_frame", 1 / 60);
  call("_stella_touch", 0, 13, 240.75, 100.25);
  call("_stella_touch", 1, 12, 200.75, 100.25);
  call("_stella_frame", 1 / 60);
  call("_stella_touch", 2, 13, 240.75, 100.25);
  call("_stella_frame", 1 / 60);
  call("_stella_touch", 2, 11, 100.75, 100.25);
  call("_stella_touch", 2, 12, 200.75, 100.25);
  assert.equal(module._stella_touch(3, 11, 0, 0), -1);
  call("_stella_key", 0, 1); call("_stella_frame", 1 / 60); call("_stella_key", 0, 0);
  for (let resume = 0; resume < 8; resume++) {
    const generation = audio().generation;
    call("_stella_active", 0);
    assert.equal(audio().started, false, "Pause exports stopped audio without a rendering frame");
    call("_stella_active", 0); assert.equal(audio().started, false);
    call("_stella_active", 1);
    assert.equal(audio().started, true, "Resume exports the actual native output gate");
    assert.equal(audio().generation, generation, "Activation retains the same audio output allocation");
    call("_stella_frame", 1 / 60); call("_stella_save");
  }
  return snapshotSave(module.FS, save);
}
const first = await boot(null);
assert.ok(first.files.some(file => file.path === "stella-device-id"));
assert.ok(first.files.some(file => file.path === "settings.lua"));
const restored = await importFiles([new File([createBackup(first)], "save.zip")]);
assert.deepEqual(new Map(restored.files.map(file => [file.path, file.data])), new Map(first.files.map(file => [file.path, file.data])), "ZIP preserves every original AppData byte");
const second = await boot(restored);
assert.equal(second.files.find(file => file.path === "stella-device-id").data, first.files.find(file => file.path === "stella-device-id").data);
console.log(`WebAssembly boot/render/resize/input/save/ZIP restore and all ${LOCALES.length} game languages passed (${second.files.length} AppData files).`);
