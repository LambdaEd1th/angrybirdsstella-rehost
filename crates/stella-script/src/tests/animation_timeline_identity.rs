//! Installed JSON probes for native Clip/Timeline identity, not merged tracks.

use super::*;

struct Fixture(PathBuf);

impl Fixture {
    fn new(actions: serde_json::Value) -> Self {
        let id = NEXT_TEST_SPRITE_SHEET_ID.fetch_add(1, Ordering::Relaxed);
        let root = std::env::temp_dir().join(format!(
            "stella-timeline-identity-{}-{id}",
            std::process::id()
        ));
        fs::create_dir_all(root.join("data/animations")).unwrap();
        fs::create_dir_all(root.join("appdata")).unwrap();
        let document = serde_json::json!({
            "children": [{"name": "LEFT"}, {"name": "RIGHT"}],
            "comps": [{"type": "game::Animation", "data": {"actions": actions}}]
        });
        fs::write(
            root.join("data/animations/test.anim.json"),
            serde_json::to_vec(&document).unwrap(),
        )
        .unwrap();
        fs::write(root.join("data/animations/test.skins.json"), b"{}").unwrap();
        Self(root)
    }

    fn runtime(&self) -> StellaLua {
        let runtime = StellaLua::new(self.0.join("data")).unwrap();
        runtime
            .execute_source(
                "AnimationWrapperNative.loadFromBundle('scene','animations/test.anim.json'); \
             events={}; AnimationWrapperNative.setPlaybackEvent('scene', \
             function(_,_,event) events[#events+1]=event end)",
            )
            .unwrap();
        runtime
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn event_track(keys: serde_json::Value) -> serde_json::Value {
    serde_json::json!({"type": "DiscreteString", "keyframes": keys})
}

fn events(runtime: &StellaLua) -> Vec<String> {
    game_environment(runtime.lua())
        .unwrap()
        .get::<mlua::Table>("events")
        .unwrap()
        .sequence_values::<String>()
        .map(Result::unwrap)
        .collect()
}

#[test]
fn different_event_targets_retain_independent_initial_and_changed_states() {
    let fixture = Fixture::new(serde_json::json!({"idle":{"clips":{"": {"targets": {
        "LEFT": {"spineEvent": event_track(serde_json::json!([[0,"left:::"],[0.25,"left-next:::"],[1,""]]))},
        "RIGHT": {"spineEvent": event_track(serde_json::json!([[0,"right:::"],[0.5,"right-next:::"],[1,""]]))}
    }}}}}));
    let runtime = fixture.runtime();
    runtime
        .execute_source(
            "AnimationWrapperNative.start('scene','idle','once'); AnimationWrapperNative.update(0)",
        )
        .unwrap();
    let initial = events(&runtime);
    assert_eq!(initial.len(), 4);
    assert_eq!(initial.iter().filter(|event| *event == "left").count(), 2);
    assert_eq!(initial.iter().filter(|event| *event == "right").count(), 2);
    runtime
        .execute_source("events={}; AnimationWrapperNative.update(0.3)")
        .unwrap();
    assert_eq!(events(&runtime), ["left-next"]);
    runtime
        .execute_source("events={}; AnimationWrapperNative.update(0.3)")
        .unwrap();
    assert_eq!(events(&runtime), ["right-next"]);
}

#[test]
fn later_clip_event_state_blocks_earlier_clip_keys_on_the_same_target() {
    let fixture = Fixture::new(serde_json::json!({"idle":{"clips":{
        "a": {"targets": {"LEFT": {"spineEvent": event_track(serde_json::json!([[0,"early:::"],[0.25,"early-next:::"],[1.5,""]]))}}},
        "z": {"targets": {"LEFT": {"spineEvent": event_track(serde_json::json!([[0,"late:::"],[1,"late-next:::"],[1.5,""]]))}}}
    }}}));
    let runtime = fixture.runtime();
    runtime
        .execute_source(
            "AnimationWrapperNative.start('scene','idle','once'); AnimationWrapperNative.update(0)",
        )
        .unwrap();
    assert_eq!(events(&runtime), ["late", "late"]);
    runtime
        .execute_source("events={}; AnimationWrapperNative.update(0.3)")
        .unwrap();
    assert!(events(&runtime).is_empty());
    runtime
        .execute_source("AnimationWrapperNative.update(0.8)")
        .unwrap();
    assert_eq!(events(&runtime), ["late-next"]);
}

#[test]
fn empty_later_event_key_masks_earlier_clip_without_erasing_other_targets() {
    let fixture = Fixture::new(serde_json::json!({"idle":{"clips":{
        "a": {"targets": {"LEFT": {"spineEvent": event_track(serde_json::json!([[0,"early:::"],[0.75,"early-next:::"],[2,""]]))}}},
        "z": {"targets": {
            "LEFT": {"spineEvent": event_track(serde_json::json!([[0,"late:::"],[0.5,""]]))},
            "RIGHT": {"spineEvent": event_track(serde_json::json!([[0,"right:::"],[1,"right-next:::"],[2,""]]))}
        }}
    }}}));
    let runtime = fixture.runtime();
    runtime.execute_source("AnimationWrapperNative.start('scene','idle','once'); AnimationWrapperNative.update(0); events={}; AnimationWrapperNative.update(0.8)").unwrap();
    assert!(events(&runtime).is_empty());
    runtime
        .execute_source("AnimationWrapperNative.update(0.3)")
        .unwrap();
    assert_eq!(events(&runtime), ["right-next"]);
}

#[test]
fn restart_and_stop_restore_each_targets_own_control_state() {
    let clip = |prefix: &str| {
        serde_json::json!({"clips":{"": {"targets": {
            "LEFT": {"spineEvent": event_track(serde_json::json!([[0,format!("{prefix}-left:::" )],[0.5,format!("{prefix}-next:::" )],[2,""]]))},
            "RIGHT": {"spineEvent": event_track(serde_json::json!([[0,format!("{prefix}-right:::" )],[2,""]]))}
        }}}})
    };
    let fixture = Fixture::new(serde_json::json!({"a":clip("a"),"b":clip("b")}));
    let runtime = fixture.runtime();
    runtime.execute_source("AnimationWrapperNative.start('scene','a','once'); AnimationWrapperNative.update(0.3); AnimationWrapperNative.start('scene','b','once'); AnimationWrapperNative.update(0); events={}; AnimationWrapperNative.stop('scene','b'); AnimationWrapperNative.update(0)").unwrap();
    let restored = events(&runtime);
    assert_eq!(restored.len(), 2);
    assert!(restored.contains(&"a-left".to_owned()));
    assert!(restored.contains(&"a-right".to_owned()));
    runtime
        .execute_source("events={}; AnimationWrapperNative.update(0.3)")
        .unwrap();
    assert_eq!(events(&runtime), ["a-next"]);
    runtime.execute_source("events={}; AnimationWrapperNative.start('scene','a','once'); AnimationWrapperNative.update(0)").unwrap();
    let restarted = events(&runtime);
    // 100410A18 resets Control time before setTime's force 2; all three
    // applications therefore expose the initial event, not the old key.
    assert_eq!(restarted.len(), 6);
    assert_eq!(restarted.iter().filter(|e| *e == "a-left").count(), 3);
    assert!(!restarted.iter().any(|e| e == "a-next"));
    assert_eq!(restarted.iter().filter(|e| *e == "a-right").count(), 3);
}

#[test]
fn stopping_duplicate_clip_states_keeps_native_empty_group_swap_order() {
    let float = |value| serde_json::json!({"type":"LinearFloat","keyframes":[[0,value],[2,value]]});
    let float2 =
        |x, y| serde_json::json!({"type":"LinearFloat2","keyframes":[[0,[x,y]],[2,[x,y]]]});
    let fixture = Fixture::new(serde_json::json!({
        "multi":{"clips":{
            "a":{"targets":{"LEFT":{"alpha":float(1.0),"translation":float2(5.0,6.0)}}},
            "z":{"targets":{"LEFT":{"alpha":float(0.5),"scale":float2(4.0,5.0)}}}
        }},
        "keep":{"clips":{"":{"targets":{"LEFT":{"rotation":float(0.0),"scale":float2(2.0,3.0)}}}}}
    }));
    let runtime = fixture.runtime();
    runtime.execute_source("AnimationWrapperNative.start('scene','multi','once'); AnimationWrapperNative.start('scene','keep','once'); AnimationWrapperNative.stop('scene','multi'); x,y=AnimationWrapperNative.getEntityScale('scene','LEFT')").unwrap();
    let env = game_environment(runtime.lua()).unwrap();
    // Alpha's first Clip leaves a second State alive. Translation removal
    // swaps in Rotation; only then can the second Alpha removal swap Scale.
    // Remaining groups apply Scale before Rotation, which discards scaling.
    assert_eq!(env.get::<f64>("x").unwrap(), 1.0);
    assert_eq!(env.get::<f64>("y").unwrap(), 1.0);
}
