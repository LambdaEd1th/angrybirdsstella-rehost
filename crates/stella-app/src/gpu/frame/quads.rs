//! Native transformed atlas quads and explicit masked-image quad submission.

use super::{geometry::shader_uniform, *};

impl AssetCatalog {
    pub(super) fn append_gpu_native_sprite_quad(
        &mut self,
        name: &str,
        bound_region: Option<&SpriteCatalogRegion>,
        positions: [[f64; 2]; 4],
        alpha: f64,
        frame: &mut PreparedFrame,
    ) -> Result<()> {
        if alpha <= 0.0 {
            return Ok(());
        }
        let asset_name = name.split_once('#').map_or(name, |(base, _)| base);
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
                eprintln!("missing native-quad sprite: {name}");
            }
            return Ok(());
        };
        let base_texture = region.texture;
        let (texture_width, texture_height, surface_format) =
            if base_texture.starts_with("<capture:") {
                (
                    frame.resolution.width as f32,
                    frame.resolution.height as f32,
                    SurfaceFormat::A8B8G8R8,
                )
            } else {
                let texture = self.texture(&base_texture)?;
                (
                    texture.width() as f32,
                    texture.height() as f32,
                    texture.upload_surface_format(),
                )
            };
        let uv = region.sprite.native_uvs(texture_width, texture_height);
        let positions = positions.map(|point| point.map(|value| value as f32));
        let mut uniform = shader_uniform(None);
        uniform.header[0] = alpha as f32;
        frame.push_quad(
            positions,
            uv,
            positions,
            positions,
            uniform,
            base_texture,
            WHITE_TEXTURE.to_owned(),
            native_sprite_program(surface_format, alpha as f32),
        );
        Ok(())
    }

    pub(super) fn append_gpu_explicit_quad(
        &mut self,
        name: &str,
        bound_region: Option<&SpriteCatalogRegion>,
        quad: RenderQuad,
        alpha: f64,
        frame: &mut PreparedFrame,
    ) -> Result<()> {
        if alpha <= 0.0
            || !quad
                .positions
                .into_iter()
                .flatten()
                .chain(quad.uv.into_iter().flatten())
                .all(f64::is_finite)
        {
            return Ok(());
        }
        let asset_name = name.split_once('#').map_or(name, |(base, _)| base);
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
                eprintln!("missing explicit-quad sprite: {name}");
            }
            return Ok(());
        };
        let base_texture = region.texture;
        let surface_format = if base_texture.starts_with("<capture:") {
            SurfaceFormat::A8B8G8R8
        } else {
            self.texture(&base_texture)?.upload_surface_format()
        };
        let positions = quad.positions.map(|[x, y]| [x as f32, y as f32]);
        let uv = quad.uv.map(|[u, v]| [u as f32, v as f32]);
        let mut uniform = shader_uniform(None);
        uniform.header[0] = alpha as f32;
        frame.push_quad(
            positions,
            uv,
            positions,
            positions,
            uniform,
            base_texture,
            WHITE_TEXTURE.to_owned(),
            native_sprite_program(surface_format, alpha as f32),
        );
        Ok(())
    }
}
