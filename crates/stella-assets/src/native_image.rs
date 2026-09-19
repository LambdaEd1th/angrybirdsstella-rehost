//! Native image-reader surface layouts recovered from Purple.
//!
//! Purple keeps the source-pixel format at reader offset `+0x458` and the
//! optional palette-entry format at `+0x45c`. Its row copier passes both to
//! the common surface converter instead of inferring them from decoded RGBA
//! pixels.

use std::io::Cursor;

use image::{
    ImageDecoder,
    codecs::{jpeg::JpegDecoder, webp::WebPDecoder},
};

use crate::{AssetError, surface_format::SurfaceFormat};

const PNG_SIGNATURE: &[u8; 8] = b"\x89PNG\r\n\x1a\n";

/// Logical image extent retained by GL_Image, without decoding its pixels.
/// Capture validation compares this extent, not a sprite's atlas subrectangle.
pub fn image_dimensions(bytes: &[u8]) -> Result<[u32; 2], AssetError> {
    if bytes.starts_with(&[0xff, 0xd8, 0xff]) {
        let decoder = JpegDecoder::new(Cursor::new(bytes))
            .map_err(|_| AssetError::InvalidJpeg("header decoding failed"))?;
        let (width, height) = decoder.dimensions();
        return Ok([width, height]);
    }
    if bytes.get(..8) == Some(PNG_SIGNATURE) {
        png_surface_layout(bytes)?;
        return Ok([
            u32::from_be_bytes(bytes[16..20].try_into().expect("PNG width")),
            u32::from_be_bytes(bytes[20..24].try_into().expect("PNG height")),
        ]);
    }
    if bytes.get(..4) == Some(b"RIFF") && bytes.get(8..12) == Some(b"WEBP") {
        let decoder = WebPDecoder::new(Cursor::new(bytes)).map_err(|_| AssetError::InvalidWebp)?;
        let (width, height) = decoder.dimensions();
        return Ok([width, height]);
    }
    let header = crate::pvr::parse_header(bytes)?;
    Ok([header.width, header.height])
}

/// Fully decoded immutable native Image pixels, retained independently of the
/// cache file that supplied them. Deferred draws can outlive file replacement.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DecodedNativeImage {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
    pub layout: ImageSurfaceLayout,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImageReaderKind {
    Png,
    Webp,
    Jpeg,
    Pvr,
    Unsupported,
}

/// 1004FB774 probes the stream before consulting its name at1004FB534.
/// Other recognized native formats remain explicitly unsupported here.
pub fn image_reader_kind(bytes: &[u8], extension: Option<&str>) -> ImageReaderKind {
    if bytes.starts_with(&[0xff, 0xd8, 0xff]) {
        return ImageReaderKind::Jpeg;
    }
    if bytes.starts_with(b"\x89PNG") {
        return ImageReaderKind::Png;
    }
    if bytes.starts_with(b"RIFF") && bytes.get(8..12) == Some(b"WEBP") {
        return ImageReaderKind::Webp;
    }
    if bytes.starts_with(b"PVR\x02") || bytes.starts_with(b"PVR\x03") {
        return ImageReaderKind::Pvr;
    }
    if bytes.starts_with(b"BM")
        || bytes.starts_with(b"DDS ")
        || bytes.starts_with(b"8BPS")
        || bytes.starts_with(b"GIF8")
        || bytes.starts_with(b"II*\0")
        || bytes.starts_with(b"MM\0*")
    {
        return ImageReaderKind::Unsupported;
    }
    match extension.map(str::to_ascii_lowercase).as_deref() {
        Some("png") => ImageReaderKind::Png,
        Some("webp") => ImageReaderKind::Webp,
        Some("jpeg" | "jpg" | "jpe") => ImageReaderKind::Jpeg,
        Some("pvr") => ImageReaderKind::Pvr,
        _ => ImageReaderKind::Unsupported,
    }
}

pub fn decode_native_image(
    bytes: &[u8],
    extension: Option<&str>,
) -> Result<DecodedNativeImage, AssetError> {
    let (format, layout) = match image_reader_kind(bytes, extension) {
        ImageReaderKind::Png => (image::ImageFormat::Png, png_surface_layout(bytes)?),
        ImageReaderKind::Webp => (image::ImageFormat::WebP, webp_surface_layout(bytes)?),
        ImageReaderKind::Jpeg => (image::ImageFormat::Jpeg, jpeg_surface_layout(bytes)?),
        ImageReaderKind::Pvr => {
            let decoded = crate::pvr::decode_rgba8(bytes)?;
            let header = crate::pvr::parse_header(bytes)?;
            return Ok(DecodedNativeImage {
                width: decoded.width,
                height: decoded.height,
                rgba: decoded.rgba8,
                layout: ImageSurfaceLayout::direct(
                    SurfaceFormat::from_pvr_v2_flags(header.flags)
                        .ok_or(AssetError::UnsupportedPvr((header.flags & 0xff) as u8))?,
                ),
            });
        }
        ImageReaderKind::Unsupported => return Err(AssetError::UnsupportedImageReader),
    };
    let decoded = image::load_from_memory_with_format(bytes, format)
        .map_err(|_| match format {
            image::ImageFormat::Png => AssetError::InvalidPng("pixel decoding failed"),
            image::ImageFormat::Jpeg => AssetError::InvalidJpeg("pixel decoding failed"),
            _ => AssetError::InvalidWebp,
        })?
        .to_rgba8();
    Ok(DecodedNativeImage {
        width: decoded.width(),
        height: decoded.height(),
        rgba: decoded.into_raw(),
        layout,
    })
}

/// JPEG1004D58E0 accepts grayscale/RGB output only: L8 or B8G8R8.
/// Four-component CMYK/YCCK input retains a four-component libjpeg output and
/// is rejected by1004D5A60; never silently convert it to a successful avatar.
pub fn jpeg_surface_layout(bytes: &[u8]) -> Result<ImageSurfaceLayout, AssetError> {
    if !bytes.starts_with(&[0xff, 0xd8]) {
        return Err(AssetError::InvalidJpeg("signature mismatch"));
    }
    let mut offset = 2;
    while offset < bytes.len() {
        if bytes[offset] != 0xff {
            return Err(AssetError::InvalidJpeg("invalid marker"));
        }
        while bytes.get(offset) == Some(&0xff) {
            offset += 1;
        }
        let marker = *bytes
            .get(offset)
            .ok_or(AssetError::InvalidJpeg("truncated marker"))?;
        offset += 1;
        if matches!(marker, 0xd9 | 0xda) {
            break;
        }
        if matches!(marker, 0x01 | 0xd0..=0xd8) {
            continue;
        }
        let len = bytes
            .get(offset..offset + 2)
            .ok_or(AssetError::InvalidJpeg("truncated segment"))?;
        let len = u16::from_be_bytes([len[0], len[1]]) as usize;
        if len < 2 || offset + len > bytes.len() {
            return Err(AssetError::InvalidJpeg("truncated segment"));
        }
        if matches!(marker,0xc0..=0xc3|0xc5..=0xc7|0xc9..=0xcb|0xcd..=0xcf) {
            let components = *bytes
                .get(offset + 7)
                .filter(|_| len >= 8)
                .ok_or(AssetError::InvalidJpeg("truncated frame"))?;
            return match components {
                1 => Ok(ImageSurfaceLayout::direct(SurfaceFormat::L8)),
                3 => Ok(ImageSurfaceLayout::direct(SurfaceFormat::B8G8R8)),
                _ => Err(AssetError::InvalidJpeg("unsupported JPEG color space")),
            };
        }
        offset += len;
    }
    Err(AssetError::InvalidJpeg("frame header missing"))
}

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

    #[test]
    fn native_avatar_image_decodes_magic_before_filename_and_requires_complete_pixels() {
        let input = RgbaImage::from_pixel(17, 13, image::Rgba([19, 61, 207, 173]));
        let mut bytes = Cursor::new(Vec::new());
        DynamicImage::ImageRgba8(input.clone())
            .write_to(&mut bytes, ImageFormat::Png)
            .unwrap();
        let decoded = decode_native_image(bytes.get_ref(), Some("jpeg")).unwrap();
        assert_eq!((decoded.width, decoded.height), (17, 13));
        assert_eq!(decoded.rgba, input.into_raw());
        assert_eq!(decoded.layout.pixels, SurfaceFormat::A8B8G8R8);
        assert!(decode_native_image(&bytes.get_ref()[..33], None).is_err());
        assert_eq!(
            image_reader_kind(b"GIF89a", Some("png")),
            ImageReaderKind::Unsupported
        );
    }

    #[test]
    fn native_avatar_jpeg_decodes_opaque_names_and_retains_rgb_or_grayscale_layout() {
        for grayscale in [false, true] {
            let source = if grayscale {
                DynamicImage::ImageLuma8(image::GrayImage::from_pixel(17, 13, image::Luma([127])))
            } else {
                DynamicImage::ImageRgb8(RgbImage::from_pixel(17, 13, image::Rgb([120, 60, 30])))
            };
            let mut bytes = Vec::new();
            image::codecs::jpeg::JpegEncoder::new_with_quality(&mut bytes, 100)
                .encode(source.as_bytes(), 17, 13, source.color().into())
                .unwrap();
            assert_eq!(image_dimensions(&bytes).unwrap(), [17, 13]);
            let decoded = decode_native_image(&bytes, None).unwrap();
            assert_eq!(
                (decoded.width, decoded.height, decoded.rgba.len()),
                (17, 13, 17 * 13 * 4)
            );
            assert_eq!(
                decoded.layout.pixels,
                if grayscale {
                    SurfaceFormat::L8
                } else {
                    SurfaceFormat::B8G8R8
                }
            );
            let expected = if grayscale {
                [127, 127, 127, 255]
            } else {
                [120, 60, 30, 255]
            };
            assert!(
                decoded
                    .rgba
                    .as_chunks::<4>()
                    .0
                    .iter()
                    .all(|pixel| pixel.iter().zip(expected).all(|(a, b)| a.abs_diff(b) <= 2))
            );
        }
        let cmyk = [0xff, 0xd8, 0xff, 0xc0, 0, 8, 8, 0, 1, 0, 1, 4];
        assert!(matches!(
            jpeg_surface_layout(&cmyk),
            Err(AssetError::InvalidJpeg("unsupported JPEG color space"))
        ));
        assert!(decode_native_image(&cmyk, Some("png")).is_err());
    }

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
    fn image_extent_probe_uses_native_png_webp_and_pvr_dimensions() {
        let mut png = Cursor::new(Vec::new());
        DynamicImage::ImageRgba8(RgbaImage::new(3, 5))
            .write_to(&mut png, ImageFormat::Png)
            .unwrap();
        assert_eq!(image_dimensions(png.get_ref()).unwrap(), [3, 5]);

        let mut webp = Cursor::new(Vec::new());
        DynamicImage::ImageRgb8(RgbImage::new(7, 2))
            .write_to(&mut webp, ImageFormat::WebP)
            .unwrap();
        assert_eq!(image_dimensions(webp.get_ref()).unwrap(), [7, 2]);

        let mut pvr = [
            52,
            2,
            3,
            0,
            0x12,
            24,
            32,
            0,
            0,
            0,
            0,
            u32::from_le_bytes(*b"PVR!"),
            1,
        ]
        .into_iter()
        .flat_map(u32::to_le_bytes)
        .collect::<Vec<_>>();
        pvr.extend_from_slice(&[0; 24]);
        assert_eq!(image_dimensions(&pvr).unwrap(), [3, 2]);
        assert!(image_dimensions(&pvr[..52]).is_err());
        assert!(image_dimensions(b"not an image").is_err());
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
