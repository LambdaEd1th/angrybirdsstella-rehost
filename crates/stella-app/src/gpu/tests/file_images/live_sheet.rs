//! Retained AtlasSprites read a live sheet Image; submitted pixels stay frozen.
use super::lifetime::{assert_reclaimed, empty, prepare, size};
use super::*;

fn scene(runtime: &StellaLua) {
    runtime.execute_source("res.createSpriteSheet('A.dat'); createNonPhysicsObject('left','A',0,0,3); createNonPhysicsObject('right','A',0.1,0,3)").unwrap();
}

#[test]
fn undrawn_scene_objects_do_not_retain_a_cleared_sheet_image() {
    let files = Files::new();
    files.sheet("A");
    files.image(OLD);
    let runtime = StellaLua::new_with_resolution(files.data(), 4, 2).unwrap();
    scene(&runtime);
    let owner = {
        let snapshot = runtime.sprite_catalog_snapshot_since(0).unwrap();
        Arc::downgrade(snapshot.regions["A"].image_owner.as_ref().unwrap())
    };
    runtime
        .execute_source("res.releaseSpriteSheet('A.dat',true)")
        .unwrap();
    assert_eq!(
        owner.strong_count(),
        0,
        "scene geometry retained the cleared Image"
    );
}

#[test]
fn cleared_scene_sheet_rejects_new_draws_and_keeps_submitted_pixels() {
    let files = Files::new();
    files.sheet("A");
    files.image(OLD);
    let runtime = StellaLua::new_with_resolution(files.data(), 4, 2).unwrap();
    scene(&runtime);
    runtime
        .execute_source("drawGameNative(); res.releaseSpriteSheet('A.dat',true)")
        .unwrap();
    let error = runtime
        .execute_source("drawGameNative(); afterInvalidDraw = true")
        .unwrap_err();
    assert!(error.to_string().contains("no current Image"));
    runtime
        .execute_source("assert(afterInvalidDraw == nil)")
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
    drop(frame);
    empty(&mut assets, &mut renderer);
    assert_reclaimed(&assets, &renderer);
}

#[test]
fn sheet_replacement_does_not_rebind_existing_scene_objects() {
    let files = Files::new();
    files.sheet("A");
    files.image(OLD);
    let runtime = StellaLua::new_with_resolution(files.data(), 4, 2).unwrap();
    scene(&runtime);
    runtime
        .execute_source("res.releaseSpriteSheet('A.dat',true); res.createSpriteSheet('A.dat')")
        .unwrap();
    assert!(
        runtime
            .execute_source("drawGameNative()")
            .unwrap_err()
            .to_string()
            .contains("no current Image")
    );
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
    runtime
        .execute_source(
            "native_setSprite('left','A'); native_setSprite('right','A'); drawGameNative()",
        )
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
}

#[test]
fn retained_scene_capture_uses_current_pixels_and_preserves_an_old_frame() {
    let files = Files::new();
    let runtime = StellaLua::new_with_resolution(files.data(), 4, 2).unwrap();
    runtime.execute_source("drawRect(1,0,0,1,0,0,4,2,true); res.captureSprite('CAP'); createNonPhysicsObject('body','CAP',0,0,3); drawGameNative()").unwrap();
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

fn composite(files: &Files) {
    let mut composite = 2_u16.to_be_bytes().to_vec();
    composite.extend_from_slice(&1_u16.to_be_bytes());
    string(&mut composite, "BODY");
    composite.extend_from_slice(&1_u16.to_be_bytes());
    string(&mut composite, "A");
    composite.extend_from_slice(&[0; 6]);
    std::fs::write(
        files.data().join("COMPOSITE.dat"),
        envelope(b"COMP", composite),
    )
    .unwrap();
}

fn retained_consumer(setup: &str, draw: &str) {
    for submitted in [false, true] {
        let files = Files::new();
        files.sheet("A");
        files.image(OLD);
        composite(&files);
        std::fs::write(
            files.data().join("test.anim.json"),
            br#"{
                "children": [{"name": "SLOT_TEST", "comps": [{"type": "game::SpriteComponentCustom"}]}],
                "comps": [{"type": "game::Animation", "data": {"actions": {"idle": {"clips": {"": {"targets": {
                    "SLOT_TEST": {
                        "sprite": {"keyframes": [[0, "A"]]},
                        "alpha": {"keyframes": [[0, 1]]},
                        "zOrder": {"keyframes": [[0, 1]]}
                    }
                }}}}}}}]
            }"#,
        ).unwrap();
        std::fs::write(files.data().join("test.skins.json"), b"{}").unwrap();
        let runtime = StellaLua::new_with_resolution(files.data(), 4, 2).unwrap();
        runtime
            .execute_source("res.createSpriteSheet('A.dat')")
            .unwrap();
        runtime.execute_source(setup).unwrap();
        let owner = {
            let snapshot = runtime.sprite_catalog_snapshot_since(0).unwrap();
            Arc::downgrade(snapshot.regions["A"].image_owner.as_ref().unwrap())
        };
        if submitted {
            runtime.execute_source(draw).unwrap();
        }
        runtime
            .execute_source("res.releaseSpriteSheet('A.dat',true)")
            .unwrap();
        if !submitted {
            assert_eq!(
                owner.strong_count(),
                0,
                "undrawn consumer retained the Image: {setup}"
            );
        }
        assert!(
            runtime
                .execute_source(draw)
                .unwrap_err()
                .to_string()
                .contains("no current Image")
        );
        files.image(NEW);
        runtime
            .execute_source("res.createSpriteSheet('A.dat',true)")
            .unwrap();
        assert!(
            runtime
                .execute_source(draw)
                .unwrap_err()
                .to_string()
                .contains("released SpriteSheet")
        );
        runtime
            .execute_source("res.releaseSpriteSheet('A.dat',false)")
            .unwrap();
        if submitted {
            let mut assets = assets(&files);
            let frame = prepare(&runtime, &mut assets);
            let mut renderer = GpuRenderer::headless(size()).unwrap();
            assert_halves(
                &renderer
                    .render_to_rgba(&assets, &frame, [0, 255, 0])
                    .unwrap(),
                OLD,
                [0, 255, 0, 255],
            );
            drop(frame);
            empty(&mut assets, &mut renderer);
            assert_reclaimed(&assets, &renderer);
            assert_eq!(
                owner.strong_count(),
                0,
                "submitted Image survives its last frame: {setup}"
            );
        }
    }
}

#[test]
fn composite_entries_borrow_live_sheet_and_only_submitted_images_stay_alive() {
    retained_consumer(
        "res.createCompoSpriteSet('COMPOSITE.dat')",
        "res.drawSprite('BODY',0,0)",
    );
}

#[test]
fn composite_scene_objects_borrow_live_sheet_and_preserve_submitted_pixels() {
    retained_consumer(
        "res.createCompoSpriteSet('COMPOSITE.dat'); createNonPhysicsObject('body','BODY',0,0,3)",
        "drawGameNative()",
    );
}

#[test]
fn animation_components_borrow_live_sheet_and_preserve_submitted_pixels() {
    retained_consumer(
        "AnimationWrapperNative.loadFromBundle('scene','test.anim.json'); AnimationWrapperNative.start('scene','idle','repeat'); AnimationWrapperNative.setTranslation('scene',1,1)",
        "AnimationWrapperNative.draw('scene')",
    );
}

#[test]
fn particles_borrow_live_sheet_and_preserve_submitted_pixels() {
    retained_consumer(
        "particleTable = {particles = {retained = {amount=1, sprites={'A'}, lifeTime=-1, gravityX=0, gravityY=0, minVel=0, maxVel=0, minAngleEmitter=0, maxAngleEmitter=0, minAngle=0, maxAngle=0, minAngleVel=0, maxAngleVel=0, minScaleBegin=1, maxScaleBegin=1, minScaleEnd=1, maxScaleEnd=1}}}; particles.native_addParticlesWithMode({definitionName='retained',x=0,y=0,w=0,h=0,angle=0,mode=3})",
        "drawMenuParticlesNative()",
    );
}

#[test]
fn hidden_composite_entries_skip_a_cleared_sheet_but_the_legacy_helper_dereferences_it() {
    let files = Files::new();
    files.sheet("A");
    files.image(OLD);
    composite(&files);
    let runtime = StellaLua::new_with_resolution(files.data(), 4, 2).unwrap();
    runtime.execute_source("res.createSpriteSheet('A.dat'); res.createCompoSpriteSet('COMPOSITE.dat'); res.setCompoSpriteEntry('BODY',0,{visible=false}); res.releaseSpriteSheet('A.dat',true); res.drawSprite('BODY',0,0)").unwrap();
    let mut assets = assets(&files);
    let frame = prepare(&runtime, &mut assets);
    let mut renderer = GpuRenderer::headless(size()).unwrap();
    assert_eq!(
        renderer
            .render_to_rgba(&assets, &frame, [0, 255, 0])
            .unwrap(),
        [0, 255, 0, 255].repeat(8)
    );
    assert_reclaimed(&assets, &renderer);
    // The older GameLua helper deliberately ignores the Entry's visible flag.
    let error = runtime
        .execute_source("drawCompoSprite('BODY',0,0,1,1)")
        .unwrap_err();
    assert!(error.to_string().contains("no current Image"));
}
