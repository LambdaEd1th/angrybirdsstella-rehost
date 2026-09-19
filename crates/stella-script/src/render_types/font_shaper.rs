//! Small compatibility facade around HarfRust's maintained shaping API.
//!
//! The original implementation used RustyBuzz's `Face` API.  HarfRust keeps
//! the same HarfBuzz shaping semantics but builds a shaper from `read-fonts`
//! data, so this facade preserves the narrow call surface used by the native
//! SystemFont emulation while avoiding the unmaintained RustyBuzz crate.

pub(super) use harfrust::{Direction, GlyphBuffer, Script, UnicodeBuffer};

pub(super) struct Face<'a> {
    font: harfrust::FontRef<'a>,
    metadata: skrifa::FontRef<'a>,
    points_per_em: Option<f32>,
}

impl<'a> Face<'a> {
    pub(super) fn from_slice(data: &'a [u8], face_index: u32) -> Option<Self> {
        Some(Self {
            font: harfrust::FontRef::from_index(data, face_index).ok()?,
            metadata: skrifa::FontRef::from_index(data, face_index).ok()?,
            points_per_em: None,
        })
    }

    pub(super) fn units_per_em(&self) -> i32 {
        harfrust::ShaperData::new(&self.font)
            .shaper(&self.font)
            .build()
            .units_per_em()
    }

    pub(super) fn glyph_index(&self, character: char) -> Option<harfrust::GlyphId> {
        // Each library can resolve a different read-fonts version. Share the
        // bytes, keeping its font reference and glyph ID in its own API.
        skrifa::MetadataProvider::charmap(&self.metadata)
            .map(character)
            .map(|glyph| harfrust::GlyphId::new(glyph.to_u32()))
    }

    // HarfRust shapes in font units and does not expose an independent ppem
    // input on this path. Point size is still significant, however: AAT
    // `trak` interpolation consumes it through `ShapeOptions::point_size`.
    pub(super) fn set_pixels_per_em(&mut self, _ppem: Option<(u16, u16)>) {}
    pub(super) fn set_points_per_em(&mut self, ptem: Option<f32>) {
        self.points_per_em = ptem;
    }
}

pub(super) fn shape(
    face: &Face<'_>,
    features: &[harfrust::Feature],
    buffer: UnicodeBuffer,
) -> GlyphBuffer {
    // Build the cache and shaper together so their lifetimes remain valid for
    // the returned glyph buffer.  HarfRust's output is detached from them.
    let data = harfrust::ShaperData::new(&face.font);
    let shaper = data.shaper(&face.font).build();
    shaper.shape(
        buffer,
        harfrust::ShapeOptions::default()
            .features(features)
            .point_size(face.points_per_em),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn point_size_reaches_aat_tracking_without_a_system_font() {
        let font_data = aat_tracking_test_font();
        let mut face = Face::from_slice(&font_data, 0).unwrap();

        let advance_at = |face: &Face<'_>| {
            let mut buffer = UnicodeBuffer::new();
            buffer.push_str("A");
            buffer.guess_segment_properties();
            shape(face, &[], buffer).glyph_positions()[0].x_advance
        };

        face.set_points_per_em(Some(12.0));
        assert_eq!(advance_at(&face), 120);
        face.set_points_per_em(Some(24.0));
        assert_eq!(advance_at(&face), 240);
    }

    fn aat_tracking_test_font() -> Vec<u8> {
        const STAT_OFFSET: u32 = 44;
        const STAT_LENGTH: u32 = 18;
        const TRAK_OFFSET: u32 = 64;
        const TRAK_LENGTH: u32 = 40;

        let mut font = Vec::with_capacity((TRAK_OFFSET + TRAK_LENGTH) as usize);
        push_u32(&mut font, 0x0001_0000); // TrueType sfnt version.
        push_u16(&mut font, 2); // Number of tables.
        push_u16(&mut font, 32); // searchRange.
        push_u16(&mut font, 1); // entrySelector.
        push_u16(&mut font, 0); // rangeShift.

        push_table_record(&mut font, *b"STAT", STAT_OFFSET, STAT_LENGTH);
        push_table_record(&mut font, *b"trak", TRAK_OFFSET, TRAK_LENGTH);

        // Minimal STAT 1.0 table. Its presence enables modern-font tracking.
        push_u32(&mut font, 0x0001_0000);
        push_u16(&mut font, 8); // designAxisSize.
        push_u16(&mut font, 0); // designAxisCount.
        push_u32(&mut font, 0); // designAxesOffset.
        push_u16(&mut font, 0); // axisValueCount.
        push_u32(&mut font, 0); // offsetToAxisValueOffsets.
        font.resize(TRAK_OFFSET as usize, 0);

        // AAT trak 1.0 with one neutral track sampled at 12pt and 24pt.
        push_u32(&mut font, 0x0001_0000);
        push_u16(&mut font, 0); // format.
        push_u16(&mut font, 12); // horizontal TrackData offset.
        push_u16(&mut font, 0); // no vertical TrackData.
        push_u16(&mut font, 0); // reserved.
        push_u16(&mut font, 1); // nTracks.
        push_u16(&mut font, 2); // nSizes.
        push_u32(&mut font, 28); // size table offset.
        push_u32(&mut font, 0); // neutral track value.
        push_u16(&mut font, 256); // name table index.
        push_u16(&mut font, 36); // per-size values offset.
        push_u32(&mut font, 12 << 16);
        push_u32(&mut font, 24 << 16);
        push_i16(&mut font, 120);
        push_i16(&mut font, 240);
        font
    }

    fn push_table_record(font: &mut Vec<u8>, tag: [u8; 4], offset: u32, length: u32) {
        font.extend_from_slice(&tag);
        push_u32(font, 0); // Checksum is irrelevant to in-memory parsing.
        push_u32(font, offset);
        push_u32(font, length);
    }

    fn push_u16(font: &mut Vec<u8>, value: u16) {
        font.extend_from_slice(&value.to_be_bytes());
    }

    fn push_i16(font: &mut Vec<u8>, value: i16) {
        font.extend_from_slice(&value.to_be_bytes());
    }

    fn push_u32(font: &mut Vec<u8>, value: u32) {
        font.extend_from_slice(&value.to_be_bytes());
    }
}
