//! Empty native resource constructors, including unread foreign-root tails.

use super::*;

#[test]
fn empty_ka3d_or_unrelated_chunks_publish_empty_sheet_and_font_without_images() {
    for body in [Vec::new(), test_chunk(b"JUNK", b"ignored")] {
        let bytes = test_container(b"KA3D", &body);
        let mut images = Vec::new();
        let sheet = SpriteSheet::parse_with_image_loader::<crate::AssetError>(&bytes, |name| {
            images.push(name.to_owned());
            Ok(())
        })
        .unwrap();
        let font = BitmapFont::parse_with_image_loader::<crate::AssetError>(&bytes, |name| {
            images.push(name.to_owned());
            Ok(())
        })
        .unwrap();
        assert!(sheet.textures.is_empty() && sheet.sprites.is_empty());
        assert!(font.texture.is_empty() && font.glyphs.is_empty());
        assert!(images.is_empty());
        assert_eq!(font.native_string_width("").unwrap(), 0);
        assert_eq!(font.native_string_height("unknown").unwrap(), 0);
        assert_eq!(font.native_max_ascending(), 0);
        assert_eq!(font.native_max_descending(), 0);
        assert!(font.native_string_width("unknown").is_err());
        assert!(font.native_leading().is_err());
        assert!(font.native_tracking().is_err());
        assert_eq!(font.native_string_width("x").unwrap(), 0);
        assert_eq!(
            font.native_draw_anchor("x", "RIGHT", "TOP").unwrap(),
            [0, 0]
        );
        assert_eq!(
            font.native_draw_anchor("x", "HCENTER", "TOP").unwrap(),
            [0, 0]
        );
        assert_eq!(
            font.native_draw_anchor("unknown", "LEFT", "TOP").unwrap(),
            [0, 0]
        );
        assert!(font.native_draw_anchor("unknown", "RIGHT", "TOP").is_err());
    }
}

#[test]
fn foreign_root_returns_empty_before_reading_length_or_tail() {
    for bytes in [
        b"NOPE".to_vec(),
        b"RVIObad".to_vec(),
        test_container_with_len(b"NOPE", u32::MAX, b"SPRT"),
    ] {
        let sheet = SpriteSheet::parse(&bytes).unwrap();
        let font = BitmapFont::parse(&bytes).unwrap();
        assert!(sheet.sprites.is_empty() && sheet.textures.is_empty());
        assert!(font.glyphs.is_empty() && font.texture.is_empty());
        assert_eq!(font.native_string_width("x").unwrap(), 0);
        assert!(font.native_string_width("xx").is_err());
    }
}

#[test]
fn empty_resource_acceptance_does_not_bypass_physical_ka3d_errors() {
    for bytes in [
        b"K".to_vec(),
        b"KA3D".to_vec(),
        test_container_with_len(b"KA3D", 1, b""),
        test_container(b"KA3D", b"J"),
        test_container(b"KA3D", &test_chunk_with_len(b"JUNK", 2, b"x")),
    ] {
        assert!(SpriteSheet::parse(&bytes).is_err(), "{bytes:?}");
        assert!(BitmapFont::parse(&bytes).is_err(), "{bytes:?}");
    }
}

#[test]
fn unsupported_font_version_leaves_spacing_uninitialized() {
    let bytes = test_container(b"KA3D", &test_chunk(b"FONT", &3_u16.to_be_bytes()));
    let font = BitmapFont::parse(&bytes).unwrap();
    assert!(font.texture.is_empty() && font.glyphs.is_empty());
    assert_eq!(font.native_string_width("").unwrap(), 0);
    assert_eq!(font.native_string_width("x").unwrap(), 0);
    assert!(font.native_string_width("xx").is_err());
    assert!(font.native_draw_anchor("xx", "HCENTER", "TOP").is_err());
}

#[test]
fn zero_glyph_font_still_loads_image_and_initializes_spacing() {
    let mut payload = 1_u16.to_be_bytes().to_vec();
    payload.extend(test_string("required.pvr"));
    for value in [-4_i16, 7, 0] {
        payload.extend(value.to_be_bytes());
    }
    let bytes = test_container(b"KA3D", &test_chunk(b"FONT", &payload));
    let mut images = Vec::new();
    let font = BitmapFont::parse_with_image_loader::<crate::AssetError>(&bytes, |name| {
        images.push(name.to_owned());
        Ok(())
    })
    .unwrap();
    assert_eq!(images, ["required.pvr"]);
    assert_eq!((font.leading, font.tracking), (-4, 7));
    assert_eq!(font.native_leading().unwrap(), -4);
    assert_eq!(font.native_tracking().unwrap(), 7);
    assert!(font.glyphs.is_empty());
    assert_eq!(font.native_string_width("???").unwrap(), 14);
    assert_eq!(
        font.native_draw_anchor("???", "RIGHT", "TOP").unwrap(),
        [-14, 0]
    );
}

#[test]
fn zero_sprite_sheet_still_loads_its_image() {
    let mut payload = 1_u16.to_be_bytes().to_vec();
    payload.extend(test_string("required.pvr"));
    payload.extend(0_u16.to_be_bytes());
    let bytes = test_container(b"KA3D", &test_chunk(b"SPRT", &payload));
    let mut images = Vec::new();
    let sheet = SpriteSheet::parse_with_image_loader::<crate::AssetError>(&bytes, |name| {
        images.push(name.to_owned());
        Ok(())
    })
    .unwrap();
    assert_eq!(images, ["required.pvr"]);
    assert_eq!(sheet.textures, ["required.pvr"]);
    assert!(sheet.sprites.is_empty());
}
