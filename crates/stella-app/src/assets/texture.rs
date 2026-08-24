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
    pub(crate) fn texture(&mut self, name: &str) -> Result<&TextureAsset> {
        if !self.textures.contains_key(name) {
            let image_path = self.root.join(name);
            let path = if image_path.is_file() {
                image_path
            } else {
                self.font_root.join(name)
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
    match path
        .extension()
        .and_then(|value| value.to_str())
        .map(str::to_ascii_lowercase)
        .as_deref()
    {
        Some("pvr") => pvr_reader::load(path, &bytes),
        Some("png") => raster_reader::load_png(path, &bytes),
        Some("webp") => raster_reader::load_webp(path, &bytes),
        _ => Err(anyhow!(
            "unsupported native image reader for {}",
            path.display()
        )),
    }
}
