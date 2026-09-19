//! Native Image identity stays stable while framebuffer captures replace its
//! pixels. Physical generations retain the pre-capture image for earlier draws
//! in the same immediate stream; sprite geometry never belongs to this map.

use super::*;
use stella_assets::surface_format::SurfaceFormat;

#[derive(Clone, Debug)]
pub(crate) struct ResolvedTexture {
    pub(crate) source: String,
    pub(crate) width: u32,
    pub(crate) height: u32,
    pub(crate) surface_format: SurfaceFormat,
}

#[derive(Default)]
pub(crate) struct CapturedTextureCatalog {
    pub(crate) bindings: HashMap<String, ResolvedTexture>,
    next_generation: u64,
}

impl AssetCatalog {
    pub(crate) fn resolve_gpu_texture(&mut self, logical_source: &str) -> Result<ResolvedTexture> {
        if let Some(texture) = self.captures.bindings.get(logical_source) {
            return Ok(texture.clone());
        }
        let source = if self.textures.contains_key(logical_source) {
            logical_source.to_owned()
        } else {
            stella_assets::image_source::image_source_path(logical_source).to_owned()
        };
        let texture = self.texture(logical_source)?;
        Ok(ResolvedTexture {
            // Separate native Image owners may share immutable upload bytes.
            // Capture is copy-on-write: only its logical owner is rebound to
            // a new physical generation, never the original file cache.
            source,
            width: texture.width(),
            height: texture.height(),
            surface_format: texture.upload_surface_format(),
        })
    }

    pub(crate) fn prepare_capture_texture(
        &mut self,
        logical_source: &str,
        resolution: GameResolution,
        temporary: bool,
    ) -> Result<(ResolvedTexture, Option<String>)> {
        let previous = self.captures.bindings.get(logical_source).cloned();
        let original = match previous.as_ref() {
            Some(texture) => texture.clone(),
            None if logical_source.starts_with("<capture:") => ResolvedTexture {
                source: logical_source.to_owned(),
                width: resolution.width,
                height: resolution.height,
                // 0x10059A1D8 selects SurfaceFormat 2 for a new image.
                surface_format: SurfaceFormat::B8G8R8,
            },
            None => self.resolve_gpu_texture(logical_source)?,
        };
        if original.width != resolution.width || original.height != resolution.height {
            return Err(anyhow!(
                "Wrong size capture target image: {} is {}x{}, framebuffer is {}x{}",
                logical_source,
                original.width,
                original.height,
                resolution.width,
                resolution.height
            ));
        }
        self.captures.next_generation += 1;
        let texture = ResolvedTexture {
            source: format!(
                "<capture-generation:{}:{}>",
                self.captures.next_generation, logical_source
            ),
            // glCopyTexImage2D(GL_RGB) changes pixels, not Image's stored
            // dimensions or surface-format field. Existing RGBA masks still
            // choose their original shader family, now sampling alpha one.
            ..original
        };
        if !temporary {
            self.captures
                .bindings
                .insert(logical_source.to_owned(), texture.clone());
        }
        Ok((texture, previous.map(|texture| texture.source)))
    }
}
