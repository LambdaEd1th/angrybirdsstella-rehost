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

#[test]
fn system_font_stroke_is_a_closed_centered_vector_outline() {
    let Some(open_sans) = open_sans() else {
        eprintln!("skipping Purple.app font regression: OpenSans-Regular.ttf is unavailable");
        return;
    };
    let font = FontRef::try_from_slice(&open_sans).unwrap();
    let units_per_em = font.units_per_em().unwrap();
    let unit_scale = 40.0 / units_per_em;
    let path = glyph_outline_path(&font, font.glyph_id('M'), unit_scale, 10.0, 50.0).unwrap();
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
        ..command
    };
    let [left, top] = native_system_label_offset(&command, 0, 2, 4);
    let transformed = text_glyph_transform(&command, left, top);
    // The native order is trunc(10.75-2, -3.25-4) = (8,-7), followed by
    // the matrix and translation: (137,197). Truncating screen space or
    // transforming the fractional anchored point produces other values.
    assert_eq!([transformed.x, transformed.y], [137.0, 197.0]);
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
    let face = ttf_parser::Face::parse(&data, 0).unwrap();
    let glyph = face.glyph_index('\u{1F600}').unwrap();
    let raster = face.glyph_raster_image(glyph, 40).unwrap();
    let decoded = decode_system_raster(
        raster,
        face.tables().sbix.is_some(),
        face.glyph_bounding_box(glyph),
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
