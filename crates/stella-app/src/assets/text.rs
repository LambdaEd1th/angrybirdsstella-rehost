use super::*;

impl AssetCatalog {
    #[cfg(test)]
    #[allow(dead_code)]
    pub(crate) fn draw_text(
        &mut self,
        command: &TextRenderCommand,
        target: &mut [u32],
    ) -> Result<()> {
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
                (font, texture_source)
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
        let scale_x = command.scale_x;
        let scale_y = command.scale_y;
        let texture = self.texture(&texture_source)?.clone();
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
                let positions = local.map(|[x, y]| {
                    project_text_3d(
                        GameResolution::default(),
                        command.x,
                        command.y,
                        projection,
                        x,
                        y,
                    )
                });
                if let [
                    Some(top_left),
                    Some(top_right),
                    Some(bottom_left),
                    Some(bottom_right),
                ] = positions
                {
                    let texture_width = f64::from(texture.width());
                    let texture_height = f64::from(texture.height());
                    draw_explicit_quad(
                        &texture.image,
                        RenderQuad {
                            positions: [top_left, top_right, bottom_left, bottom_right],
                            uv: [
                                [
                                    f64::from(glyph.x) / texture_width,
                                    f64::from(glyph.y) / texture_height,
                                ],
                                [
                                    (f64::from(glyph.x) + f64::from(glyph.width)) / texture_width,
                                    f64::from(glyph.y) / texture_height,
                                ],
                                [
                                    f64::from(glyph.x) / texture_width,
                                    (f64::from(glyph.y) + f64::from(glyph.height)) / texture_height,
                                ],
                                [
                                    (f64::from(glyph.x) + f64::from(glyph.width)) / texture_width,
                                    (f64::from(glyph.y) + f64::from(glyph.height)) / texture_height,
                                ],
                            ],
                        },
                        command.alpha,
                        command.clip_rect,
                        target,
                    );
                }
                cursor += f64::from(i32::from(glyph.width) + i32::from(font.tracking));
                continue;
            }
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
                &[],
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
    if let Some(projection) = command.projection_3d {
        let left = label_left * command.scale_x;
        let top = label_top * command.scale_y;
        let right = (label_left + f64::from(width)) * command.scale_x;
        let bottom = (label_top + f64::from(height)) * command.scale_y;
        let projected =
            [[left, top], [right, top], [left, bottom], [right, bottom]].map(|[x, y]| {
                project_text_3d(
                    GameResolution::default(),
                    command.x,
                    command.y,
                    projection,
                    x,
                    y,
                )
            });
        if let [
            Some(top_left),
            Some(top_right),
            Some(bottom_left),
            Some(bottom_right),
        ] = projected
        {
            draw_explicit_quad(
                &label.image,
                RenderQuad {
                    positions: [top_left, top_right, bottom_left, bottom_right],
                    uv: [[0.0, 0.0], [1.0, 0.0], [0.0, 1.0], [1.0, 1.0]],
                },
                command.alpha,
                command.clip_rect,
                target,
            );
        }
        return Ok(());
    }
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
        &[],
    );
    Ok(())
}
