//! COLR/CPAL color-outline rasterization for retained native system fonts.
//!
//! Purple delegates this stage to CoreGraphics/CoreText.  The cross-platform
//! renderer keeps the same separation: shaping selects a face and glyph,
//! while this module evaluates the font's paint graph into one intrinsic-color
//! raster before the recovered `drawString` passes composite it.

use anyhow::{Result, anyhow};
use image::{Rgba, RgbaImage};
use skrifa::{
    FontRef, GlyphId, MetadataProvider,
    color::{Brush, ColorPainter, CompositeMode, Extend},
    instance::LocationRef,
};
use tiny_skia::{
    BlendMode, FillRule, Mask, Path, PathBuilder, Pixmap, PixmapPaint, PremultipliedColorU8,
    Transform,
};
use ttf_parser::OutlineBuilder;

use super::{
    LABEL_POOL_BYTE_LIMIT, NativeSystemDecodedRaster, NativeSystemRasterColor, SystemFontLayoutFace,
};

#[derive(Clone, Copy, Debug)]
struct FontBounds {
    x_min: f32,
    y_min: f32,
    x_max: f32,
    y_max: f32,
}

impl FontBounds {
    fn from_ttf_rect(rect: ttf_parser::Rect) -> Self {
        Self {
            x_min: f32::from(rect.x_min),
            y_min: f32::from(rect.y_min),
            x_max: f32::from(rect.x_max),
            y_max: f32::from(rect.y_max),
        }
    }

    fn include_transformed_rect(
        &mut self,
        x_min: f32,
        y_min: f32,
        x_max: f32,
        y_max: f32,
        transform: skrifa::color::Transform,
    ) {
        for (x, y) in [
            (x_min, y_min),
            (x_min, y_max),
            (x_max, y_min),
            (x_max, y_max),
        ] {
            let (x, y) = transform.transform(x, y);
            if x.is_finite() && y.is_finite() {
                self.x_min = self.x_min.min(x);
                self.y_min = self.y_min.min(y);
                self.x_max = self.x_max.max(x);
                self.y_max = self.y_max.max(y);
            }
        }
    }
}

struct ColorBoundsPainter<'a, 'font> {
    face: &'a ttf_parser::Face<'font>,
    bounds: Option<FontBounds>,
    transform: skrifa::color::Transform,
    transform_stack: Vec<skrifa::color::Transform>,
}

impl<'a, 'font> ColorBoundsPainter<'a, 'font> {
    fn new(face: &'a ttf_parser::Face<'font>) -> Self {
        Self {
            face,
            bounds: None,
            transform: skrifa::color::Transform::default(),
            transform_stack: Vec::new(),
        }
    }

    fn include_rect(&mut self, x_min: f32, y_min: f32, x_max: f32, y_max: f32) {
        let mut bounds = self.bounds.unwrap_or(FontBounds {
            x_min: f32::INFINITY,
            y_min: f32::INFINITY,
            x_max: f32::NEG_INFINITY,
            y_max: f32::NEG_INFINITY,
        });
        bounds.include_transformed_rect(x_min, y_min, x_max, y_max, self.transform);
        if bounds.x_min.is_finite()
            && bounds.y_min.is_finite()
            && bounds.x_max.is_finite()
            && bounds.y_max.is_finite()
        {
            self.bounds = Some(bounds);
        }
    }

    fn include_glyph(&mut self, glyph_id: GlyphId) {
        let Ok(glyph_id) = u16::try_from(glyph_id.to_u32()) else {
            return;
        };
        if let Some(rect) = self.face.glyph_bounding_box(ttf_parser::GlyphId(glyph_id)) {
            self.include_rect(
                f32::from(rect.x_min),
                f32::from(rect.y_min),
                f32::from(rect.x_max),
                f32::from(rect.y_max),
            );
        }
    }
}

impl ColorPainter for ColorBoundsPainter<'_, '_> {
    fn push_transform(&mut self, transform: skrifa::color::Transform) {
        self.transform_stack.push(self.transform);
        self.transform *= transform;
    }

    fn pop_transform(&mut self) {
        if let Some(transform) = self.transform_stack.pop() {
            self.transform = transform;
        }
    }

    fn push_clip_glyph(&mut self, glyph_id: GlyphId) {
        self.include_glyph(glyph_id);
    }

    fn push_clip_box(&mut self, clip_box: skrifa::raw::types::BoundingBox<f32>) {
        self.include_rect(
            clip_box.x_min,
            clip_box.y_min,
            clip_box.x_max,
            clip_box.y_max,
        );
    }

    fn pop_clip(&mut self) {}

    fn fill(&mut self, _brush: Brush<'_>) {}

    fn fill_glyph(
        &mut self,
        glyph_id: GlyphId,
        _brush_transform: Option<skrifa::color::Transform>,
        _brush: Brush<'_>,
    ) {
        self.include_glyph(glyph_id);
    }

    fn push_layer(&mut self, _composite_mode: CompositeMode) {}
}

struct TinyOutlineBuilder {
    builder: PathBuilder,
}

impl TinyOutlineBuilder {
    fn new() -> Self {
        Self {
            builder: PathBuilder::new(),
        }
    }

    fn finish(self) -> Option<Path> {
        self.builder.finish()
    }
}

impl OutlineBuilder for TinyOutlineBuilder {
    fn move_to(&mut self, x: f32, y: f32) {
        self.builder.move_to(x, y);
    }

    fn line_to(&mut self, x: f32, y: f32) {
        self.builder.line_to(x, y);
    }

    fn quad_to(&mut self, x1: f32, y1: f32, x: f32, y: f32) {
        self.builder.quad_to(x1, y1, x, y);
    }

    fn curve_to(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, x: f32, y: f32) {
        self.builder.cubic_to(x1, y1, x2, y2, x, y);
    }

    fn close(&mut self) {
        self.builder.close();
    }
}

fn glyph_path(face: &ttf_parser::Face<'_>, glyph_id: GlyphId) -> Option<Path> {
    let glyph_id = u16::try_from(glyph_id.to_u32()).ok()?;
    let mut builder = TinyOutlineBuilder::new();
    face.outline_glyph(ttf_parser::GlyphId(glyph_id), &mut builder)?;
    builder.finish()
}

fn clip_box_path(clip_box: skrifa::raw::types::BoundingBox<f32>) -> Option<Path> {
    let mut builder = PathBuilder::new();
    builder.move_to(clip_box.x_min, clip_box.y_min);
    builder.line_to(clip_box.x_max, clip_box.y_min);
    builder.line_to(clip_box.x_max, clip_box.y_max);
    builder.line_to(clip_box.x_min, clip_box.y_max);
    builder.close();
    builder.finish()
}

#[derive(Clone, Copy, Debug)]
struct ResolvedColorStop {
    offset: f32,
    rgba: [f32; 4],
}

enum ResolvedBrush {
    Solid([f32; 4]),
    Linear {
        p0: [f32; 2],
        p1: [f32; 2],
        stops: Vec<ResolvedColorStop>,
        extend: Extend,
    },
    Radial {
        c0: [f32; 2],
        r0: f32,
        c1: [f32; 2],
        r1: f32,
        stops: Vec<ResolvedColorStop>,
        extend: Extend,
    },
    Sweep {
        center: [f32; 2],
        start_angle: f32,
        end_angle: f32,
        stops: Vec<ResolvedColorStop>,
        extend: Extend,
    },
}

impl ResolvedBrush {
    fn sample(&self, x: f32, y: f32) -> Option<[f32; 4]> {
        match self {
            Self::Solid(rgba) => Some(*rgba),
            Self::Linear {
                p0,
                p1,
                stops,
                extend,
            } => {
                let dx = p1[0] - p0[0];
                let dy = p1[1] - p0[1];
                let denominator = dx.mul_add(dx, dy * dy);
                if denominator <= f32::EPSILON {
                    return None;
                }
                let t = ((x - p0[0]).mul_add(dx, (y - p0[1]) * dy)) / denominator;
                sample_color_line(stops, *extend, t)
            }
            Self::Radial {
                c0,
                r0,
                c1,
                r1,
                stops,
                extend,
            } => radial_parameter(x, y, *c0, *r0, *c1, *r1)
                .and_then(|t| sample_color_line(stops, *extend, t)),
            Self::Sweep {
                center,
                start_angle,
                end_angle,
                stops,
                extend,
            } => {
                let angle = (-(y - center[1]).atan2(x - center[0]))
                    .to_degrees()
                    .rem_euclid(360.0);
                let angle_span = end_angle - start_angle;
                if !angle_span.is_finite() || angle_span.abs() <= f32::EPSILON {
                    return None;
                }
                sample_color_line(stops, *extend, (angle - start_angle) / angle_span)
            }
        }
    }
}

fn radial_parameter(x: f32, y: f32, c0: [f32; 2], r0: f32, c1: [f32; 2], r1: f32) -> Option<f32> {
    let qx = x - c0[0];
    let qy = y - c0[1];
    let dcx = c1[0] - c0[0];
    let dcy = c1[1] - c0[1];
    let dr = r1 - r0;
    let a = dcx.mul_add(dcx, dcy * dcy) - dr * dr;
    let b = -2.0 * (qx.mul_add(dcx, qy * dcy) + r0 * dr);
    let c = qx.mul_add(qx, qy * qy) - r0 * r0;
    let epsilon = f32::EPSILON * 64.0;
    let mut roots = [f32::NAN; 2];
    if a.abs() <= epsilon {
        if b.abs() <= epsilon {
            return None;
        }
        roots[0] = -c / b;
    } else {
        let discriminant = b.mul_add(b, -4.0 * a * c);
        if discriminant < 0.0 || !discriminant.is_finite() {
            return None;
        }
        let root = discriminant.sqrt();
        roots[0] = (-b - root) / (2.0 * a);
        roots[1] = (-b + root) / (2.0 * a);
    }
    roots
        .into_iter()
        .filter(|t| t.is_finite() && r0 + *t * dr >= -epsilon)
        .max_by(|left, right| left.total_cmp(right))
}

fn extend_parameter(extend: Extend, t: f32) -> Option<f32> {
    if !t.is_finite() {
        return None;
    }
    Some(match extend {
        Extend::Pad => t.clamp(0.0, 1.0),
        Extend::Repeat => t.rem_euclid(1.0),
        Extend::Reflect => {
            let t = t.rem_euclid(2.0);
            if t <= 1.0 { t } else { 2.0 - t }
        }
        Extend::Unknown => return None,
    })
}

fn sample_color_line(stops: &[ResolvedColorStop], extend: Extend, t: f32) -> Option<[f32; 4]> {
    let t = extend_parameter(extend, t)?;
    let first = *stops.first()?;
    if t <= first.offset {
        return Some(first.rgba);
    }
    for pair in stops.windows(2) {
        let left = pair[0];
        let right = pair[1];
        if t <= right.offset {
            if right.offset <= left.offset {
                return Some(right.rgba);
            }
            let amount = (t - left.offset) / (right.offset - left.offset);
            return Some(std::array::from_fn(|index| {
                left.rgba[index] + (right.rgba[index] - left.rgba[index]) * amount
            }));
        }
    }
    Some(stops.last()?.rgba)
}

struct ColorLayer {
    pixmap: Pixmap,
    composite_mode: CompositeMode,
}

struct ColorRasterPainter<'a, 'font> {
    face: &'a ttf_parser::Face<'font>,
    width: u32,
    height: u32,
    root_transform: Transform,
    transform: skrifa::color::Transform,
    transform_stack: Vec<skrifa::color::Transform>,
    clip: Mask,
    clip_stack: Vec<Mask>,
    layers: Vec<ColorLayer>,
    palette: Vec<[u8; 4]>,
    foreground: [u8; 4],
    error: Option<anyhow::Error>,
}

impl<'a, 'font> ColorRasterPainter<'a, 'font> {
    fn new(
        face: &'a ttf_parser::Face<'font>,
        width: u32,
        height: u32,
        root_transform: Transform,
        palette: Vec<[u8; 4]>,
        foreground: [u8; 4],
    ) -> Result<Self> {
        let mut clip = Mask::new(width, height)
            .ok_or_else(|| anyhow!("COLR system glyph clip allocation overflow"))?;
        clip.data_mut().fill(255);
        let root = Pixmap::new(width, height)
            .ok_or_else(|| anyhow!("COLR system glyph layer allocation overflow"))?;
        Ok(Self {
            face,
            width,
            height,
            root_transform,
            transform: skrifa::color::Transform::default(),
            transform_stack: Vec::new(),
            clip,
            clip_stack: Vec::new(),
            layers: vec![ColorLayer {
                pixmap: root,
                composite_mode: CompositeMode::SrcOver,
            }],
            palette,
            foreground,
            error: None,
        })
    }

    fn combined_transform(&self) -> Transform {
        self.root_transform.pre_concat(Transform::from_row(
            self.transform.xx,
            self.transform.yx,
            self.transform.xy,
            self.transform.yy,
            self.transform.dx,
            self.transform.dy,
        ))
    }

    fn push_path_clip(&mut self, path: Option<Path>) {
        self.clip_stack.push(self.clip.clone());
        if let Some(path) = path {
            self.clip
                .intersect_path(&path, FillRule::Winding, true, self.combined_transform());
        } else {
            self.clip.clear();
        }
    }

    fn resolve_palette_color(&self, palette_index: u16, alpha: f32) -> [f32; 4] {
        let rgba = if palette_index == u16::MAX {
            self.foreground
        } else {
            self.palette
                .get(usize::from(palette_index))
                .copied()
                .unwrap_or([0, 0, 0, 0])
        };
        [
            f32::from(rgba[0]) / 255.0,
            f32::from(rgba[1]) / 255.0,
            f32::from(rgba[2]) / 255.0,
            f32::from(rgba[3]) / 255.0 * alpha.clamp(0.0, 1.0),
        ]
    }

    fn resolve_stops(&self, stops: &[skrifa::color::ColorStop]) -> Vec<ResolvedColorStop> {
        stops
            .iter()
            .map(|stop| ResolvedColorStop {
                offset: stop.offset,
                rgba: self.resolve_palette_color(stop.palette_index, stop.alpha),
            })
            .collect()
    }

    fn resolve_brush(&self, brush: Brush<'_>) -> ResolvedBrush {
        match brush {
            Brush::Solid {
                palette_index,
                alpha,
            } => ResolvedBrush::Solid(self.resolve_palette_color(palette_index, alpha)),
            Brush::LinearGradient {
                p0,
                p1,
                color_stops,
                extend,
            } => ResolvedBrush::Linear {
                p0: [p0.x, p0.y],
                p1: [p1.x, p1.y],
                stops: self.resolve_stops(color_stops),
                extend,
            },
            Brush::RadialGradient {
                c0,
                r0,
                c1,
                r1,
                color_stops,
                extend,
            } => ResolvedBrush::Radial {
                c0: [c0.x, c0.y],
                r0,
                c1: [c1.x, c1.y],
                r1,
                stops: self.resolve_stops(color_stops),
                extend,
            },
            Brush::SweepGradient {
                c0,
                start_angle,
                end_angle,
                color_stops,
                extend,
            } => ResolvedBrush::Sweep {
                center: [c0.x, c0.y],
                start_angle,
                end_angle,
                stops: self.resolve_stops(color_stops),
                extend,
            },
        }
    }

    fn fill_resolved(&mut self, brush: ResolvedBrush) {
        let Some(inverse) = self.combined_transform().invert() else {
            return;
        };
        let Some(mut source) = Pixmap::new(self.width, self.height) else {
            self.error = Some(anyhow!("COLR system glyph paint allocation overflow"));
            return;
        };
        for y in 0..self.height {
            for x in 0..self.width {
                let index = y as usize * self.width as usize + x as usize;
                let coverage = self.clip.data()[index];
                if coverage == 0 {
                    continue;
                }
                let mut point = tiny_skia::Point::from_xy(x as f32 + 0.5, y as f32 + 0.5);
                inverse.map_point(&mut point);
                let Some(mut rgba) = brush.sample(point.x, point.y) else {
                    continue;
                };
                rgba[3] *= f32::from(coverage) / 255.0;
                let alpha = (rgba[3].clamp(0.0, 1.0) * 255.0).round() as u8;
                let premultiply =
                    |channel: f32| (channel.clamp(0.0, 1.0) * f32::from(alpha)).round() as u8;
                source.pixels_mut()[index] = PremultipliedColorU8::from_rgba(
                    premultiply(rgba[0]),
                    premultiply(rgba[1]),
                    premultiply(rgba[2]),
                    alpha,
                )
                .unwrap_or(PremultipliedColorU8::TRANSPARENT);
            }
        }
        if let Some(layer) = self.layers.last_mut() {
            layer.pixmap.draw_pixmap(
                0,
                0,
                source.as_ref(),
                &PixmapPaint::default(),
                Transform::identity(),
                None,
            );
        }
    }

    fn finish(mut self) -> Result<Pixmap> {
        if let Some(error) = self.error.take() {
            return Err(error);
        }
        if self.layers.len() != 1 {
            return Err(anyhow!("unbalanced COLR system glyph layer graph"));
        }
        Ok(self
            .layers
            .pop()
            .expect("one checked COLR root layer")
            .pixmap)
    }
}

impl ColorPainter for ColorRasterPainter<'_, '_> {
    fn push_transform(&mut self, transform: skrifa::color::Transform) {
        self.transform_stack.push(self.transform);
        self.transform *= transform;
    }

    fn pop_transform(&mut self) {
        if let Some(transform) = self.transform_stack.pop() {
            self.transform = transform;
        } else if self.error.is_none() {
            self.error = Some(anyhow!("unbalanced COLR system glyph transform graph"));
        }
    }

    fn push_clip_glyph(&mut self, glyph_id: GlyphId) {
        self.push_path_clip(glyph_path(self.face, glyph_id));
    }

    fn push_clip_box(&mut self, clip_box: skrifa::raw::types::BoundingBox<f32>) {
        self.push_path_clip(clip_box_path(clip_box));
    }

    fn pop_clip(&mut self) {
        if let Some(clip) = self.clip_stack.pop() {
            self.clip = clip;
        } else if self.error.is_none() {
            self.error = Some(anyhow!("unbalanced COLR system glyph clip graph"));
        }
    }

    fn fill(&mut self, brush: Brush<'_>) {
        if self.error.is_none() {
            let brush = self.resolve_brush(brush);
            self.fill_resolved(brush);
        }
    }

    fn push_layer(&mut self, composite_mode: CompositeMode) {
        if self.error.is_some() {
            return;
        }
        let Some(pixmap) = Pixmap::new(self.width, self.height) else {
            self.error = Some(anyhow!("COLR system glyph layer allocation overflow"));
            return;
        };
        self.layers.push(ColorLayer {
            pixmap,
            composite_mode,
        });
    }

    fn pop_layer(&mut self) {
        if self.error.is_some() {
            return;
        }
        if self.layers.len() <= 1 {
            self.error = Some(anyhow!("unbalanced COLR system glyph layer graph"));
            return;
        }
        let source = self.layers.pop().expect("checked COLR source layer");
        let paint = PixmapPaint {
            blend_mode: blend_mode(source.composite_mode),
            ..PixmapPaint::default()
        };
        self.layers
            .last_mut()
            .expect("COLR destination layer")
            .pixmap
            .draw_pixmap(
                0,
                0,
                source.pixmap.as_ref(),
                &paint,
                Transform::identity(),
                None,
            );
    }
}

fn blend_mode(mode: CompositeMode) -> BlendMode {
    match mode {
        CompositeMode::Clear => BlendMode::Clear,
        CompositeMode::Src => BlendMode::Source,
        CompositeMode::Dest => BlendMode::Destination,
        CompositeMode::SrcOver => BlendMode::SourceOver,
        CompositeMode::DestOver => BlendMode::DestinationOver,
        CompositeMode::SrcIn => BlendMode::SourceIn,
        CompositeMode::DestIn => BlendMode::DestinationIn,
        CompositeMode::SrcOut => BlendMode::SourceOut,
        CompositeMode::DestOut => BlendMode::DestinationOut,
        CompositeMode::SrcAtop => BlendMode::SourceAtop,
        CompositeMode::DestAtop => BlendMode::DestinationAtop,
        CompositeMode::Xor => BlendMode::Xor,
        CompositeMode::Plus => BlendMode::Plus,
        CompositeMode::Screen => BlendMode::Screen,
        CompositeMode::Overlay => BlendMode::Overlay,
        CompositeMode::Darken => BlendMode::Darken,
        CompositeMode::Lighten => BlendMode::Lighten,
        CompositeMode::ColorDodge => BlendMode::ColorDodge,
        CompositeMode::ColorBurn => BlendMode::ColorBurn,
        CompositeMode::HardLight => BlendMode::HardLight,
        CompositeMode::SoftLight => BlendMode::SoftLight,
        CompositeMode::Difference => BlendMode::Difference,
        CompositeMode::Exclusion => BlendMode::Exclusion,
        CompositeMode::Multiply => BlendMode::Multiply,
        CompositeMode::HslHue => BlendMode::Hue,
        CompositeMode::HslSaturation => BlendMode::Saturation,
        CompositeMode::HslColor => BlendMode::Color,
        CompositeMode::HslLuminosity => BlendMode::Luminosity,
        CompositeMode::Unknown => BlendMode::SourceOver,
    }
}

pub(super) fn render_system_color_outline(
    layout_face: &SystemFontLayoutFace,
    parser_face: &ttf_parser::Face<'_>,
    glyph_id: u16,
    point_size: i32,
    foreground: [u8; 4],
) -> Result<Option<NativeSystemDecodedRaster>> {
    let font = FontRef::from_index(&layout_face.font_data, layout_face.face_index)
        .map_err(|error| anyhow!("invalid COLR system font {}: {error}", layout_face.family))?;
    let color_glyphs = font.color_glyphs();
    let Some(color_glyph) = color_glyphs.get(GlyphId::new(u32::from(glyph_id))) else {
        return Ok(None);
    };

    let mut bounds_painter = ColorBoundsPainter::new(parser_face);
    color_glyph
        .paint(LocationRef::default(), &mut bounds_painter)
        .map_err(|error| {
            anyhow!(
                "invalid COLR paint graph in {}: {error}",
                layout_face.family
            )
        })?;
    let bounds = bounds_painter
        .bounds
        .unwrap_or_else(|| FontBounds::from_ttf_rect(parser_face.global_bounding_box()));
    let point_size = point_size.max(1);
    let scale = point_size as f32 / f32::from(layout_face.units_per_em);
    let left = (bounds.x_min * scale).floor() as i32 - 1;
    let bottom = (bounds.y_min * scale).floor() as i32 - 1;
    let right = (bounds.x_max * scale).ceil() as i32 + 1;
    let top = (bounds.y_max * scale).ceil() as i32 + 1;
    let width = u32::try_from(right.saturating_sub(left))
        .map_err(|_| anyhow!("COLR system glyph width overflow"))?;
    let height = u32::try_from(top.saturating_sub(bottom))
        .map_err(|_| anyhow!("COLR system glyph height overflow"))?;
    if width == 0 || height == 0 {
        return Ok(None);
    }
    let byte_len = u64::from(width)
        .saturating_mul(u64::from(height))
        .saturating_mul(4);
    if byte_len > LABEL_POOL_BYTE_LIMIT {
        return Err(anyhow!(
            "COLR system glyph consumes {byte_len} bytes, exceeding Purple's 5 MiB LabelPool"
        ));
    }

    let palette = font
        .color_palettes()
        .get(0)
        .map(|palette| {
            palette
                .colors()
                .iter()
                .map(|color| [color.red, color.green, color.blue, color.alpha])
                .collect()
        })
        .unwrap_or_default();
    let root_transform = Transform::from_row(scale, 0.0, 0.0, -scale, -left as f32, top as f32);
    let mut painter = ColorRasterPainter::new(
        parser_face,
        width,
        height,
        root_transform,
        palette,
        foreground,
    )?;
    color_glyph
        .paint(LocationRef::default(), &mut painter)
        .map_err(|error| {
            anyhow!(
                "invalid COLR paint graph in {}: {error}",
                layout_face.family
            )
        })?;
    let pixmap = painter.finish()?;
    let mut image = RgbaImage::new(width, height);
    for (target, source) in image.pixels_mut().zip(pixmap.pixels()) {
        let source = source.demultiply();
        *target = Rgba([source.red(), source.green(), source.blue(), source.alpha()]);
    }

    Ok(Some(NativeSystemDecodedRaster {
        image,
        color: NativeSystemRasterColor::Intrinsic,
        x: i16::try_from(left).map_err(|_| anyhow!("COLR system glyph x bearing exceeds i16"))?,
        y: i16::try_from(bottom).map_err(|_| anyhow!("COLR system glyph y bearing exceeds i16"))?,
        pixels_per_em: point_size.clamp(1, i32::from(u16::MAX)) as u16,
        sbix: false,
        glyph_bbox: None,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stop(offset: f32, rgba: [f32; 4]) -> ResolvedColorStop {
        ResolvedColorStop { offset, rgba }
    }

    #[test]
    fn color_line_extend_modes_follow_colr_contract() {
        let stops = [
            stop(0.0, [0.0, 0.0, 0.0, 0.0]),
            stop(1.0, [1.0, 0.5, 0.25, 1.0]),
        ];
        assert_eq!(
            sample_color_line(&stops, Extend::Pad, -0.5),
            Some(stops[0].rgba)
        );
        assert_eq!(
            sample_color_line(&stops, Extend::Repeat, 1.25),
            sample_color_line(&stops, Extend::Pad, 0.25)
        );
        assert_eq!(
            sample_color_line(&stops, Extend::Reflect, 1.25),
            sample_color_line(&stops, Extend::Pad, 0.75)
        );
    }

    #[test]
    fn two_circle_radial_parameter_rejects_negative_radius_branch() {
        let t = radial_parameter(7.0, 0.0, [0.0, 0.0], 2.0, [0.0, 0.0], 12.0)
            .expect("concentric radial parameter");
        assert!((t - 0.5).abs() < 1.0e-5);
    }

    #[test]
    fn all_colr_composite_modes_have_tiny_skia_equivalents() {
        let modes = [
            CompositeMode::Clear,
            CompositeMode::Src,
            CompositeMode::Dest,
            CompositeMode::SrcOver,
            CompositeMode::DestOver,
            CompositeMode::SrcIn,
            CompositeMode::DestIn,
            CompositeMode::SrcOut,
            CompositeMode::DestOut,
            CompositeMode::SrcAtop,
            CompositeMode::DestAtop,
            CompositeMode::Xor,
            CompositeMode::Plus,
            CompositeMode::Screen,
            CompositeMode::Overlay,
            CompositeMode::Darken,
            CompositeMode::Lighten,
            CompositeMode::ColorDodge,
            CompositeMode::ColorBurn,
            CompositeMode::HardLight,
            CompositeMode::SoftLight,
            CompositeMode::Difference,
            CompositeMode::Exclusion,
            CompositeMode::Multiply,
            CompositeMode::HslHue,
            CompositeMode::HslSaturation,
            CompositeMode::HslColor,
            CompositeMode::HslLuminosity,
        ];
        assert_eq!(modes.len(), 28);
        for mode in modes {
            let _ = blend_mode(mode);
        }
    }
}
