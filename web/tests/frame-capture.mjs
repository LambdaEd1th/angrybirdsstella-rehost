// Production WebAssembly + WebGL regressions. Fixtures wrap the original
// startup only inside MEMFS; no packaged scripts or player saves are changed.
import { clearVirtualSave } from "../storage.js";

const width = 8, height = 4;
const red = [255, 0, 0, 255], blue = [0, 0, 255, 255];
const green = [0, 255, 0, 255], black = [0, 0, 0, 255];
const split = (x, y) => y < 2 ? red : blue;
const pair = `
  drawRect(1, 0, 0, 1, 0, 0, 8, 2, true)
  drawRect(0, 0, 1, 1, 0, 2, 8, 4, true)
`;

export const FRAME_CASES = [
  {
    name: "update capture survives draw and retains native row orientation",
    source: `
      function update()
        setBGColor(0, 255, 0)
        ${pair}
        res.captureSprite("FRAME_A")
      end
      function draw() res.drawSprite("FRAME_A", 0, 0) end
    `,
    expected: [split], captures: [1, 0],
  },
  {
    name: "update draws precede clear, and captures execute only once",
    source: `
      local frame = 0
      function update()
        frame = frame + 1
        if frame == 1 then
          ${pair}
          res.captureSprite("FRAME_A")
          drawRect(0, 0, 1, 1, 0, 0, 8, 4, true)
          setBGColor(0, 255, 0)
        end
      end
      function draw()
        if frame == 2 then res.drawSprite("FRAME_A", 0, 0) end
      end
    `,
    expected: [() => green, split], captures: [1, 0],
  },
  {
    name: "draw can release and recreate an update-time capture",
    source: `
      function update()
        ${pair}
        res.captureSprite("FRAME_A")
      end
      function draw()
        res.releaseSpriteSheet("FRAME_A", false)
        drawRect(0, 0, 1, 1, 0, 0, 8, 4, true)
        res.captureSprite("FRAME_A")
        clearScreen()
        res.drawSprite("FRAME_A", 0, 0)
      end
    `,
    expected: [() => blue], captures: [1, 1],
  },
  {
    name: "recapture retains earlier draws and copies the same immediate stream",
    source: `
      function update()
        ${pair}
        res.captureSprite("FRAME_A")
      end
      function draw()
        res.drawSprite("FRAME_A", 0, 0)
        drawRect(0, 1, 0, 1, 0, 0, 4, 4, true)
        res.captureSprite("FRAME_A")
        clearScreen()
        res.drawSprite("FRAME_A", 0, 0)
      end
    `,
    expected: [(x, y) => x < 4 ? green : split(x, y)], captures: [1, 1],
  },
  {
    name: "host clear latches update color and clip before draw changes them",
    source: `
      local frame = 0
      function update()
        frame = frame + 1
        if frame == 1 then
          drawRect(1, 0, 0, 1, 0, 0, 8, 4, true)
          setBGColor(0, 255, 0)
          res.setClipRect(4, 0, 4, 4)
        else
          res.setClipRect(0, 0, 8, 4)
        end
      end
      function draw()
        res.setClipRect(0, 0, 8, 4)
        setBGColor(0, 0, 255)
      end
    `,
    expected: [(x) => x < 4 ? red : green, () => blue], captures: [0, 0],
  },
  {
    name: "signed wrapping and explicitly empty host clips preserve pixels",
    source: `
      local frame = 0
      function update()
        frame = frame + 1
        if frame == 1 then
          setBGColor(255, 0, 0)
          clearScreen()
          setBGColor(0, 255, 0)
          res.setClipRect(-2000000000, 0, 4000000000, 4)
        else
          res.setClipRect(0, 0, 0, 4)
        end
      end
      function draw() end
    `,
    expected: [() => red, () => red], captures: [0, 0], emptyClear: true,
  },
  {
    name: "disabled Lua draw still consumes update capture and host clear",
    source: `
      local frame = 0
      function update()
        frame = frame + 1
        if frame == 1 then
          ${pair}
          res.captureSprite("FRAME_A")
          setBGColor(0, 255, 0)
          setGameRenderingDisabled(true)
        else
          setGameRenderingDisabled(false)
        end
      end
      function draw()
        if frame == 1 then error("disabled draw executed") end
        res.drawSprite("FRAME_A", 0, 0)
      end
    `,
    expected: [() => green, split], captures: [1, 0],
  },
];

export function callEngine(module, name, ...args) {
  if (module[name](...args) !== 0) throw new Error(module.UTF8ToString(module._stella_error()));
}

export function readPacket(module) {
  return JSON.parse(module.UTF8ToString(module._stella_packet()));
}

function require(value, message) { if (!value) throw new Error(message); }

export function installFrameFixture(module, source) {
  const original = "/runtime/data/scripts/browser-original-game.lua";
  if (!module.FS.analyzePath(original).exists) {
    module.FS.writeFile(original, module.FS.readFile("/runtime/data/scripts/game.lua"));
  }
  module._stella_shutdown();
  clearVirtualSave(module.FS);
  module.FS.writeFile("/runtime/data/scripts/game.lua", `
    loadLuaFile("scripts/browser-original-game.lua")
    local startup = createStartUpAssets
    function createStartUpAssets()
      startup()
      setBGColor(0, 0, 0)
      ${source}
    end
  `);
  callEngine(module, "_stella_init", width, height);
}

function countCaptures(packet) {
  return packet.operations.filter(operation => operation.capture).length;
}

export function assertPixels(renderer, expected, label, w = width, h = height) {
  const gl = renderer.gl, pixels = new Uint8Array(w * h * 4);
  gl.readPixels(0, 0, w, h, gl.RGBA, gl.UNSIGNED_BYTE, pixels);
  require(gl.getError() === gl.NO_ERROR, `${label}: WebGL error`);
  const topDown = new Uint8Array(pixels.length);
  for (let y = 0; y < h; y++) for (let x = 0; x < w; x++) {
    const actual = pixels.subarray(((h - y - 1) * w + x) * 4, ((h - y - 1) * w + x) * 4 + 4);
    const wanted = expected(x, y);
    require(actual.every((value, channel) => value === wanted[channel]),
      `${label}: pixel(${x},${y}) [${actual}] != [${wanted}]`);
    topDown.set(actual, (y * w + x) * 4);
  }
  return Array.from(topDown);
}

export function runFrameCase(module, renderer, fixture) {
  installFrameFixture(module, fixture.source);
  renderer?.resize(width, height);
  const frames = [];
  for (const [index, expected] of fixture.expected.entries()) {
    callEngine(module, "_stella_frame", 1 / 60);
    const packet = readPacket(module);
    require(packet.beforeClear, `${fixture.name}: missing pre-clear stream`);
    if (index === 0) {
      require(countCaptures(packet.beforeClear) === fixture.captures[0], `${fixture.name}: update captures lost`);
      require(countCaptures(packet) === fixture.captures[1], `${fixture.name}: wrong draw capture count`);
    } else {
      require(countCaptures(packet.beforeClear) === 0, `${fixture.name}: update capture replayed`);
      require(countCaptures(packet) === 0, `${fixture.name}: draw capture replayed`);
    }
    if (fixture.emptyClear) require(packet.clearScissor[2] === 0, `${fixture.name}: empty clip became visible`);
    renderer?.render(module, packet);
    const pixels = renderer ? assertPixels(renderer, expected, `${fixture.name}, frame${index + 1}`) : null;
    // An empty out-of-band flush must not clear or replay a completed stream.
    callEngine(module, "_stella_flush");
    const empty = readPacket(module);
    require(empty.operations.length === 0 && empty.vertices.length === 0 && !empty.background,
      `${fixture.name}: completed commands remained pending`);
    renderer?.render(module, empty);
    if (renderer) assertPixels(renderer, expected, `${fixture.name}, empty flush`);
    frames.push({ updateCaptures: countCaptures(packet.beforeClear), drawCaptures: countCaptures(packet), clearScissor: packet.clearScissor, pixels });
  }
  return { name: fixture.name, frames, webglPixelsRead: !!renderer };
}

export function runResizeCase(module, renderer) {
  const name = "pause and resolution callbacks flush on their own target; old capture keeps extent";
  installFrameFixture(module, `
    function update() end
    function draw() res.drawSprite("FRAME_OLD", 0, 0) end
    function gamePaused()
      ${pair}
      res.captureSprite("FRAME_OLD")
    end
    function gameResumed() end
    function resolutionChanged()
      drawRect(0, 1, 0, 1, 0, 0, 16, 8, true)
      res.captureSprite("FRAME_NEW")
    end
  `);
  renderer?.resize(width, height);
  callEngine(module, "_stella_active", 0);
  require(module._stella_resize(16, 8) === -1, `${name}: unflushed old-size calls admitted`);
  callEngine(module, "_stella_flush");
  const old = readPacket(module);
  require(countCaptures(old) === 1 && !old.background, `${name}: pause capture lost or cleared`);
  renderer?.render(module, old);
  const oldPixels = renderer ? assertPixels(renderer, split, `${name}, before resize`) : null;
  renderer?.resize(16, 8);
  callEngine(module, "_stella_resize", 16, 8);
  callEngine(module, "_stella_flush");
  const changed = readPacket(module);
  require(countCaptures(changed) === 1 && !changed.background, `${name}: resolution stream lost`);
  renderer?.render(module, changed);
  const changedPixels = renderer ? assertPixels(renderer, () => green, `${name}, after resize`, 16, 8) : null;
  callEngine(module, "_stella_active", 1);
  callEngine(module, "_stella_frame", 1 / 60);
  const packet = readPacket(module);
  require(packet.resolution[0] === 16 && packet.resolution[1] === 8, `${name}: wrong resolution`);
  require(countCaptures(packet) + countCaptures(packet.beforeClear) === 0, `${name}: capture replayed`);
  renderer?.render(module, packet);
  const finalPixels = renderer ? assertPixels(renderer, (x, y) => x < 8 && y < 4 ? split(x, y) : black,
    `${name}, retained 8x4 capture`, 16, 8) : null;
  return { name, oldPixels, changedPixels, finalPixels, webglPixelsRead: !!renderer };
}
