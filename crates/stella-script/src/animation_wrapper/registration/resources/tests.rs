use super::*;

#[path = "reload_tests.rs"]
mod reload;

fn host_with_scene() -> StellaLua {
    let runtime = StellaLua::new("/tmp").unwrap();
    let asset = AnimationAsset {
        actions: BTreeMap::from([("idle".to_owned(), 2.0)]),
        definition: AnimationDefinition {
            actions: BTreeMap::from([(
                "idle".to_owned(),
                AnimationAction {
                    targets: BTreeMap::from([(
                        "root".to_owned(),
                        AnimationTarget {
                            translation: vec![(0.0, [10.0, 20.0]), (2.0, [30.0, 40.0])],
                            ..AnimationTarget::default()
                        },
                    )]),
                    ..AnimationAction::default()
                },
            )]),
            ..AnimationDefinition::default()
        },
    };
    let mut animation = runtime._animation_runtime.lock().unwrap();
    animation
        .bundle_cache
        .insert("fixture.anim.json".to_owned(), asset.clone());
    install_animation_asset(
        &mut animation,
        "scene".to_owned(),
        asset,
        BTreeMap::new(),
        BTreeMap::new(),
        BTreeMap::new(),
    );
    drop(animation);
    runtime
        .execute_source(
            r#"
        notifyEventManager = function() end
        AnimationWrapperNative.start("scene", "idle", "repeat")
        AnimationWrapperNative.seek("scene", 1)
    "#,
        )
        .unwrap();
    runtime
}

#[test]
fn concrete_close_preserves_scene_until_its_ordered_entity_removal_event() {
    let runtime = host_with_scene();
    runtime
        .execute_source(
            r#"
        local visited = false
        local removed = false
        _G.SkynestStorage.native_setKey("first", "value", function()
            visited = true
            assert(AnimationWrapperNative.containsEntity("scene", "root"))
            assert(not AnimationWrapperNative.isPlaying("scene"))
            local x, y = AnimationWrapperNative.getEntityPosition("scene", "root")
            assert(x == 20 and y == 30)
            _G.SkynestStorage.native_setKey("second", "value", function()
                removed = true
                assert(not AnimationWrapperNative.containsEntity("scene", "root"))
            end)
        end)
        AnimationWrapperNative.close("scene")
        assert(visited and removed)
        assert(not AnimationWrapperNative.containsEntity("scene", "root"))
    "#,
        )
        .unwrap();
}

#[test]
fn close_all_stops_then_flushes_root_between_drains_and_erases_only_wrapper_owners_last() {
    let runtime = host_with_scene();
    runtime
        .execute_source(
            r#"
        local first, second = false, false
        _G.SkynestStorage.native_setKey("first", "value", function()
            first = true
            assert(AnimationWrapperNative.containsEntity("scene", "root"))
            assert(not AnimationWrapperNative.isPlaying("scene"))
            local x, y = AnimationWrapperNative.getEntityPosition("scene", "root")
            assert(x == 10 and y == 20)
            AnimationWrapperNative.resume("scene")
            AnimationWrapperNative.loadFromBundle("middle", "fixture.anim.json")
            _G.SkynestStorage.native_setKey("second", "value", function()
                second = true
                assert(not AnimationWrapperNative.containsEntity("scene", "root"))
                assert(not AnimationWrapperNative.containsEntity("middle", "root"))
                assert(AnimationWrapperNative.isPlaying("scene"))
                AnimationWrapperNative.loadFromBundle("late", "fixture.anim.json")
                AnimationWrapperNative.start("late", "idle", "repeat")
                assert(AnimationWrapperNative.isPlaying("late"))
            end)
        end)
        AnimationWrapperNative.closeAll()
        assert(first and second)
        assert(not AnimationWrapperNative.isPlaying("scene"))
        assert(AnimationWrapperNative.containsEntity("late", "root"))
        assert(not AnimationWrapperNative.isPlaying("late"))
        local before = AnimationWrapperNative.getEntityPosition("late", "root")
        AnimationWrapperNative.update(0.5)
        local after = AnimationWrapperNative.getEntityPosition("late", "root")
        assert(after > before + 4.9)
    "#,
        )
        .unwrap();
}

#[test]
fn close_all_first_drain_error_leaves_stopped_live_scene_for_caught_error() {
    let runtime = host_with_scene();
    runtime
        .execute_source(
            r#"
        _G.SkynestStorage.native_setKey("failure", "value", function()
            error("stop-before-root-flush")
        end)
        local ok, message = pcall(AnimationWrapperNative.closeAll)
        assert(not ok and string.find(tostring(message), "stop-before-root-flush", 1, true))
        assert(AnimationWrapperNative.containsEntity("scene", "root"))
        assert(not AnimationWrapperNative.isPlaying("scene"))
        AnimationWrapperNative.resume("scene")
        assert(AnimationWrapperNative.isPlaying("scene"))
        AnimationWrapperNative.update(0)
        assert(AnimationWrapperNative.containsEntity("scene", "root"))
    "#,
        )
        .unwrap();
}

#[test]
fn queued_entity_removal_keeps_the_concrete_identity_across_tag_replacement() {
    let runtime = host_with_scene();
    runtime
        .execute_source(
            r#"
        _G.SkynestStorage.native_setKey("reload", "value", function()
            AnimationWrapperNative.loadFromBundle("scene", "fixture.anim.json")
            assert(AnimationWrapperNative.containsEntity("scene", "root"))
        end)
        AnimationWrapperNative.close("scene")
        assert(AnimationWrapperNative.containsEntity("scene", "root"))
    "#,
        )
        .unwrap();
}
