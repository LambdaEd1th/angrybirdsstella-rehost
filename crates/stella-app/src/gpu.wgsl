const MAX_HOLES: u32 = 64u;

struct DrawUniform {
    // alpha, texture scale, source mode, shader mode
    header: vec4<f32>,
    diffuse: vec4<f32>,
    // lightness, saturation, gold highlight, hole count
    params: vec4<f32>,
    // fill texture width and height
    fill: vec4<f32>,
    // x, y and radius in sprite-pivot space
    holes: array<vec4<f32>, 64>,
};

@group(0) @binding(0)
var<storage, read> draw_uniforms: array<DrawUniform>;

@group(1) @binding(0)
var base_map: texture_2d<f32>;
@group(1) @binding(1)
var base_sampler: sampler;
@group(1) @binding(2)
var fill_map: texture_2d<f32>;
@group(1) @binding(3)
var fill_sampler: sampler;

struct VertexInput {
    // Screen position is retained in the native stream for diagnostics; the
    // CPU-computed clip position follows Purple's explicit f32 FMADD route.
    @location(0) screen_position: vec2<f32>,
    @location(1) uv: vec2<f32>,
    @location(2) source: vec2<f32>,
    @location(3) local: vec2<f32>,
    @location(4) draw_index: u32,
    @location(5) clip_position: vec2<f32>,
};

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) source: vec2<f32>,
    @location(2) local: vec2<f32>,
    @location(3) @interpolate(flat) draw_index: u32,
};

@vertex
fn sprite_vertex(input: VertexInput) -> VertexOutput {
    var output: VertexOutput;
    output.position = vec4<f32>(input.clip_position, 0.0, 1.0);
    output.uv = input.uv;
    output.source = input.source;
    output.local = input.local;
    output.draw_index = input.draw_index;
    return output;
}

@fragment
fn sprite_fragment(input: VertexOutput) -> @location(0) vec4<f32> {
    let state = draw_uniforms[input.draw_index];
    let hole_count = min(u32(state.params.w + 0.5), MAX_HOLES);
    for (var index = 0u; index < MAX_HOLES; index += 1u) {
        if (index >= hole_count) {
            break;
        }
        let hole = state.holes[index];
        let delta = abs(input.local - hole.xy);
        let octagonal_distance = max(delta.x, delta.y)
            + (sqrt(2.0) - 1.0) * min(delta.x, delta.y);
        if (octagonal_distance <= hole.z) {
            discard;
        }
    }

    let source_mode = u32(state.header.z + 0.5);
    var color: vec4<f32>;
    if (source_mode == 2u) {
        color = state.diffuse;
    } else if (source_mode == 3u) {
        color = textureSample(fill_map, fill_sampler, input.uv);
    } else if (source_mode == 1u) {
        let mask = textureSample(base_map, base_sampler, input.uv);
        let fill_size = max(state.fill.xy, vec2<f32>(1.0));
        let texture_scale_magnitude = max(abs(state.header.y), 0.000001);
        let texture_scale = select(
            -texture_scale_magnitude,
            texture_scale_magnitude,
            state.header.y >= 0.0,
        );
        color = textureSample(
            fill_map,
            fill_sampler,
            input.source / texture_scale / fill_size,
        );
        color.a *= mask.a;
    } else {
        color = textureSample(base_map, base_sampler, input.uv);
    }

    let shader_mode = u32(state.header.w + 0.5);
    if (source_mode != 2u && shader_mode != 0u) {
        if (shader_mode == 4u) {
            color *= state.diffuse;
        } else {
            let grayscale = (color.r + color.g + color.b) * 0.333;
            if (shader_mode == 3u) {
                let highlight = state.params.z * grayscale * grayscale;
                let inverse = 1.0 - grayscale;
                let luminance = 1.0 - inverse * inverse + state.params.x * color.a;
                color = vec4<f32>(luminance, luminance, luminance, color.a)
                    * state.diffuse + vec4<f32>(highlight);
            } else {
                let grayscale_color = vec4<f32>(grayscale, grayscale, grayscale, color.a);
                let lightness = state.params.x * color.a;
                color = mix(grayscale_color, color, state.params.y);
                if (shader_mode == 1u) {
                    color *= state.diffuse;
                    color = vec4<f32>(color.rgb + vec3<f32>(lightness), color.a);
                } else {
                    color = vec4<f32>(color.rgb + vec3<f32>(lightness), color.a);
                    color = min(vec4<f32>(1.0), color) * state.diffuse;
                }
            }
        }
    }

    // Purple's ALPHA_FACTOR multiplies the complete fragment, not only A.
    return color * state.header.x;
}
