import { test } from "node:test";
import assert from "node:assert/strict";
import { displayDimensions, drawableDimensions, canvasPoint } from "../display.js";

const limits = { width: 16384, height: 16384 };

test("automatic sizing follows wide and portrait windows without fixing the aspect", () => {
  assert.deepEqual(displayDimensions("auto", 1429, 768), { width: 1429, height: 768 });
  assert.deepEqual(displayDimensions("auto", 768, 1024), { width: 768, height: 1024 });
  assert.deepEqual(displayDimensions("custom", 900, 600, { width: 1280, height: 720 }), { width: 1280, height: 720 });
});

test("percentage dimensions remain based on the original size after repeated Retina resizes", () => {
  for (let resize = 0; resize < 4; resize++) {
    const css = displayDimensions("150", 900, 600);
    assert.deepEqual(css, { width: 1536, height: 1152 });
    assert.deepEqual(drawableDimensions(css.width, css.height, 2, limits), { width: 3072, height: 2304 });
  }
});

test("drawable pixels follow display density and exact fractional-scale browser measurements", () => {
  assert.deepEqual(drawableDimensions(1280, 720, 1, limits), { width: 1280, height: 720 });
  assert.deepEqual(drawableDimensions(1280, 720, 2, limits), { width: 2560, height: 1440 });
  assert.deepEqual(drawableDimensions(801, 601, 1.25, limits, { width: 1002, height: 752 }), { width: 1002, height: 752 });
  assert.deepEqual(drawableDimensions(1280, 720, 2, limits, { width: 1280, height: 720 }), { width: 2560, height: 1440 });
  assert.deepEqual(drawableDimensions(640, 480, NaN, limits), { width: 640, height: 480 });
});

test("hardware-limited drawables preserve the viewport aspect and stay nonzero", () => {
  assert.deepEqual(drawableDimensions(1280, 720, 2, { width: 2048, height: 2048 }), { width: 2048, height: 1152 });
  assert.deepEqual(drawableDimensions(0.2, 0.2, 1, limits), { width: 1, height: 1 });
});

test("pointer and captured drags use the current physical drawable on both axes", () => {
  const canvas = { width: 2560, height: 1440, getBoundingClientRect: () => ({ left: 20, top: 80, width: 1280, height: 720 }) };
  assert.deepEqual(canvasPoint(canvas, { clientX: 660, clientY: 440 }), [1280, 720]);
  assert.deepEqual(canvasPoint(canvas, { clientX: 10, clientY: 800 }), [-20, 1440]);
  canvas.width = 768; canvas.height = 1024;
  assert.deepEqual(canvasPoint(canvas, { clientX: 660, clientY: 440 }), [384, 512]);
});
