use crate::{AssetError, native_image::*, surface_format::SurfaceFormat};

fn decode(bytes: &[u8]) -> DecodedNativeImage {
    decode_native_texture(bytes, Some("tga")).unwrap()
}

#[test]
fn png_native_low_bit_palette_uses_post_expand_direct_surface() {
    for (bytes, format, alpha) in [
        (
            include_bytes!("fixtures/palette4-opaque.png").as_slice(),
            SurfaceFormat::B8G8R8,
            [255, 255, 255],
        ),
        (
            include_bytes!("fixtures/palette4-alpha.png").as_slice(),
            SurfaceFormat::A8B8G8R8,
            [0, 173, 255],
        ),
    ] {
        let decoded = decode(bytes);
        assert_eq!((decoded.width, decoded.height), (3, 2));
        assert_eq!(decoded.layout, ImageSurfaceLayout::direct(format));
        assert_eq!(png_surface_layout(bytes).unwrap(), decoded.layout);
        let a = [203, 31, 7, alpha[0]];
        let b = [29, 17, 83, alpha[1]];
        let c = [61, 211, 19, alpha[2]];
        assert_eq!(decoded.rgba, [c, b, a, a, c, b].concat());
    }
}

#[test]
fn png_native_low_bit_gray_transparency_selects_alpha_after_expand() {
    let bytes = include_bytes!("fixtures/gray1-trns.png");
    let decoded = decode(bytes);
    assert_eq!(
        decoded.layout,
        ImageSurfaceLayout::direct(SurfaceFormat::A8L8)
    );
    assert_eq!(png_surface_layout(bytes).unwrap(), decoded.layout);
    let transparent = [0, 0, 0, 0];
    let white = [255, 255, 255, 255];
    assert_eq!(
        decoded.rgba,
        [
            transparent,
            white,
            transparent,
            white,
            white,
            white,
            transparent,
            white,
            transparent,
            transparent,
            transparent,
            transparent,
            white,
            white,
            transparent
        ]
        .concat()
    );
    let gray = decode(include_bytes!("fixtures/gray2.png"));
    assert_eq!(gray.layout, ImageSurfaceLayout::direct(SurfaceFormat::L8));
    assert_eq!(
        gray.rgba,
        [0u8, 85, 170, 255, 85, 255, 170, 85, 0, 170]
            .into_iter()
            .flat_map(|value| [value, value, value, 255])
            .collect::<Vec<_>>()
    );
}

#[test]
fn png_native_8_bit_palette_retains_alpha_and_white_undeclared_entries() {
    let decoded = decode(include_bytes!("fixtures/palette8-alpha.png"));
    assert_eq!(
        decoded.layout,
        ImageSurfaceLayout {
            pixels: SurfaceFormat::P8,
            palette: Some(SurfaceFormat::A8R8G8B8),
        }
    );
    let a = [203, 31, 7, 0];
    let b = [29, 17, 83, 173];
    let c = [61, 211, 19, 255];
    assert_eq!(decoded.rgba, [[255; 4], b, a, a, c, b].concat());
}

#[test]
fn png_native_unexpanded_color_key_transparency_avoids_native_null_read() {
    for bytes in [
        include_bytes!("fixtures/gray8-key.png").as_slice(),
        include_bytes!("fixtures/rgb8-key.png").as_slice(),
    ] {
        assert!(matches!(
            decode_native_image(bytes, None),
            Err(AssetError::InvalidPng(_))
        ));
        assert!(matches!(
            decode_native_texture(bytes, None),
            Err(AssetError::InvalidPng(_))
        ));
        assert!(matches!(
            png_surface_layout(bytes),
            Err(AssetError::InvalidPng(_))
        ));
    }
}

#[test]
fn png_native_interlaced_low_bit_rows_keep_original_order_and_extent() {
    let ordinary = decode(include_bytes!("fixtures/gray1-trns.png"));
    let interlaced = decode(include_bytes!("fixtures/gray1-trns-adam7.png"));
    assert_eq!(interlaced, ordinary);
    assert_eq!(
        image_dimensions(include_bytes!("fixtures/gray1-trns-adam7.png")).unwrap(),
        [5, 3]
    );
}

#[test]
fn png_native_direct_rgba_and_gray_alpha_preserve_stored_samples() {
    let decoded = decode(include_bytes!("fixtures/rgba8-gamma.png"));
    assert_eq!(
        decoded.layout,
        ImageSurfaceLayout::direct(SurfaceFormat::A8B8G8R8)
    );
    assert_eq!(
        decoded.rgba,
        [203, 31, 7, 0, 29, 17, 83, 173, 61, 211, 19, 255]
    );
    let gray = decode(include_bytes!("fixtures/gray-alpha8.png"));
    assert_eq!(gray.layout, ImageSurfaceLayout::direct(SurfaceFormat::A8L8));
    assert_eq!(gray.rgba, [17, 17, 17, 0, 99, 99, 99, 173]);
}

#[test]
fn png_native_checked_headers_and_rows_reject_unpublishable_input() {
    let bytes = include_bytes!("fixtures/gray1-trns.png");
    assert!(png_surface_layout(&bytes[..33]).is_err());
    let mut corrupt = bytes.to_vec();
    corrupt[29] ^= 1;
    assert!(png_surface_layout(&corrupt).is_err());
    assert!(decode_native_texture(&corrupt, None).is_err());
    assert!(decode_native_texture(&bytes[..bytes.len() - 30], None).is_err());
    assert!(png_surface_layout(&bytes[..bytes.len() - 30]).is_err());
    assert!(image_dimensions(&bytes[..bytes.len() - 30]).is_err());
    assert!(decode_native_texture(include_bytes!("fixtures/huge.png"), None).is_err());
}
