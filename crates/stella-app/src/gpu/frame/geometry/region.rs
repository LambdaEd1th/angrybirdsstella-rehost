//! Atlas region UV construction, pivot setup and four-vertex transform.

use super::{super::*, shader::shader_uniform};
use crate::gpu::frame::batch::screen_to_clip;

fn native_masked_quad_visible(positions: &[[f32; 2]; 4], resolution: GameResolution) -> bool {
    // TexturizedSprite's batch append (`sub_10008D428`) transforms the quad
    // to clip space first, then rejects it unless max x/y are >= -1 and min
    // x/y are < 1. Preserve those inclusive/strict edges and float32 FMADDs.
    let clip = positions.map(|position| screen_to_clip(position, resolution));
    let min_x = clip
        .iter()
        .map(|position| position[0])
        .fold(f32::INFINITY, f32::min);
    let min_y = clip
        .iter()
        .map(|position| position[1])
        .fold(f32::INFINITY, f32::min);
    let max_x = clip
        .iter()
        .map(|position| position[0])
        .fold(f32::NEG_INFINITY, f32::max);
    let max_y = clip
        .iter()
        .map(|position| position[1])
        .fold(f32::NEG_INFINITY, f32::max);
    max_x >= -1.0 && max_y >= -1.0 && min_x < 1.0 && min_y < 1.0
}

#[allow(clippy::too_many_arguments)]
pub(in crate::gpu) fn append_gpu_region(
    frame: &mut PreparedFrame,
    region: &SpriteRegion,
    transform: SpriteTransform,
    draw_size: Option<[f64; 2]>,
    pivot_override: Option<[f64; 2]>,
    base_texture: String,
    base_width: u32,
    base_height: u32,
    fill_texture: String,
    fill_width: u32,
    fill_height: u32,
    texture_scale: f64,
    masked_texture_matrix: Option<[f64; 6]>,
    source_mode: f32,
    program: NativeProgram,
    shader: Option<&SpriteShader>,
    clip_holes: &[RenderHole],
) {
    // Sprite::Draw (`sub_100467BE8`) always transforms and submits all four
    // vertices. In particular it has no determinant epsilon: near-zero
    // animation scales remain real subpixel triangles and are left to the GPU
    // rasterizer. Only host-invalid empty resources and fully transparent
    // commands can be discarded without changing visible native geometry.
    if transform.alpha <= 0.0 || region.width == 0 || region.height == 0 {
        return;
    }
    let [pivot_x, pivot_y] = pivot_override
        .map(|pivot| pivot.map(|value| value as f32))
        .unwrap_or([f32::from(region.pivot_x), f32::from(region.pivot_y)]);
    let width = f32::from(region.width);
    let height = f32::from(region.height);
    let source_points = [(0.0, 0.0), (width, 0.0), (0.0, height), (width, height)];
    let [draw_width, draw_height] = draw_size
        .map(|size| size.map(|value| value as f32))
        .unwrap_or([width, height]);
    let display_points = [
        (0.0, 0.0),
        (draw_width, 0.0),
        (0.0, draw_height),
        (draw_width, draw_height),
    ];
    let positions = display_points.map(|(x, y)| {
        let local_x = x - pivot_x;
        let local_y = y - pivot_y;
        transform.transform_point(local_x, local_y)
    });
    if program == NativeProgram::SpriteAlphaMasked
        && !native_masked_quad_visible(&positions, frame.resolution)
    {
        return;
    }
    if std::env::var_os("STELLA_TRACE_GPU_REGIONS").is_some() {
        let min_x = positions
            .iter()
            .map(|position| position[0])
            .fold(f32::INFINITY, f32::min);
        let min_y = positions
            .iter()
            .map(|position| position[1])
            .fold(f32::INFINITY, f32::min);
        let max_x = positions
            .iter()
            .map(|position| position[0])
            .fold(f32::NEG_INFINITY, f32::max);
        let max_y = positions
            .iter()
            .map(|position| position[1])
            .fold(f32::NEG_INFINITY, f32::max);
        eprintln!(
            "gpu region {:?}: ({min_x:.2},{min_y:.2})-({max_x:.2},{max_y:.2})",
            region.name
        );
    }
    let local = display_points.map(|(x, y)| [x - pivot_x, y - pivot_y]);
    let source = if let Some([tx, ty, m00, m01, m10, m11]) = masked_texture_matrix {
        let [tx, ty, m00, m01, m10, m11] = [tx, ty, m00, m01, m10, m11].map(|value| value as f32);
        local.map(|[x, y]| [tx + m00.mul_add(x, m01 * y), ty + m10.mul_add(x, m11 * y)])
    } else {
        source_points.map(|(x, y)| [x, y])
    };
    let uv = region.native_uvs(base_width as f32, base_height as f32);
    let mut uniform = shader_uniform(shader);
    uniform.header[0] = transform.alpha;
    uniform.header[1] = texture_scale as f32;
    uniform.header[2] = source_mode;
    uniform.fill[0] = fill_width.max(1) as f32;
    uniform.fill[1] = fill_height.max(1) as f32;
    let hole_count = clip_holes.len().min(MAX_HOLES);
    uniform.params[3] = hole_count as f32;
    for (target, source) in uniform.holes.iter_mut().zip(clip_holes).take(hole_count) {
        *target = [source.x as f32, source.y as f32, source.radius as f32, 0.0];
    }
    frame.push_quad(
        positions,
        uv,
        source,
        local,
        uniform,
        base_texture,
        fill_texture,
        program,
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn masked_quad_culling_keeps_native_clip_edge_strictness() {
        let resolution = GameResolution::default();
        assert!(native_masked_quad_visible(
            &[[0.0, 10.0], [20.0, 10.0], [0.0, 30.0], [20.0, 30.0]],
            resolution,
        ));
        assert!(native_masked_quad_visible(
            &[[-20.0, 10.0], [0.0, 10.0], [-20.0, 30.0], [0.0, 30.0],],
            resolution,
        ));
        assert!(!native_masked_quad_visible(
            &[
                [GAME_WIDTH as f32, 10.0],
                [GAME_WIDTH as f32 + 20.0, 10.0],
                [GAME_WIDTH as f32, 30.0],
                [GAME_WIDTH as f32 + 20.0, 30.0],
            ],
            resolution,
        ));
        assert!(!native_masked_quad_visible(
            &[[10.0, -20.0], [30.0, -20.0], [10.0, 0.0], [30.0, 0.0],],
            resolution,
        ));
    }
}
