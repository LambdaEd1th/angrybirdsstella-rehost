use super::*;

fn run(runtime: &StellaLua, source: &str) {
    runtime.execute_source(source).unwrap();
}

fn assert_identity(matrix: AnimationAffine) {
    assert_eq!(
        [
            matrix.m00, matrix.m01, matrix.m10, matrix.m11, matrix.x, matrix.y
        ],
        [1.0, 0.0, 0.0, 1.0, 0.0, 0.0]
    );
}

#[test]
fn reload_constructs_fresh_root_and_retains_only_the_old_wrapper_control() {
    let runtime = host_with_scene();
    run(
        &runtime,
        r#"
        AnimationWrapperNative.loadFromBundle("other", "fixture.anim.json")
        AnimationWrapperNative.start("other", "idle", "repeat")
        AnimationWrapperNative.setTranslation("other", 100, 200)
        AnimationWrapperNative.setTranslation("scene", 70, 90)
        AnimationWrapperNative.setScale("scene", -3, 4)
        AnimationWrapperNative.loadFromBundle("scene", "fixture.anim.json")
        reload_playing = AnimationWrapperNative.isPlaying("scene")
        reload_x = AnimationWrapperNative.getEntityPosition("scene", "root")
        AnimationWrapperNative.seek("scene", 1.5)
        AnimationWrapperNative.update(0.25)
        reload_after_x = AnimationWrapperNative.getEntityPosition("scene", "root")
    "#,
    );
    let animation = runtime._animation_runtime.lock().unwrap();
    assert_identity(animation.matrices["scene"]);
    assert!(!animation.descendant_reflections["scene"]);
    assert_eq!(animation.matrices["other"].x, 100.0);
    assert!(animation.playback["scene"].controls.is_empty());
    assert_eq!(
        animation.playback["scene"]
            .current_control()
            .unwrap()
            .elapsed,
        1.5
    );
    let environment = game_environment(runtime.lua()).unwrap();
    assert!(environment.get::<bool>("reload_playing").unwrap());
    assert_eq!(environment.get::<f64>("reload_x").unwrap(), 0.0);
    assert_eq!(environment.get::<f64>("reload_after_x").unwrap(), 0.0);
    drop(animation);
    run(
        &runtime,
        r#"AnimationWrapperNative.start("scene", "idle", "once")"#,
    );
    let animation = runtime._animation_runtime.lock().unwrap();
    assert!(animation.playback["scene"].detached_current.is_none());
    assert_eq!(animation.playback["scene"].controls.len(), 1);
}

#[test]
fn replacement_keeps_old_scene_visible_until_ordered_removal_and_attachment() {
    let runtime = host_with_scene();
    run(
        &runtime,
        r#"
        _G.SkynestStorage.native_setKey("before", "v", function()
            before_reload_x = AnimationWrapperNative.getEntityPosition("scene", "root")
            before_reload_playing = AnimationWrapperNative.isPlaying("scene")
            AnimationWrapperNative.setSpeed("scene", 3)
            _G.SkynestStorage.native_setKey("after", "v", function()
                after_reload_x = AnimationWrapperNative.getEntityPosition("scene", "root")
                after_reload_playing = AnimationWrapperNative.isPlaying("scene")
            end)
        end)
        AnimationWrapperNative.loadFromBundle("scene", "fixture.anim.json")
    "#,
    );
    let globals = game_environment(runtime.lua()).unwrap();
    assert_eq!(globals.get::<f64>("before_reload_x").unwrap(), 20.0);
    assert_eq!(globals.get::<f64>("after_reload_x").unwrap(), 0.0);
    assert!(globals.get::<bool>("before_reload_playing").unwrap());
    assert!(globals.get::<bool>("after_reload_playing").unwrap());
    assert_eq!(
        runtime._animation_runtime.lock().unwrap().playback["scene"]
            .current_control()
            .unwrap()
            .speed,
        3.0
    );
}

#[test]
fn replacement_skin_owner_changes_before_the_scene_attachment_and_is_not_reset_by_it() {
    let runtime = host_with_scene();
    {
        let mut animation = runtime._animation_runtime.lock().unwrap();
        let mut asset = animation.bundle_cache["fixture.anim.json"].clone();
        asset
            .definition
            .skins
            .insert("new-default".into(), AnimationSkin::new());
        asset
            .definition
            .skins
            .insert("selected-during-drain".into(), AnimationSkin::new());
        animation
            .bundle_cache
            .insert("new-skin.anim.json".into(), asset);
    }
    run(
        &runtime,
        r#"
        _G.SkynestStorage.native_setKey("skin", "v", function()
            skin_before_x = AnimationWrapperNative.getEntityPosition("scene", "root")
            AnimationWrapperNative.setSkin("scene", "selected-during-drain")
        end)
        AnimationWrapperNative.loadFromBundle("scene", "new-skin.anim.json")
    "#,
    );
    assert_eq!(
        game_environment(runtime.lua())
            .unwrap()
            .get::<f64>("skin_before_x")
            .unwrap(),
        20.0
    );
    let animation = runtime._animation_runtime.lock().unwrap();
    assert_eq!(animation.skins["scene"], "selected-during-drain");
    assert!(animation.skin_sets["scene"].contains_key("selected-during-drain"));
}

#[test]
fn caught_scheduler_error_does_not_make_the_queued_replacement_visible_early() {
    let runtime = host_with_scene();
    run(
        &runtime,
        r#"
        _G.SkynestStorage.native_setKey("failure", "v", function() error("reload-drain") end)
        reload_ok = pcall(AnimationWrapperNative.loadFromBundle, "scene", "fixture.anim.json")
        caught_x = AnimationWrapperNative.getEntityPosition("scene", "root")
    "#,
    );
    let environment = game_environment(runtime.lua()).unwrap();
    assert!(!environment.get::<bool>("reload_ok").unwrap());
    assert_eq!(environment.get::<f64>("caught_x").unwrap(), 20.0);
    run(
        &runtime,
        r#"
        AnimationWrapperNative.update(0)
        recovered_x = AnimationWrapperNative.getEntityPosition("scene", "root")
    "#,
    );
    assert_eq!(environment.get::<f64>("recovered_x").unwrap(), 0.0);
    assert!(
        runtime
            ._animation_runtime
            .lock()
            .unwrap()
            .pending_scene_attachments
            .is_empty()
    );
}

#[test]
fn nested_same_tag_loads_append_concrete_scenes_without_retargeting_old_deletion() {
    let runtime = host_with_scene();
    run(
        &runtime,
        r#"
        _G.SkynestStorage.native_setKey("nested", "v", function()
            AnimationWrapperNative.loadFromBundle("scene", "fixture.anim.json")
            AnimationWrapperNative.setTranslation("scene", 123, 456)
        end)
        AnimationWrapperNative.loadFromBundle("scene", "fixture.anim.json")
    "#,
    );
    {
        let animation = runtime._animation_runtime.lock().unwrap();
        assert_eq!(animation.shadow_scenes["scene"].len(), 1);
        assert_eq!(animation.matrices["scene"].x, 123.0);
    }
    run(&runtime, r#"AnimationWrapperNative.close("scene")"#);
    let animation = runtime._animation_runtime.lock().unwrap();
    assert!(animation.definitions.contains_key("scene"));
    assert_identity(animation.matrices["scene"]);
    assert!(!animation.playback["scene"].wrapper_control_present);
    assert!(animation.shadow_scenes.is_empty());
}

#[test]
fn close_all_during_replacement_cannot_revive_the_old_root() {
    let runtime = host_with_scene();
    run(
        &runtime,
        r#"
        _G.SkynestStorage.native_setKey("clear", "v", function()
            AnimationWrapperNative.closeAll()
        end)
        AnimationWrapperNative.loadFromBundle("scene", "fixture.anim.json")
    "#,
    );
    let animation = runtime._animation_runtime.lock().unwrap();
    assert!(!animation.root_present);
    assert!(animation.definitions.is_empty());
    assert!(animation.shadow_scenes.is_empty());
    assert!(animation.pending_scene_attachments.is_empty());
}

#[test]
fn invalid_animation_or_companion_json_preserves_live_scene_and_generation() {
    let runtime = host_with_scene();
    let generation = runtime._animation_runtime.lock().unwrap().scene_generations["scene"];
    let directory = std::env::temp_dir().join(format!(
        "stella-reload-json-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&directory).unwrap();
    for (file, content) in [
        ("broken.anim.json", "{"),
        ("broken.skins.json", "{}"),
        ("bad_skin.anim.json", "{}"),
        ("bad_skin.skins.json", "{"),
    ] {
        std::fs::write(directory.join(file), content).unwrap();
    }
    // Use a dedicated data root without mutating the shipped bundle.
    let runtime = {
        let fresh = StellaLua::new(&directory).unwrap();
        let source = runtime._animation_runtime.lock().unwrap();
        let asset = source.bundle_cache["fixture.anim.json"].clone();
        install_animation_asset(
            &mut fresh._animation_runtime.lock().unwrap(),
            "scene".into(),
            asset,
            BTreeMap::new(),
            BTreeMap::new(),
            BTreeMap::new(),
        );
        fresh
    };
    run(
        &runtime,
        r#"
        AnimationWrapperNative.start("scene", "idle", "repeat")
        AnimationWrapperNative.seek("scene", 1)
        AnimationWrapperNative.setTranslation("scene", 70, 90)
        missing_ok = pcall(AnimationWrapperNative.loadFromBundle, "scene", "missing.anim.json")
        broken_ok = pcall(AnimationWrapperNative.loadFromBundle, "scene", "broken.anim.json")
        skin_ok = pcall(AnimationWrapperNative.loadFromBundle, "scene", "bad_skin.anim.json")
    "#,
    );
    for flag in ["missing_ok", "broken_ok", "skin_ok"] {
        assert!(
            !game_environment(runtime.lua())
                .unwrap()
                .get::<bool>(flag)
                .unwrap()
        );
    }
    let animation = runtime._animation_runtime.lock().unwrap();
    assert_eq!(animation.scene_generations["scene"], generation);
    assert_eq!(animation.matrices["scene"].x, 70.0);
    assert_eq!(
        animation.playback["scene"]
            .current_control()
            .unwrap()
            .elapsed,
        1.0
    );
    assert!(animation.pending_scene_attachments.is_empty());
    assert!(
        !animation
            .bundle_json_cache
            .contains_key("missing.anim.json")
    );
    assert_eq!(
        animation.bundle_json_cache["broken.anim.json"],
        serde_json::Value::Null
    );
    assert_eq!(
        animation.bundle_json_cache["bad_skin.skins.json"],
        serde_json::Value::Null
    );
    drop(animation);
    // The parser's null cache entry is native state, not a request to retry
    // the file. A second call can build the null/empty scene; clearCache is
    // required before corrected bytes are re-read.
    run(
        &runtime,
        r#"
        cached_null_ok = pcall(AnimationWrapperNative.loadFromBundle, "scene", "broken.anim.json")
    "#,
    );
    assert!(
        game_environment(runtime.lua())
            .unwrap()
            .get::<bool>("cached_null_ok")
            .unwrap()
    );
    assert!(
        runtime._animation_runtime.lock().unwrap().definitions["scene"]
            .actions
            .is_empty()
    );
    std::fs::remove_dir_all(&directory).unwrap();
}

#[test]
fn preload_reads_only_its_named_json_and_load_reuses_the_native_per_file_cache() {
    let directory = std::env::temp_dir().join(format!(
        "stella-reload-preload-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&directory).unwrap();
    std::fs::write(directory.join("scene.anim.json"), b"{}").unwrap();
    let runtime = StellaLua::new(&directory).unwrap();
    run(
        &runtime,
        r#"AnimationWrapperNative.preloadFromBundle("scene.anim.json")"#,
    );
    assert!(!runtime._animation_runtime.lock().unwrap().root_present);
    std::fs::write(directory.join("scene.anim.json"), b"{").unwrap();
    std::fs::write(directory.join("scene.skins.json"), b"{}").unwrap();
    run(
        &runtime,
        r#"AnimationWrapperNative.loadFromBundle("scene", "scene.anim.json")"#,
    );
    assert!(
        runtime
            ._animation_runtime
            .lock()
            .unwrap()
            .definitions
            .contains_key("scene")
    );
    run(
        &runtime,
        r#"
        AnimationWrapperNative.clearCache()
        after_clear_ok = pcall(AnimationWrapperNative.loadFromBundle, "scene", "scene.anim.json")
    "#,
    );
    assert!(
        !game_environment(runtime.lua())
            .unwrap()
            .get::<bool>("after_clear_ok")
            .unwrap()
    );
    assert!(
        runtime
            ._animation_runtime
            .lock()
            .unwrap()
            .definitions
            .contains_key("scene")
    );
    std::fs::remove_dir_all(&directory).unwrap();
}
