use super::*;
use std::path::Path;

pub(super) fn load(path: &Path, bytes: &[u8]) -> Result<TextureAsset> {
    let header = stella_assets::pvr::parse_header(bytes)
        .with_context(|| format!("parse header for {}", path.display()))?;
    let surface_format = SurfaceFormat::from_pvr_v2_flags(header.flags).ok_or_else(|| {
        anyhow!(
            "unsupported native surface format 0x{:02x} for {}",
            header.pixel_format(),
            path.display()
        )
    })?;
    let decoded = stella_assets::pvr::decode_rgba8(bytes)
        .with_context(|| format!("decode {}", path.display()))?;
    let image = RgbaImage::from_raw(decoded.width, decoded.height, decoded.rgba8)
        .ok_or_else(|| anyhow!("invalid decoded dimensions for {}", path.display()))?;
    Ok(TextureAsset::new(image, surface_format))
}
