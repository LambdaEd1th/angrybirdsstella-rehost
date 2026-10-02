import test from "node:test";
import assert from "node:assert/strict";
import { GameAudio } from "../audio.js";

class AudioContextDouble {
  constructor() {
    this.state = "suspended"; this.currentTime = 0;
    this.sources = []; this.actions = []; this.decoders = [];
    this.destination = {};
  }
  resume() { this.actions.push("resume"); this.state = "running"; return Promise.resolve(); }
  suspend() { this.actions.push("suspend"); this.state = "suspended"; return Promise.resolve(); }
  close() { this.state = "closed"; return Promise.resolve(); }
  advance(seconds) { if (this.state === "running") this.currentTime += seconds; }
  createBuffer(channels, length, rate) {
    const data = Array.from({ length: channels }, () => new Float32Array(length));
    return { numberOfChannels: channels, sampleRate: rate, duration: length / rate, getChannelData: channel => data[channel] };
  }
  createBufferSource() {
    const node = { stops: 0, starts: 0, loop: false, connect() {}, disconnect() {},
      start() { this.starts++; }, stop() { this.stops++; } };
    this.sources.push(node); return node;
  }
  createGain() { return { gain: { value: 1 }, connect() {}, disconnect() {} }; }
  decodeAudioData() { return new Promise(resolve => this.decoders.push(resolve)); }
}

const module = { HEAPU8: new Uint8Array(80) };
const pcm = { encoded: false, pointer: 0, length: 80, channels: 1, bits: 16, rate: 4 };
const playback = (handle = 7, source = pcm) => ({ handle, source, volume: 0.5, loop: false });
const snapshot = (started = true, playbacks = [playback()], generation = 1) => ({ started, playbacks, generation });
const drain = () => new Promise(resolve => setImmediate(resolve));

function audio(t) {
  const original = globalThis.AudioContext;
  globalThis.AudioContext = AudioContextDouble;
  const output = new GameAudio();
  t.after(() => { output.dispose(); globalThis.AudioContext = original; });
  return output;
}

test("stopping native output freezes a retained one-shot cursor and restart uses the same source", async t => {
  const output = audio(t), context = output.context;
  output.sync(module, snapshot()); await drain();
  const node = context.sources[0]; context.advance(2);
  output.sync(module, snapshot(false)); context.advance(30);
  assert.equal(context.state, "suspended");
  assert.equal(context.currentTime, 2);
  assert.equal(node.stops, 0);
  output.sync(module, snapshot()); await drain(); context.advance(1);
  assert.equal(context.currentTime, 3);
  assert.equal(context.sources.length, 1);
  assert.equal(output.playbacks.get(7).node, node);
});

test("host deactivation stops physical output immediately without another render frame", async t => {
  const output = audio(t), context = output.context;
  output.sync(module, snapshot()); await drain(); context.advance(1);
  output.setActive(false); context.advance(20);
  assert.equal(context.state, "suspended");
  assert.equal(context.currentTime, 1);
  output.setActive(true); context.advance(1);
  assert.equal(context.currentTime, 2);
  assert.equal(context.sources.length, 1);
});

test("a late decoder can install a paused source without reviving background audio", async t => {
  const output = audio(t), context = output.context;
  const encoded = { encoded: true, pointer: 0, length: 8 };
  output.sync(module, snapshot(true, [playback(7, encoded)]));
  output.setActive(false);
  context.decoders[0](context.createBuffer(1, 40, 4)); await drain();
  assert.equal(context.sources.length, 1);
  assert.equal(context.state, "suspended");
  context.advance(20); assert.equal(context.currentTime, 0);
  output.setActive(true);
  assert.equal(context.state, "running");
  assert.equal(context.sources.length, 1);
});

test("focus or a new snapshot cannot resume output while either host or native gate is closed", async t => {
  const output = audio(t), context = output.context;
  output.sync(module, snapshot()); await drain();
  output.setActive(false); output.sync(module, snapshot());
  assert.equal(context.state, "suspended");
  output.sync(module, snapshot(false)); output.setActive(true);
  assert.equal(context.state, "suspended");
  output.sync(module, snapshot()); assert.equal(context.state, "running");
});

test("pause-time Lua removals stop old music and newly assigned music gets its own node", async t => {
  const output = audio(t), context = output.context;
  output.sync(module, snapshot()); await drain();
  const old = context.sources[0];
  output.setActive(false); output.sync(module, snapshot(false, []));
  assert.equal(old.stops, 1);
  output.sync(module, snapshot(true, [playback(8)])); await drain();
  assert.equal(context.state, "suspended");
  assert.equal(context.sources.length, 2);
  assert.notEqual(output.playbacks.get(8).node, old);
  output.setActive(true); assert.equal(context.state, "running");
});

test("output replacement rejects old pending decodes even when handle zero is reused", async t => {
  const output = audio(t), context = output.context;
  const encoded = { encoded: true, pointer: 0, length: 8 };
  output.sync(module, snapshot(true, [playback(0, encoded)], 1));
  output.sync(module, snapshot(true, [playback(0, encoded)], 2));
  assert.equal(context.decoders.length, 2);
  context.decoders[0](context.createBuffer(1, 40, 4)); await drain();
  assert.equal(context.sources.length, 0);
  context.decoders[1](context.createBuffer(1, 20, 4)); await drain();
  assert.equal(context.sources.length, 1);
  assert.equal(context.sources[0].buffer.duration, 5);
});

test("removal and disposal cancel pending decoder publication", async t => {
  const output = audio(t), context = output.context;
  const encoded = { encoded: true, pointer: 0, length: 8 };
  output.sync(module, snapshot(true, [playback(7, encoded)]));
  output.sync(module, snapshot(false, []));
  context.decoders[0](context.createBuffer(1, 40, 4)); await drain();
  assert.equal(context.sources.length, 0);
  output.sync(module, snapshot(true, [playback(8, { ...encoded, pointer: 8 })]));
  output.dispose();
  context.decoders[1](context.createBuffer(1, 40, 4)); await drain();
  assert.equal(context.sources.length, 0);
  assert.equal(context.state, "closed");
});

test("gain and loop updates still apply to paused retained instances", async t => {
  const output = audio(t);
  output.sync(module, snapshot()); await drain();
  const entry = output.playbacks.get(7);
  output.sync(module, snapshot(false, [{ ...playback(), volume: 0.25, loop: true }]));
  assert.equal(entry.node.loop, true);
  assert.equal(entry.gain.gain.value, 0.25);
  assert.equal(entry.node.stops, 0);
});
