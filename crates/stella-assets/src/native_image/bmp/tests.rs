use super::super::{decode_native_image, decode_native_texture, image_dimensions};

const COLORS: [[u8; 4]; 3] = [[203, 31, 7, 255], [29, 17, 83, 255], [61, 211, 19, 255]];

fn fixture(dib: u32, depth: u16, width: u16, height: u16, colors: usize, rows: &[u8]) -> Vec<u8> {
    let entry_size = if dib == 12 { 3 } else { 4 };
    let offset = 14 + dib as usize + colors * entry_size;
    let mut bytes = vec![0; offset];
    bytes[..2].copy_from_slice(b"BM");
    bytes[10..14].copy_from_slice(&(offset as u32).to_le_bytes());
    bytes[14..18].copy_from_slice(&dib.to_le_bytes());
    if dib == 12 {
        bytes[18..20].copy_from_slice(&width.to_le_bytes());
        bytes[20..22].copy_from_slice(&height.to_le_bytes());
        bytes[22..24].copy_from_slice(&1u16.to_le_bytes());
        bytes[24..26].copy_from_slice(&depth.to_le_bytes());
    } else {
        bytes[18..22].copy_from_slice(&u32::from(width).to_le_bytes());
        bytes[22..26].copy_from_slice(&u32::from(height).to_le_bytes());
        bytes[26..28].copy_from_slice(&1u16.to_le_bytes());
        bytes[28..30].copy_from_slice(&depth.to_le_bytes());
        bytes[46..50].copy_from_slice(&(colors as u32).to_le_bytes());
    }
    for index in 0..colors {
        let color = COLORS.get(index).copied().unwrap_or([0, 0, 0, 255]);
        let entry = [
            color[2],
            color[1],
            color[0],
            if index == 0 { 0 } else { 173 },
        ];
        let begin = 14 + dib as usize + index * entry_size;
        bytes[begin..begin + entry_size].copy_from_slice(&entry[..entry_size]);
    }
    bytes.extend_from_slice(rows);
    let size = bytes.len() as u32;
    bytes[2..6].copy_from_slice(&size.to_le_bytes());
    bytes
}

#[test]
fn bmp_native_info_and_os2_dimensions_use_only_low_words() {
    for dib in [40, 64] {
        let mut bytes = fixture(
            dib,
            24,
            2,
            2,
            0,
            &[
                7, 31, 203, 83, 17, 29, 91, 92, 19, 211, 61, 151, 43, 97, 93, 94,
            ],
        );
        bytes[20..22].copy_from_slice(&1u16.to_le_bytes());
        bytes[24..26].copy_from_slice(&1u16.to_le_bytes());
        assert_eq!(image_dimensions(&bytes).unwrap(), [2, 2]);
        assert_eq!(
            decode_native_texture(&bytes, Some("png")).unwrap().rgba,
            [
                61, 211, 19, 255, 97, 43, 151, 255, 203, 31, 7, 255, 29, 17, 83, 255
            ]
        );
        bytes[22..26].copy_from_slice(&(-2i32).to_le_bytes());
        assert_eq!(image_dimensions(&bytes).unwrap(), [2, 65534]);
    }
}

#[test]
fn bmp_native_64_byte_header_accepts_all_recovered_depths() {
    for depth in [4, 8, 16, 24, 32] {
        let (palette, rows): (usize, &[u8]) = match depth {
            4 => (3, &[0x01, 0x20, 91, 92, 0x21, 0x00, 93, 94]),
            8 => (3, &[0, 1, 2, 92, 2, 1, 0, 94]),
            16 => (
                0,
                &[
                    0, 0x7c, 0xe0, 3, 0x1f, 0, 92, 93, 0x1f, 0, 0xe0, 3, 0, 0x7c, 94, 95,
                ],
            ),
            24 => (
                0,
                &[
                    0, 0, 255, 0, 255, 0, 255, 0, 0, 92, 93, 94, 255, 0, 0, 0, 255, 0, 0, 0, 255,
                    95, 96, 97,
                ],
            ),
            32 => (
                0,
                &[
                    0, 0, 255, 0, 0, 255, 0, 173, 255, 0, 0, 255, 255, 0, 0, 17, 0, 255, 0, 0, 0,
                    0, 255, 173,
                ],
            ),
            _ => unreachable!(),
        };
        let bytes = fixture(64, depth, 3, 2, palette, rows);
        let expected = if depth <= 8 {
            [
                COLORS[2], COLORS[1], COLORS[0], COLORS[0], COLORS[1], COLORS[2],
            ]
        } else {
            [
                [0, 0, 255, 255],
                [0, 255, 0, 255],
                [255, 0, 0, 255],
                [255, 0, 0, 255],
                [0, 255, 0, 255],
                [0, 0, 255, 255],
            ]
        };
        assert_eq!(
            decode_native_image(&bytes, None).unwrap().rgba,
            expected.concat(),
            "depth={depth}"
        );
        if matches!(depth, 16 | 32) {
            assert!(decode_native_texture(&bytes, None).is_err());
        } else {
            assert_eq!(
                decode_native_texture(&bytes, None).unwrap().rgba,
                expected.concat()
            );
        }
    }
}

#[test]
fn bmp_native_core_ignores_planes_and_accepts_only_8_or_24_bits() {
    for depth in [8, 24] {
        let (colors, rows): (usize, &[u8]) = if depth == 8 {
            (256, &[0, 1, 91, 92, 2, 0, 93, 94])
        } else {
            (
                0,
                &[
                    7, 31, 203, 83, 17, 29, 91, 92, 19, 211, 61, 7, 31, 203, 93, 94,
                ],
            )
        };
        let mut bytes = fixture(12, depth, 2, 2, colors, rows);
        bytes[22..24].copy_from_slice(&42u16.to_le_bytes());
        assert_eq!(
            decode_native_texture(&bytes, None).unwrap().rgba,
            [COLORS[2], COLORS[0], COLORS[0], COLORS[1]].concat()
        );
    }
    for depth in [4, 16, 32] {
        let bytes = fixture(12, depth, 2, 2, if depth == 4 { 16 } else { 0 }, &[0; 16]);
        assert!(image_dimensions(&bytes).is_err());
    }
}

#[test]
fn bmp_native_palette_admits_more_than_16_entries_and_zero_initializes_unused_slots() {
    for (depth, colors, rows) in [
        (4, 17, &[0x01, 0x20, 91, 92, 0x21, 0x00, 93, 94][..]),
        (4, 3, &[0xf1, 0x20, 91, 92, 0x2f, 0x00, 93, 94][..]),
        (8, 3, &[200, 1, 2, 92, 2, 200, 0, 94][..]),
    ] {
        let bytes = fixture(40, depth, 3, 2, colors, rows);
        let expected = if colors == 17 {
            [
                COLORS[2], COLORS[1], COLORS[0], COLORS[0], COLORS[1], COLORS[2],
            ]
        } else {
            [
                COLORS[2],
                [0, 0, 0, 255],
                COLORS[0],
                [0, 0, 0, 255],
                COLORS[1],
                COLORS[2],
            ]
        };
        assert_eq!(
            decode_native_texture(&bytes, None).unwrap().rgba,
            expected.concat()
        );
    }
    let mut negative = fixture(40, 4, 2, 2, 16, &[0x01, 91, 92, 93, 0x20, 94, 95, 96]);
    negative[46..50].copy_from_slice(&u32::MAX.to_le_bytes());
    assert_eq!(
        decode_native_texture(&negative, None).unwrap().rgba,
        [COLORS[2], COLORS[0], COLORS[0], COLORS[1]].concat()
    );
}

#[test]
fn bmp_native_p4_row_stride_floors_partial_bytes_before_alignment() {
    let bytes = fixture(
        40,
        4,
        9,
        2,
        3,
        &[
            0x01, 0x20, 0x12, 0x01, 0x21, 0x02, 0x10, 0x20, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff,
            0xff, 0xff,
        ],
    );
    let expected = [
        COLORS[2], COLORS[1], COLORS[0], COLORS[2], COLORS[1], COLORS[0], COLORS[2], COLORS[0],
        COLORS[0], COLORS[0], COLORS[1], COLORS[2], COLORS[0], COLORS[1], COLORS[2], COLORS[0],
        COLORS[1], COLORS[0],
    ];
    assert_eq!(
        decode_native_texture(&bytes, None).unwrap().rgba,
        expected.concat()
    );
}

#[test]
fn bmp_native_rgb555_expansion_ignores_the_high_bit() {
    let color = (3u16 << 10) | (7 << 5) | 24;
    let rows = [color, color | 0x8000, 0, 0xffff]
        .into_iter()
        .flat_map(u16::to_le_bytes)
        .collect::<Vec<_>>();
    let bytes = fixture(40, 16, 2, 2, 0, &rows);
    assert_eq!(
        decode_native_image(&bytes, None).unwrap().rgba,
        [
            0, 0, 0, 255, 255, 255, 255, 255, 24, 57, 198, 255, 24, 57, 198, 255
        ]
    );
}

#[test]
fn bmp_native_checked_storage_never_publishes_truncated_or_undefined_pixels() {
    for dib in [12, 40, 64] {
        let bytes = fixture(
            dib,
            8,
            2,
            2,
            if dib == 12 { 256 } else { 3 },
            &[0, 1, 91, 92, 2, 0, 93, 94],
        );
        let offset = u32::from_le_bytes(bytes[10..14].try_into().unwrap()) as usize;
        for end in offset..bytes.len() {
            assert_eq!(image_dimensions(&bytes[..end]).unwrap(), [2, 2]);
            assert!(decode_native_texture(&bytes[..end], None).is_err());
        }
        assert!(decode_native_texture(&bytes, None).is_ok());
    }
    for dib in [40, 64] {
        let mut invalid = fixture(dib, 8, 2, 2, 3, &[0; 8]);
        invalid[30..34].copy_from_slice(&1u32.to_le_bytes());
        assert!(image_dimensions(&invalid).is_err());
        let invalid = fixture(dib, 4, 2, 2, 257, &[0; 8]);
        assert!(image_dimensions(&invalid).is_err());
    }
    let undefined = fixture(40, 4, 1, 1, 3, &[0; 4]);
    assert_eq!(image_dimensions(&undefined).unwrap(), [1, 1]);
    assert!(decode_native_texture(&undefined, None).is_err());
    let oversized = fixture(40, 24, 65535, 65535, 0, &[]);
    assert_eq!(image_dimensions(&oversized).unwrap(), [65535; 2]);
    assert!(decode_native_texture(&oversized, None).is_err());
}
