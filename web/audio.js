export class GameAudio {
  constructor() {
    const AudioContext = globalThis.AudioContext ?? globalThis.webkitAudioContext;
    this.context = AudioContext ? new AudioContext() : null;
    this.playbacks = new Map();
    this.cache = new Map();
    this.generation = null;
    // Constructed directly in the Start click so browser autoplay permits audio.
    this.context?.resume().catch(() => {});
  }
  prepare(module, source) {
    if (!this.context || !source) return Promise.resolve(null);
    if (source.sequence) {
      return Promise.all(source.sequence.map(child => this.prepare(module, child))).then(parts => {
        parts = parts.filter(Boolean);
        if (!parts.length) return null;
        const channels = Math.max(...parts.map(part => part.numberOfChannels));
        const rate = parts[0].sampleRate;
        const length = parts.reduce((sum, part) => sum + Math.round(part.duration * rate), 0);
        const buffer = this.context.createBuffer(channels, length, rate);
        let offset = 0;
        for (const part of parts) {
          const frames = Math.round(part.duration * rate);
          for (let channel = 0; channel < channels; channel++) {
            const input = part.getChannelData(Math.min(channel, part.numberOfChannels - 1));
            const output = buffer.getChannelData(channel);
            for (let frame = 0; frame < frames; frame++) output[offset + frame] = input[Math.min(input.length - 1, Math.floor(frame * part.sampleRate / rate))];
          }
          offset += frames;
        }
        return buffer;
      });
    }
    const key = `${source.pointer}:${source.length}`;
    if (this.cache.has(key)) return this.cache.get(key);
    // Copy before asynchronous decoding; Rust may release this clip next frame.
    const bytes = module.HEAPU8.slice(source.pointer, source.pointer + source.length);
    let result;
    if (source.encoded) result = this.context.decodeAudioData(bytes.buffer).catch(() => null);
    else {
      if (![8, 16].includes(source.bits) || !source.channels || !source.rate) return Promise.resolve(null);
      const frames = Math.floor(bytes.length / (source.bits / 8) / source.channels);
      if (!frames) return Promise.resolve(null);
      const buffer = this.context.createBuffer(source.channels, frames, source.rate);
      const view = new DataView(bytes.buffer);
      for (let channel = 0; channel < source.channels; channel++) {
        const samples = buffer.getChannelData(channel);
        for (let frame = 0; frame < frames; frame++) {
          const index = frame * source.channels + channel;
          samples[frame] = source.bits === 16 ? view.getInt16(index * 2, true) / 32768 : (bytes[index] - 128) / 128;
        }
      }
      result = Promise.resolve(buffer);
    }
    this.cache.set(key, result);
    return result;
  }
  sync(module, state) {
    if (!this.context) return;
    if (this.generation !== state.generation) { this.stop(); this.cache.clear(); this.generation = state.generation; }
    const live = new Set(state.started ? state.playbacks.map(playback => playback.handle) : []);
    for (const [handle, entry] of this.playbacks) {
      if (!live.has(handle)) { entry.node?.stop(); this.playbacks.delete(handle); }
    }
    if (!state.started) return;
    for (const playback of state.playbacks) {
      let entry = this.playbacks.get(playback.handle);
      if (!entry) {
        entry = { node: null, gain: null, playback };
        this.playbacks.set(playback.handle, entry);
        const owner = entry;
        this.prepare(module, playback.source).then(buffer => {
          if (!buffer || this.playbacks.get(playback.handle) !== owner || !this.context) return;
          const node = this.context.createBufferSource(); const gain = this.context.createGain();
          node.buffer = buffer; node.loop = owner.playback.loop;
          gain.gain.value = Math.max(0, owner.playback.volume);
          node.connect(gain); gain.connect(this.context.destination);
          owner.node = node; owner.gain = gain; node.start();
        }).catch(() => {});
      }
      entry.playback = playback;
      if (entry.node) entry.node.loop = playback.loop;
      if (entry.gain) entry.gain.gain.value = Math.max(0, playback.volume);
    }
  }
  stop() { for (const entry of this.playbacks.values()) entry.node?.stop(); this.playbacks.clear(); }
  dispose() { this.stop(); this.context?.close().catch(() => {}); this.context = null; this.cache.clear(); }
}
