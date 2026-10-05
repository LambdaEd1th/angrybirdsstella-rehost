//! Native named masks accumulate geometry until the scene/helper flush.
use super::*;

const CLEAR: [u8; 4] = [0, 255, 0, 255];
const WHITE: [u8; 4] = [255, 255, 255, 255];

fn load(files: &Files, runtime: &StellaLua, name: &str, color: [u8; 4]) {
    files.sheet(name);
    files.image(color);
    runtime
        .execute_source(&format!("res.createSpriteSheet('{name}.dat')"))
        .unwrap();
}

fn pair(runtime: &StellaLua, fill: &str) {
    runtime.execute_source(&format!("createNonPhysicsObject('left','MASK',0,0,3); createNonPhysicsObject('right','MASK',0.1,0,3); setTexture('left','{fill}'); setTexture('right','{fill}')")).unwrap();
}

#[test]
fn masked_batch_flushes_after_post_callback_ordinary_draws() {
    let files = Files::new();
    let runtime = StellaLua::new_with_resolution(files.data(), 4, 2).unwrap();
    load(&files, &runtime, "MASK", WHITE);
    load(&files, &runtime, "FILL", NEW);
    load(&files, &runtime, "PLAIN", OLD);
    pair(&runtime, "FILL");
    runtime.execute_source("native_setPostDrawFunction('left', function() res.drawSprite('PLAIN',0,0) end); drawGameNative()").unwrap();
    assert_halves(&pixels(&runtime, &mut assets(&files)), NEW, NEW);
}

#[test]
fn masked_batches_flush_in_texture_name_order() {
    let files = Files::new();
    let runtime = StellaLua::new_with_resolution(files.data(), 4, 2).unwrap();
    load(&files, &runtime, "MASK", WHITE);
    load(&files, &runtime, "Z", OLD);
    load(&files, &runtime, "A", NEW);
    runtime.execute_source("createNonPhysicsObject('first','MASK',0,0,3); createNonPhysicsObject('second','MASK',0,0,3); setTexture('first','Z'); setTexture('second','A'); drawGameNative()").unwrap();
    assert_halves(&pixels(&runtime, &mut assets(&files)), OLD, CLEAR);
}

#[test]
fn masked_batch_binds_its_last_raw_fill_image_without_retaining_the_first() {
    let files = Files::new();
    let runtime = StellaLua::new_with_resolution(files.data(), 4, 2).unwrap();
    load(&files, &runtime, "MASK", WHITE);
    load(&files, &runtime, "FILL", OLD);
    pair(&runtime, "FILL");
    let old_owner = {
        let snapshot = runtime.sprite_catalog_snapshot_since(0).unwrap();
        Arc::downgrade(snapshot.regions["FILL"].image_owner.as_ref().unwrap())
    };
    files.image(NEW);
    runtime.execute_source("native_setPostDrawFunction('left',function() res.createSpriteSheet('FILL.dat',true) end); native_setPreDrawFunction('right',function() setTexture('right','FILL') end); drawGameNative()").unwrap();
    assert_eq!(
        old_owner.strong_count(),
        0,
        "a pending mask retained its first fill Image"
    );
    assert_halves(&pixels(&runtime, &mut assets(&files)), NEW, NEW);
}

#[test]
fn masked_batch_keeps_each_fill_uv_extent_before_last_image_rebinding() {
    let files = Files::new();
    let runtime = StellaLua::new_with_resolution(files.data(), 4, 2).unwrap();
    load(&files, &runtime, "MASK", WHITE);
    load(&files, &runtime, "FILL", OLD);
    pair(&runtime, "FILL");
    RgbaImage::from_fn(4, 2, |x, _| image::Rgba(if x < 2 { OLD } else { NEW }))
        .save(files.data().join("same.png"))
        .unwrap();
    runtime.execute_source("native_setPostDrawFunction('left',function() res.createSpriteSheet('FILL.dat',true) end); native_setPreDrawFunction('right',function() setTexture('right','FILL') end); drawGameNative()").unwrap();
    assert_eq!(
        pixels(&runtime, &mut assets(&files)),
        [OLD, NEW, NEW, NEW, OLD, NEW, NEW, NEW].concat()
    );
}

#[test]
fn masked_batch_reads_alpha_after_the_last_post_callback() {
    let files = Files::new();
    let runtime = StellaLua::new_with_resolution(files.data(), 4, 2).unwrap();
    load(&files, &runtime, "MASK", WHITE);
    load(&files, &runtime, "FILL", NEW);
    pair(&runtime, "FILL");
    runtime
        .execute_source(
            "native_setPostDrawFunction('right',function() native_setAlpha(0) end); drawGameNative()",
        )
        .unwrap();
    assert_halves(&pixels(&runtime, &mut assets(&files)), CLEAR, CLEAR);
}

#[test]
fn selected_helper_rebinds_the_last_raw_mask_for_all_pending_geometry() {
    let files = Files::new();
    let runtime = StellaLua::new_with_resolution(files.data(), 4, 2).unwrap();
    load(&files, &runtime, "MASK", WHITE);
    load(&files, &runtime, "MASK2", [255, 255, 255, 0]);
    load(&files, &runtime, "FILL", NEW);
    runtime.execute_source("createNonPhysicsObject('left','MASK',0,0,3); setTexture('left','FILL'); native_setPostDrawFunction('left',function() drawSelectedTexturizedObject('MASK2','FILL',0.1,0,1,1) end); drawGameNative()").unwrap();
    assert_halves(&pixels(&runtime, &mut assets(&files)), CLEAR, CLEAR);
}

#[test]
fn masked_batch_keeps_mask_uvs_when_a_helper_binds_a_different_extent() {
    let files = Files::new();
    let runtime = StellaLua::new_with_resolution(files.data(), 4, 2).unwrap();
    load(&files, &runtime, "MASK", WHITE);
    files.sheet("MASK2");
    RgbaImage::from_fn(4, 2, |x, _| {
        image::Rgba(if x < 2 { WHITE } else { [255, 255, 255, 0] })
    })
    .save(files.data().join("same.png"))
    .unwrap();
    runtime
        .execute_source("res.createSpriteSheet('MASK2.dat')")
        .unwrap();
    load(&files, &runtime, "FILL", NEW);
    runtime.execute_source("createNonPhysicsObject('left','MASK',0,0,3); setTexture('left','FILL'); native_setPostDrawFunction('left',function() drawSelectedTexturizedObject('MASK2','FILL',0.1,0,1,1) end); drawGameNative()").unwrap();
    assert_eq!(
        pixels(&runtime, &mut assets(&files)),
        [NEW, CLEAR, NEW, NEW, NEW, CLEAR, NEW, NEW].concat()
    );
}

#[test]
fn masked_batch_reads_the_clip_after_the_last_post_callback() {
    let files = Files::new();
    let runtime = StellaLua::new_with_resolution(files.data(), 4, 2).unwrap();
    load(&files, &runtime, "MASK", WHITE);
    load(&files, &runtime, "FILL", NEW);
    pair(&runtime, "FILL");
    runtime.execute_source("native_setPostDrawFunction('right',function() res.setClipRect(2,0,2,2) end); drawGameNative()").unwrap();
    assert_halves(&pixels(&runtime, &mut assets(&files)), CLEAR, NEW);
}

#[test]
fn masked_batch_reads_projection_installed_by_the_final_callback() {
    let files = Files::new();
    let runtime = StellaLua::new_with_resolution(files.data(), 4, 2).unwrap();
    load(&files, &runtime, "MASK", WHITE);
    load(&files, &runtime, "FILL", NEW);
    pair(&runtime, "FILL");
    // drawString3D writes projection/model before checking the missing font.
    // This leaves negative homogeneous w for every pending masked vertex.
    runtime.execute_source("native_setPostDrawFunction('right',function() assert(not pcall(drawString3D,'MISSING','A',0,0,-1,0,0,0,1)) end); drawGameNative()").unwrap();
    assert_halves(&pixels(&runtime, &mut assets(&files)), CLEAR, CLEAR);
}

#[test]
fn final_callback_fill_release_is_observed_at_the_masked_flush() {
    let files = Files::new();
    let runtime = StellaLua::new_with_resolution(files.data(), 4, 2).unwrap();
    load(&files, &runtime, "MASK", WHITE);
    load(&files, &runtime, "FILL", NEW);
    pair(&runtime, "FILL");
    let owner = {
        let snapshot = runtime.sprite_catalog_snapshot_since(0).unwrap();
        Arc::downgrade(snapshot.regions["FILL"].image_owner.as_ref().unwrap())
    };
    runtime.execute_source("native_setPostDrawFunction('right',function() res.releaseSpriteSheet('FILL.dat',true) end)").unwrap();
    let error = runtime.execute_source("drawGameNative()").unwrap_err();
    assert!(error.to_string().contains("no current Image"));
    assert_eq!(owner.strong_count(), 0);
    assert!(runtime.take_render_commands().is_empty());
}

#[test]
fn selected_masked_helper_flushes_all_pending_named_batches() {
    let files = Files::new();
    let runtime = StellaLua::new_with_resolution(files.data(), 4, 2).unwrap();
    load(&files, &runtime, "MASK", WHITE);
    load(&files, &runtime, "Z", OLD);
    load(&files, &runtime, "A", NEW);
    runtime.execute_source("createNonPhysicsObject('left','MASK',0,0,3); setTexture('left','Z'); native_setPostDrawFunction('left',function() drawSelectedTexturizedObject('MASK','A',0,0,1,1) end); drawGameNative()").unwrap();
    assert_halves(&pixels(&runtime, &mut assets(&files)), OLD, CLEAR);
}

#[test]
fn post_callback_capture_precedes_pending_masked_geometry() {
    let files = Files::new();
    let runtime = StellaLua::new_with_resolution(files.data(), 4, 2).unwrap();
    load(&files, &runtime, "MASK", WHITE);
    load(&files, &runtime, "FILL", NEW);
    pair(&runtime, "FILL");
    runtime.execute_source("native_setPostDrawFunction('left',function() res.captureSprite('SHOT') end); drawGameNative(); res.drawSprite('SHOT',0,0)").unwrap();
    assert_halves(&pixels(&runtime, &mut assets(&files)), CLEAR, CLEAR);
}

#[test]
fn masked_batches_flush_separately_at_each_native_sheet_boundary() {
    let files = Files::new();
    let runtime = StellaLua::new_with_resolution(files.data(), 4, 2).unwrap();
    load(&files, &runtime, "MASK", WHITE);
    load(&files, &runtime, "MASK2", WHITE);
    load(&files, &runtime, "Z", OLD);
    load(&files, &runtime, "A", NEW);
    runtime.execute_source("createNonPhysicsObject('first','MASK',0,0,3); createNonPhysicsObject('second','MASK2',0,0,3); setTexture('first','Z'); setTexture('second','A'); drawGameNative()").unwrap();
    assert_halves(&pixels(&runtime, &mut assets(&files)), NEW, CLEAR);
}

#[test]
fn culled_nested_mask_branch_flushes_the_outer_pending_batches() {
    let files = Files::new();
    let runtime = StellaLua::new_with_resolution(files.data(), 4, 2).unwrap();
    load(&files, &runtime, "MASK", WHITE);
    load(&files, &runtime, "MASK2", WHITE);
    load(&files, &runtime, "Z", OLD);
    load(&files, &runtime, "A", NEW);
    load(&files, &runtime, "PLAIN", NEW);
    runtime.execute_source("createNonPhysicsObject('outer','MASK',0,0,3); createNonPhysicsObject('culled','MASK2',100,100,4); setTexture('outer','Z'); setTexture('culled','A'); native_setPostDrawFunction('outer',function() native_setZOrderRange(4,5); drawGameNative(); res.drawSprite('PLAIN',0,0) end); native_setZOrderRange(3,4); drawGameNative()").unwrap();
    assert_halves(&pixels(&runtime, &mut assets(&files)), NEW, CLEAR);
}
