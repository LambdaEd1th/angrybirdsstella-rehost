//! Native image-reader surface layouts recovered from Purple.
//!
//! Purple keeps the source-pixel format at reader offset `+0x458` and the
//! optional palette-entry format at `+0x45c`. Its row copier passes both to
//! the common surface converter instead of inferring them from decoded RGBA
//! pixels.

use std::io::Cursor;

use image::{ImageDecoder, codecs::webp::WebPDecoder};

use crate::{AssetError, surface_format::SurfaceFormat};

const PNG_SIGNATURE: &[u8; 8] = b"\x89PNG\r\n\x1a\n";

/// The two `img::SurfaceFormat` values carried by Purple's image reader.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ImageSurfaceLayout {
    /// Format of the source pixel/indices buffer (`+0x458`).
    pub pixels: SurfaceFormat,
    /// Format of palette entries (`+0x45c`), if the pixel buffer is indexed.
    pub palette: Option<SurfaceFormat>,
}

impl ImageSurfaceLayout {
    pub const fn direct(pixels: SurfaceFormat) -> Self {
        Self {
            pixels,
            palette: None,
        }
    }
}

/// Recover the source formats selected by PNG reader `sub_1004D62B8`.
pub fn png_surface_layout(bytes: &[u8]) -> Result<ImageSurfaceLayout, AssetError> {
    if bytes.get(..8) != Some(PNG_SIGNATURE) {
        return Err(AssetError::InvalidPng("signature mismatch"));
    }
    if bytes.len() < 29 {
        return Err(AssetError::InvalidPng("truncated IHDR"));
    }
    if u32::from_be_bytes(bytes[8..12].try_into().expect("four-byte IHDR length")) != 13
        || &bytes[12..16] != b"IHDR"
    {
        return Err(AssetError::InvalidPng("IHDR is not the first chunk"));
    }

    let width = u32::from_be_bytes(bytes[16..20].try_into().expect("four-byte PNG width"));
    let height = u32::from_be_bytes(bytes[20..24].try_into().expect("four-byte PNG height"));
    if width == 0 || height == 0 {
        return Err(AssetError::InvalidPng("zero image dimension"));
    }

    let bit_depth = bytes[24];
    let color_type = bytes[25];
    if bit_depth > 8 {
        return Err(AssetError::UnsupportedPngBitDepth(bit_depth));
    }
    if !valid_png_bit_depth(color_type, bit_depth) {
        return Err(AssetError::InvalidPng("invalid bit depth for color type"));
    }
    if bytes[26] != 0 || bytes[27] != 0 || bytes[28] > 1 {
        return Err(AssetError::InvalidPng("unsupported IHDR method"));
    }

    match color_type {
        0 => Ok(ImageSurfaceLayout::direct(SurfaceFormat::L8)),
        2 => Ok(ImageSurfaceLayout::direct(SurfaceFormat::B8G8R8)),
        // png_set_packing expands 1/2/4-bit indices to P8. Purple stores the
        // PLTE/tRNS table separately as A8R8G8B8 entries.
        3 => Ok(ImageSurfaceLayout {
            pixels: SurfaceFormat::P8,
            palette: Some(SurfaceFormat::A8R8G8B8),
        }),
        4 => Ok(ImageSurfaceLayout::direct(SurfaceFormat::A8L8)),
        6 => Ok(ImageSurfaceLayout::direct(SurfaceFormat::A8B8G8R8)),
        value => Err(AssetError::UnsupportedPngColorType(value)),
    }
}

/// Recover the RGB/RGBA choice made by WebP reader `sub_1004DB17C`.
pub fn webp_surface_layout(bytes: &[u8]) -> Result<ImageSurfaceLayout, AssetError> {
    let decoder = WebPDecoder::new(Cursor::new(bytes)).map_err(|_| AssetError::InvalidWebp)?;
    let pixels = if decoder.color_type().has_alpha() {
        SurfaceFormat::A8B8G8R8
    } else {
        SurfaceFormat::B8G8R8
    };
    Ok(ImageSurfaceLayout::direct(pixels))
}

const fn valid_png_bit_depth(color_type: u8, bit_depth: u8) -> bool {
    match color_type {
        0 | 3 => matches!(bit_depth, 1 | 2 | 4 | 8),
        2 | 4 | 6 => bit_depth == 8,
        _ => true,
    }
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use image::{DynamicImage, ImageFormat, RgbImage, RgbaImage};

    use super::*;

    fn png_ihdr(bit_depth: u8, color_type: u8) -> Vec<u8> {
        let mut bytes = PNG_SIGNATURE.to_vec();
        bytes.extend_from_slice(&13u32.to_be_bytes());
        bytes.extend_from_slice(b"IHDR");
        bytes.extend_from_slice(&1u32.to_be_bytes());
        bytes.extend_from_slice(&1u32.to_be_bytes());
        bytes.extend_from_slice(&[bit_depth, color_type, 0, 0, 0]);
        bytes.extend_from_slice(&[0; 4]);
        bytes
    }

    #[test]
    fn maps_every_png_reader_color_type() {
        assert_eq!(
            png_surface_layout(&png_ihdr(8, 0)).unwrap(),
            ImageSurfaceLayout::direct(SurfaceFormat::L8)
        );
        assert_eq!(
            png_surface_layout(&png_ihdr(8, 2)).unwrap(),
            ImageSurfaceLayout::direct(SurfaceFormat::B8G8R8)
        );
        assert_eq!(
            png_surface_layout(&png_ihdr(4, 3)).unwrap(),
            ImageSurfaceLayout {
                pixels: SurfaceFormat::P8,
                palette: Some(SurfaceFormat::A8R8G8B8),
            }
        );
        assert_eq!(
            png_surface_layout(&png_ihdr(8, 4)).unwrap(),
            ImageSurfaceLayout::direct(SurfaceFormat::A8L8)
        );
        assert_eq!(
            png_surface_layout(&png_ihdr(8, 6)).unwrap(),
            ImageSurfaceLayout::direct(SurfaceFormat::A8B8G8R8)
        );
    }

    #[test]
    fn png_reader_rejects_the_native_unsupported_16_bit_path() {
        assert!(matches!(
            png_surface_layout(&png_ihdr(16, 6)),
            Err(AssetError::UnsupportedPngBitDepth(16))
        ));
    }

    #[test]
    fn webp_feature_probe_preserves_rgb_and_rgba() {
        let mut rgb = Cursor::new(Vec::new());
        DynamicImage::ImageRgb8(RgbImage::new(2, 1))
            .write_to(&mut rgb, ImageFormat::WebP)
            .unwrap();
        assert_eq!(
            webp_surface_layout(rgb.get_ref()).unwrap(),
            ImageSurfaceLayout::direct(SurfaceFormat::B8G8R8)
        );

        let mut rgba = Cursor::new(Vec::new());
        DynamicImage::ImageRgba8(RgbaImage::new(2, 1))
            .write_to(&mut rgba, ImageFormat::WebP)
            .unwrap();
        assert_eq!(
            webp_surface_layout(rgba.get_ref()).unwrap(),
            ImageSurfaceLayout::direct(SurfaceFormat::A8B8G8R8)
        );
    }
}
