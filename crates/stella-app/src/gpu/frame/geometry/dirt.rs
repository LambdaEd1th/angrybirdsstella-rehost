//! DrawablePolygon/Dirt transient triangle-stream projection.

use super::{super::*, shader::shader_uniform};

pub(in crate::gpu) fn append_gpu_dirt_triangles(
    frame: &mut PreparedFrame,
    triangles: &[RenderTriangle],
    transform: SpriteTransform,
    texture: String,
) {
    if triangles.is_empty() {
        return;
    }
    let mut positions = Vec::with_capacity(triangles.len() * 3);
    let mut uv = Vec::with_capacity(triangles.len() * 3);
    let mut source = Vec::with_capacity(triangles.len() * 3);
    let mut local = Vec::with_capacity(triangles.len() * 3);
    for triangle in triangles {
        for [x, y] in triangle.vertices {
            let x = x as f32;
            let y = y as f32;
            let pixel_x = x * 20.0;
            let pixel_y = y * 20.0;
            positions.push(transform.transform_point(pixel_x, pixel_y));
            // DrawablePolygon passes its unscaled local physics coordinates
            // as TEX0. The material texture is configured to repeat.
            uv.push([x, y]);
            source.push([x, y]);
            local.push([pixel_x, pixel_y]);
        }
    }
    let mut uniform = shader_uniform(None);
    // 2d-sprite.fx does not enable ALPHA_FACTOR or blending for these native
    // drawables, even though the generic helper still uploads that uniform.
    uniform.header[0] = 1.0;
    uniform.header[2] = 3.0;
    frame.push_mesh(
        &positions,
        &uv,
        &source,
        &local,
        uniform,
        WHITE_TEXTURE.to_owned(),
        texture,
        NativeProgram::Sprite,
    );
}
