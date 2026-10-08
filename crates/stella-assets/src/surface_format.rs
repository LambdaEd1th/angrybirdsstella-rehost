//! Recovered `img::SurfaceFormat` identities and predicates.

/// Native image surface formats indexed by Purple's format-name table at
/// `off_100AA5D98`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum SurfaceFormat {
    Unknown = 0,
    R8G8B8 = 1,
    B8G8R8 = 2,
    A8R8G8B8 = 3,
    X8R8G8B8 = 4,
    X8B8G8R8 = 5,
    A8B8G8R8 = 6,
    R5G6B5 = 7,
    R5G5B5 = 8,
    R6G6B6 = 9,
    P4 = 10,
    P8 = 11,
    L8 = 12,
    A8L8 = 13,
    A1R5G5B5 = 14,
    X4R4G4B4 = 15,
    A4R4G4B4 = 16,
    A4B4G4R4 = 17,
    R4G4B4A4 = 18,
    A1B5G5R5 = 19,
    R5G5B5A1 = 20,
    R3G3B2 = 21,
    R3G2B3 = 22,
    A8 = 23,
    A8R3G3B2 = 24,
    A8R3G2B3 = 25,
    Dxt1 = 26,
    Dxt3 = 27,
    Dxt5 = 28,
    RgbPvrtcGl2Bpp = 29,
    RgbaPvrtcGl2Bpp = 30,
    RgbPvrtcGl4Bpp = 31,
    RgbaPvrtcGl4Bpp = 32,
    Etc1Rgb4Bpp = 33,
}

impl std::fmt::Display for SurfaceFormat {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Original format-name table off_100AA5D98; other names match Debug.
        let name = match self {
            Self::Unknown => "UNKNOWN",
            Self::Dxt1 => "DXT1",
            Self::Dxt3 => "DXT3",
            Self::Dxt5 => "DXT5",
            Self::RgbPvrtcGl2Bpp => "RGB_PVRTC_GL_2BPP",
            Self::RgbaPvrtcGl2Bpp => "RGBA_PVRTC_GL_2BPP",
            Self::RgbPvrtcGl4Bpp => "RGB_PVRTC_GL_4BPP",
            Self::RgbaPvrtcGl4Bpp => "RGBA_PVRTC_GL_4BPP",
            Self::Etc1Rgb4Bpp => "ETC1_RGB_4BPP",
            format => return write!(formatter, "{format:?}"),
        };
        formatter.write_str(name)
    }
}

impl SurfaceFormat {
    /// Exact predicate implemented by `sub_1004DC4B8`.
    pub const fn has_alpha(self) -> bool {
        matches!(
            self,
            Self::A8R8G8B8
                | Self::A8B8G8R8
                | Self::A8L8
                | Self::A1R5G5B5
                | Self::A4R4G4B4
                | Self::A4B4G4R4
                | Self::R4G4B4A4
                | Self::A1B5G5R5
                | Self::R5G5B5A1
                | Self::A8
                | Self::A8R3G3B2
                | Self::A8R3G2B3
                | Self::Dxt1
                | Self::Dxt3
                | Self::Dxt5
                | Self::RgbaPvrtcGl2Bpp
                | Self::RgbaPvrtcGl4Bpp
        )
    }

    /// Format construction recovered from the PVR-v2 reader
    /// `sub_1004D7BC8`. The low byte is the GL format code; bit 15 selects
    /// RGB/RGBA for PVRTC inputs.
    pub const fn from_pvr_v2_flags(flags: u32) -> Option<Self> {
        let alpha = flags & (1 << 15) != 0;
        match flags as u8 {
            0x10 => Some(Self::R4G4B4A4),
            0x11 => Some(Self::R5G5B5A1),
            0x12 => Some(Self::A8B8G8R8),
            0x13 => Some(Self::R5G6B5),
            0x15 => Some(Self::B8G8R8),
            0x16 => Some(Self::L8),
            0x17 => Some(Self::A8L8),
            0x18 if alpha => Some(Self::RgbaPvrtcGl2Bpp),
            0x18 => Some(Self::RgbPvrtcGl2Bpp),
            0x19 if alpha => Some(Self::RgbaPvrtcGl4Bpp),
            0x19 => Some(Self::RgbPvrtcGl4Bpp),
            0x1a => Some(Self::A8R8G8B8),
            0x20 => Some(Self::Dxt1),
            0x22 => Some(Self::Dxt3),
            0x24 => Some(Self::Dxt5),
            _ => None,
        }
    }

    /// Format selected by `GL_Context::createTexture` helper
    /// `sub_100597FD8` before constructing the actual GL texture.
    pub const fn for_gl_upload(self, etc1_supported: bool) -> Self {
        match self {
            Self::R8G8B8 => Self::B8G8R8,
            Self::A8R8G8B8 | Self::P4 | Self::P8 => Self::A8B8G8R8,
            Self::Etc1Rgb4Bpp if !etc1_supported => Self::R5G6B5,
            format => format,
        }
    }

    /// Allocation counter arithmetic from `sub_1004DE110`, including signed
    /// 32-bit multiplication/shift and minimum compressed texture extents.
    /// Callers normalize source formats with `for_gl_upload` first.
    pub const fn allocation_bytes(self, width: i32, height: i32) -> u32 {
        const BITS: [i32; 34] = [
            0, 24, 24, 32, 32, 32, 32, 16, 16, 32, 4, 8, 8, 16, 16, 16, 16, 16, 16, 16, 16, 8, 8,
            8, 16, 16, 4, 8, 8, 2, 2, 4, 4, 4,
        ];
        let (width, height) = match self {
            Self::Dxt1 | Self::Dxt3 | Self::Dxt5 => {
                let blocks_x = (width.wrapping_add(3) as u32) >> 2;
                let blocks_y = (height.wrapping_add(3) as u32) >> 2;
                return blocks_x.wrapping_mul(blocks_y).wrapping_shl(match self {
                    Self::Dxt1 => 3,
                    _ => 4,
                });
            }
            Self::RgbPvrtcGl2Bpp | Self::RgbaPvrtcGl2Bpp => (
                if width < 16 { 16 } else { width },
                if height < 8 { 8 } else { height },
            ),
            Self::RgbPvrtcGl4Bpp | Self::RgbaPvrtcGl4Bpp => (
                if width < 8 { 8 } else { width },
                if height < 8 { 8 } else { height },
            ),
            Self::Etc1Rgb4Bpp => {
                let width = if width < 4 { 4 } else { width };
                let height = if height < 4 { 4 } else { height };
                return (width.wrapping_mul(height) >> 1) as u32;
            }
            _ => (width, height),
        };
        (width.wrapping_mul(height).wrapping_mul(BITS[self as usize]) >> 3) as u32
    }

    /// Admit the normalized format through GLES2 helper `1005A11E4`.
    /// Reader support alone does not make a format usable as a native Image.
    pub const fn gl_texture_format(self, etc1_supported: bool) -> Result<Self, crate::AssetError> {
        let format = self.for_gl_upload(etc1_supported);
        match format {
            Self::R8G8B8
            | Self::B8G8R8
            | Self::A8R8G8B8
            | Self::A8B8G8R8
            | Self::R5G6B5
            | Self::L8
            | Self::A8L8
            | Self::R4G4B4A4
            | Self::R5G5B5A1
            | Self::A8
            | Self::RgbPvrtcGl2Bpp
            | Self::RgbaPvrtcGl2Bpp
            | Self::RgbPvrtcGl4Bpp
            | Self::RgbaPvrtcGl4Bpp => Ok(format),
            _ => Err(crate::AssetError::UnsupportedTextureFormat(format)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn alpha_formats_match_recovered_bit_predicate() {
        let recovered = (0u8..=33)
            .filter(|format| {
                (0x039f_6048u64 >> format) & 1 != 0
                    || (0x1a..=0x1c).contains(format)
                    || matches!(format, 0x1e | 0x20)
            })
            .collect::<Vec<_>>();
        let modeled = [
            SurfaceFormat::A8R8G8B8,
            SurfaceFormat::A8B8G8R8,
            SurfaceFormat::A8L8,
            SurfaceFormat::A1R5G5B5,
            SurfaceFormat::A4R4G4B4,
            SurfaceFormat::A4B4G4R4,
            SurfaceFormat::R4G4B4A4,
            SurfaceFormat::A1B5G5R5,
            SurfaceFormat::R5G5B5A1,
            SurfaceFormat::A8,
            SurfaceFormat::A8R3G3B2,
            SurfaceFormat::A8R3G2B3,
            SurfaceFormat::Dxt1,
            SurfaceFormat::Dxt3,
            SurfaceFormat::Dxt5,
            SurfaceFormat::RgbaPvrtcGl2Bpp,
            SurfaceFormat::RgbaPvrtcGl4Bpp,
        ]
        .map(|format| format as u8);
        assert_eq!(recovered, modeled);
    }

    #[test]
    fn maps_shipped_pvr_surface_formats() {
        assert_eq!(
            SurfaceFormat::from_pvr_v2_flags(0x10),
            Some(SurfaceFormat::R4G4B4A4)
        );
        assert_eq!(
            SurfaceFormat::from_pvr_v2_flags(0x12),
            Some(SurfaceFormat::A8B8G8R8)
        );
        assert!(SurfaceFormat::from_pvr_v2_flags(0x10).unwrap().has_alpha());
        assert!(SurfaceFormat::from_pvr_v2_flags(0x12).unwrap().has_alpha());
    }

    #[test]
    fn normalizes_reader_formats_before_gl_texture_creation() {
        assert_eq!(
            SurfaceFormat::R8G8B8.for_gl_upload(true),
            SurfaceFormat::B8G8R8
        );
        assert_eq!(
            SurfaceFormat::A8R8G8B8.for_gl_upload(true),
            SurfaceFormat::A8B8G8R8
        );
        assert_eq!(
            SurfaceFormat::P8.for_gl_upload(true),
            SurfaceFormat::A8B8G8R8
        );
        assert_eq!(
            SurfaceFormat::Etc1Rgb4Bpp.for_gl_upload(false),
            SurfaceFormat::R5G6B5
        );
        assert_eq!(
            SurfaceFormat::Etc1Rgb4Bpp.for_gl_upload(true),
            SurfaceFormat::Etc1Rgb4Bpp
        );
    }

    /// Golden results execute the unmodified ARM64 1004DE110 and its format
    /// table from Purple (ba45c91d…), not a second copy of the Rust formula.
    #[test]
    fn allocation_bytes_match_unmodified_native_arm64_for_all_formats() {
        const FORMATS: [SurfaceFormat; 34] = [
            SurfaceFormat::Unknown,
            SurfaceFormat::R8G8B8,
            SurfaceFormat::B8G8R8,
            SurfaceFormat::A8R8G8B8,
            SurfaceFormat::X8R8G8B8,
            SurfaceFormat::X8B8G8R8,
            SurfaceFormat::A8B8G8R8,
            SurfaceFormat::R5G6B5,
            SurfaceFormat::R5G5B5,
            SurfaceFormat::R6G6B6,
            SurfaceFormat::P4,
            SurfaceFormat::P8,
            SurfaceFormat::L8,
            SurfaceFormat::A8L8,
            SurfaceFormat::A1R5G5B5,
            SurfaceFormat::X4R4G4B4,
            SurfaceFormat::A4R4G4B4,
            SurfaceFormat::A4B4G4R4,
            SurfaceFormat::R4G4B4A4,
            SurfaceFormat::A1B5G5R5,
            SurfaceFormat::R5G5B5A1,
            SurfaceFormat::R3G3B2,
            SurfaceFormat::R3G2B3,
            SurfaceFormat::A8,
            SurfaceFormat::A8R3G3B2,
            SurfaceFormat::A8R3G2B3,
            SurfaceFormat::Dxt1,
            SurfaceFormat::Dxt3,
            SurfaceFormat::Dxt5,
            SurfaceFormat::RgbPvrtcGl2Bpp,
            SurfaceFormat::RgbaPvrtcGl2Bpp,
            SurfaceFormat::RgbPvrtcGl4Bpp,
            SurfaceFormat::RgbaPvrtcGl4Bpp,
            SurfaceFormat::Etc1Rgb4Bpp,
        ];
        const EXTENTS: [(i32, i32); 10] = [
            (5, 7),
            (1, 1),
            (4, 2),
            (8192, 8192),
            (16384, 16384),
            (2147483647, 7),
            (-1, 9),
            (0, 0),
            (65536, 65536),
            (32769, 65535),
        ];
        const NATIVE: [[u32; 10]; 34] = [
            [0, 0, 0, 0, 0, 0, 0, 0, 0, 0],
            [
                105, 3, 24, 201326592, 4026531840, 4294967275, 4294967269, 0, 0, 98301,
            ],
            [
                105, 3, 24, 201326592, 4026531840, 4294967275, 4294967269, 0, 0, 98301,
            ],
            [
                140, 4, 32, 4026531840, 0, 4294967268, 4294967260, 0, 0, 131068,
            ],
            [
                140, 4, 32, 4026531840, 0, 4294967268, 4294967260, 0, 0, 131068,
            ],
            [
                140, 4, 32, 4026531840, 0, 4294967268, 4294967260, 0, 0, 131068,
            ],
            [
                140, 4, 32, 4026531840, 0, 4294967268, 4294967260, 0, 0, 131068,
            ],
            [70, 2, 16, 134217728, 0, 4294967282, 4294967278, 0, 0, 65534],
            [70, 2, 16, 134217728, 0, 4294967282, 4294967278, 0, 0, 65534],
            [
                140, 4, 32, 4026531840, 0, 4294967268, 4294967260, 0, 0, 131068,
            ],
            [
                17, 0, 4, 33554432, 134217728, 4294967292, 4294967291, 0, 0, 16383,
            ],
            [
                35, 1, 8, 67108864, 4026531840, 4294967289, 4294967287, 0, 0, 32767,
            ],
            [
                35, 1, 8, 67108864, 4026531840, 4294967289, 4294967287, 0, 0, 32767,
            ],
            [70, 2, 16, 134217728, 0, 4294967282, 4294967278, 0, 0, 65534],
            [70, 2, 16, 134217728, 0, 4294967282, 4294967278, 0, 0, 65534],
            [70, 2, 16, 134217728, 0, 4294967282, 4294967278, 0, 0, 65534],
            [70, 2, 16, 134217728, 0, 4294967282, 4294967278, 0, 0, 65534],
            [70, 2, 16, 134217728, 0, 4294967282, 4294967278, 0, 0, 65534],
            [70, 2, 16, 134217728, 0, 4294967282, 4294967278, 0, 0, 65534],
            [70, 2, 16, 134217728, 0, 4294967282, 4294967278, 0, 0, 65534],
            [70, 2, 16, 134217728, 0, 4294967282, 4294967278, 0, 0, 65534],
            [
                35, 1, 8, 67108864, 4026531840, 4294967289, 4294967287, 0, 0, 32767,
            ],
            [
                35, 1, 8, 67108864, 4026531840, 4294967289, 4294967287, 0, 0, 32767,
            ],
            [
                35, 1, 8, 67108864, 4026531840, 4294967289, 4294967287, 0, 0, 32767,
            ],
            [70, 2, 16, 134217728, 0, 4294967282, 4294967278, 0, 0, 65534],
            [70, 2, 16, 134217728, 0, 4294967282, 4294967278, 0, 0, 65534],
            [
                32, 8, 8, 33554432, 134217728, 0, 0, 0, 2147483648, 1073872896,
            ],
            [64, 16, 16, 67108864, 268435456, 0, 0, 0, 0, 2147745792],
            [64, 16, 16, 67108864, 268435456, 0, 0, 0, 0, 2147745792],
            [32, 32, 32, 16777216, 67108864, 4294967294, 36, 32, 0, 8191],
            [32, 32, 32, 16777216, 67108864, 4294967294, 36, 32, 0, 8191],
            [
                32, 32, 32, 33554432, 134217728, 4294967292, 36, 32, 0, 16383,
            ],
            [
                32, 32, 32, 33554432, 134217728, 4294967292, 36, 32, 0, 16383,
            ],
            [
                17, 8, 8, 33554432, 134217728, 1073741820, 18, 8, 0, 3221241855,
            ],
        ];
        for (index, format) in FORMATS.into_iter().enumerate() {
            for (case, (width, height)) in EXTENTS.into_iter().enumerate() {
                assert_eq!(
                    format.allocation_bytes(width, height),
                    NATIVE[index][case],
                    "{format:?}, {width}x{height}"
                );
            }
        }
    }
}
