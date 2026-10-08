//! BitmapFont raw-glyph dereferences at the immediate native draw boundary.

use super::*;

pub(super) struct BitmapTextFailure {
    pub(super) has_drawn_glyph: bool,
    pub(super) error: mlua::Error,
}

impl RenderBridge {
    pub(super) fn prepare_bitmap_text(
        &mut self,
        command: &mut TextRenderCommand,
    ) -> Option<BitmapTextFailure> {
        let Some(TextFontBinding::Bitmap { font, .. }) = command.font_binding.as_ref() else {
            return None;
        };
        // 42B338 reads tracking while measuring these anchors even if no
        // glyph exists. Empty UTF-8 input bypasses that draw at 42B200.
        if !font.spacing_initialized
            && matches!(command.horizontal_anchor.as_str(), "HCENTER" | "RIGHT")
            && let Err(error) = font.native_string_width(&command.text)
        {
            return Some(BitmapTextFailure {
                has_drawn_glyph: false,
                error: runtime_error(error.to_string()),
            });
        }
        let (offset, error) = font.first_released_glyph(&command.text)?;
        let mut failure = BitmapTextFailure {
            has_drawn_glyph: false,
            error: runtime_error(error.to_string()),
        };
        // 42B338 measures RIGHT/HCENTER before touching the context or drawing.
        // LEFT/HPIVOT enter the glyph loop directly and retain a live prefix.
        if matches!(command.horizontal_anchor.as_str(), "HCENTER" | "RIGHT") {
            return Some(failure);
        }
        command.text.truncate(offset);
        let [input_x, input_y] = command
            .native_system_origin
            .unwrap_or([command.x as f32, command.y as f32]);
        let base_x = self.state.pivot_x as f32 + input_x;
        let base_y = self.state.pivot_y as f32 + input_y;
        let [_, vertical] = font
            .native_draw_anchor("", "LEFT", &command.vertical_anchor)
            .expect("LEFT cached anchors never dereference a glyph");
        let mut glyph_x = input_x;
        let glyph_y = input_y + vertical as f32;
        for character in command.text.chars() {
            let Some(glyph) = font
                .live_glyph(character as u32)
                .expect("the prefix precedes the first released glyph")
            else {
                continue;
            };
            // Preserve the f32 operations and grouping at 42B624..42B670.
            // The successful return restores the saved pivot; failure does not.
            self.state.pivot_x = f64::from(base_x + (0.0 - glyph_x));
            self.state.pivot_y = f64::from(base_y + (f32::from(glyph.pivot_y) - glyph_y));
            failure.has_drawn_glyph = true;
            glyph_x += (i32::from(glyph.width) + i32::from(font.tracking)) as f32;
        }
        Some(failure)
    }
}
