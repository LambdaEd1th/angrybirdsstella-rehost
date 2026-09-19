use super::*;
use std::path::Path;

pub(super) fn load_jpeg(path: &Path, bytes: &[u8]) -> Result<TextureAsset> {
    let layout = stella_assets::native_image::jpeg_surface_layout(bytes)
        .with_context(|| format!("probe JPEG {}", path.display()))?;
    decode(path, bytes, image::ImageFormat::Jpeg, layout)
}

pub(super) fn load_png(path: &Path, bytes: &[u8]) -> Result<TextureAsset> {
    let layout = stella_assets::native_image::png_surface_layout(bytes)
        .with_context(|| format!("probe native PNG layout for {}", path.display()))?;
    decode(path, bytes, image::ImageFormat::Png, layout)
}

pub(super) fn load_webp(path: &Path, bytes: &[u8]) -> Result<TextureAsset> {
    let layout = stella_assets::native_image::webp_surface_layout(bytes)
        .with_context(|| format!("probe native WebP layout for {}", path.display()))?;
    decode(path, bytes, image::ImageFormat::WebP, layout)
}

fn decode(
    path: &Path,
    bytes: &[u8],
    format: image::ImageFormat,
    layout: ImageSurfaceLayout,
) -> Result<TextureAsset> {
    let decoded = image::load_from_memory_with_format(bytes, format)
        .with_context(|| format!("decode {}", path.display()))?;
    Ok(TextureAsset::with_native_layout(decoded.to_rgba8(), layout))
}
