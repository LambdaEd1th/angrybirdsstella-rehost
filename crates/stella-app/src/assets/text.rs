use super::*;

impl AssetCatalog {
    #[cfg(test)]
    #[allow(dead_code)]
    pub(crate) fn draw_text(
        &mut self,
        command: &TextRenderCommand,
        target: &mut [u32],
    ) -> Result<()> {
        if command.projection_3d.is_some() {
            let frame = self.prepare_gpu_frame(&[], std::slice::from_ref(command), &[], &[])?;
            return frame.render_reference(self, target);
        }
        let (font, texture_source) = match command.font_binding.as_ref() {
            Some(TextFontBinding::Bitmap {
                font,
                texture_source,
            }) => (font.clone(), texture_source.clone()),
            Some(TextFontBinding::System(binding)) => {
                return draw_system_text(command, binding, target);
            }
            None => {
                let Some(font) = self.fonts.get(&command.font).cloned() else {
                    if std::env::var_os("STELLA_TRACE_RENDER").is_some() {
                        eprintln!("missing font: {}", command.font);
                    }
                    return Ok(());
                };
                let texture_source = font.texture.clone();
                (font.into(), texture_source)
            }
        };
        let glyphs = command
            .text
            .chars()
            .filter_map(|character| font.glyph(character as u32).copied())
            .collect::<Vec<_>>();
        let [anchor_x, anchor_y] = font.native_draw_anchor(
            &command.text,
            &command.horizontal_anchor,
            &command.vertical_anchor,
        );
        let anchor_x = f64::from(anchor_x);
        let anchor_y = f64::from(anchor_y);
        // BitmapFont::draw (sub_10042B338) submits each glyph AtlasSprite at
        // the live GL_Context scale without an extra density conversion. The
        // original UI line objects already carry their authored 0.5 scale.
        let texture = self.texture(&texture_source)?.clone();
        let mut cursor = anchor_x;
        for glyph in glyphs {
            let transform = text_glyph_transform(command, cursor, anchor_y);
            draw_region(
                &texture.image,
                &SpriteRegion {
                    name: String::new(),
                    x: glyph.x,
                    y: glyph.y,
                    width: glyph.width,
                    height: glyph.height,
                    pivot_x: 0,
                    pivot_y: glyph.pivot_y,
                    atlas_rotation: 0,
                },
                transform,
                None,
                None,
                target,
                None,
                None,
                None,
            );
            cursor += f64::from(i32::from(glyph.width) + i32::from(font.tracking));
        }
        Ok(())
    }
}

#[cfg(test)]
fn draw_system_text(
    command: &TextRenderCommand,
    binding: &SystemFontRenderBinding,
    target: &mut [u32],
) -> Result<()> {
    let Some(label) = rasterize_system_label(
        binding,
        &command.text,
        &command.horizontal_anchor,
        &command.vertical_anchor,
    )?
    else {
        return Ok(());
    };
    let [label_left, label_top] = native_system_label_offset(
        command,
        binding.stroke_width,
        label.horizontal_anchor,
        label.vertical_anchor,
    );
    let width = label.image.width();
    let height = label.image.height();
    draw_region(
        &label.image,
        &SpriteRegion {
            name: String::new(),
            x: 0,
            y: 0,
            width: width as i16,
            height: height as i16,
            pivot_x: 0,
            pivot_y: 0,
            atlas_rotation: 0,
        },
        text_glyph_transform(command, label_left, label_top),
        None,
        None,
        target,
        None,
        None,
        None,
    );
    Ok(())
}
