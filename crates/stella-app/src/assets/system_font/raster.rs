//! Embedded glyph decoding, vector outlines and native text-pass compositing.

use std::{collections::HashMap, sync::Arc};

use ab_glyph::{Font, FontRef, GlyphId, OutlineCurve};
use anyhow::{Result, anyhow};
use image::{Rgba, RgbaImage};
use tiny_skia::{Mask, Path, PathBuilder};
use ttf_parser::{RasterGlyphImage, RasterImageFormat};

use super::{
    NativeSystemDecodedRaster, NativeSystemPlacedRaster, NativeSystemRasterColor,
    SystemFontLayoutFace, SystemFontShapedLine, color_outline::render_system_color_outline,
};

#[allow(clippy::too_many_arguments)]
pub(super) fn append_line_glyphs(
    fonts: &[FontRef<'_>],
    parser_faces: &[ttf_parser::Face<'_>],
    face_scales: &[f32],
    layout_faces: &[SystemFontLayoutFace],
    point_size: i32,
    line: &SystemFontShapedLine,
    start_x: f32,
    baseline: f32,
    paths: &mut Vec<Path>,
    rasters: &mut Vec<NativeSystemPlacedRaster>,
    raster_cache: &mut HashMap<(u16, u16), Option<Arc<NativeSystemDecodedRaster>>>,
    foreground_rgba: [u8; 4],
) -> Result<()> {
    let requested_ppem = point_size.clamp(1, i32::from(u16::MAX)) as u16;
    for shaped in &line.glyphs {
        let face_slot = usize::from(shaped.face_slot);
        let (Some(font), Some(parser_face), Some(layout_face), Some(&unit_scale)) = (
            fonts.get(face_slot),
            parser_faces.get(face_slot),
            layout_faces.get(face_slot),
            face_scales.get(face_slot),
        ) else {
            continue;
        };
        let glyph_id = GlyphId(shaped.glyph_id);
        let x = start_x + shaped.x as f32;
        let y = baseline - shaped.y as f32;
        let key = (shaped.face_slot, shaped.glyph_id);
        let raster = if let Some(cached) = raster_cache.get(&key) {
            cached.clone()
        } else {
            let decoded = if let Some(image) =
                parser_face.glyph_raster_image(ttf_parser::GlyphId(shaped.glyph_id), requested_ppem)
            {
                Some(Arc::new(decode_system_raster(
                    image,
                    parser_face.tables().sbix.is_some(),
                    parser_face.glyph_bounding_box(ttf_parser::GlyphId(shaped.glyph_id)),
                )?))
            } else if parser_face.is_color_glyph(ttf_parser::GlyphId(shaped.glyph_id)) {
                render_system_color_outline(
                    layout_face,
                    parser_face,
                    shaped.glyph_id,
                    point_size,
                    foreground_rgba,
                )?
                .map(Arc::new)
            } else {
                None
            };
            raster_cache.insert(key, decoded.clone());
            decoded
        };
        if let Some(raster) = raster {
            let raster_em_size = layout_face.native_raster_em_size(point_size) as f32;
            let raster_scale = raster_em_size / f32::from(raster.pixels_per_em);
            let [apple_x, apple_down] = layout_face.native_raster_origin_offset(point_size);
            let (bbox_x, bbox_down) = if raster.sbix && !layout_face.native_is_apple_color_emoji() {
                raster.glyph_bbox.map_or((0.0, 0.0), |bbox| {
                    (
                        -f32::from(bbox.x_min) * unit_scale,
                        -f32::from(bbox.y_min) * unit_scale,
                    )
                })
            } else {
                (0.0, 0.0)
            };
            rasters.push(NativeSystemPlacedRaster {
                left: x + apple_x as f32 + bbox_x + f32::from(raster.x) * raster_scale,
                top: y + apple_down as f32 + bbox_down
                    - (raster.image.height() as f32 + f32::from(raster.y)) * raster_scale,
                scale: raster_scale,
                raster,
            });
            continue;
        }
        if let Some(path) = glyph_outline_path(font, glyph_id, unit_scale, x, y) {
            paths.push(path);
        }
    }
    Ok(())
}

pub(super) fn decode_system_raster(
    raster: RasterGlyphImage<'_>,
    sbix: bool,
    glyph_bbox: Option<ttf_parser::Rect>,
) -> Result<NativeSystemDecodedRaster> {
    let (image, color) = match raster.format {
        RasterImageFormat::PNG => (
            image::load_from_memory_with_format(raster.data, image::ImageFormat::Png)
                .map_err(|error| anyhow!("invalid embedded PNG system glyph: {error}"))?
                .into_rgba8(),
            NativeSystemRasterColor::Intrinsic,
        ),
        RasterImageFormat::BitmapPremulBgra32 => (
            decode_system_bgra32(raster.width, raster.height, raster.data)?,
            NativeSystemRasterColor::Intrinsic,
        ),
        RasterImageFormat::BitmapMono => (
            decode_system_coverage(raster.width, raster.height, raster.data, 1, true)?,
            NativeSystemRasterColor::Foreground,
        ),
        RasterImageFormat::BitmapMonoPacked => (
            decode_system_coverage(raster.width, raster.height, raster.data, 1, false)?,
            NativeSystemRasterColor::Foreground,
        ),
        RasterImageFormat::BitmapGray2 => (
            decode_system_coverage(raster.width, raster.height, raster.data, 2, true)?,
            NativeSystemRasterColor::Foreground,
        ),
        RasterImageFormat::BitmapGray2Packed => (
            decode_system_coverage(raster.width, raster.height, raster.data, 2, false)?,
            NativeSystemRasterColor::Foreground,
        ),
        RasterImageFormat::BitmapGray4 => (
            decode_system_coverage(raster.width, raster.height, raster.data, 4, true)?,
            NativeSystemRasterColor::Foreground,
        ),
        RasterImageFormat::BitmapGray4Packed => (
            decode_system_coverage(raster.width, raster.height, raster.data, 4, false)?,
            NativeSystemRasterColor::Foreground,
        ),
        RasterImageFormat::BitmapGray8 => (
            decode_system_coverage(raster.width, raster.height, raster.data, 8, true)?,
            NativeSystemRasterColor::Foreground,
        ),
    };
    if image.width() == 0 || image.height() == 0 || raster.pixels_per_em == 0 {
        return Err(anyhow!(
            "embedded system glyph has zero-sized raster metrics"
        ));
    }
    Ok(NativeSystemDecodedRaster {
        image,
        color,
        x: raster.x,
        y: raster.y,
        pixels_per_em: raster.pixels_per_em,
        sbix,
        glyph_bbox,
    })
}

pub(super) fn decode_system_coverage(
    width: u16,
    height: u16,
    data: &[u8],
    bits_per_pixel: usize,
    row_padded: bool,
) -> Result<RgbaImage> {
    let width = usize::from(width);
    let height = usize::from(height);
    let row_bits = width
        .checked_mul(bits_per_pixel)
        .ok_or_else(|| anyhow!("embedded system glyph row overflow"))?;
    let row_stride_bits = if row_padded {
        row_bits
            .checked_add(7)
            .map(|bits| bits / 8 * 8)
            .ok_or_else(|| anyhow!("embedded system glyph stride overflow"))?
    } else {
        row_bits
    };
    let required_bits = if row_padded {
        row_stride_bits.checked_mul(height)
    } else {
        row_bits.checked_mul(height)
    }
    .ok_or_else(|| anyhow!("embedded system glyph allocation overflow"))?;
    if data.len().saturating_mul(8) < required_bits {
        return Err(anyhow!("truncated embedded system glyph bitmap"));
    }
    let mut image = RgbaImage::new(width as u32, height as u32);
    let maximum = (1_u16 << bits_per_pixel) - 1;
    for y in 0..height {
        for x in 0..width {
            let bit = if row_padded {
                y * row_stride_bits + x * bits_per_pixel
            } else {
                (y * width + x) * bits_per_pixel
            };
            let byte = data[bit / 8];
            let shift = 8 - bits_per_pixel - bit % 8;
            let value = u16::from((byte >> shift) & maximum as u8);
            let alpha = ((value * 255 + maximum / 2) / maximum) as u8;
            image.put_pixel(x as u32, y as u32, Rgba([255, 255, 255, alpha]));
        }
    }
    Ok(image)
}

pub(super) fn decode_system_bgra32(width: u16, height: u16, data: &[u8]) -> Result<RgbaImage> {
    let pixels = usize::from(width)
        .checked_mul(usize::from(height))
        .ok_or_else(|| anyhow!("embedded BGRA system glyph allocation overflow"))?;
    let required = pixels
        .checked_mul(4)
        .ok_or_else(|| anyhow!("embedded BGRA system glyph byte-size overflow"))?;
    if data.len() < required {
        return Err(anyhow!("truncated embedded BGRA system glyph"));
    }
    let mut image = RgbaImage::new(u32::from(width), u32::from(height));
    for (pixel, source) in image
        .pixels_mut()
        .zip(data[..required].as_chunks::<4>().0.iter())
    {
        let alpha = source[3];
        let unpremultiply = |channel: u8| {
            if alpha == 0 {
                0
            } else {
                (u16::from(channel) * 255 / u16::from(alpha)).min(255) as u8
            }
        };
        *pixel = Rgba([
            unpremultiply(source[2]),
            unpremultiply(source[1]),
            unpremultiply(source[0]),
            alpha,
        ]);
    }
    Ok(image)
}

pub(super) fn composite_system_mask(image: &mut RgbaImage, mask: &Mask, color: [u8; 4]) {
    for (index, pixel) in image.pixels_mut().enumerate() {
        let mut target = pixel.0.map(|channel| f32::from(channel) / 255.0);
        over_layer(&mut target, color, f32::from(mask.data()[index]) / 255.0);
        *pixel = Rgba(target.map(float_channel));
    }
}

pub(super) fn composite_system_rasters(
    target: &mut RgbaImage,
    rasters: &[NativeSystemPlacedRaster],
    foreground: [u8; 4],
) {
    for placed in rasters {
        if !placed.scale.is_finite() || placed.scale <= 0.0 {
            continue;
        }
        let source = &placed.raster.image;
        let right = placed.left + source.width() as f32 * placed.scale;
        let bottom = placed.top + source.height() as f32 * placed.scale;
        let left_pixel = placed.left.floor().max(0.0) as u32;
        let top_pixel = placed.top.floor().max(0.0) as u32;
        let right_pixel = right.ceil().max(0.0).min(target.width() as f32) as u32;
        let bottom_pixel = bottom.ceil().max(0.0).min(target.height() as f32) as u32;
        for y in top_pixel..bottom_pixel {
            for x in left_pixel..right_pixel {
                let source_x = (x as f32 + 0.5 - placed.left) / placed.scale - 0.5;
                let source_y = (y as f32 + 0.5 - placed.top) / placed.scale - 0.5;
                let sampled = sample_system_raster(
                    source,
                    source_x,
                    source_y,
                    placed.raster.color,
                    foreground,
                );
                if sampled[3] <= 0.0 {
                    continue;
                }
                let pixel = target.get_pixel_mut(x, y);
                let mut destination = pixel.0.map(|channel| f32::from(channel) / 255.0);
                over_premultiplied(&mut destination, sampled);
                *pixel = Rgba(destination.map(float_channel));
            }
        }
    }
}

fn sample_system_raster(
    image: &RgbaImage,
    x: f32,
    y: f32,
    color: NativeSystemRasterColor,
    foreground: [u8; 4],
) -> [f32; 4] {
    let x0 = x.floor() as i32;
    let y0 = y.floor() as i32;
    let tx = x - x0 as f32;
    let ty = y - y0 as f32;
    let mut sampled = [0.0_f32; 4];
    for (sample_y, weight_y) in [(y0, 1.0 - ty), (y0 + 1, ty)] {
        for (sample_x, weight_x) in [(x0, 1.0 - tx), (x0 + 1, tx)] {
            if sample_x < 0
                || sample_y < 0
                || sample_x >= image.width() as i32
                || sample_y >= image.height() as i32
            {
                continue;
            }
            let pixel = image.get_pixel(sample_x as u32, sample_y as u32).0;
            let coverage = f32::from(pixel[3]) / 255.0;
            let alpha = coverage
                * if color == NativeSystemRasterColor::Foreground {
                    f32::from(foreground[3]) / 255.0
                } else {
                    1.0
                };
            let rgb = if color == NativeSystemRasterColor::Foreground {
                foreground
            } else {
                pixel
            };
            let weight = weight_x * weight_y;
            for channel in 0..3 {
                sampled[channel] += f32::from(rgb[channel]) / 255.0 * alpha * weight;
            }
            sampled[3] += alpha * weight;
        }
    }
    sampled
}

fn over_premultiplied(target: &mut [f32; 4], source: [f32; 4]) {
    let inverse = 1.0 - source[3].clamp(0.0, 1.0);
    for channel in 0..3 {
        target[channel] = source[channel] + target[channel] * inverse;
    }
    target[3] = source[3] + target[3] * inverse;
}

pub(super) fn glyph_outline_path(
    font: &FontRef<'_>,
    glyph_id: GlyphId,
    unit_scale: f32,
    origin_x: f32,
    baseline: f32,
) -> Option<Path> {
    let outline = font.outline(glyph_id)?;
    let transform_point = |point: ab_glyph::Point| {
        tiny_skia::Point::from_xy(
            origin_x + point.x * unit_scale,
            baseline - point.y * unit_scale,
        )
    };
    let mut builder = PathBuilder::new();
    let mut contour_start = None;
    let mut last_end = None;
    for curve in &outline.curves {
        let (start, end) = match curve {
            OutlineCurve::Line(start, end) => (*start, *end),
            OutlineCurve::Quad(start, _, end) => (*start, *end),
            OutlineCurve::Cubic(start, _, _, end) => (*start, *end),
        };
        if last_end != Some(start) {
            if contour_start.is_some() {
                builder.close();
            }
            let device_start = transform_point(start);
            builder.move_to(device_start.x, device_start.y);
            contour_start = Some(start);
        }
        match curve {
            OutlineCurve::Line(_, end) => {
                let end = transform_point(*end);
                builder.line_to(end.x, end.y);
            }
            OutlineCurve::Quad(_, control, end) => {
                let control = transform_point(*control);
                let end = transform_point(*end);
                builder.quad_to(control.x, control.y, end.x, end.y);
            }
            OutlineCurve::Cubic(_, control_a, control_b, end) => {
                let control_a = transform_point(*control_a);
                let control_b = transform_point(*control_b);
                let end = transform_point(*end);
                builder.cubic_to(
                    control_a.x,
                    control_a.y,
                    control_b.x,
                    control_b.y,
                    end.x,
                    end.y,
                );
            }
        }
        last_end = Some(end);
        if contour_start == Some(end) {
            builder.close();
            contour_start = None;
            last_end = None;
        }
    }
    if contour_start.is_some() {
        builder.close();
    }
    builder.finish()
}

fn over_layer(target: &mut [f32; 4], color: [u8; 4], coverage: f32) {
    let alpha = coverage.clamp(0.0, 1.0) * f32::from(color[3]) / 255.0;
    let inverse = 1.0 - alpha;
    for channel in 0..3 {
        target[channel] = f32::from(color[channel]) / 255.0 * alpha + target[channel] * inverse;
    }
    target[3] = alpha + target[3] * inverse;
}

fn float_channel(value: f32) -> u8 {
    (value.clamp(0.0, 1.0) * 255.0 + 0.5) as u8
}
