//! Last Image references and deferred physical-generation retention.
use super::*;

pub(super) fn size() -> GameResolution {
    GameResolution {
        width: 4,
        height: 2,
    }
}

pub(super) fn prepare(runtime: &StellaLua, assets: &mut AssetCatalog) -> PreparedFrame {
    assets
        .apply_sprite_catalog_snapshot(runtime.sprite_catalog_snapshot_since(0).unwrap())
        .unwrap();
    assets
        .prepare_gpu_frame_at_resolution(
            size(),
            &runtime.take_render_commands(),
            &runtime.take_text_commands(),
            &runtime.take_rect_commands(),
            &runtime.take_capture_commands(),
        )
        .unwrap()
}

pub(super) fn empty(assets: &mut AssetCatalog, renderer: &mut GpuRenderer) {
    let frame = assets
        .prepare_gpu_frame_at_resolution(size(), &[], &[], &[], &[])
        .unwrap();
    renderer
        .render_to_rgba(assets, &frame, [0, 255, 0])
        .unwrap();
}

pub(super) fn assert_reclaimed(assets: &AssetCatalog, renderer: &GpuRenderer) {
    assert!(
        assets.file_images.bindings.is_empty(),
        "dead logical file Images remain cached"
    );
    assert!(
        assets.captures.bindings.is_empty(),
        "dead logical capture Images remain cached"
    );
    assert!(
        assets.textures.is_empty(),
        "dead native file pixels remain cached"
    );
    assert_eq!(
        renderer.textures.len(),
        1,
        "only the renderer's white texture should remain"
    );
    assert!(
        renderer
            .texture_bind_groups
            .keys()
            .all(|(base, fill)| base == WHITE_TEXTURE && fill == WHITE_TEXTURE),
        "released texture views remain owned by bind groups: {:?}",
        renderer.texture_bind_groups.keys().collect::<Vec<_>>()
    );
}

#[test]
fn released_file_image_keeps_prepared_pixels_then_reclaims_its_last_generation() {
    let files = Files::new();
    files.sheet("A");
    files.image(OLD);
    let runtime = StellaLua::new_with_resolution(files.data(), 4, 2).unwrap();
    runtime.execute_source("res.createSpriteSheet('A.dat'); res.drawSprite('A',0,0); res.drawSprite('A',2,0); res.releaseSpriteSheet('A.dat',false)").unwrap();
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
    // A stored deferred frame still owns the physical input it will sample.
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
fn same_file_image_owners_share_pixels_but_release_independently() {
    let files = Files::new();
    files.sheet("A");
    files.sheet("B");
    files.image(OLD);
    let runtime = StellaLua::new_with_resolution(files.data(), 4, 2).unwrap();
    runtime.execute_source("res.createSpriteSheet('A.dat'); res.createSpriteSheet('B.dat'); res.drawSprite('A',0,0); res.drawSprite('B',2,0)").unwrap();
    let mut assets = assets(&files);
    let mut renderer = GpuRenderer::headless(size()).unwrap();
    let frame = prepare(&runtime, &mut assets);
    assert_halves(
        &renderer
            .render_to_rgba(&assets, &frame, [0, 255, 0])
            .unwrap(),
        OLD,
        OLD,
    );
    drop(frame);
    assert_eq!(assets.textures.len(), 1);
    runtime.execute_source("res.releaseSpriteSheet('A.dat',false); res.drawSprite('B',0,0); res.drawSprite('B',2,0)").unwrap();
    let frame = prepare(&runtime, &mut assets);
    assert_halves(
        &renderer
            .render_to_rgba(&assets, &frame, [0, 255, 0])
            .unwrap(),
        OLD,
        OLD,
    );
    drop(frame);
    assert_eq!(assets.file_images.bindings.len(), 1);
    assert_eq!(assets.textures.len(), 1);
    runtime
        .execute_source("res.releaseSpriteSheet('B.dat',false)")
        .unwrap();
    drop(prepare(&runtime, &mut assets));
    empty(&mut assets, &mut renderer);
    assert_reclaimed(&assets, &renderer);
}

#[test]
fn captured_image_releases_its_binding_and_gpu_pixels_after_sheet_release() {
    let files = Files::new();
    let runtime = StellaLua::new_with_resolution(files.data(), 4, 2).unwrap();
    runtime
        .execute_source("drawRect(1,0,0,1,0,0,4,2,true); res.captureSprite('CAP')")
        .unwrap();
    let mut assets = assets(&files);
    let mut renderer = GpuRenderer::headless(size()).unwrap();
    let frame = prepare(&runtime, &mut assets);
    renderer
        .render_to_rgba(&assets, &frame, [0, 255, 0])
        .unwrap();
    drop(frame);
    assert_eq!(assets.captures.bindings.len(), 1);
    let captured_source = assets
        .captures
        .bindings
        .values()
        .next()
        .unwrap()
        .source
        .clone();
    runtime.execute_source("res.drawSprite('CAP',0,0)").unwrap();
    let frame = prepare(&runtime, &mut assets);
    assert_eq!(
        renderer
            .render_to_rgba(&assets, &frame, [0, 255, 0])
            .unwrap(),
        [255, 0, 0, 255].repeat(8)
    );
    assert!(
        renderer
            .texture_bind_groups
            .keys()
            .any(|(base, _)| base == &captured_source),
        "a captured texture view must actually be cached before release"
    );
    drop(frame);
    runtime
        .execute_source("res.releaseSpriteSheet('CAP',false)")
        .unwrap();
    drop(prepare(&runtime, &mut assets));
    empty(&mut assets, &mut renderer);
    assert_reclaimed(&assets, &renderer);
}

#[test]
fn released_bitmap_font_keeps_queued_glyphs_then_reclaims_file_pixels() {
    let files = Files::new();
    files.font();
    files.image(OLD);
    let runtime = StellaLua::new_with_resolution(files.data(), 4, 2).unwrap();
    runtime.execute_source("res.createBitmapFont('F.dat'); res.useFont('F'); res.drawString('','A',0,0); res.drawString('','A',2,0); res.releaseFont('F')").unwrap();
    let mut assets = assets(&files);
    let mut renderer = GpuRenderer::headless(size()).unwrap();
    let frame = prepare(&runtime, &mut assets);
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
fn released_mask_and_fill_keep_both_images_until_deferred_pixels_are_consumed() {
    let files = Files::new();
    files.sheet("MASK");
    files.sheet("FILL");
    files.image(OLD);
    let runtime = StellaLua::new_with_resolution(files.data(), 4, 2).unwrap();
    runtime
        .execute_source("res.createSpriteSheet('MASK.dat')")
        .unwrap();
    files.image(NEW);
    runtime.execute_source("res.createSpriteSheet('FILL.dat'); drawSelectedTexturizedObject('MASK','FILL',0,0,1,1); drawSelectedTexturizedObject('MASK','FILL',0.1,0,1,1); res.releaseSpriteSheet('MASK.dat',false); res.releaseSpriteSheet('FILL.dat',false)").unwrap();
    let mut assets = assets(&files);
    let mut renderer = GpuRenderer::headless(size()).unwrap();
    let frame = prepare(&runtime, &mut assets);
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
fn captured_generation_survives_a_recapture_while_a_deferred_consumer_retains_it() {
    let files = Files::new();
    let runtime = StellaLua::new_with_resolution(files.data(), 4, 2).unwrap();
    let mut assets = assets(&files);
    let mut renderer = GpuRenderer::headless(size()).unwrap();
    runtime
        .execute_source("drawRect(1,0,0,1,0,0,4,2,true); res.captureSprite('CAP')")
        .unwrap();
    let frame = prepare(&runtime, &mut assets);
    renderer
        .render_to_rgba(&assets, &frame, [0, 255, 0])
        .unwrap();
    drop(frame);
    runtime.execute_source("res.drawSprite('CAP',0,0)").unwrap();
    let old_frame = prepare(&runtime, &mut assets);
    let old_source = old_frame.required_textures.iter().next().unwrap().clone();
    runtime
        .execute_source("drawRect(0,0,1,1,0,0,4,2,true); res.captureSprite('CAP')")
        .unwrap();
    let frame = prepare(&runtime, &mut assets);
    renderer
        .render_to_rgba(&assets, &frame, [0, 255, 0])
        .unwrap();
    drop(frame);
    empty(&mut assets, &mut renderer);
    assert_eq!(
        renderer
            .render_to_rgba(&assets, &old_frame, [0, 255, 0])
            .unwrap(),
        [255, 0, 0, 255].repeat(8)
    );
    assert!(renderer.textures.contains_key(&old_source));
    drop(old_frame);
    empty(&mut assets, &mut renderer);
    assert!(!renderer.textures.contains_key(&old_source));
    assert!(
        renderer
            .texture_bind_groups
            .keys()
            .all(|(base, fill)| base != &old_source && fill != &old_source)
    );
    runtime.execute_source("res.drawSprite('CAP',0,0)").unwrap();
    let frame = prepare(&runtime, &mut assets);
    assert_eq!(
        renderer
            .render_to_rgba(&assets, &frame, [0, 255, 0])
            .unwrap(),
        [0, 0, 255, 255].repeat(8)
    );
    drop(frame);
    runtime
        .execute_source("res.releaseSpriteSheet('CAP',false)")
        .unwrap();
    drop(prepare(&runtime, &mut assets));
    empty(&mut assets, &mut renderer);
    assert_reclaimed(&assets, &renderer);
}

#[test]
fn separate_runtimes_and_catalogs_cannot_alias_captured_pixels_on_one_renderer() {
    let files = Files::new();
    let mut renderer = GpuRenderer::headless(size()).unwrap();
    let old_runtime = StellaLua::new_with_resolution(files.data(), 4, 2).unwrap();
    let mut old_assets = assets(&files);
    old_runtime
        .execute_source("drawRect(1,0,0,1,0,0,4,2,true); res.captureSprite('CAP')")
        .unwrap();
    let frame = prepare(&old_runtime, &mut old_assets);
    renderer
        .render_to_rgba(&old_assets, &frame, [0, 255, 0])
        .unwrap();
    drop(frame);
    old_runtime
        .execute_source("res.drawSprite('CAP',0,0)")
        .unwrap();
    let old_frame = prepare(&old_runtime, &mut old_assets);
    let old_logical = old_assets.captures.bindings.keys().next().unwrap().clone();
    let old_physical = old_assets.captures.bindings[&old_logical].source.clone();
    drop(old_assets);
    drop(old_runtime);
    let new_runtime = StellaLua::new_with_resolution(files.data(), 4, 2).unwrap();
    let mut new_assets = assets(&files);
    new_runtime
        .execute_source("drawRect(0,0,1,1,0,0,4,2,true); res.captureSprite('CAP')")
        .unwrap();
    let frame = prepare(&new_runtime, &mut new_assets);
    renderer
        .render_to_rgba(&new_assets, &frame, [0, 255, 0])
        .unwrap();
    drop(frame);
    let (new_logical, new_physical) = new_assets.captures.bindings.iter().next().unwrap();
    assert_ne!(new_logical, &old_logical);
    assert_ne!(new_physical.source, old_physical);
    assert_eq!(
        renderer
            .render_to_rgba(&new_assets, &old_frame, [0, 255, 0])
            .unwrap(),
        [255, 0, 0, 255].repeat(8)
    );
    drop(old_frame);
    empty(&mut new_assets, &mut renderer);
    assert!(!renderer.textures.contains_key(&old_physical));
    new_runtime
        .execute_source("res.drawSprite('CAP',0,0)")
        .unwrap();
    let frame = prepare(&new_runtime, &mut new_assets);
    assert_eq!(
        renderer
            .render_to_rgba(&new_assets, &frame, [0, 255, 0])
            .unwrap(),
        [0, 0, 255, 255].repeat(8)
    );
    drop(frame);
    new_runtime
        .execute_source("res.releaseSpriteSheet('CAP',false)")
        .unwrap();
    drop(prepare(&new_runtime, &mut new_assets));
    empty(&mut new_assets, &mut renderer);
    assert_reclaimed(&new_assets, &renderer);
}

#[test]
fn repeated_file_sheet_load_draw_and_release_returns_cache_to_its_baseline() {
    let files = Files::new();
    files.sheet("A");
    let runtime = StellaLua::new_with_resolution(files.data(), 4, 2).unwrap();
    let mut assets = assets(&files);
    let mut renderer = GpuRenderer::headless(size()).unwrap();
    for color in [OLD, NEW].into_iter().cycle().take(8) {
        files.image(color);
        runtime.execute_source("res.createSpriteSheet('A.dat'); res.drawSprite('A',0,0); res.drawSprite('A',2,0); res.releaseSpriteSheet('A.dat',false)").unwrap();
        let frame = prepare(&runtime, &mut assets);
        assert_halves(
            &renderer
                .render_to_rgba(&assets, &frame, [0, 255, 0])
                .unwrap(),
            color,
            color,
        );
        drop(frame);
        empty(&mut assets, &mut renderer);
        assert_reclaimed(&assets, &renderer);
    }
}
