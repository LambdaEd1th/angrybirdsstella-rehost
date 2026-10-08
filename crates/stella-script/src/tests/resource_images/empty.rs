//! Native empty candidates still replace resources, with undefined fields guarded.

use super::*;

fn empty(files: &Files, name: &str) {
    fs::write(files.data().join(name), test_ka3d(b"JUNK", b"ignored")).unwrap();
}

#[test]
fn empty_sheet_is_published_without_an_image_or_sprites() {
    let files = Files::new();
    empty(&files, "A.dat");
    let runtime = files.runtime();
    runtime
        .execute_source("res.createSpriteSheet('A.dat')")
        .unwrap();
    let resources = runtime.resource_runtime.lock().unwrap();
    assert!(resources.sprite_sheets.contains("A"));
    assert!(resources.sprite_sheet_values["A"].sprites.is_empty());
    assert!(!resources.sprite_sheet_image_owners.contains_key("A"));
    assert!(resources.sprite_entries.is_empty());
}

#[test]
fn empty_sheet_reload_removes_old_sprites_and_reveals_the_previous_sheet() {
    let files = Files::new();
    files.image("same.pvr");
    files.sheet("B", "same.pvr", 9);
    files.sheet("A", "same.pvr", 2);
    let runtime = files.runtime();
    runtime.execute_source("res.createSpriteSheet('B.dat'); res.createSpriteSheet('A.dat'); res.drawSprite('S',0,0)").unwrap();
    let old_id = runtime
        .resource_runtime
        .lock()
        .unwrap()
        .sprite_sheet_identities["A"];
    empty(&files, "A.dat");
    runtime
        .execute_source("res.createSpriteSheet('A.dat',true); res.drawSprite('S',0,0)")
        .unwrap();
    let resources = runtime.resource_runtime.lock().unwrap();
    assert!(resources.sprite_sheets.contains("A"));
    assert_ne!(resources.sprite_sheet_identities["A"], old_id);
    assert_eq!(resources.sprite_entries["S"].last().unwrap().owner, "B");
    assert!(!resources.sprite_sheet_image_owners.contains_key("A"));
    drop(resources);
    let commands = runtime.take_render_commands();
    assert_eq!(commands.len(), 2);
    assert_eq!(commands[0].bound_region.as_ref().unwrap().sprite.width, 2);
    assert_eq!(commands[1].bound_region.as_ref().unwrap().sprite.width, 9);
}

#[test]
fn empty_bitmap_font_publishes_cached_zero_metrics_and_guards_uninitialized_spacing() {
    let files = Files::new();
    empty(&files, "F.dat");
    let runtime = files.runtime();
    runtime.execute_source("res.createBitmapFont('F.dat'); res.useFont('F'); assert(res.getFontHeight()==0); assert(res.getFontMaxAscending()==0); assert(res.getFontMaxDescending()==0); assert(res.getStringWidth('')==0)").unwrap();
    for method in [
        "res.getFontLeading()",
        "res.getFontTracking()",
        "res.getStringWidth('xx')",
        "res.drawString('','xx',0,0,'RIGHT','TOP')",
        "res.drawString('','xx',0,0,'HCENTER','TOP')",
    ] {
        let error = runtime.execute_source(method).unwrap_err();
        assert!(error.to_string().contains("uninitialized"), "{error}");
        assert!(runtime.take_text_commands().is_empty());
    }
    runtime
        .execute_source(
            "assert(res.getStringWidth('x')==0); res.drawString('','x',0,0,'LEFT','TOP'); res.drawString('','x',0,0,'RIGHT','TOP'); res.drawString('','x',0,0,'HCENTER','TOP'); res.drawString('','',0,0,'RIGHT','TOP')",
        )
        .unwrap();
    let resources = runtime.resource_runtime.lock().unwrap();
    assert!(resources.bitmap_fonts.contains("F"));
    assert!(resources.bitmap_font_values["F"].glyphs.is_empty());
    assert!(!resources.bitmap_font_image_owners.contains_key("F"));
    assert!(!resources.bitmap_font_decoded_images.contains_key("F"));
}

#[test]
fn empty_bitmap_replacement_retires_selected_font_until_reselection() {
    let files = Files::new();
    files.image("same.pvr");
    files.font("same.pvr", 2);
    let runtime = files.runtime();
    runtime
        .execute_source(
            "res.createBitmapFont('F.dat'); res.useFont('F'); res.drawString('','A',0,0)",
        )
        .unwrap();
    empty(&files, "F.dat");
    runtime
        .execute_source("res.createBitmapFont('F.dat',true)")
        .unwrap();
    let error = runtime.execute_source("res.getFontHeight()").unwrap_err();
    assert!(error.to_string().contains("released IFont"), "{error}");
    runtime.execute_source("res.useFont('F'); assert(res.getFontHeight()==0); res.drawString('','missing',0,0,'LEFT','TOP')").unwrap();
    assert_eq!(runtime.take_text_commands().len(), 2);
}

#[test]
fn malformed_empty_candidate_preserves_existing_font_and_sheet() {
    let files = Files::new();
    files.image("same.pvr");
    files.font("same.pvr", 2);
    files.sheet("A", "same.pvr", 2);
    let runtime = files.runtime();
    runtime
        .execute_source(
            "res.createBitmapFont('F.dat'); res.useFont('F'); res.createSpriteSheet('A.dat')",
        )
        .unwrap();
    let before = bound_image(&runtime);
    for name in ["A.dat", "F.dat"] {
        fs::write(files.data().join(name), b"KA3D").unwrap();
    }
    assert!(
        runtime
            .execute_source("res.createBitmapFont('F.dat',true)")
            .is_err()
    );
    assert!(
        runtime
            .execute_source("res.createSpriteSheet('A.dat',true)")
            .is_err()
    );
    assert_eq!(
        bound_image(&runtime).owner.identity(),
        before.owner.identity()
    );
    runtime
        .execute_source("assert(res.getStringWidth('A')==2); res.drawSprite('S',0,0)")
        .unwrap();
}

#[test]
fn assets_empty_descriptor_still_constructs_the_explicit_image_first() {
    let files = Files::new();
    empty(&files, "A.dat");
    fs::rename(files.data().join("A.dat"), files.0.join("appdata/A.dat")).unwrap();
    let runtime = files.runtime();
    let error = runtime
        .execute_source("_G.Assets.createSpriteSheet('download','A.dat','missing.pvr')")
        .unwrap_err();
    assert!(error.to_string().contains("missing.pvr"), "{error}");
    assert!(
        !runtime
            .resource_runtime
            .lock()
            .unwrap()
            .sprite_sheets
            .contains("download")
    );
    files.image("same.pvr");
    fs::rename(
        files.data().join("same.pvr"),
        files.0.join("appdata/same.pvr"),
    )
    .unwrap();
    runtime
        .execute_source("_G.Assets.createSpriteSheet('download','A.dat','same.pvr')")
        .unwrap();
    let resources = runtime.resource_runtime.lock().unwrap();
    assert!(resources.sprite_sheets.contains("download"));
    assert!(resources.sprite_sheet_values["download"].sprites.is_empty());
    assert!(resources.sprite_sheet_image_owners.contains_key("download"));
    assert!(
        resources.sprite_sheet_image_cells["download"]
            .bind()
            .snapshot()
            .unwrap()
            .image
            .is_some()
    );
}

#[test]
fn empty_sheet_capture_is_temporary_and_does_not_restore_an_image_or_sprite() {
    let files = Files::new();
    empty(&files, "A.dat");
    let runtime = files.runtime();
    runtime
        .execute_source(
            "res.createSpriteSheet('A.dat'); res.captureSprite('A'); res.captureSprite('A')",
        )
        .unwrap();
    let captures = runtime.take_capture_commands();
    assert_eq!(captures.len(), 2);
    assert!(captures.iter().all(|capture| capture.temporary));
    assert_ne!(captures[0].texture_source, captures[1].texture_source);
    let resources = runtime.resource_runtime.lock().unwrap();
    assert!(resources.sprite_sheet_values["A"].sprites.is_empty());
    assert!(!resources.sprite_sheet_image_owners.contains_key("A"));
    assert!(!resources.sprite_entries.contains_key("A"));
}

#[test]
fn empty_font_ui_leading_failure_precedes_transform_and_alpha_writes() {
    let files = Files::new();
    empty(&files, "F.dat");
    let runtime = files.runtime();
    runtime
        .execute_source("res.createBitmapFont('F.dat'); res.useFont('F')")
        .unwrap();
    let before = runtime.render.lock().unwrap().state;
    let error = runtime.execute_source("drawUITextNative({visible=true,x=0,y=0,scaleX=2,scaleY=3,width=8,font='F',hanchor='LEFT',vanchor='TOP',group='',text='missing'},0,0,1,1,0,0.5)").unwrap_err();
    assert!(
        error.to_string().contains("uninitialized native leading"),
        "{error}"
    );
    assert!(runtime.take_text_commands().is_empty());
    let after = runtime.render.lock().unwrap().state;
    assert_eq!(
        (
            after.alpha,
            after.scale_x,
            after.scale_y,
            after.pivot_x,
            after.pivot_y
        ),
        (
            before.alpha,
            before.scale_x,
            before.scale_y,
            before.pivot_x,
            before.pivot_y
        )
    );
}

#[test]
fn glyph_free_projected_text_restores_model_and_projection_after_success() {
    let files = Files::new();
    empty(&files, "F.dat");
    let runtime = files.runtime();
    runtime.execute_source("res.createBitmapFont('F.dat'); res.useFont('F'); drawString3D('','missing',1,2,3,0.25,0,0,0.5)").unwrap();
    let bridge = runtime.render.lock().unwrap();
    assert!(!bridge.perspective_projection);
    assert!(bridge.state.custom_model.is_none());
    assert_eq!(bridge.state.alpha, 0.5);
    drop(bridge);
    runtime
        .execute_source("res.drawString('','missing',0,0,'LEFT','TOP')")
        .unwrap();
    let commands = runtime.take_text_commands();
    assert_eq!(commands.len(), 2);
    assert!(commands[0].projection_3d.unwrap().custom_model);
    assert_eq!(commands[0].projection_3d.unwrap().x, 1.0);
    assert!(commands[1].projection_3d.is_none());
}
