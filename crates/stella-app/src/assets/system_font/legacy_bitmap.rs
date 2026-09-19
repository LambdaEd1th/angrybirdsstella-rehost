//! Embedded BDT tables under the parser contract used before the migration.
//!
//! Fontations intentionally exposes the standardized `EBLC`/`EBDT` and
//! `CBLC`/`CBDT` pairs, but not the older Apple table tags.  Their binary
//! layout is shared with `bloc`/`bdat`, so the maintained `read-fonts` table
//! readers re-exported by Skrifa can parse all three raw pairs without
//! bringing the retired `ttf-parser` dependency back. Doing selection here
//! also preserves the old parser's important early-return behaviour: one
//! declared strike is selected before its index subtable is queried.

use skrifa::bitmap::{BitmapData, BitmapGlyph, MaskData, Origin};
use skrifa::instance::Size;
use skrifa::raw::{
    FontRead,
    tables::{
        bitmap::{
            BitmapContent, BitmapData as RawBitmapData, BitmapDataFormat, BitmapMetrics, BitmapSize,
        },
        cbdt::Cbdt,
        cblc::Cblc,
    },
};
use skrifa::{FontRef, GlyphId, Tag};

/// Parsed bitmap data/location pair retained over the face's backing bytes.
pub(super) struct NativeBdtStrikes<'a> {
    locations: Cblc<'a>,
    data: Cbdt<'a>,
}

impl<'a> NativeBdtStrikes<'a> {
    pub(super) fn legacy(font: &FontRef<'a>) -> Option<Self> {
        Self::from_tags(font, Tag::new(b"bloc"), Tag::new(b"bdat"))
    }

    pub(super) fn ebdt(font: &FontRef<'a>) -> Option<Self> {
        Self::from_tags(font, Tag::new(b"EBLC"), Tag::new(b"EBDT"))
    }

    pub(super) fn cbdt(font: &FontRef<'a>) -> Option<Self> {
        Self::from_tags(font, Tag::new(b"CBLC"), Tag::new(b"CBDT"))
    }

    fn from_tags(font: &FontRef<'a>, location_tag: Tag, data_tag: Tag) -> Option<Self> {
        // The former parser deliberately reused its CBLC/CBDT implementation
        // for all three tag pairs. Parse the raw payloads the same way. This
        // is required for the Apple tags, which TableProvider does not alias,
        // and retains its accepted bitmap-image formats for EBDT as well.
        let locations = Cblc::read(font.table_data(location_tag)?).ok()?;
        let data = Cbdt::read(font.table_data(data_tag)?).ok()?;
        Some(Self { locations, data })
    }

    /// Matches the former parser's two-stage lookup: select exactly one size
    /// by its declared glyph range and horizontal ppem, then query that size's
    /// index subtable. A sparse/malformed selected strike must not fall through
    /// to another size or bitmap table pair.
    pub(super) fn glyph_for_size(&self, size: Size, glyph_id: GlyphId) -> Option<BitmapGlyph<'a>> {
        let requested = size.ppem().unwrap_or(f32::MAX);
        let mut best = None::<&BitmapSize>;
        let mut best_ppem = 0.0;
        for bitmap_size in self.locations.bitmap_sizes() {
            let glyph_id = glyph_id.to_u32();
            if !(bitmap_size.start_glyph_index().to_u32()..=bitmap_size.end_glyph_index().to_u32())
                .contains(&glyph_id)
            {
                continue;
            }
            let strike_size = f32::from(bitmap_size.ppem_x());
            if (requested <= strike_size && strike_size < best_ppem)
                || (requested > best_ppem && strike_size > best_ppem)
            {
                best = Some(bitmap_size);
                best_ppem = strike_size;
            }
        }
        self.glyph(best?, glyph_id)
    }

    fn glyph(&self, size: &BitmapSize, glyph_id: GlyphId) -> Option<BitmapGlyph<'a>> {
        let location = size.location(self.locations.offset_data(), glyph_id).ok()?;
        let data = self.data.data(&location).ok()?;
        legacy_bitmap_glyph(size, &data)
    }
}

fn legacy_bitmap_glyph<'a>(size: &BitmapSize, data: &RawBitmapData<'a>) -> Option<BitmapGlyph<'a>> {
    let (inner_bearing_x, inner_bearing_y, advance, width, height) = match &data.metrics {
        BitmapMetrics::Small(metrics) => (
            f32::from(metrics.bearing_x()),
            f32::from(metrics.bearing_y()),
            f32::from(metrics.advance()),
            u32::from(metrics.width()),
            u32::from(metrics.height()),
        ),
        BitmapMetrics::Big(metrics) => (
            f32::from(metrics.hori_bearing_x()),
            f32::from(metrics.hori_bearing_y()),
            f32::from(metrics.hori_advance()),
            u32::from(metrics.width()),
            u32::from(metrics.height()),
        ),
    };
    let bit_depth = size.bit_depth();
    let bitmap = match &data.content {
        BitmapContent::Data(BitmapDataFormat::Png, bytes) => BitmapData::Png(bytes),
        BitmapContent::Data(
            BitmapDataFormat::ByteAligned | BitmapDataFormat::BitAligned,
            bytes,
        ) if bit_depth == 32 => BitmapData::Bgra(bytes),
        BitmapContent::Data(BitmapDataFormat::ByteAligned, bytes)
            if matches!(bit_depth, 1 | 2 | 4 | 8) =>
        {
            BitmapData::Mask(MaskData {
                bpp: bit_depth,
                is_packed: false,
                data: bytes,
            })
        }
        BitmapContent::Data(BitmapDataFormat::BitAligned, bytes)
            if matches!(bit_depth, 1 | 2 | 4 | 8) =>
        {
            BitmapData::Mask(MaskData {
                bpp: bit_depth,
                is_packed: true,
                data: bytes,
            })
        }
        // The retired parser did not expose composite formats 8/9 either.
        BitmapContent::Data(_, _) | BitmapContent::Composite(_) => return None,
    };
    Some(BitmapGlyph {
        data: bitmap,
        bearing_x: 0.0,
        bearing_y: 0.0,
        inner_bearing_x,
        inner_bearing_y,
        ppem_x: f32::from(size.ppem_x()),
        // The retired RasterGlyphImage exposed one `pixels_per_em` value,
        // sourced from ppemX. Keep both axes normalized to that value at this
        // compatibility boundary so retained scaling remains unchanged.
        ppem_y: f32::from(size.ppem_x()),
        advance: Some(advance),
        width,
        height,
        placement_origin: Origin::TopLeft,
    })
}
