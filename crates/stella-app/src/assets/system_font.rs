//! UIKit SystemFont label-cache equivalent backed by cross-platform outlines.

use anyhow::{Result, anyhow};
use image::RgbaImage;
use skrifa::instance::Size;
use skrifa::{FontRef, MetadataProvider};
use std::sync::Arc;
use tiny_skia::{FillRule, LineJoin, Mask, Stroke, Transform};

use super::*;

mod cache;
mod color_outline;
mod placement;
mod raster;

use cache::LABEL_POOL_BYTE_LIMIT;
#[cfg(test)]
use cache::system_label_lifetime_key;
pub(crate) use cache::{SystemLabelPool, native_system_label_hash};
pub(crate) use placement::{
    native_system_label_horizontal_anchor, native_system_label_offset,
    native_system_label_vertical_anchor,
};
#[cfg(all(test, target_os = "macos"))]
use raster::decode_system_raster;
use raster::{append_line_glyphs, composite_system_mask, composite_system_rasters};
#[cfg(test)]
use raster::{decode_system_bgra32, decode_system_coverage, glyph_outline_path};

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
    glyph_bbox: Option<NativeSystemGlyphBounds>,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct NativeSystemGlyphBounds {
    pub(crate) x_min: i16,
    pub(crate) y_min: i16,
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
            FontRef::from_index(&face.font_data, face.face_index)
                .map_err(|error| anyhow!("invalid retained system font {}: {error}", face.family))
        })
        .collect::<Result<Vec<_>>>()?;
    let parser_faces = layout
        .faces
        .iter()
        .map(|face| {
            FontRef::from_index(&face.font_data, face.face_index)
                .map_err(|error| anyhow!("invalid retained system font {}: {error}", face.family))
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
    // Skrifa exposes the same typographic ascent after applying that em scale.
    let metrics = font.metrics(
        Size::new(binding.size.max(1) as f32),
        skrifa::instance::LocationRef::default(),
    );
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
    let baseline = binding.stroke_width as f32 + metrics.ascent;
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

#[cfg(test)]
mod tests;
