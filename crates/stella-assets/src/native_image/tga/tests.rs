use super::super::{decode_native_image, decode_native_texture, image_dimensions_with_extension};

fn header(depth: u8, descriptor: u8, rle: bool) -> Vec<u8> {
    let mut bytes = vec![0; 18];
    bytes[0] = 3;
    bytes[2] = if rle { 10 } else { 2 };
    bytes[12..14].copy_from_slice(&2u16.to_le_bytes());
    bytes[14..16].copy_from_slice(&2u16.to_le_bytes());
    bytes[16] = depth;
    bytes[17] = descriptor;
    bytes.extend_from_slice(b"ID!");
    bytes
}

#[test]
fn truecolor_raw_native_direction_and_alpha_attributes() {
    for depth in [24, 32] {
        for direction in [0, 0x10, 0x20, 0x30] {
            for attributes in [0, 1, 4, 8, 15] {
                let mut bytes = header(depth, direction | attributes, false);
                for color in [
                    [7, 31, 203, 41],
                    [83, 17, 29, 173],
                    [19, 211, 61, 0],
                    [151, 43, 97, 255],
                ] {
                    bytes.extend_from_slice(&color[..usize::from(depth / 8)]);
                }
                let first = [203, 31, 7, if depth == 32 { 41 } else { 255 }];
                let second = [29, 17, 83, if depth == 32 { 173 } else { 255 }];
                let third = [61, 211, 19, if depth == 32 { 0 } else { 255 }];
                let fourth = [97, 43, 151, 255];
                let expected = if direction & 0x20 == 0 {
                    [third, fourth, first, second]
                } else {
                    [first, second, third, fourth]
                };
                let decoded = decode_native_image(&bytes, Some("TGA")).unwrap();
                assert_eq!((decoded.width, decoded.height), (2, 2));
                assert_eq!(
                    decoded.rgba,
                    expected.concat(),
                    "depth={depth}, descriptor={:#x}",
                    direction | attributes
                );
            }
        }
    }
}

#[test]
fn truecolor_rle_discards_raw_packet_overflow_per_scanline() {
    for depth in [16, 24, 32] {
        let mut bytes = header(depth, 0x30, true);
        let pixels: &[&[u8]] = match depth {
            16 => &[&[0, 0x7c], &[0xe0, 3], &[0xff, 0x7f], &[0x1f, 0], &[0, 0]],
            24 => &[&[0, 0, 255], &[0, 255, 0], &[255; 3], &[255, 0, 0], &[0; 3]],
            32 => &[
                &[0, 0, 255, 255],
                &[0, 255, 0, 255],
                &[255; 4],
                &[255, 0, 0, 255],
                &[0, 0, 0, 255],
            ],
            _ => unreachable!(),
        };
        bytes.push(2); // Three input pixels, only two visible in the first row.
        for pixel in &pixels[..3] {
            bytes.extend_from_slice(pixel);
        }
        bytes.push(1);
        for pixel in &pixels[3..] {
            bytes.extend_from_slice(pixel);
        }
        assert_eq!(
            decode_native_image(&bytes, Some("tga")).unwrap().rgba,
            [255, 0, 0, 255, 0, 255, 0, 255, 0, 0, 255, 255, 0, 0, 0, 255],
            "depth={depth}"
        );
    }
}

#[test]
fn truecolor_rle_discards_repeated_packet_overflow_per_scanline() {
    for depth in [16, 24, 32] {
        let mut bytes = header(depth, 0x20, true);
        let (first, second): (&[u8], &[u8]) = match depth {
            16 => (&[0, 0x7c], &[0x1f, 0]),
            24 => (&[0, 0, 255], &[255, 0, 0]),
            32 => (&[0, 0, 255, 255], &[255, 0, 0, 255]),
            _ => unreachable!(),
        };
        bytes.push(0x82);
        bytes.extend_from_slice(first);
        bytes.push(0x81);
        bytes.extend_from_slice(second);
        assert_eq!(
            decode_native_image(&bytes, Some("tga")).unwrap().rgba,
            [
                255, 0, 0, 255, 255, 0, 0, 255, 0, 0, 255, 255, 0, 0, 255, 255
            ],
            "depth={depth}"
        );
    }
}

#[test]
fn truecolor_16_bit_uses_native_fixed_point_expansion_and_opaque_high_bit() {
    for attributes in [0, 1, 4, 8, 15] {
        let mut bytes = header(16, 0x20 | attributes, false);
        let color = (3u16 << 10) | (7 << 5) | 24;
        for pixel in [color, color | 0x8000, 0, 0xffff] {
            bytes.extend_from_slice(&pixel.to_le_bytes());
        }
        assert_eq!(
            decode_native_image(&bytes, Some("tga")).unwrap().rgba,
            [
                24, 57, 198, 255, 24, 57, 198, 255, 0, 0, 0, 255, 255, 255, 255, 255
            ]
        );
    }
}

#[test]
fn truecolor_rejects_truncated_packets_and_excessive_allocation_after_header_probe() {
    for depth in [16, 24, 32] {
        for rle in [false, true] {
            let mut bytes = header(depth, 0x20, rle);
            for _ in 0..2 {
                if rle {
                    bytes.push(1);
                }
                bytes.resize(bytes.len() + usize::from(depth / 8) * 2, 0);
            }
            let pixel_start = 21 + usize::from(rle);
            for end in pixel_start..bytes.len() {
                assert_eq!(
                    image_dimensions_with_extension(&bytes[..end], Some("tga")).unwrap(),
                    [2, 2]
                );
                assert!(decode_native_image(&bytes[..end], Some("tga")).is_err());
            }
            assert!(decode_native_image(&bytes, Some("tga")).is_ok());
        }
    }
    let mut large = header(32, 0x20, false);
    large[12..16].fill(255);
    assert_eq!(
        image_dimensions_with_extension(&large, Some("tga")).unwrap(),
        [65535; 2]
    );
    assert!(decode_native_image(&large, Some("tga")).is_err());
}

#[test]
fn truecolor_readable_16_bit_fails_native_texture_creation_before_pixel_reads() {
    use crate::{AssetError, surface_format::SurfaceFormat};
    let mut bytes = header(16, 0x20, false);
    bytes.extend_from_slice(&[0; 8]);
    assert_eq!(
        decode_native_image(&bytes, Some("tga"))
            .unwrap()
            .layout
            .pixels,
        SurfaceFormat::R5G5B5
    );
    for length in [21, bytes.len()] {
        assert!(matches!(
            decode_native_texture(&bytes[..length], Some("tga")),
            Err(AssetError::UnsupportedTextureFormat(SurfaceFormat::R5G5B5))
        ));
    }
}
