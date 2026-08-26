//! Composite/atlas sprite traversal and DrawablePolygon/Dirt mesh dispatch.

use super::{
    geometry::{append_gpu_dirt_triangles, append_gpu_region},
    *,
};

impl AssetCatalog {
    pub(super) fn append_gpu_dirt(
        &mut self,
        dirt: &DirtRenderCommand,
        transform: SpriteTransform,
        frame: &mut PreparedFrame,
    ) -> Result<()> {
        let MaskedTextureBinding::Source(background_texture) = &dirt.background_texture_binding
        else {
            return Ok(());
        };
        let MaskedTextureBinding::Source(foreground_texture) = &dirt.foreground_texture_binding
        else {
            return Ok(());
        };
        for triangles in &dirt.background_triangles {
            append_gpu_dirt_triangles(frame, triangles, transform, background_texture.clone());
        }
        for triangles in &dirt.foreground_triangles {
            append_gpu_dirt_triangles(frame, triangles, transform, foreground_texture.clone());
        }
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn append_gpu_sprite(
        &mut self,
        name: &str,
        bound_region: Option<&SpriteCatalogRegion>,
        bound_composite: Option<&[BoundCompositePart]>,
        transform: SpriteTransform,
        depth: usize,
        draw_size: Option<[f64; 2]>,
        sprite_pivot: Option<[f64; 2]>,
        masked_texture_matrix: Option<[f64; 6]>,
        masked_texture: Option<(&str, f64, Option<&MaskedTextureBinding>)>,
        shader: Option<&SpriteShader>,
        frame: &mut PreparedFrame,
    ) -> Result<()> {
        if depth > 16 {
            return Err(anyhow!("composite sprite recursion is too deep at {name}"));
        }
        if transform.alpha <= 0.0 {
            return Ok(());
        }
        let asset_name = name.split_once('#').map_or(name, |(base, _)| base);
        if let Some(parts) = bound_composite {
            for bound in parts {
                if !bound.part.visible {
                    continue;
                }
                self.append_gpu_sprite(
                    &bound.part.sprite,
                    Some(&bound.region),
                    None,
                    composite_child_transform(transform, &bound.part),
                    depth + 1,
                    None,
                    None,
                    masked_texture_matrix,
                    masked_texture,
                    shader,
                    frame,
                )?;
            }
            return Ok(());
        }
        if let Some(parts) = bound_region
            .is_none()
            .then(|| {
                self.composites
                    .get(name)
                    .or_else(|| self.composites.get(asset_name))
                    .cloned()
            })
            .flatten()
        {
            for part in parts {
                if !part.visible {
                    continue;
                }
                self.append_gpu_sprite(
                    &part.sprite,
                    None,
                    None,
                    composite_child_transform(transform, &part),
                    depth + 1,
                    None,
                    None,
                    masked_texture_matrix,
                    masked_texture,
                    shader,
                    frame,
                )?;
            }
            return Ok(());
        }
        let region = bound_region
            .map(|region| AtlasRegion {
                texture: region.texture_source.clone(),
                sprite: region.sprite.clone(),
            })
            .or_else(|| {
                self.regions
                    .get(name)
                    .or_else(|| self.regions.get(asset_name))
                    .cloned()
            });
        let Some(region) = region else {
            if std::env::var_os("STELLA_TRACE_RENDER").is_some() {
                eprintln!("missing sprite: {name}");
            }
            return Ok(());
        };
        let base_texture = region.texture.clone();
        let (base_width, base_height, surface_format) = if base_texture.starts_with("<capture:") {
            (
                frame.resolution.width,
                frame.resolution.height,
                SurfaceFormat::A8B8G8R8,
            )
        } else {
            let texture = self.texture(&base_texture)?;
            (
                texture.width(),
                texture.height(),
                texture.upload_surface_format(),
            )
        };
        if matches!(
            masked_texture,
            Some((_, _, Some(MaskedTextureBinding::Missing)))
        ) {
            return Ok(());
        }
        let fill = masked_texture.and_then(|(name, scale, binding)| {
            let texture = match binding {
                Some(MaskedTextureBinding::Source(source)) => Some(source.clone()),
                Some(MaskedTextureBinding::Missing) => None,
                None => self.masked_textures.get(name).cloned(),
            }?;
            Some((texture, scale))
        });
        let (fill_texture, fill_width, fill_height, texture_scale, source_mode, blend) =
            if let Some((fill_texture, texture_scale)) = fill {
                let (width, height) = {
                    let texture = self.texture(&fill_texture)?;
                    (texture.width(), texture.height())
                };
                (
                    fill_texture,
                    width,
                    height,
                    texture_scale,
                    1.0,
                    NativeProgram::SpriteAlphaMasked,
                )
            } else {
                let program = shader.map_or_else(
                    || native_sprite_program(surface_format, transform.alpha),
                    |_| NativeProgram::SpriteAlpha,
                );
                (WHITE_TEXTURE.to_owned(), 1, 1, 1.0, 0.0, program)
            };
        append_gpu_region(
            frame,
            &region.sprite,
            transform,
            draw_size,
            sprite_pivot,
            base_texture,
            base_width,
            base_height,
            fill_texture,
            fill_width,
            fill_height,
            texture_scale,
            masked_texture_matrix,
            source_mode,
            blend,
            shader,
        );
        Ok(())
    }
}
