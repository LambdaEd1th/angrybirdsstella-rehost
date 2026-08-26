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
        let [anchor_x, anchor_y] = font.native_draw_anchor(
            &command.text,
            &command.horizontal_anchor,
            &command.vertical_anchor,
        );
        let anchor_x = f64::from(anchor_x);
        let anchor_y = f64::from(anchor_y);
        let scale_x = command.scale_x;
        let scale_y = command.scale_y;
        let (texture_width, texture_height, surface_format) = {
            let texture = self.texture(&texture_source)?;
            (
                texture.width(),
                texture.height(),
                texture.upload_surface_format(),
            )
        };
        let mut cursor = anchor_x;
        for glyph in glyphs {
            if let Some(projection) = command.projection_3d {
                let glyph_width = f64::from(glyph.width);
                let glyph_height = f64::from(glyph.height);
                let local = [
                    [
                        cursor * scale_x,
                        (anchor_y - f64::from(glyph.pivot_y)) * scale_y,
                    ],
                    [
                        (cursor + glyph_width) * scale_x,
                        (anchor_y - f64::from(glyph.pivot_y)) * scale_y,
                    ],
                    [
                        cursor * scale_x,
                        (anchor_y - f64::from(glyph.pivot_y) + glyph_height) * scale_y,
                    ],
                    [
                        (cursor + glyph_width) * scale_x,
                        (anchor_y - f64::from(glyph.pivot_y) + glyph_height) * scale_y,
                    ],
                ];
                let projected = local.map(|[x, y]| {
                    project_text_3d(frame.resolution, command.x, command.y, projection, x, y)
                });
                if let [
                    Some(top_left),
                    Some(top_right),
                    Some(bottom_left),
                    Some(bottom_right),
                ] = projected
                {
                    let positions = [top_left, top_right, bottom_left, bottom_right]
                        .map(|point| point.map(|value| value as f32));
                    let texture_width = texture_width.max(1) as f32;
                    let texture_height = texture_height.max(1) as f32;
                    let left = f32::from(glyph.x) / texture_width;
                    let top = f32::from(glyph.y) / texture_height;
                    let right = (f32::from(glyph.x) + f32::from(glyph.width)) / texture_width;
                    let bottom = (f32::from(glyph.y) + f32::from(glyph.height)) / texture_height;
                    let mut uniform = shader_uniform(None);
                    uniform.header[0] = command.alpha.clamp(0.0, 1.0) as f32;
                    frame.push_quad(
                        positions,
                        [[left, top], [right, top], [left, bottom], [right, bottom]],
                        local.map(|point| point.map(|value| value as f32)),
                        uniform,
                        texture_source.clone(),
                        WHITE_TEXTURE.to_owned(),
                        native_sprite_program(surface_format, command.alpha.clamp(0.0, 1.0) as f32),
                    );
                }
                cursor += f64::from(i32::from(glyph.width) + i32::from(font.tracking));
                continue;
            }
            let mut transform = text_glyph_transform(command, cursor, anchor_y);
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
