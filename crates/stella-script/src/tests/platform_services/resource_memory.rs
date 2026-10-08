//! Legacy counters describe native constructor allocations, not host pixel sharing.

use super::*;

struct Fixture {
    root: PathBuf,
    data: PathBuf,
}

impl Fixture {
    fn new() -> Self {
        let serial = NEXT_TEST_SPRITE_SHEET_ID.fetch_add(1, Ordering::Relaxed);
        let root = std::env::temp_dir().join(format!(
            "stella-resource-memory-{}-{serial}",
            std::process::id()
        ));
        let data = root.join("data");
        fs::create_dir_all(data.join("images")).unwrap();
        fs::create_dir_all(root.join("appdata")).unwrap();
        Self { root, data }
    }

    fn sheet(&self, name: &str, textures: &[&str]) {
        let mut chunks = Vec::new();
        for (index, texture) in textures.iter().enumerate() {
            let chunk = test_textured_sprite_sheet(&format!("{name}_{index}"), texture, 4, 2);
            chunks.extend_from_slice(&chunk[8..]);
        }
        let mut sheet = b"KA3D".to_vec();
        sheet.extend_from_slice(&(chunks.len() as u32).to_be_bytes());
        sheet.extend(chunks);
        fs::write(self.data.join("images").join(name), sheet).unwrap();
    }

    fn pvr(&self, mipmaps: bool) {
        // 4x2 RGBA4444, optionally followed by valid 2x1 and 1x1 mip levels.
        let payload_bytes = if mipmaps { 22 } else { 16 };
        let mut pvr = Vec::new();
        for word in [
            52u32,
            2,
            4,
            if mipmaps { 2 } else { 0 },
            u32::from(stella_assets::pvr::OGL_RGBA_4444) | if mipmaps { 0x100 } else { 0 },
            payload_bytes,
            16,
            0xf000,
            0x0f00,
            0x00f0,
            0x000f,
            u32::from_le_bytes(*b"PVR!"),
            1,
        ] {
            pvr.extend_from_slice(&word.to_le_bytes());
        }
        pvr.resize(52 + payload_bytes as usize, 0);
        fs::write(self.data.join("images/atlas.pvr"), pvr).unwrap();
    }

    fn runtime(&self) -> StellaLua {
        StellaLua::new(&self.data).unwrap()
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

fn texture_memory(runtime: &StellaLua) -> f64 {
    runtime.lua().globals().get("g_usedTextureMemory").unwrap()
}

#[test]
fn shipped_sprt_counter_preserves_its_native_texture_extent() {
    let sandbox = ShippedDataSandbox::new("shipped-texture-memory");
    let runtime = StellaLua::new(&sandbox.data_root).unwrap();
    runtime
        .execute_source(
            "ResourceManager.native_createSpriteSheet('images/1024x768/CONNECTION_SCREEN_SHEET_0.dat')",
        )
        .unwrap();
    assert_eq!(texture_memory(&runtime), 96_350.0);
    let resources = runtime.resource_runtime.lock().unwrap();
    assert_eq!(
        resources.sprite_sheet_decoded_images["CONNECTION_SCREEN_SHEET_0"].len(),
        1
    );
}

#[test]
fn indexed_png_charges_normalized_texture_instead_of_source_indices() {
    let fixture = Fixture::new();
    // Valid 4x2 P8 PNG. GL_Context normalizes the indices to ABGR8888.
    let png = [
        137, 80, 78, 71, 13, 10, 26, 10, 0, 0, 0, 13, 73, 72, 68, 82, 0, 0, 0, 4, 0, 0, 0, 2, 8, 3,
        0, 0, 0, 72, 118, 141, 81, 0, 0, 0, 6, 80, 76, 84, 69, 60, 100, 180, 100, 180, 60, 200,
        101, 2, 158, 0, 0, 0, 14, 73, 68, 65, 84, 120, 156, 99, 96, 96, 4, 66, 16, 1, 0, 0, 28, 0,
        5, 249, 182, 205, 88, 0, 0, 0, 0, 73, 69, 78, 68, 174, 66, 96, 130,
    ];
    fs::write(fixture.data.join("images/indexed.png"), png).unwrap();
    fixture.sheet("indexed.dat", &["indexed.png"]);
    let runtime = fixture.runtime();
    runtime
        .execute_source("ResourceManager.native_createSpriteSheet('images/indexed.dat')")
        .unwrap();
    assert_eq!(texture_memory(&runtime), 32.0);
    let resources = runtime.resource_runtime.lock().unwrap();
    let image = resources.sprite_sheet_decoded_images["indexed"][0]
        .as_ref()
        .unwrap();
    assert_eq!(
        image.layout.pixels,
        stella_assets::surface_format::SurfaceFormat::P8
    );
}

#[test]
fn distinct_sheets_charge_shared_pixels_separately_and_release_by_input_path() {
    let fixture = Fixture::new();
    fixture.pvr(false);
    fixture.sheet("first.dat", &["atlas.pvr"]);
    fixture.sheet("second.dat", &["atlas.pvr"]);
    let runtime = fixture.runtime();
    runtime
        .execute_source("ResourceManager.native_createSpriteSheet('images/first.dat')")
        .unwrap();
    assert_eq!(texture_memory(&runtime), 16.0);
    runtime
        .execute_source("ResourceManager.native_createSpriteSheet('images/second.dat')")
        .unwrap();
    assert_eq!(texture_memory(&runtime), 32.0);
    {
        let resources = runtime.resource_runtime.lock().unwrap();
        let first = resources.sprite_sheet_decoded_images["first"][0]
            .as_ref()
            .unwrap();
        let second = resources.sprite_sheet_decoded_images["second"][0]
            .as_ref()
            .unwrap();
        assert!(
            Arc::ptr_eq(first, second),
            "host immutable pixels may still share"
        );
    }
    // Existing-object lookup succeeds without reopening either constructor input.
    fs::remove_file(fixture.data.join("images/first.dat")).unwrap();
    runtime
        .execute_source("ResourceManager.native_createSpriteSheet('images/first.dat')")
        .unwrap();
    assert_eq!(texture_memory(&runtime), 32.0);
    runtime
        .execute_source("ResourceManager.native_releaseSpriteSheet('images/first.dat')")
        .unwrap();
    assert_eq!(texture_memory(&runtime), 16.0);
    fixture.sheet("first.dat", &["atlas.pvr"]);
    runtime
        .execute_source("ResourceManager.native_createSpriteSheet('images/first.dat')")
        .unwrap();
    assert_eq!(texture_memory(&runtime), 32.0);
    runtime
        .execute_source("ResourceManager.native_releaseSpriteSheet('images/second.dat')")
        .unwrap();
    assert_eq!(texture_memory(&runtime), 16.0);
}

#[test]
fn every_sprt_record_charges_an_allocation_even_when_only_the_last_image_survives() {
    let fixture = Fixture::new();
    fixture.pvr(false);
    fixture.sheet("repeated.dat", &["atlas.pvr", "atlas.pvr"]);
    let runtime = fixture.runtime();
    runtime
        .execute_source("ResourceManager.native_createSpriteSheet('images/repeated.dat')")
        .unwrap();
    assert_eq!(texture_memory(&runtime), 32.0);
    let resources = runtime.resource_runtime.lock().unwrap();
    let images = &resources.sprite_sheet_decoded_images["repeated"];
    assert_eq!(images.len(), 2);
    assert!(images[0].is_none());
    assert!(images[1].is_some());
}

#[test]
fn png_texture_charges_the_native_surface_extent() {
    let fixture = Fixture::new();
    // A valid 4x2, 8-bit RGBA PNG, including stream CRCs.
    let png = [
        137, 80, 78, 71, 13, 10, 26, 10, 0, 0, 0, 13, 73, 72, 68, 82, 0, 0, 0, 4, 0, 0, 0, 2, 8, 6,
        0, 0, 0, 127, 168, 125, 99, 0, 0, 0, 18, 73, 68, 65, 84, 120, 156, 99, 176, 73, 217, 242,
        31, 25, 51, 160, 11, 0, 0, 49, 229, 18, 153, 159, 10, 211, 228, 0, 0, 0, 0, 73, 69, 78, 68,
        174, 66, 96, 130,
    ];
    fs::write(fixture.data.join("images/atlas.png"), png).unwrap();
    fixture.sheet("png.dat", &["atlas.png"]);
    let runtime = fixture.runtime();
    runtime
        .execute_source("ResourceManager.native_createSpriteSheet('images/png.dat')")
        .unwrap();
    assert_eq!(texture_memory(&runtime), 32.0);
}

#[test]
fn legacy_texture_counter_charges_base_allocation_instead_of_pvr_mip_payload() {
    let fixture = Fixture::new();
    fixture.pvr(true);
    fixture.sheet("mips.dat", &["atlas.pvr"]);
    let runtime = fixture.runtime();
    runtime
        .execute_source("ResourceManager.native_createSpriteSheet('images/mips.dat')")
        .unwrap();
    assert_eq!(texture_memory(&runtime), 16.0);
}

#[test]
fn legacy_counter_uses_the_constructor_resource_path() {
    let fixture = Fixture::new();
    fixture.pvr(false);
    fixture.sheet("prefixed.dat", &["atlas.pvr"]);
    let runtime = fixture.runtime();
    runtime
        .execute_source(
            "res.setPath('images'); ResourceManager.native_createSpriteSheet('prefixed.dat')",
        )
        .unwrap();
    assert_eq!(texture_memory(&runtime), 16.0);
    runtime
        .execute_source("ResourceManager.native_releaseSpriteSheet('prefixed.dat')")
        .unwrap();
    assert_eq!(texture_memory(&runtime), 0.0);
}

#[test]
fn prior_lua_resources_load_and_failed_constructor_do_not_publish_a_new_delta() {
    let fixture = Fixture::new();
    fixture.pvr(false);
    fixture.sheet("preloaded.dat", &["atlas.pvr"]);
    fixture.sheet("counted.dat", &["atlas.pvr"]);
    // Foreign roots create empty native sheets. This failed-constructor
    // fixture needs a recognized root with a truncated length instead.
    fs::write(fixture.data.join("images/broken.dat"), b"KA3D").unwrap();
    let runtime = fixture.runtime();
    runtime
        .execute_source("res.createSpriteSheet('images/preloaded.dat')")
        .unwrap();
    runtime
        .execute_source("ResourceManager.native_createSpriteSheet('absent/preloaded.dat')")
        .unwrap();
    assert!(matches!(
        runtime
            .lua()
            .globals()
            .get::<Value>("g_usedTextureMemory")
            .unwrap(),
        Value::Nil
    ));
    runtime
        .execute_source("ResourceManager.native_createSpriteSheet('images/counted.dat')")
        .unwrap();
    assert_eq!(texture_memory(&runtime), 16.0);
    assert!(
        runtime
            .execute_source("ResourceManager.native_createSpriteSheet('images/broken.dat')")
            .is_err()
    );
    assert_eq!(texture_memory(&runtime), 16.0);
    let resources = runtime.resource_runtime.lock().unwrap();
    assert!(resources.sprite_sheets.contains("preloaded"));
    assert!(resources.sprite_sheets.contains("counted"));
    assert!(!resources.sprite_sheets.contains("broken"));
    assert_eq!(resources.legacy_texture_usage.len(), 1);
}

#[test]
fn memory_globals_convert_wrapped_signed_sums_through_float32() {
    let fixture = Fixture::new();
    let runtime = fixture.runtime();
    {
        let mut resources = runtime.resource_runtime.lock().unwrap();
        resources
            .legacy_texture_usage
            .insert("large".into(), 0x7fff_fff0);
        resources.legacy_texture_usage.insert("extra".into(), 32);
    }
    runtime
        .execute_source("ResourceManager.native_releaseSpriteSheet('absent.dat')")
        .unwrap();
    assert_eq!(texture_memory(&runtime), -2_147_483_648.0);
    {
        let mut resources = runtime.resource_runtime.lock().unwrap();
        resources.legacy_texture_usage.clear();
        resources
            .legacy_texture_usage
            .insert("rounded".into(), 16_777_217);
    }
    runtime
        .execute_source("ResourceManager.native_releaseSpriteSheet('absent.dat')")
        .unwrap();
    assert_eq!(texture_memory(&runtime), 16_777_216.0);
    fs::write(fixture.data.join("small.wav"), test_pcm_wav(12)).unwrap();
    runtime
        .resource_runtime
        .lock()
        .unwrap()
        .legacy_audio_usage
        .insert("large".into(), 0x7fff_fffc);
    runtime
        .execute_source("ResourceManager.native_createAudio('small.wav', 'SMALL', false)")
        .unwrap();
    assert_eq!(
        runtime
            .lua()
            .globals()
            .get::<f64>("g_usedAudioMemory")
            .unwrap(),
        -2_147_483_648.0
    );
}
