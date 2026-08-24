//! Recovered sprite pixel-program selection and uniform publication.

use super::super::*;

pub(in crate::gpu) fn shader_uniform(shader: Option<&SpriteShader>) -> DrawUniform {
    let mut uniform = DrawUniform {
        header: [1.0, 1.0, 0.0, 0.0],
        diffuse: [1.0; 4],
        params: [0.0, 1.0, 0.0, 0.0],
        fill: [1.0, 1.0, 0.0, 0.0],
        holes: [[0.0; 4]; MAX_HOLES],
    };
    let Some(shader) = shader else {
        return uniform;
    };
    uniform.diffuse = shader.diffuse.map(|value| value as f32);
    uniform.params[0] = shader.lightness as f32;
    uniform.params[1] = shader.saturation as f32;
    uniform.params[2] = shader.highlight as f32;
    uniform.header[3] = if shader.name.starts_with("2d-sprite-colorize") {
        1.0
    } else if shader.name.starts_with("2d-sprite-silhouette") {
        2.0
    } else if shader.name.starts_with("2d-sprite-gold") {
        3.0
    } else if shader.name.starts_with("2d-sprite-diffuse-modulate") {
        4.0
    } else {
        0.0
    };
    uniform
}
