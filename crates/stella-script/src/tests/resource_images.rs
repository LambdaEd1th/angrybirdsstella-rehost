//! Image failure order and transactional sheet/font construction from IDA.

use super::*;

mod empty;
mod lookup;

#[cfg(unix)]
mod paths;

struct Files(PathBuf);

impl Files {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let root = std::env::temp_dir().join(format!(
            "stella-image-construction-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        for directory in ["data", "appdata"] {
            fs::create_dir_all(root.join(directory)).unwrap();
        }
        Self(root)
    }

    fn data(&self) -> PathBuf {
        self.0.join("data")
    }

    fn runtime(&self) -> StellaLua {
        StellaLua::new_with_resolution(self.data(), 16, 16).unwrap()
    }

    fn image(&self, name: &str) {
        let mut image = [
            52_u32,
            8,
            16,
            0,
            0x12,
            16 * 8 * 4,
            32,
            0xff,
            0xff00,
            0xff0000,
            0xff000000,
            u32::from_le_bytes(*b"PVR!"),
            1,
        ]
        .into_iter()
        .flat_map(u32::to_le_bytes)
        .collect::<Vec<_>>();
        image.extend_from_slice(&[71, 19, 103, 255].repeat(16 * 8));
        fs::write(self.data().join(name), image).unwrap();
    }

    fn sheet(&self, name: &str, texture: &str, width: u16) {
        fs::write(
            self.data().join(format!("{name}.dat")),
            test_textured_sprite_sheet("S", texture, width, 2),
        )
        .unwrap();
    }

    fn font(&self, texture: &str, width: u16) {
        fs::write(
            self.data().join("F.dat"),
            test_bitmap_font_with_glyph(texture, width),
        )
        .unwrap();
    }
}

impl Drop for Files {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn bound_image(runtime: &StellaLua) -> crate::SheetImageSnapshot {
    let resources = runtime.resource_runtime.lock().unwrap();
    let region = resources
        .active_atlas_catalog_region("S", runtime.data_root())
        .unwrap();
    region.sheet_image.as_ref().unwrap().snapshot().unwrap()
}

#[test]
fn missing_sheet_image_fails_without_publishing_resource_or_sprites() {
    let files = Files::new();
    files.sheet("A", "missing.pvr", 2);
    let runtime = files.runtime();
    let next_id = runtime
        .resource_runtime
        .lock()
        .unwrap()
        .next_sprite_sheet_identity;
    let error = runtime
        .execute_source("res.createSpriteSheet('A.dat',false,false)")
        .unwrap_err();
    assert!(error.to_string().contains("missing.pvr"));
    let resources = runtime.resource_runtime.lock().unwrap();
    assert!(!resources.sprite_sheets.contains("A"));
    assert!(!resources.sprite_sheet_values.contains_key("A"));
    assert!(!resources.sprite_entries.contains_key("S"));
    assert!(!resources.sprite_sheet_texture_sources.contains_key("A"));
    assert_eq!(resources.next_sprite_sheet_identity, next_id);
}

#[test]
fn invalid_sheet_image_reload_preserves_old_image_identity_and_geometry() {
    let files = Files::new();
    files.image("same.pvr");
    files.sheet("A", "same.pvr", 2);
    let runtime = files.runtime();
    runtime
        .execute_source("res.createSpriteSheet('A.dat'); res.drawSprite('S',0,0)")
        .unwrap();
    let old = bound_image(&runtime);
    let old_id = runtime
        .resource_runtime
        .lock()
        .unwrap()
        .sprite_sheet_identities["A"];
    files.sheet("A", "same.pvr", 9);
    fs::write(files.data().join("same.pvr"), b"corrupt image").unwrap();
    assert!(
        runtime
            .execute_source("res.createSpriteSheet('A.dat',true)")
            .is_err()
    );
    let current = bound_image(&runtime);
    assert_eq!(old.owner.identity(), current.owner.identity());
    assert_eq!(old.source, current.source);
    assert!(Arc::ptr_eq(
        old.image.as_ref().unwrap(),
        current.image.as_ref().unwrap()
    ));
    let resources = runtime.resource_runtime.lock().unwrap();
    assert_eq!(resources.sprite_sheet_identities["A"], old_id);
    assert_eq!(resources.sprite_sheet_values["A"].sprites[0].width, 2);
    drop(resources);
    runtime.execute_source("res.drawSprite('S',8,0)").unwrap();
    assert_eq!(runtime.take_render_commands().len(), 2);
}

#[test]
fn later_sprt_image_failure_does_not_publish_any_prefix_sprites() {
    let files = Files::new();
    files.image("good.pvr");
    let mut first = test_textured_sprite_sheet("PREFIX", "good.pvr", 2, 2);
    let second = test_textured_sprite_sheet("SUFFIX", "missing.pvr", 3, 2);
    first.extend_from_slice(&second[8..]);
    let length = first.len() as u32 - 8;
    first[4..8].copy_from_slice(&length.to_be_bytes());
    fs::write(files.data().join("A.dat"), first).unwrap();
    let runtime = files.runtime();
    let error = runtime
        .execute_source("res.createSpriteSheet('A.dat')")
        .unwrap_err();
    assert!(error.to_string().contains("missing.pvr"));
    let resources = runtime.resource_runtime.lock().unwrap();
    assert!(!resources.sprite_sheets.contains("A"));
    assert!(!resources.sprite_entries.contains_key("PREFIX"));
    assert!(!resources.sprite_entries.contains_key("SUFFIX"));
}

#[test]
fn sheet_image_error_precedes_truncated_sprite_geometry() {
    let files = Files::new();
    let mut payload = 1_u16.to_be_bytes().to_vec();
    payload.extend(test_ka3d_string("missing.pvr"));
    fs::write(files.data().join("A.dat"), test_ka3d(b"SPRT", &payload)).unwrap();
    let runtime = files.runtime();
    let error = runtime
        .execute_source("res.createSpriteSheet('A.dat')")
        .unwrap_err();
    assert!(error.to_string().contains("missing.pvr"), "{error}");
}

#[test]
fn missing_bitmap_image_fails_without_publishing_a_font() {
    let files = Files::new();
    files.font("missing.pvr", 2);
    let runtime = files.runtime();
    let error = runtime
        .execute_source("res.createBitmapFont('F.dat')")
        .unwrap_err();
    assert!(error.to_string().contains("missing.pvr"));
    let resources = runtime.resource_runtime.lock().unwrap();
    assert!(!resources.bitmap_fonts.contains("F"));
    assert!(!resources.bitmap_font_values.contains_key("F"));
    assert!(!resources.native_font_values.contains_key("F"));
    assert!(!resources.bitmap_font_image_owners.contains_key("F"));
}

#[test]
fn invalid_bitmap_image_reload_preserves_selected_ifont_and_queued_glyphs() {
    let files = Files::new();
    files.image("same.pvr");
    files.font("same.pvr", 2);
    let runtime = files.runtime();
    runtime
        .execute_source(
            "res.createBitmapFont('F.dat'); res.useFont('F'); res.drawString('','A',0,0)",
        )
        .unwrap();
    let old_pointer = runtime
        .resource_runtime
        .lock()
        .unwrap()
        .current_font_value
        .as_ref()
        .unwrap()
        .as_ptr();
    files.font("missing.pvr", 9);
    assert!(
        runtime
            .execute_source("res.createBitmapFont('F.dat',true)")
            .is_err()
    );
    let resources = runtime.resource_runtime.lock().unwrap();
    assert_eq!(
        resources.current_font_value.as_ref().unwrap().as_ptr(),
        old_pointer
    );
    assert!(resources.current_native_font().unwrap().is_some());
    assert_eq!(resources.bitmap_font_values["F"].glyphs[0].width, 2);
    drop(resources);
    runtime
        .execute_source("res.drawString('','A',8,0)")
        .unwrap();
    assert_eq!(runtime.take_text_commands().len(), 2);
}

#[test]
fn bitmap_image_error_precedes_truncated_glyph_header() {
    let files = Files::new();
    let mut payload = 1_u16.to_be_bytes().to_vec();
    payload.extend(test_ka3d_string("missing.pvr"));
    fs::write(files.data().join("F.dat"), test_ka3d(b"FONT", &payload)).unwrap();
    let runtime = files.runtime();
    let error = runtime
        .execute_source("res.createBitmapFont('F.dat')")
        .unwrap_err();
    assert!(error.to_string().contains("missing.pvr"), "{error}");
}

#[test]
fn json_image_error_precedes_unsupported_application_and_invalid_frames() {
    let files = Files::new();
    fs::write(
        files.data().join("A.json"),
        br#"{"meta":{"app":"unsupported","image":"missing.pvr"},"frames":false}"#,
    )
    .unwrap();
    let runtime = files.runtime();
    let error = runtime
        .execute_source("res.createSpriteSheet('A.json')")
        .unwrap_err();
    assert!(error.to_string().contains("missing.pvr"), "{error}");
}

#[test]
fn downloaded_image_error_precedes_malformed_descriptor() {
    let files = Files::new();
    fs::write(files.0.join("appdata/A.dat"), b"not KA3D").unwrap();
    let runtime = files.runtime();
    let error = runtime
        .execute_source("_G.Assets.createSpriteSheet('A','A.dat','missing.pvr')")
        .unwrap_err();
    assert!(error.to_string().contains("missing.pvr"), "{error}");
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
fn missing_image_failure_preserves_legacy_texture_accounting() {
    let files = Files::new();
    files.sheet("A", "missing.pvr", 2);
    let runtime = files.runtime();
    runtime.execute_source("g_usedTextureMemory=123").unwrap();
    assert!(
        runtime
            .execute_source("ResourceManager.native_createSpriteSheet('A.dat')")
            .is_err()
    );
    assert_eq!(
        game_environment(runtime.lua())
            .unwrap()
            .get::<i32>("g_usedTextureMemory")
            .unwrap(),
        123
    );
    assert!(
        runtime
            .resource_runtime
            .lock()
            .unwrap()
            .legacy_texture_usage
            .is_empty()
    );
}

#[test]
fn existing_sheet_and_font_skip_constructor_io_until_replacement_is_requested() {
    let files = Files::new();
    files.image("same.pvr");
    files.sheet("A", "same.pvr", 2);
    files.font("same.pvr", 2);
    let runtime = files.runtime();
    runtime
        .execute_source(
            "res.createSpriteSheet('A.dat'); res.createBitmapFont('F.dat'); res.useFont('F')",
        )
        .unwrap();
    let old = bound_image(&runtime);
    fs::remove_file(files.data().join("A.dat")).unwrap();
    fs::remove_file(files.data().join("F.dat")).unwrap();
    fs::remove_file(files.data().join("same.pvr")).unwrap();
    runtime.execute_source("res.createSpriteSheet('A.dat'); res.createBitmapFont('F.dat'); res.drawSprite('S',0,0); res.drawString('','A',0,0)").unwrap();
    assert_eq!(bound_image(&runtime).owner.identity(), old.owner.identity());
    assert_eq!(runtime.take_render_commands().len(), 1);
    assert_eq!(runtime.take_text_commands().len(), 1);
    runtime
        .execute_source("res.releaseSpriteSheet('A.dat',true); res.createSpriteSheet('A.dat')")
        .unwrap();
    let resources = runtime.resource_runtime.lock().unwrap();
    assert!(resources.sprite_sheets.contains("A"));
    assert!(!resources.sprite_sheet_catalog_regions.contains_key("A"));
}

#[test]
fn failed_bitmap_replacement_preserves_selected_system_font() {
    let files = Files::new();
    files.font("missing.pvr", 2);
    let runtime = files.runtime();
    runtime
        .execute_source("res.createSystemFont('F','Helvetica',12,0,0,0,0); res.useFont('F')")
        .unwrap();
    let old_pointer = runtime
        .resource_runtime
        .lock()
        .unwrap()
        .current_font_value
        .as_ref()
        .unwrap()
        .as_ptr();
    assert!(
        runtime
            .execute_source("res.createBitmapFont('F.dat',true)")
            .is_err()
    );
    let resources = runtime.resource_runtime.lock().unwrap();
    assert!(resources.system_fonts.contains_key("F"));
    assert!(!resources.bitmap_fonts.contains("F"));
    assert_eq!(
        resources.current_font_value.as_ref().unwrap().as_ptr(),
        old_pointer
    );
    drop(resources);
    runtime
        .execute_source("res.drawString('','A',0,0)")
        .unwrap();
    assert!(matches!(
        runtime.take_text_commands()[0].font_binding,
        Some(TextFontBinding::System(_))
    ));
}

#[test]
fn later_font_image_failure_preserves_previously_selected_font() {
    let files = Files::new();
    files.image("good.pvr");
    files.font("good.pvr", 2);
    let runtime = files.runtime();
    runtime
        .execute_source("res.createBitmapFont('F.dat'); res.useFont('F')")
        .unwrap();
    let old_pointer = runtime
        .resource_runtime
        .lock()
        .unwrap()
        .current_font_value
        .as_ref()
        .unwrap()
        .as_ptr();
    let mut candidate = test_bitmap_font_with_glyph("good.pvr", 9);
    candidate.extend_from_slice(&test_bitmap_font_with_glyph("missing.pvr", 11)[8..]);
    let size = candidate.len() as u32 - 8;
    candidate[4..8].copy_from_slice(&size.to_be_bytes());
    fs::write(files.data().join("F.dat"), candidate).unwrap();
    let error = runtime
        .execute_source("res.createBitmapFont('F.dat',true)")
        .unwrap_err();
    assert!(error.to_string().contains("missing.pvr"));
    let resources = runtime.resource_runtime.lock().unwrap();
    assert_eq!(
        resources.current_font_value.as_ref().unwrap().as_ptr(),
        old_pointer
    );
    assert_eq!(resources.bitmap_font_values["F"].glyphs[0].width, 2);
}

#[test]
fn failed_reload_preserves_shadow_stack_and_catalog_revision() {
    let files = Files::new();
    files.image("good.pvr");
    files.sheet("A", "good.pvr", 2);
    files.sheet("B", "good.pvr", 4);
    let runtime = files.runtime();
    runtime
        .execute_source("res.createSpriteSheet('A.dat'); res.createSpriteSheet('B.dat')")
        .unwrap();
    let revision = runtime.sprite_catalog_snapshot_since(0).unwrap().revision;
    let old = bound_image(&runtime);
    files.sheet("A", "missing.pvr", 9);
    assert!(
        runtime
            .execute_source("res.createSpriteSheet('A.dat',true)")
            .is_err()
    );
    assert!(runtime.sprite_catalog_snapshot_since(revision).is_none());
    assert_eq!(bound_image(&runtime).owner.identity(), old.owner.identity());
    runtime
        .execute_source("res.releaseSpriteSheet('B.dat',false)")
        .unwrap();
    assert_eq!(
        runtime.resource_runtime.lock().unwrap().sprite_sheet_values["A"].sprites[0].width,
        2
    );
    assert!(bound_image(&runtime).image.is_some());
}

#[test]
fn geometry_failure_after_valid_image_does_not_replace_sheet() {
    let files = Files::new();
    files.image("good.pvr");
    files.sheet("A", "good.pvr", 2);
    let runtime = files.runtime();
    runtime
        .execute_source("res.createSpriteSheet('A.dat')")
        .unwrap();
    let old = bound_image(&runtime);
    let mut payload = 1_u16.to_be_bytes().to_vec();
    payload.extend(test_ka3d_string("good.pvr"));
    fs::write(files.data().join("A.dat"), test_ka3d(b"SPRT", &payload)).unwrap();
    let error = runtime
        .execute_source("res.createSpriteSheet('A.dat',true)")
        .unwrap_err();
    assert!(!error.to_string().contains("Failed to load image"));
    assert_eq!(bound_image(&runtime).owner.identity(), old.owner.identity());
    assert_eq!(
        runtime.resource_runtime.lock().unwrap().sprite_sheet_values["A"].sprites[0].width,
        2
    );
}

#[test]
fn json_missing_and_empty_image_do_not_bypass_constructor() {
    let files = Files::new();
    let runtime = files.runtime();
    fs::write(
        files.data().join("A.json"),
        br#"{"meta":{"app":"unsupported"},"frames":[]}"#,
    )
    .unwrap();
    let error = runtime
        .execute_source("res.createSpriteSheet('A.json')")
        .unwrap_err();
    assert!(
        error
            .to_string()
            .contains("JSON field \"image\" must be a string"),
        "{error}"
    );
    fs::write(
        files.data().join("A.json"),
        br#"{"meta":{"image":"","app":"unsupported"},"frames":[]}"#,
    )
    .unwrap();
    let error = runtime
        .execute_source("res.createSpriteSheet('A.json')")
        .unwrap_err();
    assert!(
        error.to_string().contains("Failed to open image"),
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
fn downloaded_binary_loader_uses_supplied_image_and_ignores_descriptor_extension() {
    let files = Files::new();
    files.image("actual.pvr");
    fs::rename(
        files.data().join("actual.pvr"),
        files.0.join("appdata/actual.pvr"),
    )
    .unwrap();
    fs::write(
        files.0.join("appdata/A.json"),
        test_textured_sprite_sheet("S", "missing-embedded.pvr", 2, 2),
    )
    .unwrap();
    let runtime = files.runtime();
    runtime
        .execute_source("_G.Assets.createSpriteSheet('A','A.json','actual.pvr')")
        .unwrap();
    let image = bound_image(&runtime);
    assert!(image.source.ends_with("actual.pvr"));
    assert_eq!(image.dimensions, Some([16, 8]));
    assert_eq!(
        runtime.resource_runtime.lock().unwrap().sprite_sheet_values["A"].textures,
        ["actual.pvr"]
    );
}
