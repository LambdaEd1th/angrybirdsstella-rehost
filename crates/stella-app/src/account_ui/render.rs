//! CPU drawing of platform UI into a private premultiplied window overlay.
//! Uses original account PNGs/fonts; never registers a game texture or label.

use std::{collections::HashMap, path::PathBuf};

use anyhow::{Context, Result, anyhow};
use image::RgbaImage;
use stella_script::{AccountView, StellaLua, SystemFontRenderBinding};
use tiny_skia::{Pixmap, PixmapPaint, Transform};
use unicode_segmentation::UnicodeSegmentation;

use super::{
    Field,
    layout::{self, Element, Kind, Rect},
    state::AccountUi,
    strings::Strings,
};

use crate::platform_ui_drawing as drawing;
mod registration;
use drawing::{Canvas, draw_image, fill, text_lines};

pub(crate) struct AccountPainter {
    root: PathBuf,
    strings: Strings,
    images: HashMap<String, RgbaImage>,
    image_densities: HashMap<String, u32>,
    fonts: HashMap<(&'static str, i32, [u8; 4]), SystemFontRenderBinding>,
    pub(super) hit_regions: Vec<(&'static str, Rect)>,
    hit_owner: Option<(u64, AccountView, bool)>,
    last: Option<(u64, u32, u32, u8, bool)>,
    canvas: Canvas,
}

impl AccountPainter {
    pub(crate) fn invalidate(&mut self) {
        self.last = None;
    }

    pub(crate) fn new(root: PathBuf, runtime: &StellaLua) -> Self {
        Self {
            strings: Strings::load(&root, &runtime.platform_ui_languages()),
            root,
            images: HashMap::new(),
            image_densities: HashMap::new(),
            fonts: HashMap::new(),
            hit_regions: Vec::new(),
            hit_owner: None,
            last: None,
            canvas: Canvas::new(1024, 768),
        }
    }

    /// Input may arrive after a worker changed the page but before redraw.
    /// Never dispatch a button from the previous page/owner in that interval.
    /// The logical hit rectangles survive resizing; only their transform moves.
    pub(crate) fn synchronize_context(&mut self, state: &AccountUi, width: u32, height: u32) {
        let owner = state.snapshot.as_ref().map(|s| (s.id, s.view, s.busy));
        if self.hit_owner != owner {
            self.hit_regions.clear();
        }
        self.canvas = Canvas::new(width, height);
    }

    fn font(
        &mut self,
        runtime: &StellaLua,
        name: &'static str,
        size: f32,
        color: [u8; 4],
    ) -> Result<SystemFontRenderBinding> {
        let size = (size * self.canvas.scale).round().max(1.0) as i32;
        let key = (name, size, color);
        if let Some(font) = self.fonts.get(&key) {
            return Ok(font.clone());
        }
        let font = runtime
            .platform_ui_font(name, size, color)
            .map_err(|error| anyhow!(error.to_string()))?;
        self.fonts.insert(key, font.clone());
        Ok(font)
    }

    fn image(&mut self, name: &str) -> Result<&RgbaImage> {
        if !self.images.contains_key(name) {
            let base = name.strip_suffix(".png").unwrap_or(name);
            let candidates = [
                format!("{base}@2x~ipad.png"),
                format!("{base}~ipad.png"),
                format!("{base}@2x.png"),
                name.to_owned(),
            ];
            let path = candidates
                .iter()
                .map(|path| self.root.join(path))
                .find(|path| path.is_file())
                .ok_or_else(|| anyhow!("missing native account artwork: {name}"))?;
            let mut image = image::open(&path)
                .with_context(|| format!("read account image {}", path.display()))?
                .into_rgba8();
            for pixel in image.pixels_mut() {
                let alpha = u16::from(pixel[3]);
                for channel in &mut pixel.0[..3] {
                    *channel = ((u16::from(*channel) * alpha + 127) / 255) as u8;
                }
            }
            self.image_densities.insert(
                name.to_owned(),
                if path
                    .file_name()
                    .is_some_and(|name| name.to_string_lossy().contains("@2x"))
                {
                    2
                } else {
                    1
                },
            );
            self.images.insert(name.to_owned(), image);
        }
        Ok(&self.images[name])
    }

    /// None means unchanged, not hidden. The app separately removes a hidden
    /// overlay. Credentials are deliberately absent from this cache key.
    pub(crate) fn paint(
        &mut self,
        runtime: &StellaLua,
        state: &AccountUi,
        width: u32,
        height: u32,
        seconds: f64,
    ) -> Result<Option<RgbaImage>> {
        let Some(snapshot) = &state.snapshot else {
            return Ok(None);
        };
        let progress = if snapshot.busy {
            ((seconds % layout::PROGRESS_DURATION) / layout::PROGRESS_DURATION * 6.0) as u8
        } else {
            0
        };
        let caret = state.focus.is_some() && ((seconds * 2.0) as u64 & 1) == 0;
        let key = (state.revision, width, height, progress, caret);
        if self.last == Some(key) {
            return Ok(None);
        }
        self.canvas = Canvas::new(width, height);
        let mut output =
            Pixmap::new(width, height).ok_or_else(|| anyhow!("account overlay size is invalid"))?;
        output.fill(tiny_skia::Color::from_rgba8(0, 0, 0, 128));
        self.hit_regions.clear();
        let view = if snapshot.busy {
            Some(layout::progress_layout())
        } else {
            layout::layout_for(snapshot.view)
        };
        if let Some(view) = view {
            // The iPad layout is exact. Fitting its logical canvas to a
            // resizable desktop window is a host adaptation, not UIKit scale.
            debug_assert_eq!(view.size, [1024.0, 768.0]);
            debug_assert_eq!(view.backdrop, [0.0, 0.0, 0.0, 0.5]);
            let mut link_sizes = [0.0; 2];
            for (i, name) in ["dontHaveAccountLabel", "registerLabel"].iter().enumerate() {
                if let Some(element) = view.elements.iter().find(|e| e.name == *name) {
                    let text = self
                        .strings
                        .get(element.text_key.unwrap_or(""), element.fallback)
                        .to_owned();
                    let font =
                        self.font(runtime, element.font_name, element.font_size, element.color)?;
                    link_sizes[i] =
                        f64::from(font.native_string_width(&text)) / f64::from(self.canvas.scale);
                }
            }
            let link_rects = layout::sign_in_link_rects(link_sizes);
            let registration_rects = self.registration_group_rects(runtime, snapshot.view)?;
            for element in view.elements {
                let email = element.name.to_ascii_lowercase().starts_with("email");
                let password = element.name.to_ascii_lowercase().starts_with("password");
                let error = (email && state.email_error.is_some())
                    || (password && state.password_error.is_some());
                let red_border =
                    (email && state.email_border) || (password && state.password_border);
                if element.hidden && !error {
                    continue;
                }
                if element.name == "passwordTooltipButton" && state.password_error.is_some() {
                    continue;
                }
                let mut rect = element.rect;
                if snapshot.view == AccountView::SignIn && element.name == "dontHaveAccountLabel" {
                    rect = link_rects[0];
                }
                if snapshot.view == AccountView::SignIn && element.name == "registerLabel" {
                    rect = link_rects[1];
                }
                if let Some(group_rect) = registration_rects.get(element.name) {
                    rect = *group_rect;
                }
                let mut text = element
                    .text_key
                    .map_or(element.fallback, |key| {
                        self.strings.get(key, element.fallback)
                    })
                    .to_owned();
                if matches!(element.name, "verificationEmail" | "registrationEmail") {
                    text = state.email.text().to_owned();
                }
                if let Some(tag) = registration::date_tag(element.name) {
                    text = self.registration_date_text(state, tag, &text);
                }
                let image_name = if snapshot.busy && element.name == "progressImageView" {
                    Some(layout::PROGRESS_IMAGES[usize::from(progress.min(5))])
                } else if let Some(tag) = registration::date_tag(element.name) {
                    Some(if state.registration.picker == Some(tag) {
                        layout::registration::DATE_OPEN_IMAGE
                    } else if state.registration.date_errors[usize::from(tag)] {
                        "skynestdata/images/identity/milkshake_datemonthyear_button_error.png"
                    } else {
                        layout::registration::DATE_IMAGE
                    })
                } else if matches!(element.name, "gender_male_button" | "gender_female_button") {
                    Some(
                        if (element.name == "gender_female_button") == state.registration.female {
                            layout::registration::GENDER_ON_IMAGE
                        } else {
                            layout::registration::GENDER_OFF_IMAGE
                        },
                    )
                } else if red_border {
                    element.error_image.or(element.image)
                } else if state.pressed.as_deref() == Some(element.name) {
                    element.pressed_image.or(element.image)
                } else {
                    element.image
                };
                if element.name == "forgotPasswordLabel" {
                    let font =
                        self.font(runtime, element.font_name, element.font_size, element.color)?;
                    rect.width = font.native_string_width(&text) as f32 / self.canvas.scale;
                }
                if let Some(background) = element.button_background {
                    let pressed = state.pressed.as_deref() == Some(element.name);
                    let name = if pressed {
                        "skynestdata/images/identity/button_arrow_forward_bottom_right_down.png"
                    } else {
                        background
                    };
                    let canvas = self.canvas;
                    draw_image(&mut output, self.image(name)?, canvas.rect(rect), false);
                }
                if let Some(name) = image_name {
                    let canvas = self.canvas;
                    draw_image(
                        &mut output,
                        self.image(name)?,
                        canvas.rect(rect),
                        element.content_mode == 1,
                    );
                }
                if element.kind == Kind::Field {
                    self.field(&mut output, runtime, state, element, caret)?;
                } else if !text.is_empty() {
                    let font =
                        self.font(runtime, element.font_name, element.font_size, element.color)?;
                    text_lines(
                        &mut output,
                        &font,
                        &text,
                        self.canvas.rect(rect),
                        element.alignment,
                        element.max_lines,
                    )?;
                }
                if !snapshot.busy
                    && (matches!(element.kind, Kind::Field | Kind::Button)
                        || matches!(
                            element.name,
                            "forgotPasswordLabel" | "eulaLabel" | "privacyPolicyLabel"
                        )
                        || (snapshot.view == AccountView::SignIn
                            && element.name == "registerLabel"))
                {
                    self.hit_regions.push((element.name, rect));
                }
            }
            if !snapshot.busy {
                for (name, error, visible) in [
                    ("emailErrorButton", state.email_error, state.email_popup),
                    (
                        "passwordErrorButton",
                        state.password_error,
                        state.password_popup,
                    ),
                ] {
                    if visible
                        && let Some((key, fallback)) = error
                        && let Some(element) =
                            view.elements.iter().find(|element| element.name == name)
                    {
                        let text = self.strings.get(key, fallback).to_owned();
                        let rect = self.error_popup(&mut output, runtime, element.rect, &text)?;
                        self.hit_regions.push(("errorPopup", rect));
                    }
                }
                if state.password_help
                    && let Some(element) = view
                        .elements
                        .iter()
                        .find(|e| e.name == "passwordTooltipButton")
                {
                    let text = self
                        .strings
                        .get(
                            "rovio_id_password_help_text",
                            "Password must contain at least 8 characters",
                        )
                        .to_owned();
                    let rect = self.error_popup(&mut output, runtime, element.rect, &text)?;
                    self.hit_regions.push(("errorPopup", rect));
                }
                self.paint_registration_picker(&mut output, runtime, state)?;
            }
        } else {
            // Deliberately labelled host boundary, not a fabricated successful
            // native registration screen. The native Cancel action stays usable.
            let font = self.font(runtime, "OpenSans", 22.0, [255; 4])?;
            text_lines(
                &mut output,
                &font,
                "This account view is not implemented yet.\nPress Escape to return to the game.",
                self.canvas.rect(Rect::new(240.0, 300.0, 544.0, 160.0)),
                1,
                4,
            )?;
        }
        self.last = Some(key);
        self.hit_owner = Some((snapshot.id, snapshot.view, snapshot.busy));
        Ok(Some(
            RgbaImage::from_raw(width, height, output.take()).expect("Pixmap extent"),
        ))
    }

    fn field(
        &mut self,
        output: &mut Pixmap,
        runtime: &StellaLua,
        state: &AccountUi,
        element: &Element,
        caret: bool,
    ) -> Result<()> {
        let field = if element.name == "passwordTextField" {
            Field::Password
        } else {
            Field::Email
        };
        let editor = if field == Field::Password {
            &state.password
        } else {
            state.email_editor()
        };
        let (display, cursor, selected) = editor.display(field == Field::Password);
        let font = self.font(
            runtime,
            element.font_name,
            element.font_size,
            [0, 0, 0, 255],
        )?;
        let rect = self.canvas.rect(layout::field_content_rect(
            state.snapshot.as_ref().expect("visible field owner").view,
            element,
        ));
        let mut field_pixels = Pixmap::new(
            rect.width.ceil().max(1.0) as u32,
            rect.height.ceil().max(1.0) as u32,
        )
        .ok_or_else(|| anyhow!("invalid account field size"))?;
        if display.is_empty() {
            let mut placeholder_font = font.clone();
            placeholder_font.fill_rgba = [128, 128, 128, 255];
            let placeholder = self
                .strings
                .get(element.text_key.unwrap_or(""), element.fallback);
            text_lines(
                &mut field_pixels,
                &placeholder_font,
                placeholder,
                Rect::new(0.0, 0.0, rect.width, rect.height),
                0,
                1,
            )?;
        } else {
            let cursor_x = font.native_string_width(&display[..cursor]) as f32;
            let scroll = if state.focus == Some(field) {
                (cursor_x - rect.width + 3.0).max(0.0)
            } else {
                0.0
            };
            if state.focus == Some(field) && !selected.is_empty() {
                let left = font.native_string_width(&display[..selected.start]) as f32 - scroll;
                let right = font.native_string_width(&display[..selected.end]) as f32 - scroll;
                fill(
                    &mut field_pixels,
                    Rect::new(left, 2.0, (right - left).max(1.0), rect.height - 4.0),
                    [86, 150, 234, 100],
                );
            }
            let text_width = font.native_string_width(&display).max(1) as f32;
            text_lines(
                &mut field_pixels,
                &font,
                &display,
                Rect::new(-scroll, 0.0, text_width, rect.height),
                0,
                1,
            )?;
            if caret && state.focus == Some(field) {
                fill(
                    &mut field_pixels,
                    Rect::new(
                        cursor_x - scroll,
                        5.0 * self.canvas.scale,
                        self.canvas.scale.max(1.0),
                        rect.height - 10.0 * self.canvas.scale,
                    ),
                    [20, 80, 210, 255],
                );
            }
        }
        if caret && state.focus == Some(field) && display.is_empty() {
            fill(
                &mut field_pixels,
                Rect::new(
                    0.0,
                    5.0 * self.canvas.scale,
                    self.canvas.scale.max(1.0),
                    rect.height - 10.0 * self.canvas.scale,
                ),
                [20, 80, 210, 255],
            );
        }
        output.draw_pixmap(
            rect.x.round() as i32,
            rect.y.round() as i32,
            field_pixels.as_ref(),
            &PixmapPaint::default(),
            Transform::identity(),
            None,
        );
        Ok(())
    }

    fn error_popup(
        &mut self,
        output: &mut Pixmap,
        runtime: &StellaLua,
        anchor: Rect,
        text: &str,
    ) -> Result<Rect> {
        let font = self.font(runtime, "OpenSans", layout::ERROR_POPUP_FONT_SIZE, [255; 4])?;
        let lines = drawing::wrap(
            &font,
            text,
            layout::ERROR_POPUP_TEXT_CONSTRAINT[0] * self.canvas.scale,
            2,
        );
        let width = lines
            .iter()
            .map(|line| font.native_string_width(line))
            .max()
            .unwrap_or(0) as f32
            / self.canvas.scale;
        let height = font.label_line_height as f32 * lines.len() as f32 / self.canvas.scale;
        let logical_rect = layout::error_popup_rect(anchor, [width, height]);
        let rect = self.canvas.rect(logical_rect);
        let scale = self.canvas.scale;
        self.image(layout::ERROR_POPUP_IMAGE)?;
        let density = self.image_densities[layout::ERROR_POPUP_IMAGE];
        let image = self.image(layout::ERROR_POPUP_IMAGE)?;
        drawing::draw_stretched(
            output,
            image,
            rect,
            layout::ERROR_POPUP_CAPS,
            scale,
            density,
        );
        let inset = layout::ERROR_POPUP_TEXT_TOP_INSET * scale;
        text_lines(
            output,
            &font,
            &lines.join("\n"),
            Rect::new(rect.x, rect.y + inset, rect.width, rect.height - inset),
            1,
            2,
        )?;
        Ok(logical_rect)
    }

    pub(crate) fn hit(&self, x: f32, y: f32) -> Option<&'static str> {
        let [x, y] = self.canvas.to_logical(x, y);
        self.hit_regions
            .iter()
            .rev()
            .find(|(_, r)| x >= r.x && y >= r.y && x < r.x + r.width && y < r.y + r.height)
            .map(|(name, _)| *name)
    }

    pub(crate) fn ime_rect(&self, field: Field) -> Option<Rect> {
        let name = if field == Field::Email {
            "emailTextField"
        } else {
            "passwordTextField"
        };
        self.hit_regions
            .iter()
            .find(|(n, _)| *n == name)
            .map(|(_, rect)| self.canvas.rect(*rect))
    }

    pub(crate) fn place_cursor(
        &mut self,
        runtime: &StellaLua,
        state: &mut AccountUi,
        field: Field,
        x: f32,
        extend: bool,
    ) -> Result<()> {
        if self.ime_rect(field).is_none() {
            return Ok(());
        }
        let Some(view) = state.snapshot.as_ref().map(|snapshot| snapshot.view) else {
            return Ok(());
        };
        let name = if field == Field::Password {
            "passwordTextField"
        } else {
            "emailTextField"
        };
        let Some(element) = layout::layout_for(view)
            .and_then(|layout| layout.elements.iter().find(|element| element.name == name))
        else {
            return Ok(());
        };
        // Use exactly the same clipped content box and scroll as field().
        let rect = self.canvas.rect(layout::field_content_rect(view, element));
        let size = if state
            .snapshot
            .as_ref()
            .is_some_and(|s| s.view == AccountView::ForgotPassword)
        {
            16.0
        } else {
            18.0
        };
        let font = self.font(runtime, "OpenSans", size, [0, 0, 0, 255])?;
        // Pointer placement cancels marked text even within the same focused
        // field. Route that visible edit through the validation cache first.
        state.clear_preedit();
        let editor = if field == Field::Password {
            &mut state.password
        } else {
            state.email_editor_mut()
        };
        let text = editor.text();
        let current_cursor = editor.cursor();
        let cursor_prefix = if field == Field::Password {
            "•".repeat(text[..current_cursor].graphemes(true).count())
        } else {
            text[..current_cursor].to_owned()
        };
        let scroll = (font.native_string_width(&cursor_prefix) as f32 - rect.width + 3.0).max(0.0);
        let mut best = 0;
        let mut distance = f32::INFINITY;
        for end in text
            .grapheme_indices(true)
            .map(|(i, _)| i)
            .chain(std::iter::once(text.len()))
        {
            let prefix = if field == Field::Password {
                "•".repeat(text[..end].graphemes(true).count())
            } else {
                text[..end].to_owned()
            };
            let dx = (font.native_string_width(&prefix) as f32 - scroll - (x - rect.x)).abs();
            if dx < distance {
                distance = dx;
                best = end;
            }
        }
        editor.move_to(best, extend);
        state.dirty();
        Ok(())
    }
}

#[cfg(test)]
mod tests;
