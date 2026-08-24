//! `SystemFont::Impl::drawString` label-texture submission for wgpu.

use super::{super::geometry::append_gpu_region, *};

impl AssetCatalog {
    pub(super) fn append_gpu_system_text(
        &mut self,
        command: &TextRenderCommand,
        binding: &SystemFontRenderBinding,
        frame: &mut PreparedFrame,
    ) -> Result<()> {
        if command.text.is_empty() {
            return Ok(());
        }
        frame
            .retired_textures
            .extend(self.system_labels.enter_epoch(binding.label_pool_epoch));
        let hash = native_system_label_hash(binding, &command.text);
        let (label, horizontal_anchor, vertical_anchor) =
            if let Some(label) = self.system_labels.get(hash) {
                let horizontal_anchor = native_system_label_horizontal_anchor(
                    binding,
                    &command.text,
                    &command.horizontal_anchor,
                )?;
                let vertical_anchor =
                    native_system_label_vertical_anchor(binding, &command.vertical_anchor);
                (label, horizontal_anchor, vertical_anchor)
            } else {
                let Some(rasterized) = rasterize_system_label(
                    binding,
                    &command.text,
                    &command.horizontal_anchor,
                    &command.vertical_anchor,
                )?
                else {
                    return Ok(());
                };
                let horizontal_anchor = rasterized.horizontal_anchor;
                let vertical_anchor = rasterized.vertical_anchor;
                let (label, retired) =
                    self.system_labels
                        .insert(binding.label_pool_epoch, hash, rasterized.image)?;
                frame.retired_textures.extend(retired);
                (label, horizontal_anchor, vertical_anchor)
            };
        frame
            .transient_textures
            .insert(label.texture_key.clone(), label.texture.clone());
        let [label_left, label_top] = native_system_label_offset(
            command,
            binding.stroke_width,
            horizontal_anchor,
            vertical_anchor,
        );
        // LabelPool is keyed only by the recovered hash. A collision draws the
        // first cached label's actual dimensions while anchoring from the
        // current string, exactly as drawString does before the tree lookup.
        let (width, height) = (label.texture.width(), label.texture.height());

        if let Some(projection) = command.projection_3d {
            let left = label_left * command.scale_x;
            let top = label_top * command.scale_y;
            let right = (label_left + f64::from(width)) * command.scale_x;
            let bottom = (label_top + f64::from(height)) * command.scale_y;
            let projected =
                [[left, top], [right, top], [left, bottom], [right, bottom]].map(|[x, y]| {
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
                let source = [
                    [0.0, 0.0],
                    [width as f32, 0.0],
                    [0.0, height as f32],
                    [width as f32, height as f32],
                ];
                let mut uniform = shader_uniform(None);
                uniform.header[0] = command.alpha.clamp(0.0, 1.0) as f32;
                frame.push_quad(
                    positions,
                    [[0.0, 0.0], [1.0, 0.0], [0.0, 1.0], [1.0, 1.0]],
                    source,
                    source,
                    uniform,
                    label.texture_key,
                    WHITE_TEXTURE.to_owned(),
                    native_sprite_program(
                        SurfaceFormat::A8B8G8R8,
                        command.alpha.clamp(0.0, 1.0) as f32,
                    ),
                );
            }
            return Ok(());
        }

        let transform = text_glyph_transform(command, label_left, label_top);
        append_gpu_region(
            frame,
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
            SpriteTransform {
                alpha: transform.alpha.clamp(0.0, 1.0),
                ..transform
            },
            None,
            None,
            label.texture_key,
            width,
            height,
            WHITE_TEXTURE.to_owned(),
            1,
            1,
            1.0,
            0.0,
            native_sprite_program(
                SurfaceFormat::A8B8G8R8,
                command.alpha.clamp(0.0, 1.0) as f32,
            ),
            None,
            &[],
        );
        Ok(())
    }
}
