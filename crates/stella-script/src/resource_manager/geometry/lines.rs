//! Native textured-line and rubber-band quad construction.

use crate::{RenderCommand, RenderState, SpriteCatalogRegion, SpriteGeometrySubmission};
use std::sync::Arc;

use super::model::SpriteGeometry;

impl SpriteGeometry {
    pub(crate) fn native_textured_line_command(
        self,
        sprite: String,
        bound_region: Option<SpriteCatalogRegion>,
        render_state: RenderState,
        start: (f64, f64),
        end: (f64, f64),
        width: f64,
    ) -> Option<RenderCommand> {
        // sub_100084A9C converts all nine numeric Lua arguments to float32
        // before entering sub_10006DB0C. The latter contains only scalar and
        // packed single-precision arithmetic; keeping f64 here changes both
        // the one-pixel rejection and the generated quad at large positions.
        let x1 = start.0 as f32;
        let y1 = start.1 as f32;
        let x2 = end.0 as f32;
        let y2 = end.1 as f32;
        let width = width as f32;
        let translate_x = render_state.translate_x as f32;
        let translate_y = render_state.translate_y as f32;
        let scale_x = render_state.scale_x as f32;
        let scale_y = render_state.scale_y as f32;
        let pivot_x = render_state.pivot_x as f32;
        let pivot_y = render_state.pivot_y as f32;
        // Purple sub_10006DB0C treats the first endpoint as the rectangle
        // origin and rotates only the endpoint delta around the GL state
        // pivot. It then applies the independent context scales to the
        // resulting coordinates before rejecting sub-pixel lines.
        let (sine, cosine) = (render_state.angle as f32).sin_cos();
        let delta_x = x2 - x1;
        let delta_y = y2 - y1;

        // Mirror the two packed FMLA instructions used for each endpoint.
        let start_x = (-cosine).mul_add(pivot_x, translate_x + pivot_x + x1);
        let start_x = sine.mul_add(pivot_y, start_x);
        let start_y = (-sine).mul_add(pivot_x, translate_y + pivot_y + y1);
        let start_y = (-cosine).mul_add(pivot_y, start_y);
        let end_x = cosine.mul_add(delta_x, start_x);
        let end_x = (-sine).mul_add(delta_y, end_x);
        let end_y = sine.mul_add(delta_x, start_y);
        let end_y = cosine.mul_add(delta_y, end_y);
        let screen_delta_x = scale_x * (end_x - start_x);
        let screen_delta_y = scale_y * (end_y - start_y);
        if screen_delta_x * screen_delta_x + screen_delta_y * screen_delta_y < 1.0_f32 {
            return None;
        }

        // 0x10006DC30..0x10006DC5C calculates reciprocal length first, then
        // performs separate multiply/half-width operations. That rounding
        // sequence is observable and differs from width * 0.5 / length.
        let local_length = delta_x.mul_add(delta_x, delta_y * delta_y).sqrt();
        let inverse_length = 1.0_f32 / local_length;
        let half_x = ((delta_x * inverse_length) * width) * 0.5_f32;
        let half_y_negative = ((delta_y * inverse_length) * width) * -0.5_f32;
        let normal_x = (-sine).mul_add(half_x, cosine * half_y_negative);
        // This seemingly asymmetric scale_x term is intentional. It is the
        // exact instruction sequence at 0x10006DC78..0x10006DC8C, including
        // Purple's non-uniform-scale quirk.
        let normal_y = sine.mul_add(scale_x * normal_x, cosine * half_x);

        let start = [scale_x * start_x, scale_y * start_y];
        let end = [scale_x * end_x, scale_y * end_y];
        let normal = [scale_x * normal_x, scale_y * normal_y];

        // Native emits a triangle strip as start+, start-, end+, end-. Map it
        // to this renderer's TL, TR, BL, BR quad order without changing UVs.
        let top_left = [start[0] - normal[0], start[1] - normal[1]];
        let top_right = [end[0] - normal[0], end[1] - normal[1]];
        let bottom_left = [start[0] + normal[0], start[1] + normal[1]];
        let bottom_right = [end[0] + normal[0], end[1] + normal[1]];
        let source_width = (self.width().abs() as f32).max(f32::EPSILON);
        let source_height = (self.height().abs() as f32).max(f32::EPSILON);
        let m00 = (top_right[0] - top_left[0]) / source_width;
        let m10 = (top_right[1] - top_left[1]) / source_width;
        let m01 = (bottom_left[0] - top_left[0]) / source_height;
        let m11 = (bottom_left[1] - top_left[1]) / source_height;
        let origin_x = (-m00).mul_add(
            self.min_x as f32,
            (-m01).mul_add(self.min_y as f32, top_left[0]),
        );
        let origin_y = (-m10).mul_add(
            self.min_x as f32,
            (-m11).mul_add(self.min_y as f32, top_left[1]),
        );

        Some(RenderCommand {
            order: 0,
            sprite: sprite.into(),
            texture: None,
            texture_scale: 1.0,
            masked_texture_binding: None,
            bound_region: bound_region.map(Arc::new),
            bound_composite: None,
            geometry: Some(SpriteGeometrySubmission::NativeAtlasQuad(Arc::new(
                [top_left, top_right, bottom_left, bottom_right].map(|point| point.map(f64::from)),
            ))),
            shader: None,
            clip_holes: Vec::new(),
            dirt: None,
            x: f64::from(origin_x),
            y: f64::from(origin_y),
            state: RenderState {
                matrix: Some([m00, m01, m10, m11].map(f64::from)),
                alpha: render_state.alpha,
                clip_rect: render_state.clip_rect,
                ..RenderState::default()
            },
            world_space: true,
        })
    }

    pub(crate) fn native_rubberband_command(
        self,
        sprite: String,
        bound_region: Option<SpriteCatalogRegion>,
        render_state: RenderState,
        start: (f64, f64),
        end: (f64, f64),
        width: f64,
    ) -> Option<RenderCommand> {
        // sub_100030EB0 consumes five single-precision arguments. It obtains
        // the segment angle with double atan2 after float32 subtraction,
        // rounds the angle back to float, then uses double sincos/FMA for the
        // two leading vertices. The opposite pair is produced by separate
        // float sub/add instructions rather than by parallelogram algebra.
        // Keeping that unusual sequence matters when the resting sling has a
        // non-zero length of only a few float32 ULPs.
        let x1 = start.0 as f32;
        let y1 = start.1 as f32;
        let x2 = end.0 as f32;
        let y2 = end.1 as f32;
        let width = width as f32;
        let delta_x = x2 - x1;
        let delta_y = y2 - y1;
        let angle = f64::from(delta_y).atan2(f64::from(delta_x)) as f32;
        let length = delta_y.mul_add(delta_y, delta_x * delta_x).sqrt();
        let half_width = width * 0.5_f32;

        // Exact constant loaded at 0x1009AE794 (3*pi/2 as float32).
        let perpendicular_angle = angle + f32::from_bits(0x4096_CBE4);
        let (perpendicular_sine, perpendicular_cosine) = f64::from(perpendicular_angle).sin_cos();
        let top_start = [
            (f64::from(half_width).mul_add(perpendicular_cosine, f64::from(x1))) as f32,
            (f64::from(half_width).mul_add(perpendicular_sine, f64::from(y1))) as f32,
        ];
        let (sine, cosine) = f64::from(angle).sin_cos();
        let top_end = [
            (f64::from(length).mul_add(cosine, f64::from(top_start[0]))) as f32,
            (f64::from(length).mul_add(sine, f64::from(top_start[1]))) as f32,
        ];
        let doubled_normal_x = (top_start[0] - x1) + (top_start[0] - x1);
        let doubled_normal_y = (top_start[1] - y1) + (top_start[1] - y1);
        let bottom_end = [top_end[0] - doubled_normal_x, top_end[1] - doubled_normal_y];
        let bottom_start = [
            top_start[0] - doubled_normal_x,
            top_start[1] - doubled_normal_y,
        ];

        // Preserve the old affine representation as a CPU fallback, but give
        // wgpu all four independently rounded native corners below.
        let source_width = self.width().abs() as f32;
        let source_height = self.height().abs() as f32;
        let m00 = (bottom_start[0] - top_start[0]) / source_width;
        let m10 = (bottom_start[1] - top_start[1]) / source_width;
        let m01 = (top_end[0] - top_start[0]) / source_height;
        let m11 = (top_end[1] - top_start[1]) / source_height;
        let origin_x = (-m00).mul_add(
            self.min_x as f32,
            (-m01).mul_add(self.min_y as f32, top_start[0]),
        );
        let origin_y = (-m10).mul_add(
            self.min_x as f32,
            (-m11).mul_add(self.min_y as f32, top_start[1]),
        );

        Some(RenderCommand {
            order: 0,
            sprite: sprite.into(),
            texture: None,
            texture_scale: 1.0,
            masked_texture_binding: None,
            bound_region: bound_region.map(Arc::new),
            bound_composite: None,
            geometry: Some(SpriteGeometrySubmission::NativeAtlasQuad(Arc::new(
                [top_start, bottom_start, top_end, bottom_end].map(|point| point.map(f64::from)),
            ))),
            shader: None,
            clip_holes: Vec::new(),
            dirt: None,
            x: f64::from(origin_x),
            y: f64::from(origin_y),
            state: RenderState {
                matrix: Some([m00, m01, m10, m11].map(f64::from)),
                alpha: render_state.alpha,
                clip_rect: render_state.clip_rect,
                ..RenderState::default()
            },
            world_space: true,
        })
    }
}
