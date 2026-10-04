//! Dirt borrows constructor Texture pointers; submitted meshes retain pixels.
use super::lifetime::{assert_reclaimed, empty, prepare, size};
use super::*;

fn dirt(runtime: &StellaLua, background: &str, foreground: &str) {
    runtime
        .execute_source(&format!(
            r#"
                createBox('dirt','A',0,0,2,2,0,0.2,0,true,false,1)
                blocks = {{ DIRT_DEF = {{ components = {{ dirt = {{
                    bgTexture = '{background}', fgTexture = '{foreground}'
                }} }} }} }}
                objects.world.dirt.definition = 'DIRT_DEF'
                dirt_extension = createNativeBlockExtension('dirt','dirt')
            "#
        ))
        .unwrap();
}

#[test]
fn undrawn_dirt_does_not_retain_a_cleared_or_removed_image() {
    for retain_sheet in [true, false] {
        let files = Files::new();
        files.sheet("A");
        files.image(OLD);
        let runtime = StellaLua::new_with_resolution(files.data(), 4, 2).unwrap();
        runtime
            .execute_source("res.createSpriteSheet('A.dat')")
            .unwrap();
        dirt(&runtime, "A", "A");
        let owner = {
            let snapshot = runtime.sprite_catalog_snapshot_since(0).unwrap();
            Arc::downgrade(snapshot.regions["A"].image_owner.as_ref().unwrap())
        };
        runtime
            .execute_source(&format!("res.releaseSpriteSheet('A.dat',{retain_sheet})"))
            .unwrap();
        assert_eq!(owner.strong_count(), 0, "undrawn Dirt retained an Image");
        assert!(runtime.execute_source("drawGameNative()").is_err());
    }
}

#[test]
fn submitted_dirt_keeps_pixels_but_cannot_draw_after_its_image_is_released() {
    let files = Files::new();
    files.sheet("A");
    files.image(OLD);
    let runtime = StellaLua::new_with_resolution(files.data(), 4, 2).unwrap();
    runtime
        .execute_source("res.createSpriteSheet('A.dat')")
        .unwrap();
    dirt(&runtime, "A", "A");
    runtime
        .execute_source("drawGameNative(); res.releaseSpriteSheet('A.dat',true)")
        .unwrap();
    let error = runtime
        .execute_source("drawGameNative(); afterInvalidDirt = true")
        .unwrap_err();
    assert!(error.to_string().contains("no current Image"));
    runtime
        .execute_source("assert(afterInvalidDirt == nil)")
        .unwrap();
    let mut assets = assets(&files);
    let frame = prepare(&runtime, &mut assets);
    let mut renderer = GpuRenderer::headless(size()).unwrap();
    assert_halves(
        &renderer
            .render_to_rgba(&assets, &frame, [0, 255, 0])
            .unwrap(),
        OLD,
        OLD,
    );
    empty(&mut assets, &mut renderer);
    assert_halves(
        &renderer
            .render_to_rgba(&assets, &frame, [0, 255, 0])
            .unwrap(),
        OLD,
        OLD,
    );
    drop(frame);
    empty(&mut assets, &mut renderer);
    assert_reclaimed(&assets, &renderer);
}

#[test]
fn replacing_a_sheet_does_not_rebind_existing_dirt_textures() {
    let files = Files::new();
    files.sheet("A");
    files.image(OLD);
    let runtime = StellaLua::new_with_resolution(files.data(), 4, 2).unwrap();
    runtime
        .execute_source("res.createSpriteSheet('A.dat')")
        .unwrap();
    dirt(&runtime, "A", "A");
    files.image(NEW);
    runtime
        .execute_source("res.createSpriteSheet('A.dat',true)")
        .unwrap();
    assert!(
        runtime
            .execute_source("drawGameNative()")
            .unwrap_err()
            .to_string()
            .contains("released SpriteSheet")
    );
}

#[test]
fn dirt_keeps_its_constructor_texture_under_active_name_shadowing() {
    let files = Files::new();
    files.sheet("A");
    std::fs::copy(files.data().join("A.dat"), files.data().join("B.dat")).unwrap();
    files.image(OLD);
    let runtime = StellaLua::new_with_resolution(files.data(), 4, 2).unwrap();
    runtime
        .execute_source("res.createSpriteSheet('A.dat')")
        .unwrap();
    dirt(&runtime, "A", "A");
    files.image(NEW);
    runtime
        .execute_source("res.createSpriteSheet('B.dat'); drawGameNative()")
        .unwrap();
    let mut assets = assets(&files);
    assert_halves(&pixels(&runtime, &mut assets), OLD, OLD);
    runtime
        .execute_source("res.releaseSpriteSheet('B.dat',false); drawGameNative()")
        .unwrap();
    assert_halves(&pixels(&runtime, &mut assets), OLD, OLD);
}

#[test]
fn initialized_dirt_does_not_requery_a_cleared_shadowing_sheet() {
    let files = Files::new();
    files.sheet("A");
    std::fs::copy(files.data().join("A.dat"), files.data().join("B.dat")).unwrap();
    files.image(OLD);
    let runtime = StellaLua::new_with_resolution(files.data(), 4, 2).unwrap();
    runtime
        .execute_source("res.createSpriteSheet('A.dat')")
        .unwrap();
    dirt(&runtime, "A", "A");
    files.image(NEW);
    runtime.execute_source("res.createSpriteSheet('B.dat'); res.releaseSpriteSheet('B.dat',true); dirt_extension.render(); dirt_extension.checkCollisions(); drawGameNative()").unwrap();
    let mut assets = assets(&files);
    assert_halves(&pixels(&runtime, &mut assets), OLD, OLD);
}

#[test]
fn a_fully_cut_foreground_does_not_dereference_its_released_texture() {
    let files = Files::new();
    files.sheet("A");
    files.sheet("B");
    files.image(OLD);
    let runtime = StellaLua::new_with_resolution(files.data(), 4, 2).unwrap();
    runtime
        .execute_source("res.createSpriteSheet('A.dat'); res.createSpriteSheet('B.dat')")
        .unwrap();
    dirt(&runtime, "A", "B");
    runtime.execute_source("dirt_extension.onCollision(0,0,0,1,100,'impact',0,0); dirt_extension.checkCollisions(); drawGameNative()").unwrap();
    let commands = runtime.take_render_commands();
    let mesh = commands
        .iter()
        .find_map(|command| command.dirt.as_ref())
        .unwrap();
    assert!(mesh.foreground_triangles.iter().all(Vec::is_empty));
    drop(commands);
    runtime
        .execute_source("res.releaseSpriteSheet('B.dat',false); drawGameNative()")
        .unwrap();
    let mut assets = assets(&files);
    assert_halves(&pixels(&runtime, &mut assets), OLD, OLD);
}
