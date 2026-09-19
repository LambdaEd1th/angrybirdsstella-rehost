//! Full shipped scene transitions, distinct from direct native level-container
//! tests. This checks lifecycle execution, not visual equivalence to Purple.

use super::*;

#[test]
fn theme_constructor_fields_are_owned_by_game_lua_before_scripts() {
    let runtime = StellaLua::new("/tmp").unwrap();
    runtime
        .execute_source(
            r#"
        assert(type(rawget(gamelua, "objects")) == "table")
        assert(type(rawget(gamelua, "particles")) == "table")
        assert(rawget(gamelua, "deviceModel") == "ios")
        assert(rawequal(rawget(gamelua, "_G"), _G))
        assert(not rawequal(gamelua, _G))
        -- The host's engine-root aliases must not create a second container.
        assert(rawequal(rawget(gamelua, "objects"), _G.objects))
        assert(rawequal(rawget(gamelua, "particles"), _G.particles))
    "#,
        )
        .unwrap();
}

fn advance(runtime: &StellaLua, frames: usize, stage: &str) {
    for frame in 0..frames {
        runtime
            .update(1.0 / 60.0)
            .unwrap_or_else(|error| panic!("{stage} update frame {frame}: {error}"));
        runtime
            .draw()
            .unwrap_or_else(|error| panic!("{stage} draw frame {frame}: {error}"));
    }
}

fn assert_scene(runtime: &StellaLua, expected_root: &str, expected_level: Option<&str>) {
    // Shipped release initAssertions replaces even _G.assert with a no-op.
    // Evaluate values in GameLua, but perform acceptance checks in Rust.
    let environment = game_environment(runtime.lua()).unwrap();
    let (root, level, transition_gone) = runtime
        .lua()
        .load(
            r#"return menuManager:getRoot().name, levelName,
                notificationsFrame:getChild("levelLoadTransition") == nil"#,
        )
        .set_environment(environment)
        .eval::<(String, Option<String>, bool)>()
        .unwrap();
    assert_eq!(root, expected_root);
    if let Some(expected_level) = expected_level {
        assert_eq!(level.as_deref(), Some(expected_level));
    }
    assert!(transition_gone, "the scene transition must have finished");
}

#[test]
fn shipped_scene_transitions_restart_and_return_to_island_without_fallbacks() {
    let sandbox = ShippedDataSandbox::new("scene-transitions");
    let runtime = StellaLua::new(&sandbox.data_root).unwrap();
    runtime.enable_local_services().unwrap();
    runtime.boot("scripts/game.lua").unwrap();
    advance(&runtime, 600, "startup");

    // The chapter order includes boss entries: ordinal 16 is G01, not L16.
    // Resolve the requested name through the shipped order rather than
    // assuming a visible level number is also an array index.
    let environment = game_environment(runtime.lua()).unwrap();
    let levels = environment
        .get::<mlua::Table>("actions")
        .unwrap()
        .get::<mlua::Table>("myLevels")
        .unwrap()
        .get::<mlua::Table>("Chapter02")
        .unwrap();
    let ordinal = levels
        .sequence_values::<String>()
        .position(|name| name.unwrap() == "Chapter02_L16")
        .expect("shipped Chapter02 order must contain L16")
        + 1;
    runtime
        .execute_source(&format!(
            "LevelLoad.transitionToLevel('Chapter02', {ordinal})"
        ))
        .unwrap();
    advance(&runtime, 360, "chapter02-entry");
    assert_scene(&runtime, "GameScene", Some("Chapter02_L16"));
    runtime
        .execute_source("menuManager:getRoot():triggerRestart()")
        .unwrap();
    advance(&runtime, 360, "restart");
    assert_scene(&runtime, "GameScene", Some("Chapter02_L16"));
    runtime
        .execute_source("LevelLoad.transitionToMenu('IslandMap')")
        .unwrap();
    advance(&runtime, 360, "return-to-island");
    // gotoMenuLevel loads IslandMap as a level inside GameScene; the map UI
    // is an additional child, not the root scene itself.
    assert_scene(&runtime, "GameScene", Some("IslandMap"));
    assert!(runtime.fallback_calls.lock().unwrap().is_empty());
    assert!(runtime.compatibility_bindings.lock().unwrap().is_empty());
}

mod levels;
mod shot;

mod tap;

mod retry;

mod hold;

mod earned_retry;
mod practice;
mod progression;
