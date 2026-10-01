//! Shared pixels and text for platform-owned window overlays.

use anyhow::{Result, anyhow};
use image::RgbaImage;
use stella_script::SystemFontRenderBinding;
use tiny_skia::{Pixmap, PixmapPaint, PixmapRef, Transform};
use unicode_segmentation::UnicodeSegmentation;

#[derive(Clone, Copy)]
pub(crate) struct Canvas {
    pub scale: f32,
    offset: [f32; 2],
}

impl Canvas {
    pub fn new(width: u32, height: u32) -> Self {
        let scale = (width as f32 / 1024.0)
            .min(height as f32 / 768.0)
            .max(f32::EPSILON);
        Self {
            scale,
            offset: [
                (width as f32 - 1024.0 * scale) * 0.5,
                (height as f32 - 768.0 * scale) * 0.5,
            ],
        }
    }
    pub fn rect(self, rect: Rect) -> Rect {
        Rect::new(
            rect.x * self.scale + self.offset[0],
            rect.y * self.scale + self.offset[1],
            rect.width * self.scale,
            rect.height * self.scale,
        )
    }
    pub fn to_logical(self, x: f32, y: f32) -> [f32; 2] {
        [
            (x - self.offset[0]) / self.scale,
            (y - self.offset[1]) / self.scale,
        ]
    }
}

pub(crate) fn draw_image(output: &mut Pixmap, image: &RgbaImage, mut rect: Rect, aspect_fit: bool) {
    if aspect_fit {
        let scale = (rect.width / image.width() as f32).min(rect.height / image.height() as f32);
        let width = image.width() as f32 * scale;
        let height = image.height() as f32 * scale;
        rect.x += (rect.width - width) * 0.5;
        rect.y += (rect.height - height) * 0.5;
        rect.width = width;
        rect.height = height;
    }
    let Some(pixels) = PixmapRef::from_bytes(image.as_raw(), image.width(), image.height()) else {
        return;
    };
    output.draw_pixmap(
        0,
        0,
        pixels,
        &PixmapPaint {
            quality: tiny_skia::FilterQuality::Bilinear,
            ..Default::default()
        },
        Transform::from_row(
            rect.width / image.width() as f32,
            0.0,
            0.0,
            rect.height / image.height() as f32,
            rect.x,
            rect.y,
        ),
        None,
    );
}

pub(crate) fn fill(output: &mut Pixmap, rect: Rect, color: [u8; 4]) {
    if let Some(rect) = tiny_skia::Rect::from_xywh(rect.x, rect.y, rect.width, rect.height) {
        let mut paint = tiny_skia::Paint::default();
        paint.set_color_rgba8(color[0], color[1], color[2], color[3]);
        output.fill_rect(rect, &paint, Transform::identity(), None);
    }
}

/// NSLineBreakMode values used by the original account labels.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum LineBreak {
    WordWrap,
    Clip,
    TruncateTail,
}

#[derive(Clone, Copy)]
pub(crate) struct TextLayout {
    pub alignment: u8,
    pub max_lines: u32,
    pub line_break: LineBreak,
}

struct WrappedLine<'a> {
    text: &'a str,
    /// The original paragraph suffix, without inserting spaces at soft wraps.
    remainder: &'a str,
}

fn word_wrapped_lines<'a>(
    font: &SystemFontRenderBinding,
    text: &'a str,
    width: f32,
) -> Vec<WrappedLine<'a>> {
    let mut lines = Vec::new();
    for paragraph in text.split('\n') {
        let (mut start, mut end) = (0, 0);
        let line = |start, end| WrappedLine {
            text: paragraph[start..end].trim_end(),
            remainder: &paragraph[start..],
        };
        for (offset, piece) in paragraph.split_word_bound_indices() {
            let candidate = &paragraph[start..offset + piece.len()];
            if start < end && font.native_string_width(candidate) as f32 > width {
                lines.push(line(start, end));
                start = offset;
                end = offset;
            }
            // CJK/long unbroken strings also obey the field's geometry.
            for (cluster_offset, cluster) in piece.grapheme_indices(true) {
                let offset = offset + cluster_offset;
                let next = offset + cluster.len();
                if start < end && font.native_string_width(&paragraph[start..next]) as f32 > width {
                    lines.push(line(start, end));
                    start = offset;
                    end = offset;
                }
                if start < end || !cluster.chars().all(char::is_whitespace) {
                    end = next;
                } else {
                    start = next;
                    end = next;
                }
            }
        }
        lines.push(line(start, end));
    }
    lines
}

fn truncate_tail(font: &SystemFontRenderBinding, text: &str, width: f32, force: bool) -> String {
    if !force && font.native_string_width(text) as f32 <= width {
        return text.to_owned();
    }
    let text = text.trim_end();
    let mut end = 0;
    // Only shape prefixes that can still be visible, instead of repeatedly
    // shaping an arbitrarily long hidden suffix while removing one cluster.
    for (offset, cluster) in text.grapheme_indices(true) {
        let next = offset + cluster.len();
        let prefix = text[..next].trim_end();
        if font.native_string_width(&format!("{prefix}…")) as f32 > width {
            break;
        }
        end = next;
    }
    format!("{}…", text[..end].trim_end())
}

pub(crate) fn label_lines(
    font: &SystemFontRenderBinding,
    text: &str,
    width: f32,
    layout: TextLayout,
) -> Vec<String> {
    if layout.line_break == LineBreak::Clip || layout.max_lines == 1 {
        let paragraphs: Vec<_> = text.split('\n').collect();
        return paragraphs
            .iter()
            .take(if layout.max_lines == 0 {
                usize::MAX
            } else {
                layout.max_lines as usize
            })
            .enumerate()
            .map(|(index, line)| {
                if layout.line_break == LineBreak::TruncateTail {
                    truncate_tail(font, line, width, index + 1 < paragraphs.len())
                } else {
                    (*line).to_owned()
                }
            })
            .collect();
    }
    let wrapped = word_wrapped_lines(font, text, width);
    let count = if layout.max_lines == 0 {
        wrapped.len()
    } else {
        wrapped.len().min(layout.max_lines as usize)
    };
    wrapped
        .iter()
        .take(count)
        .enumerate()
        .map(|(index, line)| {
            if layout.line_break == LineBreak::TruncateTail
                && index + 1 == count
                && count < wrapped.len()
            {
                truncate_tail(font, line.remainder, width, true)
            } else {
                line.text.to_owned()
            }
        })
        .collect()
}

pub(crate) fn wrap(
    font: &SystemFontRenderBinding,
    text: &str,
    width: f32,
    limit: u32,
) -> Vec<String> {
    label_lines(
        font,
        text,
        width,
        TextLayout {
            alignment: 0,
            max_lines: limit,
            line_break: LineBreak::TruncateTail,
        },
    )
}

/// NSString's constrained measurement is independent of UILabel.numberOfLines.
pub(crate) fn word_wrap_size(
    font: &SystemFontRenderBinding,
    text: &str,
    constraint: [f32; 2],
) -> [f32; 2] {
    if text.is_empty() {
        return [0.0; 2];
    }
    let line_height = font.label_line_height.max(1) as f32;
    let limit = (constraint[1] / line_height).floor().max(1.0) as usize;
    let lines = word_wrapped_lines(font, text, constraint[0]);
    let visible = &lines[..lines.len().min(limit)];
    let width = visible
        .iter()
        .map(|line| font.native_string_width(line.text))
        .max()
        .unwrap_or(0) as f32;
    [
        width.min(constraint[0]).max(0.0),
        line_height * visible.len() as f32,
    ]
}

pub(crate) fn text_lines(
    output: &mut Pixmap,
    font: &SystemFontRenderBinding,
    text: &str,
    rect: Rect,
    align: u8,
    limit: u32,
) -> Result<()> {
    text_lines_with_shadow(output, font, text, rect, align, limit, None)
}

/// UILabel draws its unblurred shadow behind the text, clipped to the same
/// label bounds. The offset is in drawable pixels after logical UI scaling.
pub(crate) fn text_lines_with_shadow(
    output: &mut Pixmap,
    font: &SystemFontRenderBinding,
    text: &str,
    rect: Rect,
    align: u8,
    limit: u32,
    shadow: Option<(&SystemFontRenderBinding, [f32; 2])>,
) -> Result<()> {
    text_with_layout(
        output,
        font,
        text,
        rect,
        TextLayout {
            alignment: align,
            max_lines: limit,
            line_break: if limit == 1 {
                LineBreak::Clip
            } else {
                LineBreak::TruncateTail
            },
        },
        shadow,
    )
}

pub(crate) fn text_with_layout(
    output: &mut Pixmap,
    font: &SystemFontRenderBinding,
    text: &str,
    rect: Rect,
    mut layout: TextLayout,
    shadow: Option<(&SystemFontRenderBinding, [f32; 2])>,
) -> Result<()> {
    if rect.width <= 0.0 || rect.height <= 0.0 {
        return Ok(());
    }
    let line_height = font.label_line_height.max(1) as f32;
    let capacity = (rect.height / line_height).floor().max(1.0) as u32;
    layout.max_lines = if layout.max_lines == 0 {
        capacity
    } else {
        layout.max_lines.min(capacity)
    };
    let lines = label_lines(font, text, rect.width, layout);
    let mut y = rect.y + (rect.height - lines.len() as f32 * line_height) * 0.5;
    for line in lines {
        if let Some(label) = crate::assets::rasterize_system_label(font, &line, "LEFT", "TOP")? {
            let x = match layout.alignment {
                1 => rect.x + (rect.width - label.image.width() as f32) * 0.5,
                2 => rect.x + rect.width - label.image.width() as f32,
                _ => rect.x,
            };
            // UILabel clips to its bounds. Draw into a tiny transparent view
            // first so long input cannot paint over adjacent native controls.
            let mut label_view = Pixmap::new(
                rect.width.ceil().max(1.0) as u32,
                rect.height.ceil().max(1.0) as u32,
            )
            .ok_or_else(|| anyhow!("account label size is invalid"))?;
            if let Some(pixels) = PixmapRef::from_bytes(
                label.image.as_raw(),
                label.image.width(),
                label.image.height(),
            ) {
                if let Some((shadow_font, [offset_x, offset_y])) = shadow
                    && let Some(shadow_label) =
                        crate::assets::rasterize_system_label(shadow_font, &line, "LEFT", "TOP")?
                    && let Some(shadow_pixels) = PixmapRef::from_bytes(
                        shadow_label.image.as_raw(),
                        shadow_label.image.width(),
                        shadow_label.image.height(),
                    )
                {
                    label_view.draw_pixmap(
                        (x - rect.x + offset_x).round() as i32,
                        (y - rect.y + offset_y).round() as i32,
                        shadow_pixels,
                        &PixmapPaint::default(),
                        Transform::identity(),
                        None,
                    );
                }
                label_view.draw_pixmap(
                    (x - rect.x).round() as i32,
                    (y - rect.y).round() as i32,
                    pixels,
                    &PixmapPaint::default(),
                    Transform::identity(),
                    None,
                );
                output.draw_pixmap(
                    rect.x.round() as i32,
                    rect.y.round() as i32,
                    label_view.as_ref(),
                    &PixmapPaint::default(),
                    Transform::identity(),
                    None,
                );
            }
        }
        y += line_height;
    }
    Ok(())
}

/// UIImage stretchableImageWithLeftCapWidth:topCapHeight: keeps the two
/// fixed sides and stretches one central point. Source uses the @2x asset.
pub(crate) fn draw_stretched(
    output: &mut Pixmap,
    image: &RgbaImage,
    rect: Rect,
    caps: [u32; 2],
    scale: f32,
    density: u32,
) {
    let left = (caps[0] * density).min(image.width().saturating_sub(1));
    let top = (caps[1] * density).min(image.height().saturating_sub(1));
    let right = image.width().saturating_sub(left + density);
    let bottom = image.height().saturating_sub(top + density);
    let fixed_x = [
        left as f32 / density as f32 * scale,
        right as f32 / density as f32 * scale,
    ];
    let fixed_y = [
        top as f32 / density as f32 * scale,
        bottom as f32 / density as f32 * scale,
    ];
    let sx = [0, left, image.width() - right, image.width()];
    let sy = [0, top, image.height() - bottom, image.height()];
    let dx = [
        rect.x,
        rect.x + fixed_x[0],
        rect.x + rect.width - fixed_x[1],
        rect.x + rect.width,
    ];
    let dy = [
        rect.y,
        rect.y + fixed_y[0],
        rect.y + rect.height - fixed_y[1],
        rect.y + rect.height,
    ];
    for row in 0..3 {
        for col in 0..3 {
            if sx[col + 1] <= sx[col] || sy[row + 1] <= sy[row] {
                continue;
            }
            let part = image::imageops::crop_imm(
                image,
                sx[col],
                sy[row],
                sx[col + 1] - sx[col],
                sy[row + 1] - sy[row],
            )
            .to_image();
            draw_image(
                output,
                &part,
                Rect::new(
                    dx[col],
                    dy[row],
                    dx[col + 1] - dx[col],
                    dy[row + 1] - dy[row],
                ),
                false,
            );
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Rect {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

impl Rect {
    pub(crate) const fn new(x: f32, y: f32, width: f32, height: f32) -> Self {
        Self {
            x,
            y,
            width,
            height,
        }
    }
}
