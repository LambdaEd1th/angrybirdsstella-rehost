use super::*;

fn test_string(value: &str) -> Vec<u8> {
    let mut bytes = (value.len() as u16).to_be_bytes().to_vec();
    bytes.extend_from_slice(value.as_bytes());
    bytes
}

fn test_chunk_with_len(tag: &[u8; 4], declared_len: u32, payload: &[u8]) -> Vec<u8> {
    let mut bytes = tag.to_vec();
    bytes.extend_from_slice(&declared_len.to_be_bytes());
    bytes.extend_from_slice(payload);
    bytes
}

fn test_chunk(tag: &[u8; 4], payload: &[u8]) -> Vec<u8> {
    test_chunk_with_len(tag, payload.len() as u32, payload)
}

fn test_container_with_len(tag: &[u8; 4], declared_len: u32, body: &[u8]) -> Vec<u8> {
    let mut bytes = tag.to_vec();
    bytes.extend_from_slice(&declared_len.to_be_bytes());
    bytes.extend_from_slice(body);
    bytes
}

fn test_container(tag: &[u8; 4], body: &[u8]) -> Vec<u8> {
    test_container_with_len(tag, body.len() as u32, body)
}

fn test_sprt_payload(texture: &str, name: &str, x: i16) -> Vec<u8> {
    let mut payload = 1u16.to_be_bytes().to_vec();
    payload.extend(test_string(texture));
    payload.extend_from_slice(&1u16.to_be_bytes());
    payload.extend(test_string(name));
    for value in [x, 2, 3, 4, 5, 6] {
        payload.extend_from_slice(&value.to_be_bytes());
    }
    payload
}

#[test]
fn parses_nested_envelope() {
    let mut bytes = b"KA3D".to_vec();
    bytes.extend_from_slice(&12u32.to_be_bytes());
    bytes.extend_from_slice(b"SPRT");
    bytes.extend_from_slice(&4u32.to_be_bytes());
    bytes.extend_from_slice(b"data");
    let parsed = Ka3dEnvelope::parse(&bytes).unwrap();
    assert_eq!(parsed.container_type_str(), "KA3D");
    assert_eq!(parsed.resource_type_str(), "SPRT");
    assert_eq!(parsed.payload, b"data");
}

#[test]
fn envelope_lengths_are_native_upper_bounds_and_first_chunk_is_bounded() {
    let mut body = test_chunk(b"JUNK", b"abc");
    body.extend(test_chunk(b"NEXT", b"tail"));
    let bytes = test_container_with_len(b"KA3D", 0, &body);
    let parsed = Ka3dEnvelope::parse(&bytes).unwrap();
    assert_eq!(parsed.resource_type, *b"JUNK");
    assert_eq!(parsed.payload, b"abc");
    let next = Ka3dEnvelope::find(&bytes, b"NEXT").unwrap();
    assert_eq!(next.payload, b"tail");

    let malformed = test_container_with_len(b"KA3D", body.len() as u32 + 1, &body);
    assert!(Ka3dEnvelope::parse(&malformed).is_err());
}

#[test]
fn parses_sprite_sheet() {
    let mut payload = Vec::new();
    payload.extend_from_slice(&1u16.to_be_bytes());
    payload.extend_from_slice(&7u16.to_be_bytes());
    payload.extend_from_slice(b"one.pvr");
    payload.extend_from_slice(&1u16.to_be_bytes());
    payload.extend_from_slice(&4u16.to_be_bytes());
    payload.extend_from_slice(b"BIRD");
    for value in [-10i16, -20, 30, 40, 15, 22] {
        payload.extend_from_slice(&value.to_be_bytes());
    }
    let mut bytes = b"KA3D".to_vec();
    bytes.extend_from_slice(&(payload.len() as u32 + 8).to_be_bytes());
    bytes.extend_from_slice(b"SPRT");
    bytes.extend_from_slice(&(payload.len() as u32).to_be_bytes());
    bytes.extend_from_slice(&payload);
    let sheet = SpriteSheet::parse(&bytes).unwrap();
    assert_eq!(sheet.textures, ["one.pvr"]);
    assert_eq!(sheet.sprites[0].name, "BIRD");
    assert_eq!((sheet.sprites[0].x, sheet.sprites[0].y), (-10, -20));
    assert_eq!(sheet.sprites[0].width, 30);
    assert_eq!(sheet.sprites[0].pivot_y, 22);
    assert_eq!(sheet.sprites[0].atlas_rotation, 0);
}

#[test]
fn sprite_loader_skips_unknown_chunks_and_overwrites_duplicate_map_entries() {
    let mut body = test_chunk(b"JUNK", b"abc");
    let first = test_sprt_payload("first.pvr", "DUP", 10);
    body.extend(test_chunk_with_len(b"SPRT", 0, &first));
    let second = test_sprt_payload("second.pvr", "DUP", 20);
    body.extend(test_chunk_with_len(b"SPRT", 1, &second));
    let bytes = test_container_with_len(b"KA3D", 0, &body);

    let sheet = SpriteSheet::parse(&bytes).unwrap();
    assert_eq!(sheet.textures, ["first.pvr", "second.pvr"]);
    assert_eq!(sheet.sprites.len(), 1);
    assert_eq!(sheet.sprites[0].x, 20);
    assert_eq!(sheet.texture_for(&sheet.sprites[0]), Some("second.pvr"));
}

#[test]
fn unknown_chunk_skip_rejects_physical_truncation() {
    let body = test_chunk_with_len(b"JUNK", 4, b"x");
    let bytes = test_container_with_len(b"KA3D", 0, &body);
    assert!(SpriteSheet::parse(&bytes).is_err());
}

#[test]
fn sprite_native_atlas_corners_match_all_constructor_rotation_branches() {
    let mut sprite = SpriteRegion {
        name: "ROTATED".to_owned(),
        x: -4,
        y: -8,
        width: 10,
        height: 20,
        pivot_x: 3,
        pivot_y: 7,
        atlas_rotation: 0,
    };
    assert_eq!(
        sprite.native_atlas_corners(),
        [[-4.0, -8.0], [6.0, -8.0], [-4.0, 12.0], [6.0, 12.0]]
    );
    sprite.atlas_rotation = 1;
    assert_eq!(
        sprite.native_atlas_corners(),
        [[16.0, -8.0], [16.0, 2.0], [-4.0, -8.0], [-4.0, 2.0]]
    );
    sprite.atlas_rotation = 2;
    assert_eq!(
        sprite.native_atlas_corners(),
        [[6.0, -8.0], [-4.0, -8.0], [6.0, 12.0], [-4.0, 12.0]]
    );
    sprite.atlas_rotation = 3;
    assert_eq!(
        sprite.native_atlas_corners(),
        [[-4.0, 12.0], [6.0, 12.0], [-4.0, -8.0], [6.0, -8.0]]
    );
}

#[test]
fn parses_composite_sprite_set() {
    let mut payload = Vec::new();
    payload.extend_from_slice(&2u16.to_be_bytes());
    payload.extend_from_slice(&1u16.to_be_bytes());
    payload.extend_from_slice(&4u16.to_be_bytes());
    payload.extend_from_slice(b"LOGO");
    payload.extend_from_slice(&1u16.to_be_bytes());
    payload.extend_from_slice(&4u16.to_be_bytes());
    payload.extend_from_slice(b"PART");
    payload.extend_from_slice(&(-10i16).to_be_bytes());
    payload.extend_from_slice(&20i16.to_be_bytes());
    payload.extend_from_slice(&1u16.to_be_bytes());
    payload.extend_from_slice(&6u16.to_be_bytes());
    payload.extend_from_slice(b"ANCHOR");
    payload.extend_from_slice(&7u16.to_be_bytes());
    payload.extend_from_slice(&9u16.to_be_bytes());
    let mut bytes = b"KA3D".to_vec();
    bytes.extend_from_slice(&(payload.len() as u32 + 8).to_be_bytes());
    bytes.extend_from_slice(b"COMP");
    bytes.extend_from_slice(&(payload.len() as u32).to_be_bytes());
    bytes.extend_from_slice(&payload);
    let set = CompositeSpriteSet::parse(&bytes).unwrap();
    assert_eq!(set.sprites[0].name, "LOGO");
    assert_eq!(set.sprites[0].parts[0].x, -10.0);
    assert_eq!(set.sprites[0].parts[0].y, 20.0);
    assert_eq!(set.sprites[0].parts[0].scale_x, 1.0);
}

#[test]
fn parses_rvio_composite_entry_layout_and_native_fields() {
    let mut payload = Vec::new();
    payload.extend_from_slice(&1u16.to_be_bytes());
    payload.extend_from_slice(&1u16.to_be_bytes());
    payload.extend_from_slice(&2u16.to_be_bytes());
    payload.extend_from_slice(b"BG");
    payload.extend_from_slice(&1u16.to_be_bytes());
    payload.extend_from_slice(&4u16.to_be_bytes());
    payload.extend_from_slice(b"PART");
    payload.extend_from_slice(&5u16.to_be_bytes());
    payload.extend_from_slice(b"front");
    payload.extend_from_slice(&166i16.to_be_bytes());
    payload.extend_from_slice(&(-225i16).to_be_bytes());
    payload.extend_from_slice(&1.25f32.to_be_bytes());
    payload.extend_from_slice(&0.75f32.to_be_bytes());
    payload.extend_from_slice(&90.0f32.to_be_bytes());
    payload.extend_from_slice(&[1, 0]);
    let mut bytes = b"RVIO".to_vec();
    bytes.extend_from_slice(&(payload.len() as u32 + 8).to_be_bytes());
    bytes.extend_from_slice(b"COMP");
    bytes.extend_from_slice(&(payload.len() as u32).to_be_bytes());
    bytes.extend_from_slice(&payload);

    let set = CompositeSpriteSet::parse(&bytes).unwrap();
    let part = &set.sprites[0].parts[0];
    assert_eq!(part.sprite, "PART#front");
    assert_eq!(part.x, 166.0);
    assert_eq!(part.y, -225.0);
    assert_eq!(part.scale_x, 1.25);
    assert_eq!(part.scale_y, 0.75);
    assert_eq!(part.flip_x, -1.0);
    assert_eq!(part.flip_y, 1.0);
    assert_eq!(
        part.angle.to_bits(),
        (90.0_f32 * (std::f32::consts::PI / 180.0)).to_bits()
    );
}

#[test]
fn composite_loader_scans_multiple_chunks_and_keeps_the_last_named_value() {
    let composite = |part_name: Option<&str>| {
        let mut payload = 1u16.to_be_bytes().to_vec();
        payload.extend_from_slice(&1u16.to_be_bytes());
        payload.extend(test_string("DUP"));
        payload.extend_from_slice(&(part_name.is_some() as u16).to_be_bytes());
        if let Some(part_name) = part_name {
            payload.extend(test_string(part_name));
            payload.extend_from_slice(&7i16.to_be_bytes());
            payload.extend_from_slice(&9i16.to_be_bytes());
        }
        payload
    };
    let mut body = test_chunk_with_len(b"COMP", 0, &composite(None));
    body.extend(test_chunk(b"SKIP", b"ok"));
    body.extend(test_chunk_with_len(b"COMP", 0, &composite(Some("LATEST"))));
    let set = CompositeSpriteSet::parse(&test_container_with_len(b"KA3D", 0, &body)).unwrap();
    assert_eq!(set.sprites.len(), 1);
    assert_eq!(set.sprites[0].parts[0].sprite, "LATEST");
}

#[test]
fn parses_bitmap_font_glyphs() {
    let mut payload = Vec::new();
    payload.extend_from_slice(&1u16.to_be_bytes());
    payload.extend_from_slice(&8u16.to_be_bytes());
    payload.extend_from_slice(b"font.pvr");
    payload.extend_from_slice(&(-2i16).to_be_bytes());
    payload.extend_from_slice(&3i16.to_be_bytes());
    payload.extend_from_slice(&1u16.to_be_bytes());
    for value in [b'A' as u16, 2, 4, 10, 12, 11] {
        payload.extend_from_slice(&value.to_be_bytes());
    }
    let mut bytes = b"KA3D".to_vec();
    bytes.extend_from_slice(&(payload.len() as u32 + 8).to_be_bytes());
    bytes.extend_from_slice(b"FONT");
    bytes.extend_from_slice(&(payload.len() as u32).to_be_bytes());
    bytes.extend_from_slice(&payload);

    let font = BitmapFont::parse(&bytes).unwrap();
    assert_eq!(font.texture, "font.pvr");
    assert_eq!(font.leading, -2);
    assert_eq!(font.tracking, 3);
    assert_eq!(font.glyphs[0].codepoint, u32::from(b'A'));
    assert_eq!(font.glyphs[0].pivot_y, 11);
}

#[test]
fn parses_bitmap_font_v2_utf32_codepoints_and_signed_pivots() {
    let mut payload = Vec::new();
    payload.extend_from_slice(&2u16.to_be_bytes());
    payload.extend_from_slice(&8u16.to_be_bytes());
    payload.extend_from_slice(b"font.pvr");
    payload.extend_from_slice(&1i16.to_be_bytes());
    payload.extend_from_slice(&(-2i16).to_be_bytes());
    payload.extend_from_slice(&1u16.to_be_bytes());
    payload.extend_from_slice(&0x1f600u32.to_be_bytes());
    for value in [-2i16, -4, 10, 12] {
        payload.extend_from_slice(&value.to_be_bytes());
    }
    payload.extend_from_slice(&(-3i16).to_be_bytes());
    let mut bytes = b"KA3D".to_vec();
    bytes.extend_from_slice(&(payload.len() as u32 + 8).to_be_bytes());
    bytes.extend_from_slice(b"FONT");
    bytes.extend_from_slice(&(payload.len() as u32).to_be_bytes());
    bytes.extend_from_slice(&payload);

    let font = BitmapFont::parse(&bytes).unwrap();
    assert_eq!(font.glyphs[0].codepoint, 0x1f600);
    assert_eq!((font.glyphs[0].x, font.glyphs[0].y), (-2, -4));
    assert_eq!(font.glyphs[0].pivot_y, -3);
    assert_eq!(font.native_string_width("😀😀"), 18);
    assert_eq!(font.native_string_height("x😀"), 12);
    assert_eq!(font.native_draw_anchor("😀", "RIGHT", "TOP"), [-10, 0]);
    assert_eq!(
        font.native_string_bounds("😀", "RIGHT", "TOP"),
        [-10, 0, 0, 12]
    );
}

#[test]
fn font_loader_overwrites_header_and_lookup_but_retains_cached_metric_history() {
    let font = |texture: &str, width: i16, pivot_y: i16| {
        let mut payload = 1u16.to_be_bytes().to_vec();
        payload.extend(test_string(texture));
        payload.extend_from_slice(&3i16.to_be_bytes());
        payload.extend_from_slice(&4i16.to_be_bytes());
        payload.extend_from_slice(&1u16.to_be_bytes());
        payload.extend_from_slice(&(b'A' as u16).to_be_bytes());
        for value in [0i16, 0, width, 12, pivot_y] {
            payload.extend_from_slice(&value.to_be_bytes());
        }
        payload
    };
    let mut body = test_chunk(b"NOPE", b"x");
    body.extend(test_chunk_with_len(b"FONT", 0, &font("first.pvr", 10, 20)));
    body.extend(test_chunk_with_len(b"FONT", 1, &font("last.pvr", 5, 1)));
    let parsed = BitmapFont::parse(&test_container_with_len(b"KA3D", 0, &body)).unwrap();
    assert_eq!(parsed.texture, "last.pvr");
    assert_eq!(parsed.glyph(b'A' as u32).unwrap().width, 5);
    assert_eq!(parsed.native_max_ascending(), 20);
}

#[test]
fn bitmap_font_native_metrics_keep_missing_glyph_spacing_and_w_register_wrap() {
    let font = BitmapFont {
        texture: "font.pvr".to_owned(),
        leading: -4,
        tracking: 3,
        glyphs: vec![FontGlyph {
            codepoint: u32::from(b'A'),
            x: 0,
            y: 0,
            width: 10,
            height: 12,
            pivot_y: -3,
        }],
    };
    assert_eq!(font.native_string_width("AxA"), 26);
    assert_eq!(font.native_string_height("xAx"), 12);
    assert_eq!(font.native_max_ascending(), 0);
    assert_eq!(font.native_max_descending(), 15);
    assert_eq!(font.native_draw_anchor("A", "LEFT", "VCENTER"), [0, -7]);
    assert_eq!(
        font.native_string_bounds("A", "LEFT", "TOP"),
        [0, 0, 10, 12]
    );

    let wrapping = BitmapFont {
        tracking: 0,
        glyphs: vec![FontGlyph {
            width: i16::MAX,
            ..font.glyphs[0]
        }],
        ..font
    };
    assert_eq!(
        wrapping.native_string_width(&"A".repeat(65_539)),
        -2_147_450_883
    );
}

#[test]
fn parses_localization_table() {
    let string = |value: &str| {
        let mut bytes = (value.len() as u16).to_be_bytes().to_vec();
        bytes.extend_from_slice(value.as_bytes());
        bytes
    };
    let chunk = |tag: &[u8; 4], payload: &[u8]| {
        let mut bytes = tag.to_vec();
        bytes.extend_from_slice(&(payload.len() as u32).to_be_bytes());
        bytes.extend_from_slice(payload);
        bytes
    };
    let mut locales = 1u16.to_be_bytes().to_vec();
    locales.extend(string("en_EN"));
    let mut ids = 1u16.to_be_bytes().to_vec();
    ids.extend(string("TEXT_OK"));
    let translations = string("OK");
    let mut payload = 1u16.to_be_bytes().to_vec();
    payload.extend(chunk(b"LDAT", &locales));
    payload.extend(chunk(b"LIDS", &ids));
    payload.extend(chunk(b"TXGP", &translations));
    let mut bytes = b"KA3D".to_vec();
    bytes.extend_from_slice(&(payload.len() as u32 + 8).to_be_bytes());
    bytes.extend_from_slice(b"TEXT");
    bytes.extend_from_slice(&(payload.len() as u32).to_be_bytes());
    bytes.extend_from_slice(&payload);

    let table = LocalizationTable::parse(&bytes).unwrap();
    assert_eq!(table.locales, ["en_EN"]);
    assert_eq!(table.ids, ["TEXT_OK"]);
    assert_eq!(table.translations[0], ["OK"]);
}

#[test]
fn localization_uses_native_nested_scan_and_locale_indexed_txgp_passes() {
    let mut locale_payload = 2u16.to_be_bytes().to_vec();
    locale_payload.extend(test_string("en_EN"));
    locale_payload.extend(test_string("fr_FR"));
    let mut id_payload = 1u16.to_be_bytes().to_vec();
    id_payload.extend(test_string("TEXT_OK"));
    let en = test_string("OK");
    let fr = test_string("D'accord");

    let mut text_payload = 1u16.to_be_bytes().to_vec();
    text_payload.extend(test_chunk(b"NOPE", b"ignored"));
    text_payload.extend(test_chunk(b"LDAT", &locale_payload));
    text_payload.extend(test_chunk(b"LIDS", &id_payload));
    text_payload.extend(test_chunk(b"TXGP", &en));
    text_payload.extend(test_chunk(b"TXGP", &fr));
    let mut body = test_chunk(b"SKIP", b"outer");
    body.extend(test_chunk_with_len(b"TEXT", 0, &text_payload));
    let bytes = test_container_with_len(b"KA3D", 0, &body);

    let table = LocalizationTable::parse(&bytes).unwrap();
    assert_eq!(table.locales, ["en_EN", "fr_FR"]);
    assert_eq!(table.ids, ["TEXT_OK"]);
    assert_eq!(table.translations, [["OK"], ["D'accord"]]);
}

#[test]
fn localization_rejects_txgp_before_lids_like_native_loader() {
    let mut locale_payload = 1u16.to_be_bytes().to_vec();
    locale_payload.extend(test_string("en_EN"));
    let mut text_payload = 1u16.to_be_bytes().to_vec();
    text_payload.extend(test_chunk(b"LDAT", &locale_payload));
    text_payload.extend(test_chunk(b"TXGP", &test_string("orphan")));
    let body = test_chunk(b"TEXT", &text_payload);
    assert!(LocalizationTable::parse(&test_container(b"KA3D", &body)).is_err());
}

#[test]
fn parses_pre_ka3d_legacy_localization_offsets() {
    let mut locale_section = vec![2];
    locale_section.extend(test_string("en_EN"));
    locale_section.extend(test_string("fr_FR"));
    let mut bytes = vec![1];
    bytes.extend_from_slice(&(locale_section.len() as u32).to_be_bytes());
    bytes.extend_from_slice(&locale_section);
    bytes.extend_from_slice(&1u16.to_be_bytes());
    bytes.extend(test_string("TEXT_OK"));
    let en = test_string("OK");
    let fr = test_string("D'accord");
    // Offset zero is measured after its own entry and crosses the remaining
    // offset entry. Offset one is measured after entry one and crosses group0.
    bytes.extend_from_slice(&4u32.to_be_bytes());
    bytes.extend_from_slice(&(en.len() as u32).to_be_bytes());
    bytes.extend(en);
    bytes.extend(fr);

    let table = LocalizationTable::parse(&bytes).unwrap();
    assert_eq!(table.locales, ["en_EN", "fr_FR"]);
    assert_eq!(table.ids, ["TEXT_OK"]);
    assert_eq!(table.translations, [["OK"], ["D'accord"]]);
}
