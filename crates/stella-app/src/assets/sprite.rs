use super::*;

impl AssetCatalog {
    #[cfg(test)]
    #[allow(dead_code)]
    pub(crate) fn draw_sprite(
        &mut self,
        name: &str,
        transform: SpriteTransform,
        target: &mut [u32],
        depth: usize,
        options: SpriteDrawOptions<'_>,
    ) -> Result<()> {
        if depth > 16 {
            return Err(anyhow!("composite sprite recursion is too deep at {name}"));
        }
        let asset_name = name.split_once('#').map_or(name, |(base, _)| base);
        if let Some(parts) = self
            .composites
            .get(name)
            .or_else(|| self.composites.get(asset_name))
            .cloned()
        {
            let options = SpriteDrawOptions {
                draw_size: None,
                sprite_pivot: None,
                ..options
            };
            for part in parts {
                if !part.visible {
                    continue;
                }
                let child = composite_child_transform(transform, &part);
                self.draw_sprite(&part.sprite, child, target, depth + 1, options)?;
            }
            return Ok(());
        }
        let Some(region) = self
            .regions
            .get(name)
            .or_else(|| self.regions.get(asset_name))
            .cloned()
        else {
            if std::env::var_os("STELLA_TRACE_RENDER").is_some() {
                eprintln!("missing sprite: {name}");
            }
            return Ok(());
        };
        let texture_name = region.texture.clone();
        self.texture(&texture_name)?;
        let fill = options.masked_texture.and_then(|(name, scale)| {
            self.masked_textures
                .get(name)
                .cloned()
                .map(|texture| (texture, scale))
        });
        if let Some((fill_name, _)) = &fill {
            self.texture(fill_name)?;
        }
        let texture = self
            .textures
            .get(&texture_name)
            .ok_or_else(|| anyhow!("texture cache lost {texture_name}"))?;
        let fill = fill.and_then(|(fill_name, scale)| {
            self.textures
                .get(&fill_name)
                .map(|texture| (texture, scale))
        });
        draw_region(
            &texture.image,
            &region.sprite,
            transform,
            options.draw_size,
            options.sprite_pivot,
            target,
            fill.map(|(texture, scale)| (&texture.image, scale)),
            options.masked_texture_matrix,
            options.shader,
        );
        Ok(())
    }

    #[cfg(test)]
    pub(crate) fn draw_explicit_quad(
        &mut self,
        name: &str,
        bound_region: Option<&SpriteCatalogRegion>,
        quad: RenderQuad,
        alpha: f32,
        clip_rect: Option<[i32; 4]>,
        target: &mut [u32],
    ) -> Result<()> {
        let asset_name = name.split_once('#').map_or(name, |(base, _)| base);
        if let Some(region) = bound_region {
            self.retain_decoded_image(region)?;
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
            return Ok(());
        };
        let texture = self.texture(&region.texture)?.clone();
        draw_explicit_quad(&texture.image, quad, f64::from(alpha), clip_rect, target);
        Ok(())
    }
}
