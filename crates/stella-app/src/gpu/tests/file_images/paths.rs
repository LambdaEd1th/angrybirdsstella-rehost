//! Literal file names stay separate from framebuffer capture image identities.

use super::*;

const LITERAL: &str = "<capture:literal.png>";

fn image(files: &Files, color: [u8; 4]) {
    RgbaImage::from_pixel(4, 2, image::Rgba(color))
        .save_with_format(files.data().join(LITERAL), image::ImageFormat::Png)
        .unwrap();
}

fn sheet(files: &Files, name: &str, sprite: &str) {
    let mut payload = 1_u16.to_be_bytes().to_vec();
    string(&mut payload, LITERAL);
    payload.extend_from_slice(&1_u16.to_be_bytes());
    string(&mut payload, sprite);
    for value in [0_i16, 0, 2, 2, 0, 0] {
        payload.extend_from_slice(&value.to_be_bytes());
    }
    std::fs::write(files.data().join(name), envelope(b"SPRT", payload)).unwrap();
}

fn font(files: &Files) {
    let mut payload = 1_u16.to_be_bytes().to_vec();
    string(&mut payload, LITERAL);
    for value in [0_u16, 0, 1, u16::from(b'A'), 0, 0, 2, 2, 0] {
        payload.extend_from_slice(&value.to_be_bytes());
    }
    std::fs::write(files.data().join("F.dat"), envelope(b"FONT", payload)).unwrap();
}

#[test]
fn literal_capture_prefix_sprite_and_font_keep_pixels_after_source_files_disappear() {
    let files = Files::new();
    image(&files, OLD);
    sheet(&files, "S.dat", "S");
    font(&files);
    let runtime = StellaLua::new_with_resolution(files.data(), 4, 2).unwrap();
    runtime
        .execute_source(
            "res.createSpriteSheet('S.dat'); res.createBitmapFont('F.dat'); res.useFont('F')",
        )
        .unwrap();
    for name in [LITERAL, "S.dat", "F.dat"] {
        std::fs::remove_file(files.data().join(name)).unwrap();
    }
    runtime
        .execute_source("res.drawSprite('S',0,0); res.drawString('','A',2,0)")
        .unwrap();
    assert_halves(&pixels(&runtime, &mut assets(&files)), OLD, OLD);
}

#[test]
fn literal_capture_prefix_reload_preserves_old_commands_and_updates_warm_file_pixels() {
    let files = Files::new();
    image(&files, OLD);
    sheet(&files, "S.dat", "S");
    let runtime = StellaLua::new_with_resolution(files.data(), 4, 2).unwrap();
    runtime
        .execute_source(
            "res.createSpriteSheet('S.dat'); res.drawSprite('S',0,0); res.drawSprite('S',2,0)",
        )
        .unwrap();
    let mut assets = assets(&files);
    assert_halves(&pixels(&runtime, &mut assets), OLD, OLD);
    runtime.execute_source("res.drawSprite('S',0,0)").unwrap();
    image(&files, NEW);
    runtime
        .execute_source("res.createSpriteSheet('S.dat',true); res.drawSprite('S',2,0)")
        .unwrap();
    std::fs::remove_file(files.data().join(LITERAL)).unwrap();
    assert_halves(&pixels(&runtime, &mut assets), OLD, NEW);
}

#[test]
fn capturing_a_literal_prefix_file_image_preserves_another_image_using_the_same_file() {
    let files = Files::new();
    image(&files, OLD);
    sheet(&files, "S.dat", "S");
    sheet(&files, "T.dat", "T");
    let runtime = StellaLua::new_with_resolution(files.data(), 4, 2).unwrap();
    runtime
        .execute_source("res.createSpriteSheet('S.dat'); res.createSpriteSheet('T.dat')")
        .unwrap();
    std::fs::remove_file(files.data().join(LITERAL)).unwrap();
    runtime
        .execute_source("res.captureSprite('S'); res.drawSprite('S',0,0); res.drawSprite('T',2,0)")
        .unwrap();
    assert_halves(
        &pixels(&runtime, &mut assets(&files)),
        [0, 255, 0, 255],
        OLD,
    );
}

#[test]
fn real_capture_sheet_with_a_literal_handle_name_retains_framebuffer_pixels() {
    let files = Files::new();
    let runtime = StellaLua::new_with_resolution(files.data(), 4, 2).unwrap();
    runtime
        .execute_source("res.captureSprite('<capture:literal.png>'); res.drawSprite('<capture:literal.png>',0,0)")
        .unwrap();
    assert_eq!(
        pixels(&runtime, &mut assets(&files)),
        [0, 255, 0, 255].repeat(8)
    );
}
