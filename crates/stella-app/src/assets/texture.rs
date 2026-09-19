use super::*;
use std::path::Path;

use stella_assets::{native_image::ImageSurfaceLayout, surface_format::SurfaceFormat};

mod pvr_reader;
mod raster_reader;

/// Decoded upload pixels plus Purple's reader and GL-texture surface identities.
#[derive(Debug, Clone)]
pub(crate) struct TextureAsset {
    pub(crate) image: RgbaImage,
    pub(crate) source_layout: ImageSurfaceLayout,
}

impl TextureAsset {
    pub(crate) fn new(image: RgbaImage, surface_format: SurfaceFormat) -> Self {
        Self::with_native_layout(image, ImageSurfaceLayout::direct(surface_format))
    }

    pub(crate) fn with_native_layout(image: RgbaImage, native_layout: ImageSurfaceLayout) -> Self {
        Self {
            image,
            source_layout: native_layout,
        }
    }

    /// Format returned by Purple's GL texture vtable slot `+0x38`, after the
    /// context has normalized the source reader's format.
    pub(crate) const fn upload_surface_format(&self) -> SurfaceFormat {
        self.source_layout.pixels.for_gl_upload(true)
    }

    pub(crate) fn width(&self) -> u32 {
        self.image.width()
    }

    pub(crate) fn height(&self) -> u32 {
        self.image.height()
    }
}

impl AssetCatalog {
    pub(crate) fn retain_decoded_image(&mut self, region: &SpriteCatalogRegion) -> Result<()> {
        if let Some(image) = &region.decoded_image
            && !self.textures.contains_key(&region.texture_source)
        {
            let pixels = RgbaImage::from_raw(image.width, image.height, image.rgba.clone())
                .ok_or_else(|| anyhow!("invalid retained native image pixels"))?;
            self.textures.insert(
                region.texture_source.clone(),
                TextureAsset::with_native_layout(pixels, image.layout),
            );
        }
        Ok(())
    }
    pub(crate) fn texture(&mut self, name: &str) -> Result<&TextureAsset> {
        let resolved = self
            .captures
            .bindings
            .get(name)
            .map(|image| image.source.clone());
        // Native Images have independent write identities. Their initial
        // immutable file pixels can still share one upload until capture
        // installs a private physical generation for that logical owner.
        let name = resolved.as_deref().unwrap_or_else(|| {
            if self.textures.contains_key(name) {
                name
            } else {
                stella_assets::image_source::image_source_path(name)
            }
        });
        if !self.textures.contains_key(name) {
            // Resolve only immutable file input here. Mutable image identity
            // lives in captures.bindings; its GPU generations are prepared
            // separately and must never overwrite this original file entry.
            let file_source = stella_assets::image_source::image_source_path(name);
            let image_path = self.root.join(file_source);
            let path = if image_path.is_file() {
                image_path
            } else {
                self.font_root.join(file_source)
            };
            let texture = load_texture(&path)?;
            self.textures.insert(name.to_owned(), texture);
        }
        self.textures
            .get(name)
            .ok_or_else(|| anyhow!("texture cache lost {name}"))
    }
}

fn load_texture(path: &Path) -> Result<TextureAsset> {
    let bytes = fs::read(path).with_context(|| format!("read {}", path.display()))?;
    use stella_assets::native_image::{ImageReaderKind, image_reader_kind};
    match image_reader_kind(&bytes, path.extension().and_then(|value| value.to_str())) {
        ImageReaderKind::Pvr => pvr_reader::load(path, &bytes),
        ImageReaderKind::Png => raster_reader::load_png(path, &bytes),
        ImageReaderKind::Webp => raster_reader::load_webp(path, &bytes),
        ImageReaderKind::Jpeg => raster_reader::load_jpeg(path, &bytes),
        ImageReaderKind::Unsupported => Err(anyhow!(
            "unsupported native image reader for {}",
            path.display()
        )),
    }
}
