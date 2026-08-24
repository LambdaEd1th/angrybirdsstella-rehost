@group(0) @binding(0)
var game_texture: texture_2d<f32>;
@group(0) @binding(1)
var game_sampler: sampler;

struct BlitVertex {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

@vertex
fn blit_vertex(@builtin(vertex_index) index: u32) -> BlitVertex {
    var output: BlitVertex;
    if (index == 0u) {
        output.position = vec4<f32>(-1.0, -1.0, 0.0, 1.0);
        output.uv = vec2<f32>(0.0, 1.0);
    } else if (index == 1u) {
        output.position = vec4<f32>(3.0, -1.0, 0.0, 1.0);
        output.uv = vec2<f32>(2.0, 1.0);
    } else {
        output.position = vec4<f32>(-1.0, 3.0, 0.0, 1.0);
        output.uv = vec2<f32>(0.0, -1.0);
    }
    return output;
}

@fragment
fn blit_fragment(input: BlitVertex) -> @location(0) vec4<f32> {
    return textureSample(game_texture, game_sampler, input.uv);
}
