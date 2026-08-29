//! Small compatibility facade around HarfRust's maintained shaping API.
//!
//! The original implementation used RustyBuzz's `Face` API.  HarfRust keeps
//! the same HarfBuzz shaping semantics but builds a shaper from `read-fonts`
//! data, so this facade preserves the narrow call surface used by the native
//! SystemFont emulation while avoiding the unmaintained RustyBuzz crate.

pub(super) use harfrust::{Direction, GlyphBuffer, Script, UnicodeBuffer};

pub(super) struct Face<'a> {
    font: harfrust::FontRef<'a>,
}

impl<'a> Face<'a> {
    pub(super) fn from_slice(data: &'a [u8], face_index: u32) -> Option<Self> {
        Some(Self {
            font: harfrust::FontRef::from_index(data, face_index).ok()?,
        })
    }

    pub(super) fn units_per_em(&self) -> i32 {
        harfrust::ShaperData::new(&self.font)
            .shaper(&self.font)
            .build()
            .units_per_em()
    }

    pub(super) fn glyph_index(&self, character: char) -> Option<harfrust::GlyphId> {
        skrifa::MetadataProvider::charmap(&self.font).map(character)
    }

    // HarfRust shapes in font units.  These setters were only used by the
    // old API for raster/hinting callbacks and are intentionally no-ops here;
    // raster sizing remains controlled by the retained point-size scale.
    pub(super) fn set_pixels_per_em(&mut self, _ppem: Option<(u16, u16)>) {}
    pub(super) fn set_points_per_em(&mut self, _ptem: Option<f32>) {}
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
    shaper.shape(buffer, harfrust::ShapeOptions::default().features(features))
}
