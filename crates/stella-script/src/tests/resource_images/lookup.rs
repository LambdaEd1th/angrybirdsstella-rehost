//! Native descriptor-relative images and independent AppData input streams.

use super::*;

fn app_data(files: &Files) -> PathBuf {
    files.0.join("appdata")
}

fn place_image(files: &Files, destination: &Path, color: [u8; 4]) {
    files.image("fixture.pvr");
    let mut bytes = fs::read(files.data().join("fixture.pvr")).unwrap();
    bytes[52..].copy_from_slice(&color.repeat(16 * 8));
    fs::create_dir_all(destination.parent().unwrap()).unwrap();
    fs::write(destination, bytes).unwrap();
}

fn nested_sheet(files: &Files, texture: &str) {
    fs::create_dir_all(files.data().join("nested")).unwrap();
    fs::write(
        files.data().join("nested/A.dat"),
        test_textured_sprite_sheet("S", texture, 2, 2),
    )
    .unwrap();
}

fn assert_sheet_absent(runtime: &StellaLua, name: &str) {
    let resources = runtime.resource_runtime.lock().unwrap();
    assert!(!resources.sprite_sheets.contains(name));
    assert!(!resources.sprite_sheet_values.contains_key(name));
    assert!(!resources.sprite_entries.contains_key("S"));
    assert!(!resources.sprite_sheet_texture_sources.contains_key(name));
}

fn add_fallback_decoys(files: &Files) {
    for destination in [
        files.data().join("same.pvr"),
        app_data(files).join("same.pvr"),
        files.data().join("images/1024x768/same.pvr"),
        files.data().join("fonts/1024x768/same.pvr"),
    ] {
        place_image(files, &destination, [71, 19, 103, 255]);
    }
}

#[test]
fn binary_sheet_missing_sibling_image_does_not_use_other_directories() {
    let files = Files::new();
    nested_sheet(&files, "same.pvr");
    add_fallback_decoys(&files);
    let runtime = files.runtime();
    let error = runtime
        .execute_source("res.createSpriteSheet('nested/A.dat')")
        .unwrap_err();
    assert!(
        error
            .to_string()
            .replace('\\', "/")
            .contains("nested/same.pvr"),
        "{error}"
    );
    assert_sheet_absent(&runtime, "A");
}

#[test]
fn bitmap_font_missing_sibling_image_does_not_use_other_directories() {
    let files = Files::new();
    fs::create_dir_all(files.data().join("nested")).unwrap();
    fs::write(
        files.data().join("nested/F.dat"),
        test_bitmap_font_with_glyph("same.pvr", 2),
    )
    .unwrap();
    add_fallback_decoys(&files);
    let runtime = files.runtime();
    let error = runtime
        .execute_source("res.createBitmapFont('nested/F.dat')")
        .unwrap_err();
    assert!(
        error
            .to_string()
            .replace('\\', "/")
            .contains("nested/same.pvr"),
        "{error}"
    );
    let resources = runtime.resource_runtime.lock().unwrap();
    assert!(!resources.native_font_values.contains_key("F"));
    assert!(!resources.bitmap_font_image_owners.contains_key("F"));
}

#[test]
fn json_sheet_missing_sibling_image_does_not_use_other_directories() {
    let files = Files::new();
    fs::create_dir_all(files.data().join("nested")).unwrap();
    fs::write(
        files.data().join("nested/A.json"),
        br#"{"meta":{"image":"same.pvr","app":"http://www.texturepacker.com"},"frames":[{"filename":"S","frame":{"x":0,"y":0,"w":2,"h":2},"rotated":false}]}"#,
    )
    .unwrap();
    add_fallback_decoys(&files);
    let runtime = files.runtime();
    let error = runtime
        .execute_source("res.createSpriteSheet('nested/A.json')")
        .unwrap_err();
    assert!(
        error
            .to_string()
            .replace('\\', "/")
            .contains("nested/same.pvr"),
        "{error}"
    );
    assert_sheet_absent(&runtime, "A");
}

#[test]
fn descriptor_image_leading_separator_remains_relative_to_descriptor() {
    let files = Files::new();
    nested_sheet(&files, "/same.pvr");
    place_image(
        &files,
        &files.data().join("nested/same.pvr"),
        [71, 19, 103, 255],
    );
    let runtime = files.runtime();
    runtime
        .execute_source("res.createSpriteSheet('nested/A.dat')")
        .unwrap();
    assert_eq!(
        bound_image(&runtime).image.as_ref().unwrap().rgba,
        [71, 19, 103, 255].repeat(16 * 8)
    );
}

#[test]
fn descriptor_image_normalizes_backslashes_and_parent_segments_before_open() {
    let files = Files::new();
    nested_sheet(&files, r"uncreated\..\same.pvr");
    place_image(
        &files,
        &files.data().join("nested/same.pvr"),
        [71, 19, 103, 255],
    );
    let runtime = files.runtime();
    runtime
        .execute_source("res.createSpriteSheet('nested/A.dat')")
        .unwrap();
    assert_eq!(bound_image(&runtime).dimensions, Some([16, 8]));
}

#[test]
fn missing_sibling_reload_preserves_old_identity_pixels_and_queued_draws() {
    let files = Files::new();
    nested_sheet(&files, "same.pvr");
    place_image(
        &files,
        &files.data().join("nested/same.pvr"),
        [71, 19, 103, 255],
    );
    let runtime = files.runtime();
    runtime
        .execute_source("res.createSpriteSheet('nested/A.dat'); res.drawSprite('S',0,0)")
        .unwrap();
    let previous = bound_image(&runtime);
    fs::remove_file(files.data().join("nested/same.pvr")).unwrap();
    add_fallback_decoys(&files);
    let error = runtime
        .execute_source("res.createSpriteSheet('nested/A.dat',true)")
        .unwrap_err();
    assert!(
        error
            .to_string()
            .replace('\\', "/")
            .contains("nested/same.pvr"),
        "{error}"
    );
    let current = bound_image(&runtime);
    assert_eq!(previous.owner.identity(), current.owner.identity());
    assert!(Arc::ptr_eq(
        previous.image.as_ref().unwrap(),
        current.image.as_ref().unwrap()
    ));
    runtime.execute_source("res.drawSprite('S',8,0)").unwrap();
    let commands = runtime.take_render_commands();
    assert_eq!(commands.len(), 2);
    for command in commands {
        assert_eq!(
            command
                .bound_region
                .as_ref()
                .unwrap()
                .image_owner
                .as_ref()
                .unwrap()
                .identity(),
            previous.owner.identity()
        );
    }
}

#[test]
fn assets_explicit_image_uses_appdata_root_independently_of_descriptor() {
    let files = Files::new();
    fs::create_dir_all(app_data(&files).join("nested")).unwrap();
    fs::write(
        app_data(&files).join("nested/A.dat"),
        test_textured_sprite_sheet("S", "ignored.pvr", 2, 2),
    )
    .unwrap();
    place_image(
        &files,
        &app_data(&files).join("same.pvr"),
        [71, 19, 103, 255],
    );
    place_image(
        &files,
        &app_data(&files).join("nested/same.pvr"),
        [200, 2, 3, 255],
    );
    let runtime = files.runtime();
    runtime
        .execute_source("_G.Assets.createSpriteSheet('A','nested/A.dat','same.pvr')")
        .unwrap();
    assert_eq!(
        bound_image(&runtime).image.as_ref().unwrap().rgba,
        [71, 19, 103, 255].repeat(16 * 8)
    );
}

#[test]
fn assets_missing_appdata_image_does_not_use_bundle_image() {
    let files = Files::new();
    fs::write(
        app_data(&files).join("A.dat"),
        test_textured_sprite_sheet("S", "ignored.pvr", 2, 2),
    )
    .unwrap();
    files.image("same.pvr");
    let runtime = files.runtime();
    let error = runtime
        .execute_source("_G.Assets.createSpriteSheet('A','A.dat','same.pvr')")
        .unwrap_err();
    assert!(
        error.to_string().replace('\\', "/").contains(
            &app_data(&files)
                .join("same.pvr")
                .to_string_lossy()
                .replace('\\', "/")
        ),
        "{error}"
    );
    assert_sheet_absent(&runtime, "A");
}

#[test]
fn assets_missing_appdata_descriptor_does_not_use_bundle_descriptor() {
    let files = Files::new();
    files.sheet("A", "ignored.pvr", 2);
    place_image(
        &files,
        &app_data(&files).join("same.pvr"),
        [71, 19, 103, 255],
    );
    let runtime = files.runtime();
    let error = runtime
        .execute_source("_G.Assets.createSpriteSheet('A','A.dat','same.pvr')")
        .unwrap_err();
    assert!(error.to_string().contains("A.dat"), "{error}");
    assert_sheet_absent(&runtime, "A");
}

#[test]
fn assets_descriptor_open_failure_precedes_explicit_image_failure() {
    let files = Files::new();
    let runtime = files.runtime();
    let error = runtime
        .execute_source("_G.Assets.createSpriteSheet('A','absent.dat','missing.pvr')")
        .unwrap_err();
    assert!(error.to_string().contains("absent.dat"), "{error}");
    assert!(
        !error.to_string().contains("Failed to open image"),
        "{error}"
    );
    assert_sheet_absent(&runtime, "A");
}

#[cfg(unix)]
#[test]
fn image_trailing_backslash_is_a_separator_even_with_literal_decoy() {
    let files = Files::new();
    nested_sheet(&files, "same.pvr\\");
    place_image(
        &files,
        &files.data().join("nested/same.pvr\\"),
        [71, 19, 103, 255],
    );
    let runtime = files.runtime();
    let error = runtime
        .execute_source("res.createSpriteSheet('nested/A.dat')")
        .unwrap_err();
    assert!(
        error.to_string().contains("Failed to open image"),
        "{error}"
    );
    assert_sheet_absent(&runtime, "A");
}

#[cfg(unix)]
fn write_tga(path: &Path) {
    let mut bytes = vec![0_u8; 18];
    bytes[2] = 2;
    bytes[12..14].copy_from_slice(&2_u16.to_le_bytes());
    bytes[14..16].copy_from_slice(&2_u16.to_le_bytes());
    bytes[16] = 32;
    bytes[17] = 0x28;
    bytes.extend_from_slice(&[103, 19, 71, 255].repeat(4));
    fs::write(path, bytes).unwrap();
}

#[cfg(unix)]
#[test]
fn symlink_image_reader_uses_requested_extension_instead_of_target_extension() {
    let files = Files::new();
    nested_sheet(&files, "logical.tga");
    write_tga(&files.data().join("nested/storage.bin"));
    std::os::unix::fs::symlink("storage.bin", files.data().join("nested/logical.tga")).unwrap();
    let runtime = files.runtime();
    runtime
        .execute_source("res.createSpriteSheet('nested/A.dat')")
        .unwrap();
    let image = bound_image(&runtime);
    assert_eq!(
        stella_assets::image_source::image_source_path(&image.source),
        files.data().join("nested/logical.tga").to_str().unwrap()
    );
    assert_eq!(image.dimensions, Some([2, 2]));
    assert_eq!(
        image.image.as_ref().unwrap().rgba,
        [71, 19, 103, 255].repeat(4)
    );
}

#[cfg(unix)]
#[test]
fn symlink_target_extension_cannot_make_an_unknown_requested_image_readable() {
    let files = Files::new();
    nested_sheet(&files, "same.pvr");
    place_image(
        &files,
        &files.data().join("nested/same.pvr"),
        [71, 19, 103, 255],
    );
    let runtime = files.runtime();
    runtime
        .execute_source("res.createSpriteSheet('nested/A.dat'); res.drawSprite('S',0,0)")
        .unwrap();
    let previous = bound_image(&runtime);
    nested_sheet(&files, "logical.bin");
    write_tga(&files.data().join("nested/storage.tga"));
    std::os::unix::fs::symlink("storage.tga", files.data().join("nested/logical.bin")).unwrap();
    let error = runtime
        .execute_source("res.createSpriteSheet('nested/A.dat',true)")
        .unwrap_err();
    assert!(error.to_string().contains("nested/logical.bin"), "{error}");
    assert!(
        error
            .to_string()
            .contains("unsupported native image reader"),
        "{error}"
    );
    let current = bound_image(&runtime);
    assert_eq!(previous.owner.identity(), current.owner.identity());
    assert!(Arc::ptr_eq(
        previous.image.as_ref().unwrap(),
        current.image.as_ref().unwrap()
    ));
    runtime.execute_source("res.drawSprite('S',8,0)").unwrap();
    let commands = runtime.take_render_commands();
    assert_eq!(commands.len(), 2);
    for command in commands {
        assert_eq!(
            command
                .bound_region
                .as_ref()
                .unwrap()
                .image_owner
                .as_ref()
                .unwrap()
                .identity(),
            previous.owner.identity()
        );
    }
}
