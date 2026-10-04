//! Native Image identity stays stable while framebuffer captures replace its
//! pixels. Physical generations retain the pre-capture image for earlier draws
//! in the same immediate stream; sprite geometry never belongs to this map.

use super::*;
use stella_assets::surface_format::SurfaceFormat;

static NEXT_CAPTURE_GENERATION: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);

#[derive(Clone, Debug)]
pub(crate) struct ResolvedTexture {
    pub(crate) source: String,
    pub(crate) width: u32,
    pub(crate) height: u32,
    pub(crate) surface_format: SurfaceFormat,
    pub(crate) lease: Option<Arc<()>>,
}

#[derive(Default)]
pub(crate) struct CapturedTextureCatalog {
    pub(crate) bindings: HashMap<String, ResolvedTexture>,
}

impl AssetCatalog {
    pub(crate) fn resolve_gpu_texture(&mut self, logical_source: &str) -> Result<ResolvedTexture> {
        if let Some(texture) = self
            .captures
            .bindings
            .get(logical_source)
            .or_else(|| self.file_images.bindings.get(logical_source))
        {
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
            lease: None,
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
                lease: None,
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
        let generation = NEXT_CAPTURE_GENERATION.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let lease = (!temporary).then(|| Arc::new(()));
        let texture = ResolvedTexture {
            source: format!("<capture-generation:{}:{}>", generation, logical_source),
            lease,
            // glCopyTexImage2D(GL_RGB) changes pixels, not Image's stored
            // dimensions or surface-format field. Existing RGBA masks still
            // choose their original shader family, now sampling alpha one.
            ..original
        };
        if !temporary {
            if let Some(lease) = &texture.lease {
                self.file_images
                    .lifetimes
                    .insert(texture.source.clone(), Arc::downgrade(lease));
            }
            self.captures
                .bindings
                .insert(logical_source.to_owned(), texture.clone());
            // The Image now owns its captured pixels. Earlier frames retain
            // the original immutable upload through their physical lease.
            self.file_images.bindings.remove(logical_source);
        }
        Ok((texture, previous.map(|texture| texture.source)))
    }
}
