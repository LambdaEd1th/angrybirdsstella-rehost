//! UIKit SystemFont label-cache equivalent backed by cross-platform outlines.

use ab_glyph::{Font, FontRef, GlyphId, OutlineCurve, PxScale, ScaleFont};
use anyhow::{Result, anyhow};
use image::{Rgba, RgbaImage};
use std::sync::Arc;
use tiny_skia::{FillRule, LineJoin, Mask, Path, PathBuilder, Stroke, Transform};
use ttf_parser::{RasterGlyphImage, RasterImageFormat};

use super::*;

mod cache;
mod color_outline;
mod placement;

use cache::LABEL_POOL_BYTE_LIMIT;
#[cfg(test)]
use cache::system_label_lifetime_key;
pub(crate) use cache::{SystemLabelPool, native_system_label_hash};
use color_outline::render_system_color_outline;
pub(crate) use placement::{
    native_system_label_horizontal_anchor, native_system_label_offset,
    native_system_label_vertical_anchor,
};

pub(crate) struct RasterizedSystemLabel {
    pub(crate) image: RgbaImage,
    pub(crate) horizontal_anchor: i32,
    pub(crate) vertical_anchor: i32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum NativeSystemRasterColor {
    Intrinsic,
    Foreground,
}

#[derive(Debug)]
struct NativeSystemDecodedRaster {
    image: RgbaImage,
    color: NativeSystemRasterColor,
    x: i16,
    y: i16,
    pixels_per_em: u16,
    sbix: bool,
    glyph_bbox: Option<ttf_parser::Rect>,
}

#[derive(Debug, Clone)]
struct NativeSystemPlacedRaster {
    raster: Arc<NativeSystemDecodedRaster>,
    left: f32,
    top: f32,
    scale: f32,
}

pub(crate) fn rasterize_system_label(
    binding: &SystemFontRenderBinding,
    text: &str,
    horizontal_anchor: &str,
    vertical_anchor: &str,
) -> Result<Option<RasterizedSystemLabel>> {
    if text.is_empty() {
        return Ok(None);
    }
    let layout = binding
        .native_system_font_layout(text)
        .ok_or_else(|| anyhow!("cannot shape retained system font {}", binding.family))?;
    let fonts = layout
        .faces
        .iter()
        .map(|face| {
            FontRef::try_from_slice_and_index(&face.font_data, face.face_index)
                .map_err(|_| anyhow!("invalid retained system font {}", face.family))
        })
        .collect::<Result<Vec<_>>>()?;
    let parser_faces = layout
        .faces
        .iter()
        .map(|face| {
            ttf_parser::Face::parse(&face.font_data, face.face_index)
                .map_err(|_| anyhow!("invalid retained system font {}", face.family))
        })
        .collect::<Result<Vec<_>>>()?;
    let face_scales = layout
        .faces
        .iter()
        .map(|face| binding.size as f32 / f32::from(face.units_per_em))
        .collect::<Vec<_>>();
    let Some(font) = fonts.first() else {
        return Err(anyhow!("system font layout retained no base face"));
    };
    let unit_scale = face_scales[0];
    if binding.size <= 0 || unit_scale <= 0.0 {
        return Ok(None);
    }

    // UIFont's point size is pixels-per-em in Purple's logical framebuffer.
    // ab_glyph's PxScale instead denotes ascent-minus-descent, so convert the
    // em scale explicitly rather than treating the point size as PxScale.
    let scale = PxScale::from(font.height_unscaled() * unit_scale);
    let scaled = font.as_scaled(scale);
    let native_width = layout.width;
    // getStringHeight/drawString measure the complete NSString independently;
    // they do not sum the constructor's three already-truncated metric ints.
    let line_height = binding.label_line_height.max(1);
    let native_height = binding.native_string_height(text);
    let texture_width = native_width.saturating_add(binding.stroke_width.saturating_mul(2));
    let texture_height = native_height.saturating_add(binding.stroke_width.saturating_mul(2));
    let (width, height) = match (u32::try_from(texture_width), u32::try_from(texture_height)) {
        (Ok(width @ 1..), Ok(height @ 1..)) => (width, height),
        _ => return Ok(None),
    };
    if width > u32::from(u16::MAX) || height > u32::from(u16::MAX) {
        return Err(anyhow!(
            "system label {}x{} exceeds Purple's 16-bit draw geometry",
            width,
            height
        ));
    }
    let pixel_count = usize::try_from(width)
        .ok()
        .and_then(|width| {
            usize::try_from(height)
                .ok()
                .and_then(|height| width.checked_mul(height))
        })
        .ok_or_else(|| anyhow!("system label allocation overflow"))?;
    let mut glyph_paths = Vec::new();
    let mut raster_glyphs = Vec::new();
    let mut raster_cache = HashMap::<(u16, u16), Option<Arc<NativeSystemDecodedRaster>>>::new();
    let baseline = binding.stroke_width as f32 + scaled.ascent();
    for (line_index, line) in layout.lines.iter().enumerate() {
        append_line_glyphs(
            &fonts,
            &parser_faces,
            &face_scales,
            &layout.faces,
            binding.size,
            line,
            binding.stroke_width as f32,
            baseline + line_index as f32 * line_height as f32,
            &mut glyph_paths,
            &mut raster_glyphs,
            &mut raster_cache,
            binding.fill_rgba,
        )?;
    }
    let mut fill_mask =
        Mask::new(width, height).ok_or_else(|| anyhow!("system label mask allocation overflow"))?;
    debug_assert_eq!(fill_mask.data().len(), pixel_count);
    for path in &glyph_paths {
        fill_mask.fill_path(path, FillRule::Winding, true, Transform::identity());
    }
    let stroke_mask = if binding.stroke_width >= 1 {
        let stroke = Stroke {
            width: binding.stroke_width as f32 * 2.0,
            line_join: LineJoin::Round,
            ..Stroke::default()
        };
        let mut mask = Mask::new(width, height)
            .ok_or_else(|| anyhow!("system label stroke allocation overflow"))?;
        for path in &glyph_paths {
            if let Some(outline) = path.stroke(&stroke, 1.0) {
                mask.fill_path(&outline, FillRule::Winding, true, Transform::identity());
            }
        }
        Some(mask)
    } else {
        None
    };
    let mut image = RgbaImage::new(width, height);
    if let Some(stroke_mask) = &stroke_mask {
        composite_system_mask(&mut image, stroke_mask, binding.stroke_rgba);
        composite_system_rasters(&mut image, &raster_glyphs, binding.stroke_rgba);
    }
    composite_system_mask(&mut image, &fill_mask, binding.fill_rgba);
    composite_system_rasters(&mut image, &raster_glyphs, binding.fill_rgba);

    let horizontal = match horizontal_anchor {
        "RIGHT" => native_width,
        "HCENTER" => native_width >> 1,
        _ => 0,
    };
    let vertical = native_system_label_vertical_anchor(binding, vertical_anchor);
    Ok(Some(RasterizedSystemLabel {
        image,
        horizontal_anchor: horizontal,
        vertical_anchor: vertical,
    }))
}

#[allow(clippy::too_many_arguments)]
fn append_line_glyphs(
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

fn decode_system_raster(
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

fn decode_system_coverage(
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

fn decode_system_bgra32(width: u16, height: u16, data: &[u8]) -> Result<RgbaImage> {
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
    for (pixel, source) in image.pixels_mut().zip(data[..required].chunks_exact(4)) {
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

fn composite_system_mask(image: &mut RgbaImage, mask: &Mask, color: [u8; 4]) {
    for (index, pixel) in image.pixels_mut().enumerate() {
        let mut target = pixel.0.map(|channel| f32::from(channel) / 255.0);
        over_layer(&mut target, color, f32::from(mask.data()[index]) / 255.0);
        *pixel = Rgba(target.map(float_channel));
    }
}

fn composite_system_rasters(
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

fn glyph_outline_path(
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

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;

    fn open_sans() -> Option<Vec<u8>> {
        std::fs::read(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../angry birds stella v1.1.6/Payload/Purple.app/OpenSans-Regular.ttf"),
        )
        .ok()
    }

    fn binding() -> SystemFontRenderBinding {
        SystemFontRenderBinding {
            label_pool_epoch: 0,
            family: "ArialRoundedMTBold".to_owned(),
            font_data: Arc::from([]),
            face_index: 0,
            fallback_catalog: None,
            size: 40,
            fill_rgba: [0, 0, 0, 255],
            stroke_width: 0,
            stroke_rgba: [0, 0, 0, 255],
            style: 0,
            ascending: 37,
            descending: 8,
            leading: 0,
            label_line_height: 46,
        }
    }

    #[test]
    fn system_font_stroke_is_a_closed_centered_vector_outline() {
        let Some(open_sans) = open_sans() else {
            eprintln!("skipping Purple.app font regression: OpenSans-Regular.ttf is unavailable");
            return;
        };
        let font = FontRef::try_from_slice(&open_sans).unwrap();
        let units_per_em = font.units_per_em().unwrap();
        let unit_scale = 40.0 / units_per_em;
        let path = glyph_outline_path(&font, font.glyph_id('M'), unit_scale, 10.0, 50.0).unwrap();
        assert!(
            path.segments()
                .any(|segment| matches!(segment, tiny_skia::PathSegment::Close))
        );

        let radius = 3.0;
        let stroke = Stroke {
            width: radius * 2.0,
            line_join: LineJoin::Round,
            ..Stroke::default()
        };
        let stroked = path.stroke(&stroke, 1.0).unwrap();
        let fill_bounds = path.compute_tight_bounds().unwrap();
        let stroke_bounds = stroked.compute_tight_bounds().unwrap();
        assert!(stroke_bounds.left() <= fill_bounds.left() - radius + 0.01);
        assert!(stroke_bounds.right() >= fill_bounds.right() + radius - 0.01);
        assert!(stroke_bounds.top() <= fill_bounds.top() - radius + 0.01);
        assert!(stroke_bounds.bottom() >= fill_bounds.bottom() + radius - 0.01);
    }

    #[test]
    fn label_hash_sign_extends_native_aarrggbb_fields() {
        assert_eq!(
            native_system_label_hash(&binding(), "Stella"),
            0x0e93_feaf_8762_57ce
        );
        let mut next_epoch = binding();
        next_epoch.label_pool_epoch = 1;
        assert_ne!(
            system_label_lifetime_key(&binding(), "Stella"),
            system_label_lifetime_key(&next_epoch, "Stella")
        );
    }

    #[test]
    fn label_offset_truncates_anchored_local_coordinates_toward_zero() {
        let command = TextRenderCommand {
            order: 0,
            text: String::new(),
            font: String::new(),
            font_binding: None,
            x: 0.0,
            y: 0.0,
            native_system_origin: Some([100.75, -20.75]),
            scale_x: 1.0,
            scale_y: 1.0,
            angle: 0.0,
            matrix: None,
            alpha: 1.0,
            horizontal_anchor: String::new(),
            vertical_anchor: String::new(),
            projection_3d: None,
            clip_rect: None,
        };
        assert_eq!(
            native_system_label_offset(&command, 2, 10, 4),
            [-12.75, -5.25]
        );

        let command = TextRenderCommand {
            x: 131.25,
            y: 226.75,
            native_system_origin: Some([10.75, -3.25]),
            matrix: Some([2.0, -3.0, 4.0, 5.0]),
            ..command
        };
        let [left, top] = native_system_label_offset(&command, 0, 2, 4);
        let transformed = text_glyph_transform(&command, left, top);
        // The native order is trunc(10.75-2, -3.25-4) = (8,-7), followed by
        // the matrix and translation: (137,197). Truncating screen space or
        // transforming the fractional anchored point produces other values.
        assert_eq!([transformed.x, transformed.y], [137.0, 197.0]);
    }

    #[test]
    fn vertical_anchor_uses_native_wrapping_add_and_signed_half() {
        let mut font = binding();
        font.ascending = i32::MAX;
        font.descending = 2;
        assert_eq!(native_system_label_vertical_anchor(&font, "TOP"), 0);
        assert_eq!(
            native_system_label_vertical_anchor(&font, "VCENTER"),
            -1_073_741_823
        );
        assert_eq!(
            native_system_label_vertical_anchor(&font, "BOTTOM"),
            -2_147_483_647
        );
        assert_eq!(
            native_system_label_vertical_anchor(&font, "BASELINE"),
            i32::MAX
        );
        assert_eq!(native_system_label_vertical_anchor(&font, "VPIVOT"), 0);
    }

    #[test]
    fn label_pool_uses_five_mib_fifo_without_hit_reordering() {
        let mut pool = SystemLabelPool::default();
        assert!(pool.enter_epoch(7).is_empty());
        let image = || RgbaImage::new(512, 1024); // 2 MiB
        let (first, retired) = pool.insert(7, 1, image()).unwrap();
        assert!(retired.is_empty());
        let (_, retired) = pool.insert(7, 2, image()).unwrap();
        assert!(retired.is_empty());

        // A hit must not promote hash 1. The third insertion reaches the
        // native limit exactly and therefore still evicts nothing.
        assert_eq!(pool.get(1).unwrap().texture_key, first.texture_key);
        let (_, retired) = pool.insert(7, 3, RgbaImage::new(512, 512)).unwrap();
        assert!(retired.is_empty());
        assert_eq!(pool.bytes, LABEL_POOL_BYTE_LIMIT);

        // Four more bytes exceed 0x500000, so end()-1 is hash 1 even though it
        // was just hit. Hash 2 would be selected by an LRU implementation.
        let (_, retired) = pool.insert(7, 4, RgbaImage::new(1, 1)).unwrap();
        assert_eq!(retired.as_slice(), std::slice::from_ref(&first.texture_key));
        assert!(pool.get(1).is_none());
        assert!(pool.get(2).is_some());
        assert!(pool.get(3).is_some());
        assert!(pool.get(4).is_some());

        // Re-insertion gets a distinct deferred-wgpu identity so a draw that
        // used the evicted texture earlier in this frame cannot be rebound.
        let (reinserted, _) = pool.insert(7, 1, image()).unwrap();
        assert_ne!(reinserted.texture_key, first.texture_key);
    }

    #[test]
    fn label_pool_epoch_clear_retires_all_cached_labels_and_resets_bytes() {
        let mut pool = SystemLabelPool::default();
        pool.enter_epoch(10);
        let (old, _) = pool.insert(10, 42, RgbaImage::new(16, 16)).unwrap();
        let retired = pool.enter_epoch(11);
        assert_eq!(retired.as_slice(), std::slice::from_ref(&old.texture_key));
        assert_eq!(pool.bytes, 0);
        assert!(pool.newest_first.is_empty());
        assert!(pool.labels.is_empty());

        let (new, _) = pool.insert(11, 42, RgbaImage::new(16, 16)).unwrap();
        assert_ne!(new.texture_key, old.texture_key);
    }

    #[test]
    fn label_larger_than_native_pool_limit_is_rejected_safely() {
        let mut pool = SystemLabelPool::default();
        pool.enter_epoch(0);
        let result = pool.insert(0, 1, RgbaImage::new(1281, 1024));
        assert!(result.is_err());
        assert_eq!(pool.bytes, 0);
        assert!(pool.labels.is_empty());
    }

    #[test]
    fn embedded_bitmap_coverage_decodes_padded_rows_and_bit_depths() {
        let mono = decode_system_coverage(3, 2, &[0b1010_0000, 0b0100_0000], 1, true).unwrap();
        assert_eq!(
            mono.pixels().map(|pixel| pixel[3]).collect::<Vec<_>>(),
            [255, 0, 255, 0, 255, 0]
        );

        let gray = decode_system_coverage(4, 1, &[0b00_01_10_11], 2, false).unwrap();
        assert_eq!(
            gray.pixels().map(|pixel| pixel[3]).collect::<Vec<_>>(),
            [0, 85, 170, 255]
        );
    }

    #[test]
    fn premultiplied_bgra_bitmap_is_unpremultiplied_before_sampling() {
        let image = decode_system_bgra32(1, 2, &[25, 50, 100, 128, 0, 0, 0, 0]).unwrap();
        assert_eq!(image.get_pixel(0, 0).0, [199, 99, 49, 128]);
        assert_eq!(image.get_pixel(0, 1).0, [0, 0, 0, 0]);
    }

    #[test]
    fn intrinsic_bitmap_is_source_over_composited_once_per_native_text_pass() {
        let placed = NativeSystemPlacedRaster {
            raster: Arc::new(NativeSystemDecodedRaster {
                image: RgbaImage::from_pixel(1, 1, Rgba([200, 100, 50, 128])),
                color: NativeSystemRasterColor::Intrinsic,
                x: 0,
                y: 0,
                pixels_per_em: 1,
                sbix: true,
                glyph_bbox: None,
            }),
            left: 0.0,
            top: 0.0,
            scale: 1.0,
        };
        let mut image = RgbaImage::new(1, 1);
        composite_system_rasters(&mut image, std::slice::from_ref(&placed), [1, 2, 3, 4]);
        assert_eq!(image.get_pixel(0, 0).0, [100, 50, 25, 128]);
        composite_system_rasters(&mut image, &[placed], [5, 6, 7, 8]);
        assert_eq!(image.get_pixel(0, 0).0, [150, 75, 38, 192]);
    }

    #[test]
    #[cfg(target_os = "macos")]
    fn apple_sbix_png_rasterizes_in_intrinsic_color_without_an_outline_square() {
        use std::collections::HashSet;

        let Ok(data) = std::fs::read("/System/Library/Fonts/Apple Color Emoji.ttc") else {
            return;
        };
        let face = ttf_parser::Face::parse(&data, 0).unwrap();
        let glyph = face.glyph_index('\u{1F600}').unwrap();
        let raster = face.glyph_raster_image(glyph, 40).unwrap();
        let decoded = decode_system_raster(
            raster,
            face.tables().sbix.is_some(),
            face.glyph_bounding_box(glyph),
        )
        .unwrap();
        assert_eq!(decoded.color, NativeSystemRasterColor::Intrinsic);
        assert_eq!(decoded.image.dimensions(), (40, 40));
        assert_eq!(decoded.pixels_per_em, 40);

        let binding = SystemFontRenderBinding {
            label_pool_epoch: 0,
            family: "AppleColorEmoji".to_owned(),
            font_data: Arc::from(data),
            face_index: 0,
            fallback_catalog: None,
            size: 40,
            fill_rgba: [255, 0, 255, 255],
            stroke_width: 0,
            stroke_rgba: [0, 255, 0, 255],
            style: 0,
            ascending: 40,
            descending: 12,
            leading: 0,
            label_line_height: 52,
        };
        let label = rasterize_system_label(&binding, "\u{1F600}", "LEFT", "TOP")
            .unwrap()
            .unwrap();
        assert_eq!(label.image.dimensions(), (40, 52));
        let colors = label
            .image
            .pixels()
            .filter(|pixel| pixel[3] != 0)
            .map(|pixel| [pixel[0], pixel[1], pixel[2]])
            .collect::<HashSet<_>>();
        assert!(colors.len() > 32);
        assert!(!colors.contains(&binding.fill_rgba[..3]));
    }
}
