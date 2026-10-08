//! Literal file names never enter the host's capture-handle namespace.

use super::*;
use stella_assets::ka3d::SpriteSheet;

const LITERAL: &str = "<capture:literal.pvr>";

fn assert_literal_image(image: &crate::SheetImageSnapshot, root: &Path) {
    let path = root.join(LITERAL);
    assert_eq!(
        stella_assets::image_source::image_source_path(&image.source),
        path.to_str().unwrap()
    );
    assert_eq!(
        image.image.as_ref().unwrap().rgba,
        [71, 19, 103, 255].repeat(16 * 8)
    );
    assert!(!image.source.starts_with("<capture:"));
}

#[test]
fn binary_sheet_loads_a_literal_capture_prefix_file_at_construction() {
    let files = Files::new();
    files.image(LITERAL);
    files.sheet("A", LITERAL, 2);
    let runtime = files.runtime();
    runtime
        .execute_source("res.createSpriteSheet('A.dat'); res.drawSprite('S',0,0)")
        .unwrap();
    assert_literal_image(&bound_image(&runtime), &files.data());
}

#[test]
fn bitmap_font_loads_a_literal_capture_prefix_file_at_construction() {
    let files = Files::new();
    files.image(LITERAL);
    files.font(LITERAL, 2);
    let runtime = files.runtime();
    runtime
        .execute_source(
            "res.createBitmapFont('F.dat'); res.useFont('F'); res.drawString('','A',0,0)",
        )
        .unwrap();
    let commands = runtime.take_text_commands();
    let Some(TextFontBinding::Bitmap {
        texture_source,
        decoded_image,
        ..
    }) = commands[0].font_binding.as_ref()
    else {
        panic!("bitmap font expected")
    };
    assert_eq!(
        stella_assets::image_source::image_source_path(texture_source),
        files.data().join(LITERAL).to_str().unwrap()
    );
    assert_eq!(
        decoded_image.as_ref().unwrap().rgba,
        [71, 19, 103, 255].repeat(16 * 8)
    );
}

#[test]
fn json_sheet_loads_a_literal_capture_prefix_file_at_construction() {
    let files = Files::new();
    files.image(LITERAL);
    fs::write(files.data().join("A.json"), format!(r#"{{"meta":{{"image":"{LITERAL}","app":"http://www.texturepacker.com"}},"frames":[{{"filename":"S","frame":{{"x":0,"y":0,"w":2,"h":2}},"rotated":false}}]}}"#)).unwrap();
    let runtime = files.runtime();
    runtime
        .execute_source("res.createSpriteSheet('A.json'); res.drawSprite('S',0,0)")
        .unwrap();
    assert_literal_image(&bound_image(&runtime), &files.data());
}

#[test]
fn assets_explicit_image_is_a_file_even_when_it_resembles_a_capture_handle() {
    let files = Files::new();
    files.image(LITERAL);
    files.sheet("A", "embedded-is-ignored.pvr", 2);
    let appdata = files.0.join("appdata");
    for filename in ["A.dat", LITERAL] {
        fs::rename(files.data().join(filename), appdata.join(filename)).unwrap();
    }
    let runtime = files.runtime();
    runtime
        .execute_source(&format!(
            "_G.Assets.createSpriteSheet('download','A.dat','{LITERAL}'); res.drawSprite('S',0,0)"
        ))
        .unwrap();
    assert_literal_image(&bound_image(&runtime), &appdata);
}

#[test]
fn diagnostic_sheet_binding_does_not_manufacture_a_capture_from_a_file_name() {
    let files = Files::new();
    files.image(LITERAL);
    let runtime = files.runtime();
    let sheet = SpriteSheet::parse(&test_textured_sprite_sheet("S", LITERAL, 2, 2)).unwrap();
    let mut resources = runtime.resource_runtime.lock().unwrap();
    resources.replace_sprite_sheet_value("A", sheet);
    resources
        .cache_sprite_sheet_host_bindings("A", &files.data())
        .unwrap();
    drop(resources);
    assert_literal_image(&bound_image(&runtime), &files.data());
}

#[test]
fn missing_literal_capture_prefix_file_reports_the_resolved_file_path() {
    let files = Files::new();
    files.sheet("A", LITERAL, 2);
    let runtime = files.runtime();
    let error = runtime
        .execute_source("res.createSpriteSheet('A.dat')")
        .unwrap_err();
    assert!(
        error
            .to_string()
            .contains(files.data().join(LITERAL).to_str().unwrap()),
        "{error}"
    );
    assert!(
        !runtime
            .resource_runtime
            .lock()
            .unwrap()
            .sprite_sheets
            .contains("A")
    );
}

#[test]
fn ordinary_image_identity_prefix_is_also_preserved_as_a_literal_file_name() {
    let files = Files::new();
    let literal = "<image:font:7>literal.pvr";
    files.image(literal);
    files.sheet("A", literal, 2);
    let runtime = files.runtime();
    runtime
        .execute_source("res.createSpriteSheet('A.dat'); res.drawSprite('S',0,0)")
        .unwrap();
    let image = bound_image(&runtime);
    assert_eq!(
        stella_assets::image_source::image_source_path(&image.source),
        files.data().join(literal).to_str().unwrap()
    );
    assert!(image.image.is_some());
}

#[test]
fn new_capture_with_a_handle_like_sheet_name_still_creates_a_native_capture() {
    let files = Files::new();
    let runtime = files.runtime();
    runtime.execute_source("res.captureSprite('<capture:literal.pvr>'); res.drawSprite('<capture:literal.pvr>',0,0)").unwrap();
    let captures = runtime.take_capture_commands();
    assert_eq!(captures.len(), 1);
    assert!(captures[0].texture_source.starts_with("<capture:image:"));
    let draws = runtime.take_render_commands();
    assert_eq!(draws.len(), 1);
    assert_eq!(
        draws[0].bound_region.as_ref().unwrap().texture_source,
        captures[0].texture_source
    );
}
