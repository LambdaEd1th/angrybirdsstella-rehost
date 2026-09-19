use std::sync::Arc;

use image::Rgba;

use super::*;

fn open_sans() -> Option<Vec<u8>> {
    std::fs::read(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../angry birds stella v1.1.6/Payload/Purple.app/OpenSans-Regular.ttf"),
    )
    .ok()
}

fn binding() -> SystemFontRenderBinding {
    SystemFontRenderBinding {
        label_pool_epoch: 0,
        family: "ArialRoundedMTBold".to_owned(),
        font_data: Arc::from([]),
        face_index: 0,
        fallback_catalog: None,
        size: 40,
        fill_rgba: [0, 0, 0, 255],
        stroke_width: 0,
        stroke_rgba: [0, 0, 0, 255],
        style: 0,
        ascending: 37,
        descending: 8,
        leading: 0,
        label_line_height: 46,
    }
}

fn append_u16(bytes: &mut Vec<u8>, value: u16) {
    bytes.extend_from_slice(&value.to_be_bytes());
}

fn append_u32(bytes: &mut Vec<u8>, value: u32) {
    bytes.extend_from_slice(&value.to_be_bytes());
}

fn synthetic_bitmap_tables(size_glyph: u16, index_glyph: u16) -> (Vec<u8>, Vec<u8>) {
    let mut bdat = Vec::new();
    append_u32(&mut bdat, 0x0002_0000);
    bdat.extend_from_slice(&[
        2,           // height
        3,           // width
        1,           // horizontal bearing X
        2,           // horizontal bearing Y (top bearing)
        4,           // horizontal advance
        0b1010_0000, // first byte-aligned one-bit row
        0b0100_0000, // second row
    ]);

    let mut bloc = Vec::new();
    append_u32(&mut bloc, 0x0002_0000);
    append_u32(&mut bloc, 1); // one BitmapSize
    append_u32(&mut bloc, 56); // IndexSubtableList offset
    append_u32(&mut bloc, 20); // record plus format-1 subtable
    append_u32(&mut bloc, 1); // one index subtable
    append_u32(&mut bloc, 0); // colorRef
    bloc.extend_from_slice(&[0; 24]); // horizontal and vertical line metrics
    append_u16(&mut bloc, size_glyph); // declared start glyph
    append_u16(&mut bloc, size_glyph); // declared end glyph
    bloc.extend_from_slice(&[7, 11, 1, 1]); // differing ppem X/Y, one bit, horizontal
    assert_eq!(bloc.len(), 56);
    append_u16(&mut bloc, index_glyph); // first glyph in subtable
    append_u16(&mut bloc, index_glyph); // last glyph in subtable
    append_u32(&mut bloc, 8); // subtable follows this record
    append_u16(&mut bloc, 1); // index format 1
    append_u16(&mut bloc, 1); // small metrics, byte-aligned bitmap
    append_u32(&mut bloc, 4); // image bytes follow bdat version
    append_u32(&mut bloc, 0); // first glyph offset
    append_u32(&mut bloc, 7); // glyph end offset
    (bdat, bloc)
}

fn synthetic_sfnt(mut tables: Vec<([u8; 4], Vec<u8>)>) -> Vec<u8> {
    tables.sort_unstable_by_key(|(tag, _)| *tag);
    let table_count = u16::try_from(tables.len()).unwrap();
    let directory_len = 12 + tables.len() * 16;
    let mut next_offset = directory_len;
    let records = tables
        .iter()
        .map(|(tag, data)| {
            let offset = next_offset;
            next_offset = (offset + data.len() + 3) & !3;
            (*tag, offset, data.len())
        })
        .collect::<Vec<_>>();

    let power = if table_count == 0 {
        0
    } else {
        u16::BITS - 1 - table_count.leading_zeros()
    };
    let search_range = if table_count == 0 {
        0
    } else {
        (1_u16 << power) * 16
    };
    let mut font = Vec::new();
    append_u32(&mut font, 0x0001_0000);
    append_u16(&mut font, table_count);
    append_u16(&mut font, search_range);
    append_u16(&mut font, u16::try_from(power).unwrap());
    append_u16(&mut font, table_count * 16 - search_range);
    for (tag, offset, len) in &records {
        font.extend_from_slice(tag);
        append_u32(&mut font, 0); // checksum is irrelevant to table reads
        append_u32(&mut font, u32::try_from(*offset).unwrap());
        append_u32(&mut font, u32::try_from(*len).unwrap());
    }
    for ((_, data), (_, offset, _)) in tables.into_iter().zip(records) {
        font.resize(offset, 0);
        font.extend_from_slice(&data);
    }
    font
}

/// Small, self-contained sfnt whose glyph 5 is a 3x2 monochrome Apple
/// `bloc`/`bdat` bitmap. No host or Purple.app font is involved in this
/// regression.
fn synthetic_legacy_bitmap_font() -> Vec<u8> {
    let (bdat, bloc) = synthetic_bitmap_tables(5, 5);
    synthetic_sfnt(vec![(*b"bdat", bdat), (*b"bloc", bloc)])
}

fn synthetic_standard_bitmap_font() -> Vec<u8> {
    let (ebdt, eblc) = synthetic_bitmap_tables(5, 5);
    synthetic_sfnt(vec![(*b"EBDT", ebdt), (*b"EBLC", eblc)])
}

fn synthetic_bitmap_priority_font() -> Vec<u8> {
    // The legacy size declares glyph 5 but its sole index subtable contains
    // glyph 6, while the lower-priority EBDT pair has a valid glyph 5.
    let (bdat, bloc) = synthetic_bitmap_tables(5, 6);
    let (ebdt, eblc) = synthetic_bitmap_tables(5, 5);
    synthetic_sfnt(vec![
        (*b"EBDT", ebdt),
        (*b"EBLC", eblc),
        (*b"bdat", bdat),
        (*b"bloc", bloc),
    ])
}

#[test]
fn system_font_stroke_is_a_closed_centered_vector_outline() {
    let Some(open_sans) = open_sans() else {
        eprintln!("skipping Purple.app font regression: OpenSans-Regular.ttf is unavailable");
        return;
    };
    let font = FontRef::new(&open_sans).unwrap();
    let units_per_em = font
        .metrics(
            skrifa::instance::Size::unscaled(),
            skrifa::instance::LocationRef::default(),
        )
        .units_per_em;
    let unit_scale = 40.0 / f32::from(units_per_em);
    let path = glyph_outline_path(
        &font,
        font.charmap().map('M').unwrap(),
        unit_scale,
        10.0,
        50.0,
    )
    .unwrap();
    assert!(
        path.segments()
            .any(|segment| matches!(segment, tiny_skia::PathSegment::Close))
    );

    let radius = 3.0;
    let stroke = Stroke {
        width: radius * 2.0,
        line_join: LineJoin::Round,
        ..Stroke::default()
    };
    let stroked = path.stroke(&stroke, 1.0).unwrap();
    let fill_bounds = path.compute_tight_bounds().unwrap();
    let stroke_bounds = stroked.compute_tight_bounds().unwrap();
    assert!(stroke_bounds.left() <= fill_bounds.left() - radius + 0.01);
    assert!(stroke_bounds.right() >= fill_bounds.right() + radius - 0.01);
    assert!(stroke_bounds.top() <= fill_bounds.top() - radius + 0.01);
    assert!(stroke_bounds.bottom() >= fill_bounds.bottom() + radius - 0.01);
}

#[test]
fn label_hash_sign_extends_native_aarrggbb_fields() {
    assert_eq!(
        native_system_label_hash(&binding(), "Stella"),
        0x0e93_feaf_8762_57ce
    );
    let mut next_epoch = binding();
    next_epoch.label_pool_epoch = 1;
    assert_ne!(
        system_label_lifetime_key(&binding(), "Stella"),
        system_label_lifetime_key(&next_epoch, "Stella")
    );
}

#[test]
fn label_offset_truncates_anchored_local_coordinates_toward_zero() {
    let command = TextRenderCommand {
        order: 0,
        text: String::new(),
        font: String::new(),
        font_binding: None,
        x: 0.0,
        y: 0.0,
        native_system_origin: Some([100.75, -20.75]),
        scale_x: 1.0,
        scale_y: 1.0,
        angle: 0.0,
        matrix: None,
        position_matrix: None,
        alpha: 1.0,
        horizontal_anchor: String::new(),
        vertical_anchor: String::new(),
        projection_3d: None,
        clip_rect: None,
    };
    assert_eq!(
        native_system_label_offset(&command, 2, 10, 4),
        [-12.75, -5.25]
    );

    let command = TextRenderCommand {
        x: 131.25,
        y: 226.75,
        native_system_origin: Some([10.75, -3.25]),
        matrix: Some([2.0, -3.0, 4.0, 5.0]),
        position_matrix: Some([2.0, 0.0, 0.0, 5.0]),
        ..command
    };
    let [left, top] = native_system_label_offset(&command, 0, 2, 4);
    let transformed = text_glyph_transform(&command, left, top);
    // The native order is trunc(10.75-2, -3.25-4) = (8,-7), followed by
    // axis scale and translation: (125.75,208). GL_Image uses the full matrix
    // only for the cached label quad, so its rotation/shear remains intact.
    assert_eq!([transformed.x, transformed.y], [125.75, 208.0]);
    assert_eq!(
        (
            transformed.m00,
            transformed.m01,
            transformed.m10,
            transformed.m11
        ),
        (2.0, -3.0, 4.0, 5.0)
    );
}

#[test]
fn vertical_anchor_uses_native_wrapping_add_and_signed_half() {
    let mut font = binding();
    font.ascending = i32::MAX;
    font.descending = 2;
    assert_eq!(native_system_label_vertical_anchor(&font, "TOP"), 0);
    assert_eq!(
        native_system_label_vertical_anchor(&font, "VCENTER"),
        -1_073_741_823
    );
    assert_eq!(
        native_system_label_vertical_anchor(&font, "BOTTOM"),
        -2_147_483_647
    );
    assert_eq!(
        native_system_label_vertical_anchor(&font, "BASELINE"),
        i32::MAX
    );
    assert_eq!(native_system_label_vertical_anchor(&font, "VPIVOT"), 0);
}

#[test]
fn label_pool_uses_five_mib_fifo_without_hit_reordering() {
    let mut pool = SystemLabelPool::default();
    assert!(pool.enter_epoch(7).is_empty());
    let image = || RgbaImage::new(512, 1024); // 2 MiB
    let (first, retired) = pool.insert(7, 1, image()).unwrap();
    assert!(retired.is_empty());
    let (_, retired) = pool.insert(7, 2, image()).unwrap();
    assert!(retired.is_empty());

    // A hit must not promote hash 1. The third insertion reaches the
    // native limit exactly and therefore still evicts nothing.
    assert_eq!(pool.get(1).unwrap().texture_key, first.texture_key);
    let (_, retired) = pool.insert(7, 3, RgbaImage::new(512, 512)).unwrap();
    assert!(retired.is_empty());
    assert_eq!(pool.bytes, LABEL_POOL_BYTE_LIMIT);

    // Four more bytes exceed 0x500000, so end()-1 is hash 1 even though it
    // was just hit. Hash 2 would be selected by an LRU implementation.
    let (_, retired) = pool.insert(7, 4, RgbaImage::new(1, 1)).unwrap();
    assert_eq!(retired.as_slice(), std::slice::from_ref(&first.texture_key));
    assert!(pool.get(1).is_none());
    assert!(pool.get(2).is_some());
    assert!(pool.get(3).is_some());
    assert!(pool.get(4).is_some());

    // Re-insertion gets a distinct deferred-wgpu identity so a draw that
    // used the evicted texture earlier in this frame cannot be rebound.
    let (reinserted, _) = pool.insert(7, 1, image()).unwrap();
    assert_ne!(reinserted.texture_key, first.texture_key);
}

#[test]
fn label_pool_epoch_clear_retires_all_cached_labels_and_resets_bytes() {
    let mut pool = SystemLabelPool::default();
    pool.enter_epoch(10);
    let (old, _) = pool.insert(10, 42, RgbaImage::new(16, 16)).unwrap();
    let retired = pool.enter_epoch(11);
    assert_eq!(retired.as_slice(), std::slice::from_ref(&old.texture_key));
    assert_eq!(pool.bytes, 0);
    assert!(pool.newest_first.is_empty());
    assert!(pool.labels.is_empty());

    let (new, _) = pool.insert(11, 42, RgbaImage::new(16, 16)).unwrap();
    assert_ne!(new.texture_key, old.texture_key);
}

#[test]
fn label_larger_than_native_pool_limit_is_rejected_safely() {
    let mut pool = SystemLabelPool::default();
    pool.enter_epoch(0);
    let result = pool.insert(0, 1, RgbaImage::new(1281, 1024));
    assert!(result.is_err());
    assert_eq!(pool.bytes, 0);
    assert!(pool.labels.is_empty());
}

#[test]
fn embedded_bitmap_coverage_decodes_padded_rows_and_bit_depths() {
    let mono = decode_system_coverage(3, 2, &[0b1010_0000, 0b0100_0000], 1, true).unwrap();
    assert_eq!(
        mono.pixels().map(|pixel| pixel[3]).collect::<Vec<_>>(),
        [255, 0, 255, 0, 255, 0]
    );

    let gray = decode_system_coverage(4, 1, &[0b00_01_10_11], 2, false).unwrap();
    assert_eq!(
        gray.pixels().map(|pixel| pixel[3]).collect::<Vec<_>>(),
        [0, 85, 170, 255]
    );
}

#[test]
fn legacy_bloc_bdat_font_uses_raw_fontations_tables_and_preserves_bdt_origin() {
    let data = synthetic_legacy_bitmap_font();
    let face = FontRef::new(&data).unwrap();
    // This is the exact migration gap: Skrifa's standard strike facade does
    // not alias Apple's legacy tags.
    assert!(face.bitmap_strikes().is_empty());

    let glyph = raster::native_system_bitmap_glyph(
        &face,
        skrifa::instance::Size::new(7.0),
        skrifa::GlyphId::new(5),
    )
    .unwrap();
    assert_eq!((glyph.width, glyph.height), (3, 2));
    assert_eq!((glyph.inner_bearing_x, glyph.inner_bearing_y), (1.0, 2.0));
    // The pre-migration payload selected and scaled BDT strikes with ppemX.
    assert_eq!((glyph.ppem_x, glyph.ppem_y), (7.0, 7.0));

    let decoded = decode_system_raster(glyph, false, None).unwrap();
    assert_eq!(decoded.color, NativeSystemRasterColor::Foreground);
    assert_eq!((decoded.x, decoded.y, decoded.pixels_per_em), (1, 0, 7));
    assert_eq!(
        decoded
            .image
            .pixels()
            .map(|pixel| pixel[3])
            .collect::<Vec<_>>(),
        [255, 0, 255, 0, 255, 0]
    );

    // The same ppem-axis and origin normalization also restores the retired
    // parser contract for standardized EBDT/EBLC tables after the migration.
    let data = synthetic_standard_bitmap_font();
    let face = FontRef::new(&data).unwrap();
    assert_eq!(
        face.bitmap_strikes().format(),
        Some(skrifa::bitmap::BitmapFormat::Ebdt)
    );
    let glyph = raster::native_system_bitmap_glyph(
        &face,
        skrifa::instance::Size::new(7.0),
        skrifa::GlyphId::new(5),
    )
    .unwrap();
    assert_eq!((glyph.ppem_x, glyph.ppem_y), (7.0, 7.0));
    let decoded = decode_system_raster(glyph, false, None).unwrap();
    assert_eq!((decoded.x, decoded.y, decoded.pixels_per_em), (1, 0, 7));
}

#[test]
fn legacy_bitmap_pair_is_terminal_after_selected_strike_misses_glyph() {
    let data = synthetic_bitmap_priority_font();
    let face = FontRef::new(&data).unwrap();
    assert_eq!(
        face.bitmap_strikes().format(),
        Some(skrifa::bitmap::BitmapFormat::Ebdt)
    );

    // This intentionally differs from Skrifa's default strike facade. The
    // former parser selected the higher-priority bdat size from its declared
    // glyph range and returned its failed sparse lookup directly, rather than
    // falling through to the valid lower-priority EBDT glyph.
    assert!(
        raster::native_system_bitmap_glyph(
            &face,
            skrifa::instance::Size::new(7.0),
            skrifa::GlyphId::new(5),
        )
        .is_none()
    );
}

#[test]
fn premultiplied_bgra_bitmap_is_unpremultiplied_before_sampling() {
    let image = decode_system_bgra32(1, 2, &[25, 50, 100, 128, 0, 0, 0, 0]).unwrap();
    assert_eq!(image.get_pixel(0, 0).0, [199, 99, 49, 128]);
    assert_eq!(image.get_pixel(0, 1).0, [0, 0, 0, 0]);
}

#[test]
fn intrinsic_bitmap_is_source_over_composited_once_per_native_text_pass() {
    let placed = NativeSystemPlacedRaster {
        raster: Arc::new(NativeSystemDecodedRaster {
            image: RgbaImage::from_pixel(1, 1, Rgba([200, 100, 50, 128])),
            color: NativeSystemRasterColor::Intrinsic,
            x: 0,
            y: 0,
            pixels_per_em: 1,
            sbix: true,
            glyph_bbox: None,
        }),
        left: 0.0,
        top: 0.0,
        scale: 1.0,
    };
    let mut image = RgbaImage::new(1, 1);
    composite_system_rasters(&mut image, std::slice::from_ref(&placed), [1, 2, 3, 4]);
    assert_eq!(image.get_pixel(0, 0).0, [100, 50, 25, 128]);
    composite_system_rasters(&mut image, &[placed], [5, 6, 7, 8]);
    assert_eq!(image.get_pixel(0, 0).0, [150, 75, 38, 192]);
}

#[test]
#[cfg(target_os = "macos")]
fn apple_sbix_png_rasterizes_in_intrinsic_color_without_an_outline_square() {
    use std::collections::HashSet;

    let Ok(data) = std::fs::read("/System/Library/Fonts/Apple Color Emoji.ttc") else {
        return;
    };
    use skrifa::instance::Size;
    use skrifa::{FontRef, GlyphId, MetadataProvider};
    let face = FontRef::from_index(&data, 0).unwrap();
    let glyph = face.charmap().map('\u{1F600}').unwrap();
    let strikes = face.bitmap_strikes();
    let raster = strikes
        .glyph_for_size(Size::new(40.0), GlyphId::new(glyph.to_u32()))
        .unwrap();
    let decoded = decode_system_raster(
        raster,
        strikes.format() == Some(skrifa::bitmap::BitmapFormat::Sbix),
        None,
    )
    .unwrap();
    assert_eq!(decoded.color, NativeSystemRasterColor::Intrinsic);
    assert_eq!(decoded.image.dimensions(), (40, 40));
    assert_eq!(decoded.pixels_per_em, 40);

    let binding = SystemFontRenderBinding {
        label_pool_epoch: 0,
        family: "AppleColorEmoji".to_owned(),
        font_data: Arc::from(data),
        face_index: 0,
        fallback_catalog: None,
        size: 40,
        fill_rgba: [255, 0, 255, 255],
        stroke_width: 0,
        stroke_rgba: [0, 255, 0, 255],
        style: 0,
        ascending: 40,
        descending: 12,
        leading: 0,
        label_line_height: 52,
    };
    let label = rasterize_system_label(&binding, "\u{1F600}", "LEFT", "TOP")
        .unwrap()
        .unwrap();
    assert_eq!(label.image.dimensions(), (40, 52));
    let colors = label
        .image
        .pixels()
        .filter(|pixel| pixel[3] != 0)
        .map(|pixel| [pixel[0], pixel[1], pixel[2]])
        .collect::<HashSet<_>>();
    assert!(colors.len() > 32);
    assert!(!colors.contains(&binding.fill_rgba[..3]));
}
