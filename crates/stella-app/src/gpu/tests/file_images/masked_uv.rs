//! Masked sprites use live Texture extents and raw, unpermuted atlas bounds.
use super::lifetime::size;
use super::*;

const CLEAR: [u8; 4] = [0, 255, 0, 255];
const THIRD: [u8; 4] = [71, 151, 9, 255];
const FOURTH: [u8; 4] = [179, 63, 211, 255];

fn changed_mask(files: &Files, runtime: &StellaLua, dimensions: [u32; 2]) {
    files.multi_sheet();
    files.image(OLD);
    RgbaImage::from_fn(dimensions[0], dimensions[1], |x, y| {
        image::Rgba([255, 255, 255, if x < 2 && y < 2 { 255 } else { 0 }])
    })
    .save(files.data().join("next.png"))
    .unwrap();
    runtime
        .execute_source("res.createSpriteSheet('AB.dat')")
        .unwrap();
    files.sheet("FILL");
    files.image(NEW);
    runtime
        .execute_source("res.createSpriteSheet('FILL.dat')")
        .unwrap();
    std::fs::remove_file(files.data().join("same.png")).unwrap();
    std::fs::remove_file(files.data().join("next.png")).unwrap();
}

fn scene_extent(dimensions: [u32; 2]) {
    let files = Files::new();
    let runtime = StellaLua::new_with_resolution(files.data(), 4, 2).unwrap();
    changed_mask(&files, &runtime, dimensions);
    runtime.execute_source("createNonPhysicsObject('left','A',0,0,3); createNonPhysicsObject('right','B',0.1,0,3); setTexture('left','FILL'); setTexture('right','FILL'); drawGameNative()").unwrap();
    let mut assets = assets(&files);
    assert_halves(&pixels(&runtime, &mut assets), NEW, NEW);
}

#[test]
fn masked_scene_uvs_use_current_mask_texture_width() {
    scene_extent([4, 2]);
}

#[test]
fn masked_scene_uvs_use_current_mask_texture_height() {
    scene_extent([2, 4]);
    scene_extent([4, 4]);
}

#[test]
fn selected_masked_uvs_use_current_mask_texture_extent() {
    for dimensions in [[4, 2], [2, 4], [4, 4]] {
        let files = Files::new();
        let runtime = StellaLua::new_with_resolution(files.data(), 4, 2).unwrap();
        changed_mask(&files, &runtime, dimensions);
        runtime.execute_source("drawSelectedTexturizedObject('A','FILL',0,0,1,1); drawSelectedTexturizedObject('B','FILL',0.1,0,1,1)").unwrap();
        let mut assets = assets(&files);
        assert_halves(&pixels(&runtime, &mut assets), NEW, NEW);
    }
}

fn rotated_pixels(rotation: u8, masked: bool) -> Vec<u8> {
    let files = Files::new();
    files.sheet("MASK");
    RgbaImage::from_fn(2, 2, |x, y| {
        image::Rgba(if masked {
            [255, 255, 255, if x == 0 && y == 0 { 255 } else { 0 }]
        } else {
            [[OLD, NEW], [THIRD, FOURTH]][y as usize][x as usize]
        })
    })
    .save(files.data().join("same.png"))
    .unwrap();
    let runtime = StellaLua::new_with_resolution(files.data(), 4, 2).unwrap();
    runtime
        .execute_source("res.createSpriteSheet('MASK.dat')")
        .unwrap();
    files.sheet("FILL");
    files.image(NEW);
    runtime.execute_source("res.createSpriteSheet('FILL.dat'); drawSelectedTexturizedObject('MASK','FILL',0,0,1,1); drawSelectedTexturizedObject('MASK','FILL',0.1,0,1,1)").unwrap();
    let mut commands = runtime.take_render_commands();
    for command in &mut commands {
        // Exercise the native constructor's four rotation flags with the same
        // raw +0x28/+0x2a/+0x2c/+0x2e fields. Binary SPRT uses flag zero;
        // imported atlas metadata may select the other constructor cases.
        let region = Arc::make_mut(command.bound_region.as_mut().unwrap());
        region.sprite.atlas_rotation = rotation;
        if !masked {
            command.texture = None;
        }
    }
    let mut assets = assets(&files);
    assets
        .apply_sprite_catalog_snapshot(runtime.sprite_catalog_snapshot_since(0).unwrap())
        .unwrap();
    let frame = assets
        .prepare_gpu_frame_at_resolution(size(), &commands, &[], &[], &[])
        .unwrap();
    let mut renderer = GpuRenderer::headless(size()).unwrap();
    renderer
        .render_to_rgba(&assets, &frame, [0, 255, 0])
        .unwrap()
}

#[test]
fn masked_quads_ignore_constructor_uv_permutations() {
    for rotation in 0..=3 {
        assert_eq!(
            rotated_pixels(rotation, true),
            [NEW, CLEAR, NEW, CLEAR, CLEAR, CLEAR, CLEAR, CLEAR].concat(),
            "masked constructor rotation {rotation}"
        );
    }
}

#[test]
fn ordinary_quads_keep_constructor_uv_permutations() {
    for (rotation, expected) in [
        (0, [OLD, NEW, OLD, NEW, THIRD, FOURTH, THIRD, FOURTH]),
        (1, [NEW, FOURTH, NEW, FOURTH, OLD, THIRD, OLD, THIRD]),
        (2, [NEW, OLD, NEW, OLD, FOURTH, THIRD, FOURTH, THIRD]),
        (3, [THIRD, FOURTH, THIRD, FOURTH, OLD, NEW, OLD, NEW]),
    ] {
        assert_eq!(rotated_pixels(rotation, false), expected.concat());
    }
}
