//! Portable presentation of Purple's three-button UIKit rating alert.
//! The message/order/state are native; window geometry uses a host modal.

use anyhow::{Result, anyhow};
use image::RgbaImage;
use stella_script::{AppRatingChoice, AppRatingPrompt, StellaLua};
use tiny_skia::Pixmap;

use crate::platform_ui_drawing::{Canvas, Rect, fill, text_lines, wrap};

#[derive(Default)]
pub(crate) struct AppRatingUi {
    prompt: Option<AppRatingPrompt>,
    pointer: [f32; 2],
    pressed: Option<(u64, AppRatingChoice)>,
    focused: usize,
    regions: Vec<(AppRatingChoice, Rect)>,
    size: [u32; 2],
    dirty: bool,
}

impl AppRatingUi {
    #[cfg(test)]
    pub(crate) fn button_rect(&self, choice: AppRatingChoice) -> Option<Rect> {
        self.regions
            .iter()
            .find_map(|(candidate, rect)| (*candidate == choice).then_some(*rect))
    }

    pub(crate) fn visible(&self) -> bool {
        self.prompt.is_some()
    }

    /// Clear input across prompt replacement or resize. A press must begin
    /// and end in the same rendered button of the same retained alert.
    pub(crate) fn sync(
        &mut self,
        prompt: Option<AppRatingPrompt>,
        width: u32,
        height: u32,
    ) -> bool {
        let changed = self.prompt != prompt;
        if changed || self.size != [width, height] {
            self.pressed = None;
            self.regions.clear();
            self.dirty = true;
            self.size = [width, height];
        }
        if changed {
            self.focused = 0;
            self.prompt = prompt;
        }
        changed
    }

    pub(crate) fn move_pointer(&mut self, x: f32, y: f32) {
        if self.pointer != [x, y] {
            self.pointer = [x, y];
            self.dirty = true;
        }
    }

    fn hit(&self) -> Option<AppRatingChoice> {
        let [x, y] = self.pointer;
        self.regions.iter().find_map(|(choice, rect)| {
            (x >= rect.x && y >= rect.y && x < rect.x + rect.width && y < rect.y + rect.height)
                .then_some(*choice)
        })
    }

    pub(crate) fn press(&mut self) {
        self.pressed = self
            .prompt
            .as_ref()
            .and_then(|p| self.hit().map(|choice| (p.id, choice)));
        self.dirty = true;
    }

    pub(crate) fn cancel_press(&mut self) {
        self.pressed = None;
        self.dirty = true;
    }

    pub(crate) fn release(&mut self) -> Option<(u64, AppRatingChoice)> {
        self.dirty = true;
        self.pressed.take().filter(|(id, choice)| {
            self.prompt.as_ref().is_some_and(|p| p.id == *id) && self.hit() == Some(*choice)
        })
    }

    pub(crate) fn focus_next(&mut self, backwards: bool) {
        self.focused = (self.focused + if backwards { 2 } else { 1 }) % 3;
        self.dirty = true;
    }

    pub(crate) fn focused_answer(&self) -> Option<(u64, AppRatingChoice)> {
        self.prompt
            .as_ref()
            .map(|p| (p.id, p.buttons[self.focused].choice))
    }

    /// Repaint is forced when another platform overlay may have replaced us.
    pub(crate) fn paint(&mut self, runtime: &StellaLua, force: bool) -> Result<Option<RgbaImage>> {
        if !self.dirty && !force {
            return Ok(None);
        }
        let Some(prompt) = &self.prompt else {
            return Ok(None);
        };
        let [width, height] = self.size;
        if width == 0 || height == 0 {
            return Ok(None);
        }
        let canvas = Canvas::new(width, height);
        let font_size = (23.0 * canvas.scale).round().max(1.0) as i32;
        let message_font = runtime
            .platform_ui_font(
                ".HelveticaNeueInterface-Regular",
                font_size,
                [32, 33, 35, 255],
            )
            .map_err(|e| anyhow!(e.to_string()))?;
        let button_font = runtime
            .platform_ui_font(
                ".HelveticaNeueInterface-Regular",
                font_size,
                [0, 105, 215, 255],
            )
            .map_err(|e| anyhow!(e.to_string()))?;
        let message_width = 420.0 * canvas.scale;
        let lines = wrap(&message_font, &prompt.message, message_width, 0);
        let message_height =
            (lines.len() as f32 * message_font.label_line_height as f32 / canvas.scale + 44.0)
                .max(108.0);
        let mut button_heights = [64.0_f32; 3];
        for (i, button) in prompt.buttons.iter().enumerate() {
            let lines = wrap(&button_font, &button.title, message_width, 0);
            button_heights[i] =
                (lines.len() as f32 * button_font.label_line_height as f32 / canvas.scale + 24.0)
                    .max(64.0);
        }
        let panel_height = message_height + button_heights.iter().sum::<f32>();
        let panel = Rect::new(280.0, (768.0 - panel_height) * 0.5, 464.0, panel_height);
        let mut output = Pixmap::new(width, height)
            .ok_or_else(|| anyhow!("app rating overlay size is invalid"))?;
        fill(
            &mut output,
            Rect::new(0.0, 0.0, width as f32, height as f32),
            [0, 0, 0, 125],
        );
        fill(&mut output, canvas.rect(panel), [247, 247, 249, 255]);
        text_lines(
            &mut output,
            &message_font,
            &prompt.message,
            canvas.rect(Rect::new(
                panel.x + 22.0,
                panel.y + 22.0,
                420.0,
                message_height - 44.0,
            )),
            1,
            0,
        )?;
        self.regions.clear();
        let mut y = panel.y + message_height;
        for (i, button) in prompt.buttons.iter().enumerate() {
            let rect = Rect::new(panel.x, y, panel.width, button_heights[i]);
            let physical = canvas.rect(rect);
            self.regions.push((button.choice, physical));
            if self.pressed == Some((prompt.id, button.choice)) || i == self.focused {
                fill(&mut output, physical, [223, 234, 249, 255]);
            }
            fill(
                &mut output,
                canvas.rect(Rect::new(rect.x, rect.y, rect.width, 1.0)),
                [203, 204, 206, 255],
            );
            text_lines(
                &mut output,
                &button_font,
                &button.title,
                canvas.rect(Rect::new(
                    rect.x + 22.0,
                    rect.y + 8.0,
                    420.0,
                    rect.height - 16.0,
                )),
                1,
                0,
            )?;
            y += rect.height;
        }
        self.dirty = false;
        Ok(Some(
            RgbaImage::from_raw(width, height, output.take())
                .ok_or_else(|| anyhow!("app rating overlay pixels are invalid"))?,
        ))
    }
}

#[cfg(test)]
mod tests;
