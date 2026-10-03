use super::*;
use std::path::Path;

use super::captures::ResolvedTexture;
use stella_assets::{
    native_image::{DecodedNativeImage, ImageSurfaceLayout},
    surface_format::SurfaceFormat,
};

// A renderer can outlive a catalog replacement. Its upload cache therefore
// needs a process-unique physical generation, independent of catalog counters.
static NEXT_FILE_TEXTURE_GENERATION: std::sync::atomic::AtomicU64 =
    std::sync::atomic::AtomicU64::new(1);

/// Logical Image owners share immutable uploads only when the decoded pixels
/// and native surface layout match. Physical keys never alias an owner that
/// capture can later mutate.
#[derive(Default)]
pub(crate) struct FileImageCatalog {
    pub(crate) bindings: HashMap<String, ResolvedTexture>,
    versions: HashMap<String, Vec<String>>,
}

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
        if let Some(image) = &region.decoded_image {
            self.retain_native_image(&region.texture_source, image)?;
        }
        Ok(())
    }

    pub(crate) fn retain_native_image(
        &mut self,
        source: &str,
        image: &DecodedNativeImage,
    ) -> Result<()> {
        if self.file_images.bindings.contains_key(source) {
            return Ok(());
        }
        image.layout.pixels.gl_texture_format(true)?;
        let path = stella_assets::image_source::image_source_path(source);
        let shared = self
            .file_images
            .versions
            .get(path)
            .and_then(|versions| {
                versions.iter().find(|key| {
                    self.textures.get(*key).is_some_and(|texture| {
                        texture.width() == image.width
                            && texture.height() == image.height
                            && texture.source_layout == image.layout
                            && texture.image.as_raw() == &image.rgba
                    })
                })
            })
            .cloned();
        let physical = if let Some(shared) = shared {
            shared
        } else {
            let pixels = RgbaImage::from_raw(image.width, image.height, image.rgba.clone())
                .ok_or_else(|| anyhow!("invalid retained native image pixels"))?;
            let generation =
                NEXT_FILE_TEXTURE_GENERATION.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            let key = format!("<file-image-generation:{generation}>{path}");
            self.textures.insert(
                key.clone(),
                TextureAsset::with_native_layout(pixels, image.layout),
            );
            self.file_images
                .versions
                .entry(path.to_owned())
                .or_default()
                .push(key.clone());
            key
        };
        self.file_images.bindings.insert(
            source.to_owned(),
            ResolvedTexture {
                source: physical,
                width: image.width,
                height: image.height,
                surface_format: image.layout.pixels.for_gl_upload(true),
            },
        );
        Ok(())
    }

    pub(crate) fn texture(&mut self, name: &str) -> Result<&TextureAsset> {
        let resolved = self
            .captures
            .bindings
            .get(name)
            .or_else(|| self.file_images.bindings.get(name))
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
    let texture = match image_reader_kind(&bytes, path.extension().and_then(|value| value.to_str()))
    {
        ImageReaderKind::Pvr => pvr_reader::load(path, &bytes),
        ImageReaderKind::Bmp | ImageReaderKind::Tga | ImageReaderKind::Png => {
            raster_reader::load_native(path, &bytes)
        }
        ImageReaderKind::Webp => raster_reader::load_webp(path, &bytes),
        ImageReaderKind::Jpeg => raster_reader::load_jpeg(path, &bytes),
        ImageReaderKind::Unsupported => Err(anyhow!(
            "unsupported native image reader for {}",
            path.display()
        )),
    }?;
    texture.source_layout.pixels.gl_texture_format(true)?;
    Ok(texture)
}
