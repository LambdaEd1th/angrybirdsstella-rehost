//! Repeated FONT records retire glyphs without rebinding them to the last atlas.

use super::*;

fn font_record(image: &str, character: u16, pivot: i16) -> Vec<u8> {
    let mut payload = 1_u16.to_be_bytes().to_vec();
    string(&mut payload, image);
    payload.extend_from_slice(&0_i16.to_be_bytes());
    payload.extend_from_slice(&0_i16.to_be_bytes());
    payload.extend_from_slice(&1_u16.to_be_bytes());
    payload.extend_from_slice(&character.to_be_bytes());
    for value in [0_i16, 0, 2, 2, pivot] {
        payload.extend_from_slice(&value.to_be_bytes());
    }
    envelope(b"FONT", payload)
}

fn repeated_font(files: &Files, same_image: bool, replacement: bool) {
    files.image(OLD);
    RgbaImage::from_pixel(2, 2, image::Rgba(NEW))
        .save(files.data().join("next.png"))
        .unwrap();
    let mut bytes = font_record("same.png", u16::from(b'A'), 2);
    let next = font_record(
        if same_image { "same.png" } else { "next.png" },
        u16::from(if replacement { b'A' } else { b'B' }),
        0,
    );
    bytes.extend_from_slice(&next[8..]);
    let length = bytes.len() as u32 - 8;
    bytes[4..8].copy_from_slice(&length.to_be_bytes());
    std::fs::write(files.data().join("F.dat"), bytes).unwrap();
}

fn runtime_without_files(files: &Files) -> StellaLua {
    let runtime = StellaLua::new_with_resolution(files.data(), 4, 2).unwrap();
    runtime
        .execute_source("res.createBitmapFont('F.dat'); res.useFont('F')")
        .unwrap();
    for name in ["F.dat", "same.png", "next.png"] {
        std::fs::remove_file(files.data().join(name)).unwrap();
    }
    runtime
}

#[test]
fn repeated_font_keeps_live_prefix_pixels_and_rejects_retired_glyphs() {
    for same_image in [false, true] {
        let files = Files::new();
        repeated_font(&files, same_image, false);
        let runtime = runtime_without_files(&files);
        let error = runtime
            .execute_source("res.drawString('','BA',0,-2)")
            .unwrap_err();
        assert!(error.to_string().contains("released private SpriteSheet"));
        let mut assets = assets(&files);
        let live = if same_image { OLD } else { NEW };
        assert_halves(&pixels(&runtime, &mut assets), live, [0, 255, 0, 255]);
        runtime
            .execute_source("res.drawString('','BB',0,-2); assert(res.getStringWidth('B')==2)")
            .unwrap();
        assert_halves(&pixels(&runtime, &mut assets), live, live);
    }
}

#[test]
fn centered_repeated_font_error_submits_no_pixels_or_draw_order() {
    let files = Files::new();
    repeated_font(&files, false, false);
    let runtime = runtime_without_files(&files);
    for anchor in ["RIGHT", "HCENTER"] {
        let error = runtime
            .execute_source(&format!("res.drawString('','BA',0,-2,'{anchor}')"))
            .unwrap_err();
        assert!(error.to_string().contains("released private SpriteSheet"));
        assert!(runtime.take_text_commands().is_empty());
    }
    runtime
        .execute_source("res.drawString('','BB',0,-2)")
        .unwrap();
    assert_halves(&pixels(&runtime, &mut assets(&files)), NEW, NEW);
}

#[test]
fn repeated_font_replacement_uses_final_pixels_and_historical_cached_anchor() {
    let files = Files::new();
    repeated_font(&files, false, true);
    let runtime = runtime_without_files(&files);
    runtime
        .execute_source("assert(res.getFontMaxAscending()==2); assert(res.getFontMaxDescending()==2); assert(res.getStringWidth('A')==2); res.drawString('','AA',0,-2)")
        .unwrap();
    assert_halves(&pixels(&runtime, &mut assets(&files)), NEW, NEW);
}

#[test]
fn font_retirement_does_not_rebind_queued_pixels_to_a_replacement_font() {
    let files = Files::new();
    repeated_font(&files, false, false);
    let runtime = StellaLua::new_with_resolution(files.data(), 4, 2).unwrap();
    runtime
        .execute_source(
            "res.createBitmapFont('F.dat'); res.useFont('F'); res.drawString('','BB',0,-2)",
        )
        .unwrap();
    let mut assets = assets(&files);
    assert_halves(&pixels(&runtime, &mut assets), NEW, NEW);
    assert!(
        runtime
            .execute_source("res.drawString('','BA',0,-2)")
            .is_err()
    );
    files.font();
    runtime
        .execute_source(
            "res.createBitmapFont('F.dat',true); res.useFont('F'); res.drawString('','A',2,0)",
        )
        .unwrap();
    for name in ["F.dat", "same.png", "next.png"] {
        std::fs::remove_file(files.data().join(name)).unwrap();
    }
    assert_halves(&pixels(&runtime, &mut assets), NEW, OLD);
}
