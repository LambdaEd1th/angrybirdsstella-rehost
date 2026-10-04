//! Scene setTexture borrows an Image; only submitted draws retain pixels.
use super::lifetime::{assert_reclaimed, empty, prepare, size};
use super::*;

fn scene(runtime: &StellaLua, sprite: &str, fill: &str) {
    runtime.execute_source(&format!("createNonPhysicsObject('left','{sprite}',0,0,3); createNonPhysicsObject('right','{sprite}',0.1,0,3); setTexture('left','{fill}'); setTexture('right','{fill}')")).unwrap();
}

fn resources(files: &Files, runtime: &StellaLua) {
    files.image(OLD);
    runtime
        .execute_source("res.createSpriteSheet('MASK.dat')")
        .unwrap();
    files.image(NEW);
    runtime
        .execute_source("res.createSpriteSheet('FILL.dat')")
        .unwrap();
}

#[test]
fn undrawn_masked_scene_does_not_own_its_fill_image() {
    for retain_sheet in [false, true] {
        let files = Files::new();
        files.sheet("MASK");
        files.sheet("FILL");
        let runtime = StellaLua::new_with_resolution(files.data(), 4, 2).unwrap();
        resources(&files, &runtime);
        scene(&runtime, "MASK", "FILL");
        let owner = {
            let snapshot = runtime.sprite_catalog_snapshot_since(0).unwrap();
            Arc::downgrade(snapshot.regions["FILL"].image_owner.as_ref().unwrap())
        };
        runtime
            .execute_source(&format!(
                "res.releaseSpriteSheet('FILL.dat',{retain_sheet})"
            ))
            .unwrap();
        assert_eq!(owner.strong_count(), 0, "setTexture retained the Image");
        assert!(runtime.execute_source("drawGameNative()").is_err());
    }
}

#[test]
fn submitted_masked_pixels_survive_release_but_new_draws_fail() {
    let files = Files::new();
    files.sheet("MASK");
    files.sheet("FILL");
    let runtime = StellaLua::new_with_resolution(files.data(), 4, 2).unwrap();
    resources(&files, &runtime);
    scene(&runtime, "MASK", "FILL");
    runtime
        .execute_source("drawGameNative(); res.releaseSpriteSheet('FILL.dat',true)")
        .unwrap();
    let error = runtime
        .execute_source("drawGameNative(); afterInvalidFill = true")
        .unwrap_err();
    assert!(error.to_string().contains("no current Image"));
    runtime
        .execute_source("assert(afterInvalidFill == nil); res.releaseSpriteSheet('MASK.dat',false)")
        .unwrap();
    let mut assets = assets(&files);
    let frame = prepare(&runtime, &mut assets);
    let mut renderer = GpuRenderer::headless(size()).unwrap();
    assert_halves(
        &renderer
            .render_to_rgba(&assets, &frame, [0, 255, 0])
            .unwrap(),
        NEW,
        NEW,
    );
    empty(&mut assets, &mut renderer);
    assert_halves(
        &renderer
            .render_to_rgba(&assets, &frame, [0, 255, 0])
            .unwrap(),
        NEW,
        NEW,
    );
    drop(frame);
    empty(&mut assets, &mut renderer);
    assert_reclaimed(&assets, &renderer);
}

#[test]
fn replacing_a_fill_requires_an_explicit_scene_texture_rebind() {
    let files = Files::new();
    files.sheet("MASK");
    files.sheet("FILL");
    let runtime = StellaLua::new_with_resolution(files.data(), 4, 2).unwrap();
    resources(&files, &runtime);
    scene(&runtime, "MASK", "FILL");
    runtime.execute_source("drawGameNative()").unwrap();
    let mut assets = assets(&files);
    let old = prepare(&runtime, &mut assets);
    files.image(OLD);
    runtime
        .execute_source("res.createSpriteSheet('FILL.dat',true)")
        .unwrap();
    assert!(
        runtime
            .execute_source("drawGameNative()")
            .unwrap_err()
            .to_string()
            .contains("released SpriteSheet")
    );
    runtime
        .execute_source("setTexture('left','FILL'); setTexture('right','FILL'); drawGameNative()")
        .unwrap();
    let current = prepare(&runtime, &mut assets);
    let mut renderer = GpuRenderer::headless(size()).unwrap();
    assert_halves(
        &renderer
            .render_to_rgba(&assets, &current, [0, 255, 0])
            .unwrap(),
        OLD,
        OLD,
    );
    assert_halves(
        &renderer.render_to_rgba(&assets, &old, [0, 255, 0]).unwrap(),
        NEW,
        NEW,
    );
}

#[test]
fn a_null_scene_fill_uses_the_ordinary_sprite_branch() {
    let files = Files::new();
    files.sheet("MASK");
    files.image(OLD);
    let runtime = StellaLua::new_with_resolution(files.data(), 4, 2).unwrap();
    runtime
        .execute_source("res.createSpriteSheet('MASK.dat')")
        .unwrap();
    scene(&runtime, "MASK", "MISSING");
    runtime.execute_source("drawGameNative()").unwrap();
    let mut assets = assets(&files);
    assert_halves(&pixels(&runtime, &mut assets), OLD, OLD);
    files.sheet("MISSING");
    files.image(NEW);
    runtime
        .execute_source("res.createSpriteSheet('MISSING.dat'); drawGameNative()")
        .unwrap();
    assert_halves(&pixels(&runtime, &mut assets), OLD, OLD);
    runtime
        .execute_source(
            "setTexture('left','MISSING'); setTexture('right','MISSING'); drawGameNative()",
        )
        .unwrap();
    assert_halves(&pixels(&runtime, &mut assets), NEW, NEW);
}

#[test]
fn composite_scene_ignores_a_released_fill_image() {
    let files = Files::new();
    files.sheet("MASK");
    files.sheet("FILL");
    let mut payload = 2_u16.to_be_bytes().to_vec();
    payload.extend_from_slice(&1_u16.to_be_bytes());
    string(&mut payload, "BODY");
    payload.extend_from_slice(&1_u16.to_be_bytes());
    string(&mut payload, "MASK");
    payload.extend_from_slice(&[0; 6]);
    std::fs::write(
        files.data().join("COMPOSITE.dat"),
        envelope(b"COMP", payload),
    )
    .unwrap();
    let runtime = StellaLua::new_with_resolution(files.data(), 4, 2).unwrap();
    resources(&files, &runtime);
    runtime
        .execute_source("res.createCompoSpriteSet('COMPOSITE.dat')")
        .unwrap();
    scene(&runtime, "BODY", "FILL");
    runtime
        .execute_source("res.releaseSpriteSheet('FILL.dat',false); drawGameNative()")
        .unwrap();
    let mut assets = assets(&files);
    assert_halves(&pixels(&runtime, &mut assets), OLD, OLD);
}

#[test]
fn masked_scene_recapture_reads_current_pixels_and_keeps_old_frames() {
    let files = Files::new();
    files.sheet("MASK");
    files.image(OLD);
    let runtime = StellaLua::new_with_resolution(files.data(), 4, 2).unwrap();
    runtime.execute_source("res.createSpriteSheet('MASK.dat'); drawRect(1,0,0,1,0,0,4,2,true); res.captureSprite('CAP')").unwrap();
    scene(&runtime, "MASK", "CAP");
    runtime.execute_source("drawGameNative()").unwrap();
    let mut assets = assets(&files);
    let mut renderer = GpuRenderer::headless(size()).unwrap();
    let captured = prepare(&runtime, &mut assets);
    assert_eq!(
        renderer
            .render_to_rgba(&assets, &captured, [0, 255, 0])
            .unwrap(),
        [255, 0, 0, 255].repeat(8)
    );
    drop(captured);
    runtime.execute_source("drawGameNative()").unwrap();
    let old = prepare(&runtime, &mut assets);
    runtime
        .execute_source(
            "drawRect(0,0,1,1,0,0,4,2,true); res.captureSprite('CAP'); drawGameNative()",
        )
        .unwrap();
    let current = prepare(&runtime, &mut assets);
    assert_eq!(
        renderer
            .render_to_rgba(&assets, &current, [0, 255, 0])
            .unwrap(),
        [0, 0, 255, 255].repeat(8)
    );
    assert_eq!(
        renderer.render_to_rgba(&assets, &old, [0, 255, 0]).unwrap(),
        [255, 0, 0, 255].repeat(8)
    );
}

#[test]
fn masked_viewport_rejection_precedes_released_image_dereferences() {
    for (x, y, rejected) in [
        (0.2, 0.0, true),
        (-0.1, 0.0, false),
        (0.0, -0.1, true),
        (0.0, 0.1, false),
        (0.0, 0.0, false),
    ] {
        let files = Files::new();
        files.sheet("MASK");
        files.sheet("FILL");
        let runtime = StellaLua::new_with_resolution(files.data(), 4, 2).unwrap();
        resources(&files, &runtime);
        runtime.execute_source(&format!("createNonPhysicsObject('body','MASK',{x},{y},3); setTexture('body','FILL'); res.releaseSpriteSheet('FILL.dat',true); res.releaseSpriteSheet('MASK.dat',true)")).unwrap();
        assert_eq!(
            runtime.execute_source("drawGameNative()").is_ok(),
            rejected,
            "viewport edge at ({x},{y})"
        );
        if rejected {
            assert!(runtime.take_render_commands().is_empty());
        }
    }
}

#[test]
fn masked_scene_nonuniform_rotation_uses_scale_then_rotation_rows() {
    for (x, pivot) in [
        (0.2, "setPivotOffset('body',0,0)"),
        (0.0, "setPivotOffset('body',1,1)"),
    ] {
        let files = Files::new();
        files.sheet("MASK");
        files.sheet("FILL");
        let runtime = StellaLua::new_with_resolution(files.data(), 4, 2).unwrap();
        resources(&files, &runtime);
        runtime.execute_source(&format!("createNonPhysicsObject('body','MASK',{x},0,3); setTexture('body','FILL'); setScale('body',2,1); setRotation('body',1.5707963267948966); {pivot}; drawGameNative()")).unwrap();
        let mut assets = assets(&files);
        assert_halves(&pixels(&runtime, &mut assets), NEW, NEW);
    }
}
