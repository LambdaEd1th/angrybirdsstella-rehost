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
}
