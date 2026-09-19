//! Native nib geometry and view state, with a bounded desktop picker surface.
//! The iOS UIPickerView wheel shader/momentum is not part of the game binary;
//! arrows/wheel/row clicks preserve its recovered selection and dismissal rules.

use super::*;
use layout::registration as native;

pub(super) fn date_tag(name: &str) -> Option<u8> {
    native::DATE_PICKERS
        .iter()
        .find(|picker| picker.field == name)
        .map(|picker| picker.tag)
}

impl AccountPainter {
    pub(super) fn registration_group_rects(
        &mut self,
        runtime: &StellaLua,
        view: AccountView,
    ) -> Result<HashMap<&'static str, Rect>> {
        let mut result = HashMap::new();
        for group in native::groups(view) {
            let mut widths = Vec::new();
            for element in group.elements {
                let text = self
                    .strings
                    .get(element.text_key.unwrap_or(""), element.fallback)
                    .to_owned();
                let width = if element.kind == Kind::Label {
                    let font = self
                        .font(runtime, element.font_name, element.font_size, element.color)
                        .with_context(|| format!("measure native account {} group", group.name))?;
                    f64::from(font.native_string_width(&text)) / f64::from(self.canvas.scale)
                } else {
                    f64::from(element.rect.width)
                };
                widths.push(width);
            }
            // The strings object records the language actually selected by
            // bundle lookup, rather than assuming OS preference[0] was found.
            let rects = native::group_rects(group, &widths, self.strings.is_russian());
            result.extend(
                group
                    .elements
                    .iter()
                    .zip(rects)
                    .map(|(e, rect)| (e.name, rect)),
            );
        }
        Ok(result)
    }

    pub(super) fn registration_date_text(
        &self,
        state: &AccountUi,
        tag: u8,
        placeholder: &str,
    ) -> String {
        if let Some(value) = state.registration.values[usize::from(tag)] {
            format!(
                "{}{}",
                native::DATE_VALUE_PREFIX,
                self.date_value_text(tag, value)
            )
        } else {
            format!("{}{placeholder}", native::DATE_PLACEHOLDER_PREFIX)
        }
    }

    fn date_value_text(&self, tag: u8, value: i32) -> String {
        if tag == 1 && (1..=12).contains(&value) {
            let (key, fallback) = native::MONTH_LABELS[value as usize - 1];
            self.strings.get(key, fallback).to_owned()
        } else {
            value.to_string()
        }
    }

    pub(super) fn paint_registration_picker(
        &mut self,
        output: &mut Pixmap,
        runtime: &StellaLua,
        state: &AccountUi,
    ) -> Result<()> {
        let Some(tag) = state.registration.picker else {
            return Ok(());
        };
        let picker = &native::DATE_PICKERS[usize::from(tag)];
        let rect = self.canvas.rect(picker.rect);
        let rgba = |color: [f32; 4]| color.map(|c| (c * 255.0).round() as u8);
        // UIKit's wheel is host UI. Retain the exact containing frame and
        // configured corner/border/background, not invented game textures.
        let radius = native::PICKER_CORNER_RADIUS * self.canvas.scale;
        if let Some(path) = rounded_rect(rect, radius) {
            let mut paint = tiny_skia::Paint::default();
            let [r, g, b, a] = rgba(native::PICKER_BACKGROUND);
            paint.set_color_rgba8(r, g, b, a);
            output.fill_path(
                &path,
                &paint,
                tiny_skia::FillRule::Winding,
                Transform::identity(),
                None,
            );
            let [r, g, b, a] = rgba(native::PICKER_BORDER);
            paint.set_color_rgba8(r, g, b, a);
            output.stroke_path(
                &path,
                &paint,
                &tiny_skia::Stroke {
                    width: native::PICKER_BORDER_WIDTH * self.canvas.scale,
                    ..Default::default()
                },
                Transform::identity(),
                None,
            );
        }
        self.hit_regions.push((picker.name, picker.rect));
        const ROW_NAMES: [&str; 5] = [
            "pickerRowMinus2",
            "pickerRowMinus1",
            "pickerRow0",
            "pickerRowPlus1",
            "pickerRowPlus2",
        ];
        let row_height = picker.rect.height / 5.0;
        for (i, name) in ROW_NAMES.into_iter().enumerate() {
            let row = state.registration.row + i as i32 - 2;
            if !(0..state.registration.row_count(tag)).contains(&row) {
                continue;
            }
            let logical = Rect::new(
                picker.rect.x + 1.0,
                picker.rect.y + row_height * i as f32,
                picker.rect.width - 2.0,
                row_height,
            );
            if i == 2 {
                fill(output, self.canvas.rect(logical), [225, 229, 234, 220]);
            }
            let font = self.font(
                runtime,
                ".HelveticaNeueInterface-Regular",
                21.0,
                if i == 2 {
                    [0, 0, 0, 255]
                } else {
                    [92, 92, 92, 255]
                },
            )?;
            let text = self.date_value_text(tag, row + if tag == 2 { 1900 } else { 1 });
            text_lines(output, &font, &text, self.canvas.rect(logical), 1, 1)?;
            self.hit_regions.push((name, logical));
        }
        Ok(())
    }
}

fn rounded_rect(rect: Rect, radius: f32) -> Option<tiny_skia::Path> {
    let mut path = tiny_skia::PathBuilder::new();
    let (x, y, right, bottom) = (rect.x, rect.y, rect.x + rect.width, rect.y + rect.height);
    path.move_to(x + radius, y);
    path.line_to(right - radius, y);
    path.quad_to(right, y, right, y + radius);
    path.line_to(right, bottom - radius);
    path.quad_to(right, bottom, right - radius, bottom);
    path.line_to(x + radius, bottom);
    path.quad_to(x, bottom, x, bottom - radius);
    path.line_to(x, y + radius);
    path.quad_to(x, y, x + radius, y);
    path.close();
    path.finish()
}
