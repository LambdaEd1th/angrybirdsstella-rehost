use super::*;

#[allow(clippy::too_many_arguments)]
pub(crate) fn draw_region(
    texture: &RgbaImage,
    region: &SpriteRegion,
    transform: SpriteTransform,
    draw_size: Option<[f32; 2]>,
    pivot_override: Option<[f32; 2]>,
    target: &mut [u32],
    masked_texture: Option<(&RgbaImage, f64)>,
    masked_texture_matrix: Option<[f32; 6]>,
    shader: Option<&SpriteShader>,
) {
    // The native transform has already been quantized and composed in f32.
    // Widen only for the test-only software sampler's pixel traversal.
    let transform_x = f64::from(transform.x);
    let transform_y = f64::from(transform.y);
    let m00 = f64::from(transform.m00);
    let m01 = f64::from(transform.m01);
    let m10 = f64::from(transform.m10);
    let m11 = f64::from(transform.m11);
    let transform_alpha = f64::from(transform.alpha);
    let determinant = m00 * m11 - m01 * m10;
    if determinant.abs() < f64::EPSILON || transform_alpha <= 0.0 {
        return;
    }
    let [pivot_x, pivot_y] = pivot_override
        .map(|pivot| pivot.map(f64::from))
        .unwrap_or([f64::from(region.pivot_x), f64::from(region.pivot_y)]);
    let width = f64::from(region.width);
    let height = f64::from(region.height);
    let [draw_width, draw_height] = draw_size
        .map(|size| size.map(f64::from))
        .unwrap_or([width, height]);
    if draw_width.abs() < f64::EPSILON || draw_height.abs() < f64::EPSILON {
        return;
    }
    let map_point = |x: f64, y: f64| {
        let local_x = x - pivot_x;
        let local_y = y - pivot_y;
        (
            transform_x + m00 * local_x + m01 * local_y,
            transform_y + m10 * local_x + m11 * local_y,
        )
    };
    let corners = [
        map_point(0.0, 0.0),
        map_point(draw_width, 0.0),
        map_point(0.0, draw_height),
        map_point(draw_width, draw_height),
    ];
    let min_x = corners
        .iter()
        .map(|point| point.0)
        .fold(f64::INFINITY, f64::min)
        .floor()
        .max(0.0) as i32;
    let max_x = corners
        .iter()
        .map(|point| point.0)
        .fold(f64::NEG_INFINITY, f64::max)
        .ceil()
        .min(GAME_WIDTH as f64) as i32;
    let min_y = corners
        .iter()
        .map(|point| point.1)
        .fold(f64::INFINITY, f64::min)
        .floor()
        .max(0.0) as i32;
    let max_y = corners
        .iter()
        .map(|point| point.1)
        .fold(f64::NEG_INFINITY, f64::max)
        .ceil()
        .min(GAME_HEIGHT as f64) as i32;
    for output_y in min_y..max_y {
        for output_x in min_x..max_x {
            let relative_x = output_x as f64 + 0.5 - transform_x;
            let relative_y = output_y as f64 + 0.5 - transform_y;
            let display_x = (m11 * relative_x - m01 * relative_y) / determinant + pivot_x;
            let display_y = (-m10 * relative_x + m00 * relative_y) / determinant + pivot_y;
            let within_width = if draw_width >= 0.0 {
                (0.0..draw_width).contains(&display_x)
            } else {
                (draw_width..0.0).contains(&display_x)
            };
            let within_height = if draw_height >= 0.0 {
                (0.0..draw_height).contains(&display_y)
            } else {
                (draw_height..0.0).contains(&display_y)
            };
            if !within_width || !within_height {
                continue;
            }
            let source_x = display_x * width / draw_width;
            let source_y = display_y * height / draw_height;
            let local_x = display_x - pivot_x;
            let local_y = display_y - pivot_y;
            // Purple fixes GL_TEXTURE_MAG_FILTER to GL_LINEAR (0x2601) in
            // sub_10018AFB8. sub_100467760 constructs UVs directly from the
            // atlas rectangle boundaries (x/textureWidth through
            // (x+width)/textureWidth), so filtering is clamped only at the
            // complete texture edge rather than at each sprite rectangle.
            let normalized_x = display_x / draw_width;
            let normalized_y = display_y / draw_height;
            let atlas = region
                .native_atlas_corners()
                .map(|point| point.map(f64::from));
            let atlas_x = (atlas[1][0] - atlas[0][0]).mul_add(
                normalized_x,
                (atlas[2][0] - atlas[0][0]).mul_add(normalized_y, atlas[0][0]),
            );
            let atlas_y = (atlas[1][1] - atlas[0][1]).mul_add(
                normalized_x,
                (atlas[2][1] - atlas[0][1]).mul_add(normalized_y, atlas[0][1]),
            );
            let mask =
                super::texture::sample_bilinear_clamped(texture, atlas_x - 0.5, atlas_y - 0.5);
            let (source, mask_alpha) = if let Some((fill, texture_scale)) = masked_texture {
                let [fill_x, fill_y] = masked_texture_matrix.map_or(
                    [source_x, source_y],
                    |[tx, ty, m00, m01, m10, m11]| {
                        let [tx, ty, m00, m01, m10, m11] =
                            [tx, ty, m00, m01, m10, m11].map(f64::from);
                        [
                            tx + m00.mul_add(local_x, m01 * local_y),
                            ty + m10.mul_add(local_x, m11 * local_y),
                        ]
                    },
                );
                let texture_scale = if texture_scale < 0.0 {
                    -texture_scale.abs().max(0.000001)
                } else {
                    texture_scale.abs().max(0.000001)
                };
                (
                    super::texture::sample_bilinear_repeat(
                        fill,
                        fill_x / texture_scale - 0.5,
                        fill_y / texture_scale - 0.5,
                    ),
                    f64::from(mask[3]) / 255.0,
                )
            } else {
                (mask, 1.0)
            };
            let source = shader.map_or(source, |shader| {
                super::shader::apply_sprite_shader(source, shader)
            });
            let alpha = (f64::from(source[3]) * mask_alpha * transform_alpha).round() as u32;
            if alpha == 0 {
                continue;
            }
            let index = (output_y as u32 * GAME_WIDTH + output_x as u32) as usize;
            target[index] = super::color_mesh::alpha_blend(target[index], source, alpha.min(255));
        }
    }
}

pub(crate) fn draw_explicit_quad(
    texture: &RgbaImage,
    quad: RenderQuad,
    alpha: f64,
    clip_rect: Option<[i32; 4]>,
    target: &mut [u32],
) {
    if alpha <= 0.0
        || texture.width() == 0
        || texture.height() == 0
        || !quad
            .positions
            .into_iter()
            .flatten()
            .chain(quad.uv.into_iter().flatten())
            .all(f64::is_finite)
    {
        return;
    }
    let [clip_left, clip_top, clip_right, clip_bottom] =
        clip_rect.unwrap_or([0, 0, GAME_WIDTH as i32, GAME_HEIGHT as i32]);
    let min_x = quad
        .positions
        .iter()
        .map(|position| position[0])
        .fold(f64::INFINITY, f64::min)
        .floor()
        .max(f64::from(clip_left.max(0))) as i32;
    let max_x = quad
        .positions
        .iter()
        .map(|position| position[0])
        .fold(f64::NEG_INFINITY, f64::max)
        .ceil()
        .min(f64::from(clip_right.min(GAME_WIDTH as i32))) as i32;
    let min_y = quad
        .positions
        .iter()
        .map(|position| position[1])
        .fold(f64::INFINITY, f64::min)
        .floor()
        .max(f64::from(clip_top.max(0))) as i32;
    let max_y = quad
        .positions
        .iter()
        .map(|position| position[1])
        .fold(f64::NEG_INFINITY, f64::max)
        .ceil()
        .min(f64::from(clip_bottom.min(GAME_HEIGHT as i32))) as i32;
    let triangles = [[0usize, 1, 2], [2, 1, 3]];
    for output_y in min_y..max_y {
        for output_x in min_x..max_x {
            let point = [f64::from(output_x) + 0.5, f64::from(output_y) + 0.5];
            let mut interpolated_uv = None;
            for indices in triangles {
                let [a, b, c] = indices.map(|index| quad.positions[index]);
                let denominator = (b[1] - c[1]) * (a[0] - c[0]) + (c[0] - b[0]) * (a[1] - c[1]);
                if denominator.abs() <= f64::EPSILON {
                    continue;
                }
                let wa = ((b[1] - c[1]) * (point[0] - c[0]) + (c[0] - b[0]) * (point[1] - c[1]))
                    / denominator;
                let wb = ((c[1] - a[1]) * (point[0] - c[0]) + (a[0] - c[0]) * (point[1] - c[1]))
                    / denominator;
                let wc = 1.0 - wa - wb;
                if wa >= -1e-9 && wb >= -1e-9 && wc >= -1e-9 {
                    let [uv_a, uv_b, uv_c] = indices.map(|index| quad.uv[index]);
                    interpolated_uv = Some([
                        uv_a[0] * wa + uv_b[0] * wb + uv_c[0] * wc,
                        uv_a[1] * wa + uv_b[1] * wb + uv_c[1] * wc,
                    ]);
                    break;
                }
            }
            let Some([u, v]) = interpolated_uv else {
                continue;
            };
            let source = super::texture::sample_bilinear_clamped(
                texture,
                u * f64::from(texture.width()) - 0.5,
                v * f64::from(texture.height()) - 0.5,
            );
            let source_alpha = (f64::from(source[3]) * alpha.clamp(0.0, 1.0)).round() as u32;
            if source_alpha == 0 {
                continue;
            }
            let index = (output_y as u32 * GAME_WIDTH + output_x as u32) as usize;
            target[index] =
                super::color_mesh::alpha_blend(target[index], source, source_alpha.min(255));
        }
    }
}
