const vertexSource = `#version 300 es
precision highp float;
layout(location=0) in vec2 aUv;
layout(location=1) in vec2 aSource;
layout(location=2) in vec4 aClip;
layout(location=3) in uint aDraw;
out vec2 vUv;
out vec2 vSource;
flat out uint vDraw;
void main() {
  gl_Position = vec4(aClip.xy, aClip.z * 2.0 - aClip.w, aClip.w);
  vUv = aUv; vSource = aSource; vDraw = aDraw;
}`;

const fragmentSource = `#version 300 es
precision highp float;
precision highp int;
uniform sampler2D uBase;
uniform sampler2D uFill;
uniform highp sampler2D uDraws;
uniform int uDrawWidth;
uniform bool uBaseCaptured;
uniform bool uFillCaptured;
in vec2 vUv;
in vec2 vSource;
flat in uint vDraw;
out vec4 outColor;
vec4 state(int offset) {
  int index = int(vDraw) * 4 + offset;
  return texelFetch(uDraws, ivec2(index % uDrawWidth, index / uDrawWidth), 0);
}
vec2 orient(vec2 uv, bool captured) { return captured ? vec2(uv.x, 1.0 - uv.y) : uv; }
void main() {
  vec4 header = state(0), diffuse = state(1), params = state(2), fill = state(3);
  int sourceMode = int(header.z + 0.5), shaderMode = int(header.w + 0.5);
  vec4 color;
  if (sourceMode == 2) color = diffuse;
  else if (sourceMode == 3) color = texture(uFill, orient(vUv, uFillCaptured));
  else if (sourceMode == 1) {
    vec4 mask = texture(uBase, orient(vUv, uBaseCaptured));
    float magnitude = max(abs(header.y), 0.000001);
    float scale = header.y >= 0.0 ? magnitude : -magnitude;
    color = texture(uFill, orient(vSource / scale / max(fill.xy, vec2(1.0)), uFillCaptured));
    color.a *= mask.a;
  } else color = texture(uBase, orient(vUv, uBaseCaptured));
  if (sourceMode != 2 && shaderMode != 0) {
    if (shaderMode == 4) color *= diffuse;
    else {
      float gray = (color.r + color.g + color.b) * 0.333;
      if (shaderMode == 3) {
        float highlight = params.z * gray * gray;
        float inverse = 1.0 - gray;
        float luminance = 1.0 - inverse * inverse + params.x * color.a;
        color = vec4(luminance, luminance, luminance, color.a) * diffuse + vec4(highlight);
      } else {
        float lightness = params.x * color.a;
        color = mix(vec4(gray, gray, gray, color.a), color, params.y);
        if (shaderMode == 1) { color *= diffuse; color.rgb += vec3(lightness); }
        else { color.rgb += vec3(lightness); color = min(vec4(1.0), color) * diffuse; }
      }
    }
  }
  outColor = color * header.x;
}`;

export class GameRenderer {
  constructor(canvas) {
    const gl = canvas.getContext("webgl2", { alpha: false, antialias: false, preserveDrawingBuffer: true });
    if (!gl) throw new Error("此浏览器无法启用 WebGL2，请使用支持硬件加速的浏览器。");
    this.gl = gl;
    this.textures = new Map();
    this.program = gl.createProgram();
    for (const [type, source] of [[gl.VERTEX_SHADER, vertexSource], [gl.FRAGMENT_SHADER, fragmentSource]]) {
      const shader = gl.createShader(type);
      gl.shaderSource(shader, source); gl.compileShader(shader);
      if (!gl.getShaderParameter(shader, gl.COMPILE_STATUS)) throw new Error(gl.getShaderInfoLog(shader));
      gl.attachShader(this.program, shader); gl.deleteShader(shader);
    }
    gl.linkProgram(this.program);
    if (!gl.getProgramParameter(this.program, gl.LINK_STATUS)) throw new Error(gl.getProgramInfoLog(this.program));
    gl.useProgram(this.program);
    this.locations = Object.fromEntries(["uBase", "uFill", "uDraws", "uDrawWidth", "uBaseCaptured", "uFillCaptured"]
      .map(name => [name, gl.getUniformLocation(this.program, name)]));
    gl.uniform1i(this.locations.uBase, 0); gl.uniform1i(this.locations.uFill, 1); gl.uniform1i(this.locations.uDraws, 2);
    this.vao = gl.createVertexArray(); gl.bindVertexArray(this.vao);
    this.vertices = gl.createBuffer(); gl.bindBuffer(gl.ARRAY_BUFFER, this.vertices);
    for (const [index, size, offset] of [[0, 2, 8], [1, 2, 16], [2, 4, 24]]) {
      gl.enableVertexAttribArray(index); gl.vertexAttribPointer(index, size, gl.FLOAT, false, 48, offset);
    }
    gl.enableVertexAttribArray(3); gl.vertexAttribIPointer(3, 1, gl.UNSIGNED_INT, 48, 40);
    this.draws = gl.createTexture(); gl.activeTexture(gl.TEXTURE2); gl.bindTexture(gl.TEXTURE_2D, this.draws);
    gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MIN_FILTER, gl.NEAREST);
    gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MAG_FILTER, gl.NEAREST);
    this.baseSampler = gl.createSampler(); this.fillSampler = gl.createSampler();
    for (const [sampler, wrap] of [[this.baseSampler, gl.CLAMP_TO_EDGE], [this.fillSampler, gl.REPEAT]]) {
      gl.samplerParameteri(sampler, gl.TEXTURE_MIN_FILTER, gl.LINEAR);
      gl.samplerParameteri(sampler, gl.TEXTURE_MAG_FILTER, gl.LINEAR);
      gl.samplerParameteri(sampler, gl.TEXTURE_WRAP_S, wrap); gl.samplerParameteri(sampler, gl.TEXTURE_WRAP_T, wrap);
    }
    gl.bindSampler(0, this.baseSampler); gl.bindSampler(1, this.fillSampler);
    this.upload({ name: "<stella-white>", width: 1, height: 1 }, new Uint8Array([255, 255, 255, 255]));
  }
  upload(info, pixels) {
    const gl = this.gl;
    const texture = gl.createTexture(); gl.activeTexture(gl.TEXTURE0); gl.bindTexture(gl.TEXTURE_2D, texture);
    gl.texImage2D(gl.TEXTURE_2D, 0, gl.RGBA8, info.width, info.height, 0, gl.RGBA, gl.UNSIGNED_BYTE, pixels);
    gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MIN_FILTER, gl.LINEAR);
    gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MAG_FILTER, gl.LINEAR);
    this.textures.set(info.name, { texture, captured: false });
  }
  render(module, packet) {
    const gl = this.gl;
    if (gl.isContextLost()) throw new Error("图形上下文已丢失。请导出存档后重新打开游戏。");
    gl.viewport(0, 0, gl.canvas.width, gl.canvas.height);
    gl.useProgram(this.program);
    gl.bindVertexArray(this.vao);
    for (const info of packet.textures) {
      this.upload(info, module.HEAPU8.subarray(info.pointer, info.pointer + info.length));
    }
    gl.bindBuffer(gl.ARRAY_BUFFER, this.vertices);
    gl.bufferData(gl.ARRAY_BUFFER, module.HEAPU8.subarray(packet.vertices.pointer, packet.vertices.pointer + packet.vertices.length), gl.DYNAMIC_DRAW);
    const count = packet.uniforms.count;
    const width = Math.min(1024, gl.getParameter(gl.MAX_TEXTURE_SIZE));
    const height = Math.max(1, Math.ceil(count * 4 / width));
    if (height > gl.getParameter(gl.MAX_TEXTURE_SIZE)) throw new Error("场景超过显卡的绘制容量。");
    const uniforms = new Float32Array(width * height * 4);
    uniforms.set(module.HEAPF32.subarray(packet.uniforms.pointer / 4, packet.uniforms.pointer / 4 + count * 16));
    gl.activeTexture(gl.TEXTURE2); gl.bindTexture(gl.TEXTURE_2D, this.draws);
    gl.texImage2D(gl.TEXTURE_2D, 0, gl.RGBA32F, width, height, 0, gl.RGBA, gl.FLOAT, uniforms);
    gl.uniform1i(this.locations.uDrawWidth, width);
    const setClip = clip => {
      if (clip) { gl.enable(gl.SCISSOR_TEST); gl.scissor(clip[0], gl.canvas.height - clip[1] - clip[3], clip[2], clip[3]); }
      else gl.disable(gl.SCISSOR_TEST);
    };
    // The original clear inherits the previous frame's framebuffer scissor.
    const clip = packet.clearClip;
    setClip(clip ? [Math.max(0, clip[0]), Math.max(0, clip[1]), Math.max(0, Math.min(gl.canvas.width, clip[2]) - Math.max(0, clip[0])), Math.max(0, Math.min(gl.canvas.height, clip[3]) - Math.max(0, clip[1]))] : null);
    gl.clearColor(...packet.background.map(value => value / 255), 1); gl.clear(gl.COLOR_BUFFER_BIT);
    for (const operation of packet.operations) {
      if (operation.capture) {
        const texture = gl.createTexture(); gl.activeTexture(gl.TEXTURE0); gl.bindTexture(gl.TEXTURE_2D, texture);
        gl.copyTexImage2D(gl.TEXTURE_2D, 0, gl.RGB, 0, 0, gl.canvas.width, gl.canvas.height, 0);
        gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MIN_FILTER, gl.LINEAR);
        gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MAG_FILTER, gl.LINEAR);
        this.textures.set(operation.capture, { texture, captured: true });
        continue;
      }
      setClip(operation.scissor);
      if (operation.program === 0 || operation.program === 2) gl.disable(gl.BLEND);
      else {
        gl.enable(gl.BLEND);
        const factor = operation.program === 3 ? gl.ONE : gl.SRC_ALPHA;
        gl.blendFuncSeparate(factor, gl.ONE_MINUS_SRC_ALPHA, gl.ONE, gl.ONE_MINUS_SRC_ALPHA);
      }
      for (const [unit, name, captured] of [[0, operation.base, "uBaseCaptured"], [1, operation.fill, "uFillCaptured"]]) {
        const entry = this.textures.get(name);
        if (!entry) throw new Error(`缺少游戏纹理：${name}`);
        gl.activeTexture(gl.TEXTURE0 + unit); gl.bindTexture(gl.TEXTURE_2D, entry.texture);
        gl.uniform1i(this.locations[captured], entry.captured ? 1 : 0);
      }
      gl.drawArrays(gl.TRIANGLES, operation.first, operation.count);
    }
    for (const name of packet.retired) {
      const entry = this.textures.get(name);
      if (entry) { gl.deleteTexture(entry.texture); this.textures.delete(name); }
    }
  }
  dispose() {
    const gl = this.gl;
    for (const entry of this.textures.values()) gl.deleteTexture(entry.texture);
    this.textures.clear(); gl.deleteTexture(this.draws); gl.deleteBuffer(this.vertices);
    gl.deleteSampler(this.baseSampler); gl.deleteSampler(this.fillSampler); gl.deleteProgram(this.program);
    gl.deleteVertexArray(this.vao);
  }
}
