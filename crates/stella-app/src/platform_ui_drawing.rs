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

pub(crate) fn wrap(
    font: &SystemFontRenderBinding,
    text: &str,
    width: f32,
    limit: u32,
) -> Vec<String> {
    let mut lines = Vec::new();
    for paragraph in text.split('\n') {
        let mut line = String::new();
        for piece in paragraph.split_word_bounds() {
            let candidate = format!("{line}{piece}");
            if !line.is_empty() && font.native_string_width(&candidate) as f32 > width {
                lines.push(line.trim_end().to_owned());
                line.clear();
            }
            // CJK/long unbroken strings also obey the field's geometry.
            for cluster in piece.graphemes(true) {
                if !line.is_empty()
                    && font.native_string_width(&format!("{line}{cluster}")) as f32 > width
                {
                    lines.push(line.trim_end().to_owned());
                    line.clear();
                }
                if !line.is_empty() || !cluster.chars().all(char::is_whitespace) {
                    line.push_str(cluster);
                }
            }
        }
        lines.push(line);
    }
    if limit > 0 && lines.len() > limit as usize {
        lines.truncate(limit as usize);
        let last = lines.last_mut().expect("nonzero limit");
        while !last.is_empty() && font.native_string_width(&format!("{last}…")) as f32 > width {
            let start = last
                .grapheme_indices(true)
                .next_back()
                .map_or(0, |(i, _)| i);
            last.truncate(start);
        }
        last.push('…');
    }
    lines
}

pub(crate) fn text_lines(
    output: &mut Pixmap,
    font: &SystemFontRenderBinding,
    text: &str,
    rect: Rect,
    align: u8,
    limit: u32,
) -> Result<()> {
    if rect.width <= 0.0 || rect.height <= 0.0 {
        return Ok(());
    }
    let lines = if limit == 1 {
        vec![text.split('\n').next().unwrap_or("").to_owned()]
    } else {
        wrap(font, text, rect.width, limit)
    };
    let line_height = font.label_line_height as f32;
    let mut y = rect.y + (rect.height - lines.len() as f32 * line_height) * 0.5;
    for line in lines {
        if let Some(label) = crate::assets::rasterize_system_label(font, &line, "LEFT", "TOP")? {
            let x = match align {
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
