//! Native file Image construction owns pixels before deferred GPU drawing.
use super::*;

mod dirt_texture;
mod lifetime;
mod live_sheet;
mod masked_batches;
mod masked_scene;
mod masked_uv;

const OLD: [u8; 4] = [203, 31, 7, 255];
const NEW: [u8; 4] = [29, 17, 83, 255];

struct Files(PathBuf);

impl Files {
    fn new() -> Self {
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let index = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "stella-file-image-{}-{unique}-{index}",
            std::process::id()
        ));
        std::fs::create_dir_all(root.join("data")).unwrap();
        std::fs::create_dir_all(root.join("appdata")).unwrap();
        Self(root)
    }

    fn data(&self) -> PathBuf {
        self.0.join("data")
    }

    fn image(&self, color: [u8; 4]) {
        RgbaImage::from_pixel(2, 2, image::Rgba(color))
            .save(self.data().join("same.png"))
            .unwrap();
    }

    fn sheet(&self, name: &str) {
        let mut payload = 1_u16.to_be_bytes().to_vec();
        string(&mut payload, "same.png");
        payload.extend_from_slice(&1_u16.to_be_bytes());
        string(&mut payload, name);
        for value in [0_u16, 0, 2, 2, 0, 0] {
            payload.extend_from_slice(&value.to_be_bytes());
        }
        std::fs::write(
            self.data().join(format!("{name}.dat")),
            envelope(b"SPRT", payload),
        )
        .unwrap();
    }

    fn font(&self) {
        let mut payload = 1_u16.to_be_bytes().to_vec();
        string(&mut payload, "same.png");
        payload.extend_from_slice(&0_i16.to_be_bytes());
        payload.extend_from_slice(&2_i16.to_be_bytes());
        payload.extend_from_slice(&1_u16.to_be_bytes());
        for value in [u16::from(b'A'), 0, 0, 2, 2, 0] {
            payload.extend_from_slice(&value.to_be_bytes());
        }
        std::fs::write(self.data().join("F.dat"), envelope(b"FONT", payload)).unwrap();
    }

    fn multi_sheet(&self) {
        self.sheet("A");
        self.sheet("B");
        let mut sheet = std::fs::read(self.data().join("A.dat")).unwrap();
        let second = std::fs::read(self.data().join("B.dat")).unwrap();
        sheet.extend_from_slice(&second[8..]);
        let size = sheet.len() as u32 - 8;
        sheet[4..8].copy_from_slice(&size.to_be_bytes());
        let offset = sheet
            .windows(8)
            .rposition(|bytes| bytes == b"same.png")
            .unwrap();
        sheet[offset..offset + 8].copy_from_slice(b"next.png");
        std::fs::write(self.data().join("AB.dat"), sheet).unwrap();
    }
}

impl Drop for Files {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn string(bytes: &mut Vec<u8>, text: &str) {
    bytes.extend_from_slice(&(text.len() as u16).to_be_bytes());
    bytes.extend_from_slice(text.as_bytes());
}

fn envelope(kind: &[u8; 4], payload: Vec<u8>) -> Vec<u8> {
    let mut bytes = b"KA3D".to_vec();
    bytes.extend_from_slice(&(payload.len() as u32 + 8).to_be_bytes());
    bytes.extend_from_slice(kind);
    bytes.extend_from_slice(&(payload.len() as u32).to_be_bytes());
    bytes.extend_from_slice(&payload);
    bytes
}

fn assets(files: &Files) -> AssetCatalog {
    AssetCatalog {
        root: files.data(),
        font_root: files.data(),
        regions: HashMap::new(),
        composites: HashMap::new(),
        masked_textures: HashMap::new(),
        fonts: HashMap::new(),
        textures: HashMap::new(),
        system_labels: Default::default(),
        captures: Default::default(),
        file_images: Default::default(),
    }
}

fn pixels(runtime: &StellaLua, assets: &mut AssetCatalog) -> Vec<u8> {
    let size = GameResolution {
        width: 4,
        height: 2,
    };
    assets
        .apply_sprite_catalog_snapshot(runtime.sprite_catalog_snapshot_since(0).unwrap())
        .unwrap();
    let frame = assets
        .prepare_gpu_frame_at_resolution(
            size,
            &runtime.take_render_commands(),
            &runtime.take_text_commands(),
            &runtime.take_rect_commands(),
            &runtime.take_capture_commands(),
        )
        .unwrap();
    let mut renderer = GpuRenderer::headless(size).unwrap();
    renderer
        .render_to_rgba(assets, &frame, [0, 255, 0])
        .unwrap()
}

fn assert_halves(actual: &[u8], left: [u8; 4], right: [u8; 4]) {
    let expected = [left, left, right, right, left, left, right, right].concat();
    assert_eq!(actual, expected);
}

#[test]
fn native_file_image_constructor_freezes_pixels_before_first_gpu_draw() {
    let files = Files::new();
    files.sheet("A");
    files.sheet("B");
    files.image(OLD);
    let runtime = StellaLua::new_with_resolution(files.data(), 4, 2).unwrap();
    runtime
        .execute_source("res.createSpriteSheet('A.dat')")
        .unwrap();
    files.image(NEW);
    runtime
        .execute_source(
            "res.createSpriteSheet('B.dat'); res.drawSprite('A',0,0); res.drawSprite('B',2,0)",
        )
        .unwrap();
    let mut assets = assets(&files);
    assert_halves(&pixels(&runtime, &mut assets), OLD, NEW);
    std::fs::remove_file(files.data().join("same.png")).unwrap();
    runtime
        .execute_source("res.drawSprite('A',0,0); res.drawSprite('B',2,0)")
        .unwrap();
    assert_halves(&pixels(&runtime, &mut assets), OLD, NEW);
}

#[test]
fn native_file_image_reload_keeps_old_command_and_replaces_warm_file_pixels() {
    let files = Files::new();
    files.sheet("A");
    files.image(OLD);
    let runtime = StellaLua::new_with_resolution(files.data(), 4, 2).unwrap();
    runtime
        .execute_source(
            "res.createSpriteSheet('A.dat'); res.drawSprite('A',0,0); res.drawSprite('A',2,0)",
        )
        .unwrap();
    let mut assets = assets(&files);
    assert_halves(&pixels(&runtime, &mut assets), OLD, OLD);
    runtime.execute_source("res.drawSprite('A',0,0)").unwrap();
    files.image(NEW);
    runtime
        .execute_source("res.createSpriteSheet('A.dat',true); res.drawSprite('A',2,0)")
        .unwrap();
    assert_halves(&pixels(&runtime, &mut assets), OLD, NEW);
}

#[test]
fn native_file_bitmap_font_reload_keeps_old_glyphs_and_replaces_warm_atlas_pixels() {
    let files = Files::new();
    files.font();
    files.image(OLD);
    let runtime = StellaLua::new_with_resolution(files.data(), 4, 2).unwrap();
    runtime.execute_source("res.createBitmapFont('F.dat'); res.useFont('F'); res.drawString('','A',0,0); res.drawString('','A',2,0)").unwrap();
    let mut assets = assets(&files);
    assert_halves(&pixels(&runtime, &mut assets), OLD, OLD);
    runtime
        .execute_source("res.drawString('','A',0,0)")
        .unwrap();
    files.image(NEW);
    runtime
        .execute_source(
            "res.createBitmapFont('F.dat',true); res.useFont('F'); res.drawString('','A',2,0)",
        )
        .unwrap();
    assert_halves(&pixels(&runtime, &mut assets), OLD, NEW);
}

#[test]
fn native_file_image_uses_sheet_current_image_after_later_sprt_record_replaces_it() {
    let files = Files::new();
    // sub_10046AC1C replaces SpriteSheet+0x20 on every SPRT record.
    // Both AtlasSprites read that current Image at draw (sub_100467A00).
    files.multi_sheet();
    files.image(OLD);
    RgbaImage::from_pixel(2, 2, image::Rgba(NEW))
        .save(files.data().join("next.png"))
        .unwrap();
    let runtime = StellaLua::new_with_resolution(files.data(), 4, 2).unwrap();
    runtime
        .execute_source("res.createSpriteSheet('AB.dat')")
        .unwrap();
    std::fs::remove_file(files.data().join("same.png")).unwrap();
    std::fs::remove_file(files.data().join("next.png")).unwrap();
    runtime.execute_source("res.drawSprite('A',0,0); res.drawSprite('B',2,0); res.releaseSpriteSheet('AB.dat',false)").unwrap();
    let mut assets = assets(&files);
    assert_halves(&pixels(&runtime, &mut assets), NEW, NEW);
}

#[test]
fn native_atlas_uvs_keep_constructor_extent_when_current_sheet_image_changes() {
    let files = Files::new();
    files.multi_sheet();
    files.image(OLD);
    RgbaImage::from_fn(4, 2, |x, _| image::Rgba(if x < 2 { OLD } else { NEW }))
        .save(files.data().join("next.png"))
        .unwrap();
    let runtime = StellaLua::new_with_resolution(files.data(), 4, 2).unwrap();
    runtime
        .execute_source("res.createSpriteSheet('AB.dat')")
        .unwrap();
    std::fs::remove_file(files.data().join("same.png")).unwrap();
    std::fs::remove_file(files.data().join("next.png")).unwrap();
    runtime
        .execute_source("res.drawSprite('A',0,0); res.drawSprite('B',2,0)")
        .unwrap();
    let mut assets = assets(&files);
    // A's 2x2 constructor image gives UV 0..1; B's 4x2 image gives 0..0.5.
    // Both draw the final 4x2 image. Each literal row must therefore differ.
    assert_eq!(
        pixels(&runtime, &mut assets),
        [OLD, NEW, OLD, OLD].concat().repeat(2)
    );
}

#[test]
fn native_file_masked_fill_retains_constructor_pixels_after_release_before_gpu_draw() {
    let files = Files::new();
    files.sheet("MASK");
    files.sheet("FILL");
    files.image(OLD);
    let runtime = StellaLua::new_with_resolution(files.data(), 4, 2).unwrap();
    runtime
        .execute_source("res.createSpriteSheet('MASK.dat')")
        .unwrap();
    files.image(NEW);
    runtime.execute_source("res.createSpriteSheet('FILL.dat'); drawSelectedTexturizedObject('MASK','FILL',0,0,1,1); drawSelectedTexturizedObject('MASK','FILL',0.1,0,1,1); res.releaseSpriteSheet('FILL.dat',false); res.releaseSpriteSheet('MASK.dat',false)").unwrap();
    std::fs::remove_file(files.data().join("same.png")).unwrap();
    let mut assets = assets(&files);
    assert_halves(&pixels(&runtime, &mut assets), NEW, NEW);
}
