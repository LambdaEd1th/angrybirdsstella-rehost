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

mod bmp;
mod png;
mod tga;

const PNG_SIGNATURE: &[u8; 8] = b"\x89PNG\r\n\x1a\n";

/// Logical image extent retained by GL_Image. PNG probes validate the loaded
/// rows, matching its native reader constructor.
/// Capture validation compares this extent, not a sprite's atlas subrectangle.
pub fn image_dimensions(bytes: &[u8]) -> Result<[u32; 2], AssetError> {
    image_dimensions_with_extension(bytes, None)
}

/// TGA has no reliable signature, so callers with a source name may supply it.
pub fn image_dimensions_with_extension(
    bytes: &[u8],
    extension: Option<&str>,
) -> Result<[u32; 2], AssetError> {
    if bytes.starts_with(&[0xff, 0xd8, 0xff]) {
        let decoder = JpegDecoder::new(Cursor::new(bytes))
            .map_err(|_| AssetError::InvalidJpeg("header decoding failed"))?;
        let (width, height) = decoder.dimensions();
        return Ok([width, height]);
    }
    if bytes.get(..8) == Some(PNG_SIGNATURE) {
        let (width, height, _) = png::probe(bytes)?;
        return Ok([width, height]);
    }
    if bytes.get(..4) == Some(b"RIFF") && bytes.get(8..12) == Some(b"WEBP") {
        let decoder = WebPDecoder::new(Cursor::new(bytes)).map_err(|_| AssetError::InvalidWebp)?;
        let (width, height) = decoder.dimensions();
        return Ok([width, height]);
    }
    if bytes.starts_with(b"BM") {
        let (width, height, _) = bmp_header(bytes)?;
        return Ok([width, height]);
    }
    if extension.is_some_and(|value| value.eq_ignore_ascii_case("tga")) {
        let (width, height, _) = tga_header(bytes)?;
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
    Bmp,
    Tga,
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
    if bytes.len() >= 52 && bytes[..4] == 52u32.to_le_bytes() && bytes[44..48] == *b"PVR!" {
        return ImageReaderKind::Pvr;
    }
    if bytes.starts_with(b"BM") {
        return ImageReaderKind::Bmp;
    }
    if bytes.starts_with(b"DDS ")
        || bytes.starts_with(b"8BPS")
        || bytes.starts_with(b"GIF8")
        || bytes.starts_with(b"II*\0")
        || bytes.starts_with(b"MM\0*")
    {
        return ImageReaderKind::Unsupported;
    }
    match extension.map(str::to_ascii_lowercase).as_deref() {
        Some("bmp") => ImageReaderKind::Bmp,
        Some("tga") => ImageReaderKind::Tga,
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
    decode(bytes, extension, false)
}

/// Decode an Image after the GL texture format admission used by 100597B20.
/// Some readable source formats fail texture construction before row decoding.
pub fn decode_native_texture(
    bytes: &[u8],
    extension: Option<&str>,
) -> Result<DecodedNativeImage, AssetError> {
    decode(bytes, extension, true)
}

fn decode(
    bytes: &[u8],
    extension: Option<&str>,
    texture: bool,
) -> Result<DecodedNativeImage, AssetError> {
    let (format, layout) = match image_reader_kind(bytes, extension) {
        ImageReaderKind::Bmp => {
            let header = bmp::Header::parse(bytes)?;
            if texture {
                header.layout.pixels.gl_texture_format(true)?;
            }
            return bmp::decode(bytes, header);
        }
        ImageReaderKind::Tga => {
            let (width, height, layout) = tga_header(bytes)?;
            if texture {
                layout.pixels.gl_texture_format(true)?;
            }
            return tga::decode(bytes, width, height, layout);
        }
        ImageReaderKind::Png => return png::decode(bytes, texture),
        ImageReaderKind::Webp => (image::ImageFormat::WebP, webp_surface_layout(bytes)?),
        ImageReaderKind::Jpeg => (image::ImageFormat::Jpeg, jpeg_surface_layout(bytes)?),
        ImageReaderKind::Pvr => {
            let header = crate::pvr::parse_header(bytes)?;
            let layout = ImageSurfaceLayout::direct(
                SurfaceFormat::from_pvr_v2_flags(header.flags)
                    .ok_or(AssetError::UnsupportedPvr((header.flags & 0xff) as u8))?,
            );
            if texture {
                layout.pixels.gl_texture_format(true)?;
            }
            let decoded = crate::pvr::decode_rgba8(bytes)?;
            return Ok(DecodedNativeImage {
                width: decoded.width,
                height: decoded.height,
                rgba: decoded.rgba8,
                layout,
            });
        }
        ImageReaderKind::Unsupported => return Err(AssetError::UnsupportedImageReader),
    };
    if texture {
        layout.pixels.gl_texture_format(true)?;
    }
    let decoded = image::load_from_memory_with_format(bytes, format)
        .map_err(|_| match format {
            image::ImageFormat::Png => AssetError::InvalidPng("pixel decoding failed"),
            image::ImageFormat::Jpeg => AssetError::InvalidJpeg("pixel decoding failed"),
            image::ImageFormat::Bmp => AssetError::InvalidBmp("pixel decoding failed"),
            image::ImageFormat::Tga => AssetError::InvalidTga("pixel decoding failed"),
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

/// CORE/INFOHEADER subset of BMP reader `1004D4128`. Its source format
/// follows pixel depth, including native indexed/palette and 16-bit paths.
fn bmp_header(bytes: &[u8]) -> Result<(u32, u32, ImageSurfaceLayout), AssetError> {
    let header = bmp::Header::parse(bytes)?;
    Ok((header.width, header.height, header.layout))
}

/// Masks/5-bit scale from 1009F03B0/1009F07A0 and converter 1004DC50C.
fn rgb555_color(bytes: &[u8]) -> [u8; 4] {
    let pixel = u16::from_le_bytes([bytes[0], bytes[1]]);
    let expand = |shift: u32| ((((pixel >> shift) & 31u16) * 2114u16) >> 8) as u8;
    [expand(10), expand(5), expand(0), 255]
}

/// Verified TGA reader paths from `1004DA424`: raw/RLE true-color and
/// 8-bit indices with a zero-origin, at-most-256-entry 24/32-bit palette.
fn tga_header(bytes: &[u8]) -> Result<(u32, u32, ImageSurfaceLayout), AssetError> {
    if bytes.len() < 18 {
        return Err(AssetError::InvalidTga("truncated header"));
    }
    if bytes[17] & 0xc0 != 0 {
        return Err(AssetError::InvalidTga("unsupported descriptor"));
    }
    let indexed = matches!(bytes[2], 1 | 9);
    if indexed {
        let palette_origin = u16::from_le_bytes([bytes[3], bytes[4]]);
        let palette_length = u16::from_le_bytes([bytes[5], bytes[6]]);
        if bytes[1] != 1 || palette_origin != 0 || !(1..=256).contains(&palette_length) {
            return Err(AssetError::InvalidTga("invalid color map"));
        }
        if bytes[16] != 8 || !matches!(bytes[7], 24 | 32) {
            return Err(AssetError::InvalidTga(
                "unsupported indexed pixel or palette depth",
            ));
        }
    } else if bytes[1] != 0 || !matches!(bytes[2], 2 | 10) {
        return Err(AssetError::InvalidTga(
            "unsupported color map, type or descriptor",
        ));
    }
    let width = u16::from_le_bytes([bytes[12], bytes[13]]) as u32;
    let height = u16::from_le_bytes([bytes[14], bytes[15]]) as u32;
    if width == 0 || height == 0 {
        return Err(AssetError::InvalidTga("zero image dimension"));
    }
    let layout = match bytes[16] {
        8 if indexed => ImageSurfaceLayout {
            pixels: SurfaceFormat::P8,
            palette: Some(SurfaceFormat::X8B8G8R8),
        },
        16 => ImageSurfaceLayout::direct(SurfaceFormat::R5G5B5),
        24 => ImageSurfaceLayout::direct(SurfaceFormat::R8G8B8),
        32 => ImageSurfaceLayout::direct(SurfaceFormat::A8R8G8B8),
        _ => return Err(AssetError::InvalidTga("unsupported pixel depth")),
    };
    Ok((width, height, layout))
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
    png::probe(bytes).map(|(_, _, layout)| layout)
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
    fn native_texture_format_error_precedes_unimplemented_pvr_pixel_decode() {
        let mut bytes = Vec::new();
        for word in [
            52u32,
            4,
            4,
            0,
            0x20,
            8,
            4,
            0,
            0,
            0,
            0,
            u32::from_le_bytes(*b"PVR!"),
            1,
        ] {
            bytes.extend_from_slice(&word.to_le_bytes());
        }
        bytes.extend_from_slice(&[0; 8]);
        assert_eq!(image_dimensions(&bytes).unwrap(), [4, 4]);
        assert!(matches!(
            decode_native_image(&bytes, None),
            Err(AssetError::UnsupportedPvr(0x20))
        ));
        assert_eq!(
            decode_native_texture(&bytes, None).unwrap_err().to_string(),
            "Unsupported texture format: DXT1"
        );
    }

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

    fn bmp_fixture(depth: u16) -> Vec<u8> {
        let stride = (2usize * usize::from(depth) / 8).next_multiple_of(4);
        let mut bytes = vec![0u8; 54 + 2 * stride];
        bytes[..2].copy_from_slice(b"BM");
        let length = bytes.len() as u32;
        bytes[2..6].copy_from_slice(&length.to_le_bytes());
        bytes[10..14].copy_from_slice(&54u32.to_le_bytes());
        bytes[14..18].copy_from_slice(&40u32.to_le_bytes());
        bytes[18..22].copy_from_slice(&2i32.to_le_bytes());
        bytes[22..26].copy_from_slice(&2i32.to_le_bytes());
        bytes[26..28].copy_from_slice(&1u16.to_le_bytes());
        bytes[28..30].copy_from_slice(&depth.to_le_bytes());
        let colors = [[255, 0, 0], [255, 255, 255], [0, 0, 255], [0, 255, 0]];
        let channels = usize::from(depth / 8);
        for (index, color) in colors.into_iter().enumerate() {
            let start = 54 + index / 2 * stride + index % 2 * channels;
            bytes[start..start + 3].copy_from_slice(&color);
            if channels == 4 {
                bytes[start + 3] = 0;
            }
        }
        bytes
    }

    fn tga_fixture(depth: u8) -> Vec<u8> {
        let mut bytes = vec![0u8; 18];
        bytes[2] = 2;
        bytes[12..14].copy_from_slice(&2u16.to_le_bytes());
        bytes[14..16].copy_from_slice(&1u16.to_le_bytes());
        bytes[16] = depth;
        bytes[17] = if depth == 32 { 0x28 } else { 0x20 };
        bytes.extend_from_slice(&[0, 0, 255]);
        if depth == 32 {
            bytes.push(41);
        }
        bytes.extend_from_slice(&[0, 255, 0]);
        if depth == 32 {
            bytes.push(173);
        }
        bytes
    }

    fn bmp_indexed_fixture(depth: u16) -> Vec<u8> {
        let pixel_offset = 54usize + 2 * 4;
        let mut bytes = vec![0u8; pixel_offset + 4];
        bytes[..2].copy_from_slice(b"BM");
        let length = bytes.len() as u32;
        bytes[2..6].copy_from_slice(&length.to_le_bytes());
        bytes[10..14].copy_from_slice(&(pixel_offset as u32).to_le_bytes());
        bytes[14..18].copy_from_slice(&40u32.to_le_bytes());
        bytes[18..22].copy_from_slice(&2i32.to_le_bytes());
        bytes[22..26].copy_from_slice(&1i32.to_le_bytes());
        bytes[26..28].copy_from_slice(&1u16.to_le_bytes());
        bytes[28..30].copy_from_slice(&depth.to_le_bytes());
        bytes[46..50].copy_from_slice(&2u32.to_le_bytes());
        bytes[54..58].copy_from_slice(&[0, 0, 255, 0]);
        bytes[58..62].copy_from_slice(&[0, 255, 0, 0]);
        if depth == 4 {
            bytes[pixel_offset] = 0x01;
        } else {
            bytes[pixel_offset..pixel_offset + 2].copy_from_slice(&[0, 1]);
        }
        bytes
    }

    fn bmp_16_fixture() -> Vec<u8> {
        let mut bytes = vec![0u8; 58];
        bytes[..2].copy_from_slice(b"BM");
        bytes[2..6].copy_from_slice(&58u32.to_le_bytes());
        bytes[10..14].copy_from_slice(&54u32.to_le_bytes());
        bytes[14..18].copy_from_slice(&40u32.to_le_bytes());
        bytes[18..22].copy_from_slice(&2i32.to_le_bytes());
        bytes[22..26].copy_from_slice(&1i32.to_le_bytes());
        bytes[26..28].copy_from_slice(&1u16.to_le_bytes());
        bytes[28..30].copy_from_slice(&16u16.to_le_bytes());
        bytes[54..58].copy_from_slice(&[0x00, 0x7c, 0xe0, 0x03]);
        bytes
    }

    fn bmp_core_24_fixture() -> Vec<u8> {
        let info = bmp_fixture(24);
        let mut bytes = vec![0u8; 26];
        bytes[..2].copy_from_slice(b"BM");
        bytes[10..14].copy_from_slice(&26u32.to_le_bytes());
        bytes[14..18].copy_from_slice(&12u32.to_le_bytes());
        bytes[18..20].copy_from_slice(&2u16.to_le_bytes());
        bytes[20..22].copy_from_slice(&2u16.to_le_bytes());
        bytes[22..24].copy_from_slice(&1u16.to_le_bytes());
        bytes[24..26].copy_from_slice(&24u16.to_le_bytes());
        bytes.extend_from_slice(&info[54..]);
        let length = bytes.len() as u32;
        bytes[2..6].copy_from_slice(&length.to_le_bytes());
        bytes
    }

    fn bmp_core_8_fixture() -> Vec<u8> {
        let pixel_offset = 26 + 256 * 3;
        let mut bytes = vec![0u8; pixel_offset + 4];
        let length = bytes.len() as u32;
        bytes[..2].copy_from_slice(b"BM");
        bytes[2..6].copy_from_slice(&length.to_le_bytes());
        bytes[10..14].copy_from_slice(&(pixel_offset as u32).to_le_bytes());
        bytes[14..18].copy_from_slice(&12u32.to_le_bytes());
        bytes[18..20].copy_from_slice(&2u16.to_le_bytes());
        bytes[20..22].copy_from_slice(&1u16.to_le_bytes());
        bytes[22..24].copy_from_slice(&1u16.to_le_bytes());
        bytes[24..26].copy_from_slice(&8u16.to_le_bytes());
        bytes[26..32].copy_from_slice(&[0, 0, 255, 0, 255, 0]);
        bytes[pixel_offset..pixel_offset + 2].copy_from_slice(&[0, 1]);
        bytes
    }

    #[test]
    fn native_bmp_truecolor_preserves_pixels_and_source_formats() {
        for (depth, expected_layout) in [(24, SurfaceFormat::R8G8B8), (32, SurfaceFormat::X8R8G8B8)]
        {
            let bmp = bmp_fixture(depth);
            assert_eq!(image_reader_kind(&bmp, Some("png")), ImageReaderKind::Bmp);
            assert_eq!(image_dimensions(&bmp).unwrap(), [2, 2]);
            let decoded = decode_native_image(&bmp, Some("png")).unwrap();
            assert_eq!(decoded.layout, ImageSurfaceLayout::direct(expected_layout));
            assert_eq!(
                decoded.rgba,
                [
                    255, 0, 0, 255, 0, 255, 0, 255, 0, 0, 255, 255, 255, 255, 255, 255
                ]
            );
            assert!(decode_native_image(&bmp[..54], None).is_err());
        }
    }

    #[test]
    fn native_bmp_indexed_and_16_bit_paths_keep_source_layouts() {
        for (depth, pixels) in [(4, SurfaceFormat::P4), (8, SurfaceFormat::P8)] {
            let bmp = bmp_indexed_fixture(depth);
            let decoded = decode_native_image(&bmp, None).unwrap();
            assert_eq!(decoded.rgba, [255, 0, 0, 255, 0, 255, 0, 255]);
            assert_eq!(
                decoded.layout,
                ImageSurfaceLayout {
                    pixels,
                    palette: Some(SurfaceFormat::X8B8G8R8),
                }
            );
        }
        let decoded = decode_native_image(&bmp_16_fixture(), None).unwrap();
        assert_eq!(decoded.rgba, [255, 0, 0, 255, 0, 255, 0, 255]);
        assert_eq!(
            decoded.layout,
            ImageSurfaceLayout::direct(SurfaceFormat::R5G5B5)
        );
    }

    #[test]
    fn native_bmp_core_header_decodes_with_original_row_order() {
        let bmp = bmp_core_24_fixture();
        assert_eq!(image_dimensions(&bmp).unwrap(), [2, 2]);
        let decoded = decode_native_image(&bmp, None).unwrap();
        assert_eq!(decoded.layout.pixels, SurfaceFormat::R8G8B8);
        assert_eq!(
            decoded.rgba,
            [
                255, 0, 0, 255, 0, 255, 0, 255, 0, 0, 255, 255, 255, 255, 255, 255
            ]
        );

        let indexed = decode_native_image(&bmp_core_8_fixture(), None).unwrap();
        assert_eq!(indexed.rgba, [255, 0, 0, 255, 0, 255, 0, 255]);
        assert_eq!(
            indexed.layout,
            ImageSurfaceLayout {
                pixels: SurfaceFormat::P8,
                palette: Some(SurfaceFormat::X8B8G8R8),
            }
        );
    }

    #[test]
    fn native_tga_truecolor_uses_name_and_keeps_alpha() {
        for (depth, expected_layout, expected_alpha) in [
            (24, SurfaceFormat::R8G8B8, [255, 255]),
            (32, SurfaceFormat::A8R8G8B8, [41, 173]),
        ] {
            let tga = tga_fixture(depth);
            assert_eq!(image_reader_kind(&tga, Some("TGA")), ImageReaderKind::Tga);
            assert_eq!(
                image_dimensions_with_extension(&tga, Some("tga")).unwrap(),
                [2, 1]
            );
            let decoded = decode_native_image(&tga, Some("TGA")).unwrap();
            assert_eq!(decoded.layout, ImageSurfaceLayout::direct(expected_layout));
            assert_eq!(
                decoded.rgba,
                [255, 0, 0, expected_alpha[0], 0, 255, 0, expected_alpha[1]]
            );
            assert!(decode_native_image(&tga, None).is_err());
        }
    }

    #[test]
    fn native_tga_16_bit_and_rle_truecolor_paths_decode_pixels() {
        let mut tga16 = vec![0u8; 18];
        tga16[2] = 2;
        tga16[12..14].copy_from_slice(&2u16.to_le_bytes());
        tga16[14..16].copy_from_slice(&1u16.to_le_bytes());
        tga16[16] = 16;
        tga16[17] = 0x20;
        tga16.extend_from_slice(&[0x00, 0x7c, 0xe0, 0x03]);
        let decoded = decode_native_image(&tga16, Some("tga")).unwrap();
        assert_eq!(decoded.rgba, [255, 0, 0, 255, 0, 255, 0, 255]);
        assert_eq!(
            decoded.layout,
            ImageSurfaceLayout::direct(SurfaceFormat::R5G5B5)
        );

        let mut rle = tga_fixture(32);
        rle[2] = 10;
        rle.insert(18, 1); // One raw packet containing the following two pixels.
        let decoded = decode_native_image(&rle, Some("tga")).unwrap();
        assert_eq!(decoded.rgba, [255, 0, 0, 41, 0, 255, 0, 173]);
        assert_eq!(
            decoded.layout,
            ImageSurfaceLayout::direct(SurfaceFormat::A8R8G8B8)
        );
    }

    fn indexed_tga_fixture(palette_depth: u8, descriptor: u8, rle: bool) -> Vec<u8> {
        let mut bytes = vec![0u8; 18];
        bytes[0] = 3;
        bytes[1] = 1;
        bytes[2] = if rle { 9 } else { 1 };
        bytes[5..7].copy_from_slice(&3u16.to_le_bytes());
        bytes[7] = palette_depth;
        bytes[12..14].copy_from_slice(&3u16.to_le_bytes());
        bytes[14..16].copy_from_slice(&2u16.to_le_bytes());
        bytes[16] = 8;
        bytes[17] = descriptor;
        bytes.extend_from_slice(b"MCP");
        for color in [[7, 31, 203, 17], [90, 22, 13, 0], [5, 199, 37, 173]] {
            bytes.extend_from_slice(&color[..usize::from(palette_depth / 8)]);
        }
        bytes
    }

    #[test]
    fn native_tga_indexed_raw_and_rle_preserve_opaque_palette_and_row_order() {
        let top_down = [
            203, 31, 7, 255, 13, 22, 90, 255, 37, 199, 5, 255, 37, 199, 5, 255, 13, 22, 90, 255,
            203, 31, 7, 255,
        ];
        for palette_depth in [24, 32] {
            for descriptor in [0, 0x20, 0x28, 0x30] {
                for rle in [false, true] {
                    let mut bytes = indexed_tga_fixture(palette_depth, descriptor, rle);
                    bytes.extend_from_slice(if rle {
                        &[2, 0, 1, 2, 2, 2, 1, 0]
                    } else {
                        &[0, 1, 2, 2, 1, 0]
                    });
                    assert_eq!(
                        image_dimensions_with_extension(&bytes, Some("tga")).unwrap(),
                        [3, 2]
                    );
                    let decoded = decode_native_image(&bytes, Some("TGA")).unwrap();
                    assert_eq!(
                        decoded.layout,
                        ImageSurfaceLayout {
                            pixels: SurfaceFormat::P8,
                            palette: Some(SurfaceFormat::X8B8G8R8),
                        }
                    );
                    let expected = if descriptor & 0x20 == 0 {
                        [&top_down[12..], &top_down[..12]].concat()
                    } else {
                        top_down.to_vec()
                    };
                    assert_eq!(decoded.rgba, expected);
                    assert!(decode_native_image(&bytes, None).is_err());
                }
            }
        }
    }

    #[test]
    fn native_tga_indexed_rle_clips_packets_at_each_native_scanline() {
        let mut repeated = indexed_tga_fixture(32, 0x20, true);
        repeated.extend_from_slice(&[0x84, 0, 0x82, 1]);
        let decoded = decode_native_image(&repeated, Some("tga")).unwrap();
        assert_eq!(
            decoded.rgba,
            [
                203, 31, 7, 255, 203, 31, 7, 255, 203, 31, 7, 255, 13, 22, 90, 255, 13, 22, 90,
                255, 13, 22, 90, 255,
            ]
        );
        let mut raw = indexed_tga_fixture(24, 0x20, true);
        // Native reads all five source indices, discarding the two past row end.
        raw.extend_from_slice(&[4, 0, 1, 2, 255, 255, 2, 2, 1, 0]);
        let decoded = decode_native_image(&raw, Some("tga")).unwrap();
        assert_eq!(
            decoded.rgba,
            [
                203, 31, 7, 255, 13, 22, 90, 255, 37, 199, 5, 255, 37, 199, 5, 255, 13, 22, 90,
                255, 203, 31, 7, 255,
            ]
        );
    }

    #[test]
    fn native_tga_indexed_rejects_invalid_maps_truncation_and_unverified_depths() {
        let mut valid = indexed_tga_fixture(24, 0x20, false);
        valid.extend_from_slice(&[0, 1, 2, 2, 1, 0]);
        let mut cases = Vec::new();
        for (offset, value) in [
            (1, 0),
            (1, 2),
            (3, 1),
            (5, 0),
            (7, 16),
            (16, 4),
            (16, 16),
            (17, 0x40),
        ] {
            let mut bytes = valid.clone();
            bytes[offset] = value;
            cases.push(bytes);
        }
        let mut too_many_colors = valid.clone();
        too_many_colors[5..7].copy_from_slice(&257u16.to_le_bytes());
        cases.push(too_many_colors);
        for length in [17, 20, 29, valid.len() - 1] {
            cases.push(valid[..length].to_vec());
        }
        let mut invalid_index = valid.clone();
        *invalid_index.last_mut().unwrap() = 3;
        cases.push(invalid_index);
        for payload in [&[0x82][..], &[2, 0, 1][..]] {
            let mut bytes = indexed_tga_fixture(24, 0x20, true);
            bytes.extend_from_slice(payload);
            cases.push(bytes);
        }
        let mut oversized = valid.clone();
        oversized[12..16].fill(255);
        cases.push(oversized);
        for bytes in cases {
            assert!(matches!(
                decode_native_image(&bytes, Some("tga")),
                Err(AssetError::InvalidTga(_))
            ));
        }
    }

    #[test]
    fn native_bmp_tga_reject_unverified_variants_without_filename_fallback() {
        let mut compressed = bmp_fixture(24);
        compressed[30..34].copy_from_slice(&1u32.to_le_bytes());
        assert!(matches!(
            decode_native_image(&compressed, Some("png")),
            Err(AssetError::InvalidBmp("unsupported compression"))
        ));
        let mut indexed = tga_fixture(24);
        indexed[2] = 1;
        assert!(matches!(
            decode_native_image(&indexed, Some("tga")),
            Err(AssetError::InvalidTga(_))
        ));
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
        assert_eq!(image_reader_kind(&pvr, None), ImageReaderKind::Pvr);
        assert!(image_dimensions(&pvr[..52]).is_err());
        assert!(image_dimensions(b"not an image").is_err());
    }

    #[test]
    fn maps_every_png_reader_color_type() {
        for (source, format) in [
            (
                DynamicImage::ImageLuma8(image::GrayImage::new(1, 1)),
                SurfaceFormat::L8,
            ),
            (
                DynamicImage::ImageRgb8(RgbImage::new(1, 1)),
                SurfaceFormat::B8G8R8,
            ),
            (
                DynamicImage::ImageLumaA8(image::GrayAlphaImage::new(1, 1)),
                SurfaceFormat::A8L8,
            ),
            (
                DynamicImage::ImageRgba8(RgbaImage::new(1, 1)),
                SurfaceFormat::A8B8G8R8,
            ),
        ] {
            let mut bytes = Cursor::new(Vec::new());
            source.write_to(&mut bytes, ImageFormat::Png).unwrap();
            assert_eq!(
                png_surface_layout(bytes.get_ref()).unwrap(),
                ImageSurfaceLayout::direct(format)
            );
        }
        assert_eq!(
            png_surface_layout(include_bytes!(
                "native_image/png/fixtures/palette8-alpha.png"
            ))
            .unwrap(),
            ImageSurfaceLayout {
                pixels: SurfaceFormat::P8,
                palette: Some(SurfaceFormat::A8R8G8B8),
            }
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
