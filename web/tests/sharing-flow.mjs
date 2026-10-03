// Production native Lua/WASM call order; analytical WebGL pixel oracles.
// Fixtures and save state live only in this module's isolated MEMFS.
import { installFrameFixture, callEngine, readPacket, assertPixels } from "./frame-capture.mjs";

function require(value, message) { if (!value) throw new Error(message); }
function shares(packet) { return packet.operations.filter(operation => operation.share).map(operation => operation.share); }
function checkCapture(capture, expected, label) {
  require(capture.rgba.length === capture.width * capture.height * 4, `${label}: wrong extent`);
  for (let y = 0; y < capture.height; y++) for (let x = 0; x < capture.width; x++) {
    const offset = (y * capture.width + x) * 4, actual = capture.rgba.subarray(offset, offset + 4), wanted = expected(x, y);
    require(actual.every((value, channel) => value === wanted[channel]), `${label}: (${x},${y}) [${actual}] != [${wanted}]`);
  }
}

export const SHARING_FIXTURE = `
  share_frame = 0
  function update()
    share_frame = share_frame + 1
    if share_frame == 2 then native_shareScreenShot("previous green") end
  end
  function draw()
    if share_frame == 1 then
      drawRect(1, 0, 0, 1, 0, 0, 8, 4, true)
      native_shareScreenShot("red 🐦")
      drawRect(0, 0, 1, 0.5, 0, 2, 8, 4, true)
      res.setClipRect(1, 1, 1, 1)
      native_shareScreenShot("fractional alpha")
      res.setClipRect(0, 0, 8, 4)
      drawRect(0, 1, 0, 1, 0, 0, 8, 4, true)
    else
      drawRect(0, 0, 1, 1, 0, 0, 8, 4, true)
      native_shareScreenShot("blue")
      drawRect(1, 1, 1, 1, 0, 0, 8, 4, true)
    end
  end
  function gamePaused() native_shareScreenShot("before resize") end
  function gameResumed() end
  function resolutionChanged()
    res.setClipRect(0, 0, 16, 8)
    drawRect(0, 1, 0, 1, 0, 0, 16, 8, true)
    native_shareScreenShot("new drawable")
    drawRect(0, 0, 1, 1, 0, 0, 16, 8, true)
  end
`;

export function runSharingSuite(module, renderer = null) {
  installFrameFixture(module, SHARING_FIXTURE); renderer?.resize(8, 4);
  const metadata = [], captures = [], cases = [];
  for (let frame = 0; frame < 2; frame++) {
    callEngine(module, "_stella_frame", 1 / 60); const packet = readPacket(module);
    const before = shares(packet.beforeClear), after = shares(packet);
    require(JSON.stringify(before.map(s => s.title)) === JSON.stringify(frame === 0 ? [] : ["previous green"]), "update share was lost before the clear");
    require(JSON.stringify(after.map(s => s.title)) === JSON.stringify(frame === 0 ? ["red 🐦", "fractional alpha"] : ["blue"]), "draw shares were lost or replayed");
    metadata.push(...before, ...after);
    const snapshots = renderer?.render(module, packet) || [];
    if (renderer) {
      require(snapshots.length === before.length + after.length, "ordered snapshot count changed");
      const expected = frame === 0 ? [() => [255, 0, 0, 255], (_, y) => y < 2 ? [255, 0, 0, 255] : [128, 0, 127, 191]]
        : [() => [0, 255, 0, 255], () => [0, 0, 255, 255]];
      snapshots.forEach((capture, index) => checkCapture(capture, expected[index], capture.request.title));
      assertPixels(renderer, () => frame === 0 ? [0, 255, 0, 255] : [255, 255, 255, 255], `final frame ${frame}`);
    }
    captures.push(...snapshots); cases.push({ name: `native update/draw frame ${frame}`, shares: [...before, ...after], pixels: snapshots.length * 32 });
  }
  callEngine(module, "_stella_active", 0);
  require(module._stella_resize(16, 8) === -1, "a share-only pending stream was discarded by resize");
  callEngine(module, "_stella_flush"); const old = readPacket(module);
  require(shares(old).length === 1 && !old.background, "old-target share was cleared or lost");
  const oldCaptures = renderer?.render(module, old) || [];
  if (renderer) { checkCapture(oldCaptures[0], () => [255, 255, 255, 255], "before resize"); require(oldCaptures[0].width === 8 && oldCaptures[0].height === 4, "old extent changed"); }
  metadata.push(...shares(old)); captures.push(...oldCaptures);
  renderer?.resize(16, 8); callEngine(module, "_stella_resize", 16, 8);
  callEngine(module, "_stella_flush"); const changed = readPacket(module);
  require(shares(changed).length === 1 && shares(changed)[0].title === "new drawable", "new-target share was lost");
  const newCaptures = renderer?.render(module, changed) || [];
  if (renderer) {
    checkCapture(newCaptures[0], () => [0, 255, 0, 255], "new drawable");
    require(newCaptures[0].width === 16 && newCaptures[0].height === 8, "new extent changed");
    checkCapture(captures[0], () => [255, 0, 0, 255], "retained old PNG");
    assertPixels(renderer, () => [0, 0, 255, 255], "after resize", 16, 8);
  }
  metadata.push(...shares(changed)); captures.push(...newCaptures);
  cases.push({ name: "share-only callbacks preserve old/new drawable extent", shares: [...shares(old), ...shares(changed)], pixels: renderer ? 160 : 0 });
  for (const [index, request] of metadata.entries()) {
    require(request.filename === `Stella_Screenshot${request.sequence}.png`, "native filename changed");
    if (index) require(request.sequence === ((metadata[index - 1].sequence + 1) | 0), "process signed sequence reordered");
  }
  callEngine(module, "_stella_share_preview", 1); callEngine(module, "_stella_pointer", 1, 1, 1); callEngine(module, "_stella_share_preview", 0);
  return { cases, captures, webglPixelsRead: !!renderer };
}
