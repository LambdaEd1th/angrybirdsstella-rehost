//! Golden RGBA bytes from the unmodified PowerVR Native SDK decoder at
//! fa7396af369a3c803be43545504f82c7d5cfa4a9 (MIT, Imagination Technologies).
//! Fixtures include rectangular Morton layouts, wraparound, both endpoint
//! encodings, modulation modes, punchthrough alpha and minimum-size cropping.

use crate::{
    AssetError,
    native_image::{decode_native_texture, image_dimensions},
    pvr::decode_rgba8,
    surface_format::SurfaceFormat,
};

fn image(width: u32, height: u32, bits: u32, alpha: bool, payload: &[u8]) -> Vec<u8> {
    let flags = if bits == 2 { 0x18 } else { 0x19 } | if alpha { 1 << 15 } else { 0 };
    let mut bytes = Vec::new();
    for word in [
        52,
        height,
        width,
        0,
        flags,
        payload.len() as u32,
        bits,
        0,
        0,
        0,
        0,
        u32::from_le_bytes(*b"PVR!"),
        1,
    ] {
        bytes.extend_from_slice(&word.to_le_bytes());
    }
    bytes.extend_from_slice(payload);
    bytes
}

fn check_fixture(width: u32, height: u32, bits: u32, payload: &[u8], expected: &[u8]) {
    for alpha in [true, false] {
        let bytes = image(width, height, bits, alpha, payload);
        let mut expected = expected.to_vec();
        if !alpha {
            for pixel in expected.as_chunks_mut::<4>().0 {
                pixel[3] = 255;
            }
        }
        let decoded = decode_rgba8(&bytes).unwrap();
        assert_eq!([decoded.width, decoded.height], [width, height]);
        assert_eq!(decoded.rgba8, expected, "{bits}bpp, alpha={alpha}");
        let texture = decode_native_texture(&bytes, Some("pvr")).unwrap();
        assert_eq!(texture.rgba, expected);
        assert_eq!([texture.width, texture.height], [width, height]);
        assert_eq!(image_dimensions(&bytes).unwrap(), [width, height]);
        assert_eq!(
            texture.layout.pixels,
            match (bits, alpha) {
                (2, false) => SurfaceFormat::RgbPvrtcGl2Bpp,
                (2, true) => SurfaceFormat::RgbaPvrtcGl2Bpp,
                (4, false) => SurfaceFormat::RgbPvrtcGl4Bpp,
                (4, true) => SurfaceFormat::RgbaPvrtcGl4Bpp,
                _ => unreachable!(),
            }
        );
        assert_eq!(
            texture
                .layout
                .pixels
                .allocation_bytes(width as i32, height as i32),
            payload.len() as u32
        );
    }
}

macro_rules! fixture {
    ($name:ident, $file:literal, $width:literal, $height:literal, $bits:literal) => {
        #[test]
        fn $name() {
            check_fixture(
                $width,
                $height,
                $bits,
                include_bytes!(concat!("../pvrtc/fixtures/", $file, ".bin")),
                include_bytes!(concat!("../pvrtc/fixtures/", $file, ".rgba")),
            );
        }
    };
}

fixture!(sdk_4bpp_8x8, "4bpp-8x8", 8, 8, 4);
fixture!(sdk_4bpp_16x8, "4bpp-16x8", 16, 8, 4);
fixture!(sdk_4bpp_8x16, "4bpp-8x16", 8, 16, 4);
fixture!(sdk_4bpp_16x16, "4bpp-16x16", 16, 16, 4);
fixture!(sdk_4bpp_32x8, "4bpp-32x8", 32, 8, 4);
fixture!(sdk_4bpp_4x2, "4bpp-4x2", 4, 2, 4);
fixture!(sdk_4bpp_1x1, "4bpp-1x1", 1, 1, 4);
fixture!(sdk_2bpp_16x8, "2bpp-16x8", 16, 8, 2);
fixture!(sdk_2bpp_32x8, "2bpp-32x8", 32, 8, 2);
fixture!(sdk_2bpp_16x16, "2bpp-16x16", 16, 16, 2);
fixture!(sdk_2bpp_32x16, "2bpp-32x16", 32, 16, 2);
fixture!(sdk_2bpp_16x32, "2bpp-16x32", 16, 32, 2);
fixture!(sdk_2bpp_4x2, "2bpp-4x2", 4, 2, 2);
fixture!(sdk_2bpp_1x1, "2bpp-1x1", 1, 1, 2);

#[test]
fn compressed_format_determines_base_size_instead_of_bpp_field() {
    let payload = include_bytes!("../pvrtc/fixtures/4bpp-16x16.bin");
    let expected = include_bytes!("../pvrtc/fixtures/4bpp-16x16.rgba");
    let mut bytes = image(16, 16, 4, true, payload);
    bytes[24..28].copy_from_slice(&32u32.to_le_bytes());
    assert_eq!(decode_rgba8(&bytes).unwrap().rgba8, expected);
}

#[test]
fn only_the_base_mip_is_decoded() {
    let mut payload = include_bytes!("../pvrtc/fixtures/2bpp-16x16.bin").to_vec();
    payload.extend_from_slice(&[0x5a; 32]);
    let mut bytes = image(16, 16, 2, true, &payload);
    bytes[12..16].copy_from_slice(&1u32.to_le_bytes());
    assert_eq!(
        decode_rgba8(&bytes).unwrap().rgba8,
        include_bytes!("../pvrtc/fixtures/2bpp-16x16.rgba")
    );
}

#[test]
fn missing_compressed_words_fail_before_allocation() {
    for bits in [2, 4] {
        let bytes = image(if bits == 2 { 16 } else { 8 }, 8, bits, true, &[0; 31]);
        assert!(matches!(
            decode_rgba8(&bytes),
            Err(AssetError::InvalidPvr("PVRTC base mip is truncated"))
        ));
    }
    let bytes = image(1 << 31, 1 << 31, 4, true, &[]);
    assert!(matches!(
        decode_rgba8(&bytes),
        Err(AssetError::InvalidPvr(_))
    ));
}

#[test]
fn non_power_of_two_word_grids_fail_without_panicking() {
    for bits in [2, 4] {
        let bytes = image(if bits == 2 { 24 } else { 12 }, 8, bits, true, &[0; 48]);
        assert!(matches!(
            decode_rgba8(&bytes),
            Err(AssetError::InvalidPvr(
                "PVRTC storage dimensions must be powers of two"
            ))
        ));
    }
}
