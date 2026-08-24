//! Whole-bundle level construction and first-frame regression coverage.

use super::*;

fn shipped_level_names(data_root: &Path) -> Vec<String> {
    let mut levels = Vec::new();
    let level_root = data_root.join("levels");
    for directory in fs::read_dir(&level_root).unwrap() {
        let directory = directory.unwrap().path();
        if !directory.is_dir() {
            continue;
        }
        for entry in fs::read_dir(directory).unwrap() {
            let path = entry.unwrap().path();
            if path.extension().and_then(|value| value.to_str()) != Some("lua") {
                continue;
            }
            let mut relative = path.strip_prefix(data_root).unwrap().to_path_buf();
            relative.set_extension("");
            levels.push(relative.to_string_lossy().replace('\\', "/"));
        }
    }
    levels.sort();
    levels
}

#[test]
fn every_shipped_level_constructs_updates_and_reaches_native_draw() {
    let sandbox = ShippedDataSandbox::new("all-levels");
    let runtime = StellaLua::new(&sandbox.data_root).unwrap();
    runtime.boot("scripts/game.lua").unwrap();
    // gamelogic.initParams changes the native constructor's 1.0f at +0x50c
    // to physicsToWorld (20) before any shipped level is constructed.
    assert_eq!(
        runtime.render.lock().unwrap().physics_simulation_scale,
        20.0
    );
    runtime
        .execute_source("SpriteSheetManager.useGroupSet('INGAME')")
        .unwrap();
    let levels = shipped_level_names(&sandbox.data_root);
    assert_eq!(levels.len(), 149);

    for level in &levels {
        runtime
            .execute_source(&format!("loadLevel({level:?})"))
            .unwrap_or_else(|error| panic!("{level} failed to load: {error}"));
        let unresolved_sprites = runtime
            .render
            .lock()
            .expect("render bridge lock poisoned")
            .scene
            .values()
            .filter(|object| {
                object.sprite_bound
                    && !object.sprite.is_empty()
                    && object.sprite_region.is_none()
                    && object.composite_sprite.as_ref().is_none_or(Vec::is_empty)
            })
            .map(|object| object.sprite.clone())
            .collect::<BTreeSet<_>>();
        assert!(
            unresolved_sprites.is_empty(),
            "{level} retained unresolved native sprites: {unresolved_sprites:?}"
        );
        // This direct container audit bypasses GameScene's surrounding
        // initialization. Empty/noninteractive containers therefore lack
        // the Lua component arrays consumed by the real updatePhysics even
        // though Purple still calls it for an empty b2World. Stub only that
        // callback for this artificial gap; the host frame and Lua update
        // still run, and body-bearing levels exercise the shipped callback.
        let lacks_active_body = !runtime
            .render
            .lock()
            .expect("render bridge lock poisoned")
            .scene
            .values()
            .any(|object| object.moves_during_step() && object.active);
        if lacks_active_body {
            runtime
                .execute_source(
                    r#"
                        native_container_update_physics = updatePhysics
                        native_container_remove_blocks = removeBlocks
                        updatePhysics = function() end
                        removeBlocks = function() end
                    "#,
                )
                .unwrap();
        }
        runtime
            .update(1.0 / 60.0)
            .unwrap_or_else(|error| panic!("{level} failed its first update: {error}"));
        if lacks_active_body {
            runtime
                .execute_source(
                    r#"
                        updatePhysics = native_container_update_physics
                        removeBlocks = native_container_remove_blocks
                        native_container_update_physics = nil
                        native_container_remove_blocks = nil
                    "#,
                )
                .unwrap();
        }
        assert!(
            runtime
                .call_global("drawGameNative")
                .unwrap_or_else(|error| panic!("{level} failed native draw: {error}")),
            "{level} lost the native draw entry"
        );
        runtime.take_render_commands();
        assert!(
            runtime.fallback_calls().is_empty(),
            "{level} invoked compatibility fallbacks: {:?}",
            runtime.fallback_calls()
        );
        assert!(
            runtime.compatibility_bindings().is_empty(),
            "{level} retained compatibility bindings: {:?}",
            runtime.compatibility_bindings()
        );
    }
}
