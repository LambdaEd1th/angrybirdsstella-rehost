//! Bitmap-font and projected 3D-text expansion into native-order wgpu quads.

use super::{geometry::append_gpu_region, *};

mod system;

impl AssetCatalog {
    pub(super) fn append_gpu_text(
        &mut self,
        command: &TextRenderCommand,
        frame: &mut PreparedFrame,
    ) -> Result<()> {
        let (font, texture_source) = match command.font_binding.as_ref() {
            Some(TextFontBinding::Bitmap {
                font,
                texture_source,
            }) => (font.clone(), texture_source.clone()),
            Some(TextFontBinding::System(binding)) => {
                return self.append_gpu_system_text(command, binding, frame);
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
        let raw_vertices = command.projection_3d.is_some_and(|p| p.custom_model);
        frame.current_raw_vertices = raw_vertices;
        frame.current_vertex_depth = if raw_vertices { 0.0 } else { 0.001 };
        let [anchor_x, anchor_y] = font.native_draw_anchor(
            &command.text,
            &command.horizontal_anchor,
            &command.vertical_anchor,
        );
        let anchor_x = f64::from(anchor_x);
        let anchor_y = f64::from(anchor_y);
        let texture = self.resolve_gpu_texture(&texture_source)?;
        let (texture_source, texture_width, texture_height, surface_format) = (
            texture.source,
            texture.width,
            texture.height,
            texture.surface_format,
        );
        let mut cursor = anchor_x;
        for glyph in glyphs {
            let mut transform = if raw_vertices {
                let [x, y] = command
                    .native_system_origin
                    .unwrap_or([command.x as f32, command.y as f32]);
                SpriteTransform::from_scale_rotation(
                    x + cursor as f32,
                    y + anchor_y as f32,
                    1.0,
                    1.0,
                    0.0,
                    command.alpha as f32,
                )
            } else {
                text_glyph_transform(command, cursor, anchor_y)
            };
            transform.alpha = transform.alpha.clamp(0.0, 1.0);
            append_gpu_region(
                frame,
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
                texture_source.clone(),
                texture_width,
                texture_height,
                WHITE_TEXTURE.to_owned(),
                1,
                1,
                1.0,
                None,
                0.0,
                native_sprite_program(surface_format, transform.alpha),
                None,
            );
            cursor += f64::from(i32::from(glyph.width) + i32::from(font.tracking));
        }
        Ok(())
    }
}
