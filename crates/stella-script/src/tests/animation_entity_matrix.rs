//! Installed Lua regressions for EntityTarget's ordered, retained matrix writes.

use super::*;

struct Fixture(PathBuf);

impl Fixture {
    fn new(mut actions: serde_json::Value, attachment: serde_json::Value) -> Self {
        let id = NEXT_TEST_SPRITE_SHEET_ID.fetch_add(1, Ordering::Relaxed);
        let root =
            std::env::temp_dir().join(format!("stella-entity-matrix-{}-{id}", std::process::id()));
        fs::create_dir_all(root.join("data/animations")).unwrap();
        fs::create_dir_all(root.join("data/images")).unwrap();
        fs::create_dir_all(root.join("appdata")).unwrap();
        fs::write(
            root.join("data/images/SHEET.dat"),
            test_textured_sprite_sheet("TEST_SPRITE", "atlas.pvr", 2, 2),
        )
        .unwrap();
        fs::write(
            root.join("data/images/atlas.pvr"),
            test_rgba_pvr(64, 64, [255; 4]),
        )
        .unwrap();
        for action in actions.as_object_mut().unwrap().values_mut() {
            let targets = action["clips"][""]["targets"].as_object_mut().unwrap();
            let slot = targets
                .entry("SLOT_TEST")
                .or_insert_with(|| serde_json::json!({}));
            slot.as_object_mut().unwrap().entry("sprite").or_insert_with(|| {
                serde_json::json!({"type": "DiscreteString", "keyframes": [[0, "ATTACHMENT"]]})
            });
        }
        let document = serde_json::json!({
            "children": [{"name": "JOINT", "children": [{
                "name": "SLOT_TEST",
                "comps": [{"type": "game::SpriteComponentCustom"}],
                "children": [{"name": "CHILD"}]
            }]}],
            "comps": [{"type": "game::Animation", "data": {"actions": actions}}]
        });
        let skins = serde_json::json!({"default": {"TEST": {"ATTACHMENT": attachment}}});
        for (name, value) in [("test.anim.json", document), ("test.skins.json", skins)] {
            fs::write(
                root.join("data/animations").join(name),
                serde_json::to_vec(&value).unwrap(),
            )
            .unwrap();
        }
        Self(root)
    }

    fn runtime(&self) -> StellaLua {
        let runtime = StellaLua::new(self.0.join("data")).unwrap();
        runtime
            .execute_source(
                "res.createSpriteSheet('images/SHEET.dat'); \
                 AnimationWrapperNative.loadFromBundle('scene','animations/test.anim.json')",
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

fn action(targets: serde_json::Value) -> serde_json::Value {
    serde_json::json!({"clips": {"": {"targets": targets}}})
}

fn float(value: f64) -> serde_json::Value {
    serde_json::json!({"type": "LinearFloat", "keyframes": [[0, value]]})
}

fn float2(x: f64, y: f64) -> serde_json::Value {
    serde_json::json!({"type": "LinearFloat2", "keyframes": [[0, [x, y]]]})
}

fn identity_skin() -> serde_json::Value {
    serde_json::json!({"name": "TEST_SPRITE"})
}

fn start(runtime: &StellaLua, name: &str) {
    runtime
        .execute_source(&format!(
            "AnimationWrapperNative.start('scene','{name}','once')"
        ))
        .unwrap();
}

fn pair(runtime: &StellaLua, method: &str, entity: &str) -> [f64; 2] {
    runtime
        .execute_source(&format!(
            "probe_x, probe_y = AnimationWrapperNative.{method}('scene','{entity}')"
        ))
        .unwrap();
    let env = game_environment(runtime.lua()).unwrap();
    [env.get("probe_x").unwrap(), env.get("probe_y").unwrap()]
}

fn draw(runtime: &StellaLua) -> Vec<RenderCommand> {
    runtime
        .execute_source("AnimationWrapperNative.draw('scene')")
        .unwrap();
    runtime.take_render_commands()
}

#[test]
fn ordinary_scale_keeps_native_normalized_basis_rounding_in_draw_and_queries() {
    let fixture = Fixture::new(
        serde_json::json!({"idle": action(serde_json::json!({"JOINT": {
            "rotation": float(0.002), "scale": float2(1.0, 1.0)
        }}))}),
        identity_skin(),
    );
    let runtime = fixture.runtime();
    start(&runtime, "idle");
    let commands = draw(&runtime);
    assert_eq!(commands.len(), 1);
    // Literal libSystem float results of the IDA verified sincosf ->
    // FMUL/FMADD/FSQRT/FDIV normalization sequence. Raw sincosf differs.
    let basis = commands[0].state.matrix.unwrap().map(f32::to_bits);
    assert_eq!(basis, [0x3f7f_ffe0, 0xbb03_126a, 0x3b03_126a, 0x3f7f_ffe0]);
    assert_eq!(
        pair(&runtime, "getEntityScale", "JOINT"),
        [f64::from(f32::from_bits(0x3f80_0001)); 2]
    );
    runtime
        .execute_source("AnimationWrapperNative.pause('scene'); AnimationWrapperNative.update(0)")
        .unwrap();
    assert_eq!(
        draw(&runtime)[0].state.matrix.unwrap().map(f32::to_bits),
        basis
    );
}

#[test]
fn rotation_group_created_after_scale_replaces_the_existing_basis() {
    let fixture = Fixture::new(
        serde_json::json!({
            "scale": action(serde_json::json!({"JOINT": {"scale": float2(2.0, 3.0), "translation": float2(7.0, 9.0)}})),
            "rotation": action(serde_json::json!({"JOINT": {"rotation": float(0.0)}}))
        }),
        identity_skin(),
    );
    let runtime = fixture.runtime();
    start(&runtime, "scale");
    assert_eq!(pair(&runtime, "getEntityScale", "JOINT"), [2.0, 3.0]);
    start(&runtime, "rotation");
    assert_eq!(pair(&runtime, "getEntityScale", "JOINT"), [1.0, 1.0]);
    assert_eq!(pair(&runtime, "getEntityPosition", "JOINT"), [7.0, 9.0]);
    assert_eq!(
        draw(&runtime)[0].state.matrix.unwrap(),
        [1.0, 0.0, 0.0, 1.0]
    );
    runtime
        .execute_source("AnimationWrapperNative.stop('scene','rotation')")
        .unwrap();
    assert_eq!(pair(&runtime, "getEntityScale", "JOINT"), [2.0, 3.0]);
}

#[test]
fn removing_empty_groups_uses_native_swap_removal_before_new_groups_append() {
    let fixture = Fixture::new(
        serde_json::json!({
            "first": action(serde_json::json!({"JOINT": {"alpha": float(1.0), "rotation": float(0.0)}})),
            "second": action(serde_json::json!({"JOINT": {"scale": float2(2.0, 3.0), "translation": float2(7.0, 9.0)}})),
            "third": action(serde_json::json!({"JOINT": {"rotation": float(0.0), "scale": float2(4.0, 5.0)}}))
        }),
        identity_skin(),
    );
    let runtime = fixture.runtime();
    start(&runtime, "first");
    start(&runtime, "second");
    assert_eq!(pair(&runtime, "getEntityScale", "JOINT"), [2.0, 3.0]);
    runtime
        .execute_source("AnimationWrapperNative.stop('scene','first')")
        .unwrap();
    start(&runtime, "third");
    // [alpha, rotation, scale, translation] -> [translation, scale],
    // then rotation appends after the surviving scale group.
    assert_eq!(pair(&runtime, "getEntityScale", "JOINT"), [1.0, 1.0]);
    assert_eq!(pair(&runtime, "getEntityPosition", "JOINT"), [7.0, 9.0]);
}

#[test]
fn stopping_a_middle_control_preserves_the_usage_state_vector_precedence() {
    let fixture = Fixture::new(
        serde_json::json!({
            "a": action(serde_json::json!({"JOINT": {"translation": float2(1.0, 0.0)}})),
            "b": action(serde_json::json!({"JOINT": {"translation": float2(2.0, 0.0)}})),
            "c": action(serde_json::json!({"JOINT": {"translation": float2(3.0, 0.0)}})),
            "d": action(serde_json::json!({"JOINT": {"translation": float2(4.0, 0.0)}}))
        }),
        identity_skin(),
    );
    let runtime = fixture.runtime();
    for name in ["a", "b", "c", "d"] {
        start(&runtime, name);
    }
    runtime
        .execute_source("AnimationWrapperNative.stop('scene','b')")
        .unwrap();
    assert_eq!(pair(&runtime, "getEntityPosition", "JOINT"), [4.0, 0.0]);
    runtime
        .execute_source("AnimationWrapperNative.stop('scene','d')")
        .unwrap();
    assert_eq!(pair(&runtime, "getEntityPosition", "JOINT"), [3.0, 0.0]);
}

#[test]
fn restarting_a_control_reattaches_its_state_after_other_active_states() {
    let fixture = Fixture::new(
        serde_json::json!({
            "a": action(serde_json::json!({"JOINT": {"translation": float2(1.0, 0.0)}})),
            "b": action(serde_json::json!({"JOINT": {"translation": float2(2.0, 0.0)}}))
        }),
        identity_skin(),
    );
    let runtime = fixture.runtime();
    start(&runtime, "a");
    start(&runtime, "b");
    assert_eq!(pair(&runtime, "getEntityPosition", "JOINT"), [2.0, 0.0]);
    start(&runtime, "a");
    assert_eq!(pair(&runtime, "getEntityPosition", "JOINT"), [1.0, 0.0]);
    runtime
        .execute_source("AnimationWrapperNative.stop('scene','a')")
        .unwrap();
    assert_eq!(pair(&runtime, "getEntityPosition", "JOINT"), [2.0, 0.0]);
}

#[test]
fn skin_replaces_slot_basis_and_later_translation_writes_are_inherited_by_children() {
    let fixture = Fixture::new(
        serde_json::json!({"idle": action(serde_json::json!({
            "SLOT_TEST": {"rotation": float(0.0), "scale": float2(7.0, 9.0), "translation": float2(100.0, 200.0)},
            "CHILD": {"translation": float2(5.0, 6.0)}
        }))}),
        serde_json::json!({"name": "TEST_SPRITE", "x": 3, "y": 4, "scaleX": 2, "scaleY": 3}),
    );
    let runtime = fixture.runtime();
    start(&runtime, "idle");
    assert_eq!(
        pair(&runtime, "getEntityPosition", "SLOT_TEST"),
        [100.0, 200.0]
    );
    assert_eq!(pair(&runtime, "getEntityScale", "SLOT_TEST"), [2.0, 3.0]);
    assert_eq!(
        pair(&runtime, "getEntityWorldPosition", "CHILD"),
        [110.0, 218.0]
    );
    let commands = draw(&runtime);
    assert_eq!(commands.len(), 1);
    assert_eq!((commands[0].x, commands[0].y), (100.0, 200.0));
    assert_eq!(commands[0].state.matrix.unwrap(), [2.0, 0.0, 0.0, 3.0]);
    runtime
        .execute_source(
            "l,t,r,b = AnimationWrapperNative.getEntityWorldBounds('scene','SLOT_TEST')",
        )
        .unwrap();
    let env = game_environment(runtime.lua()).unwrap();
    assert_eq!(
        [
            env.get::<f64>("l").unwrap(),
            env.get("t").unwrap(),
            env.get("r").unwrap(),
            env.get("b").unwrap()
        ],
        [98.0, 197.0, 102.0, 203.0]
    );
}

#[test]
fn missing_sprite_clears_the_pointer_but_preserves_the_slot_matrix_and_child_transform() {
    let fixture = Fixture::new(
        serde_json::json!({"idle": action(serde_json::json!({
            "SLOT_TEST": {"sprite": {"type": "DiscreteString", "keyframes": [[0, "ATTACHMENT"], [1, "MISSING"]]}},
            "CHILD": {"translation": float2(5.0, 6.0)}
        }))}),
        serde_json::json!({"name": "TEST_SPRITE", "x": 3, "y": 4, "scaleX": 2, "scaleY": 3}),
    );
    let runtime = fixture.runtime();
    start(&runtime, "idle");
    assert_eq!(pair(&runtime, "getEntityPosition", "SLOT_TEST"), [3.0, 4.0]);
    assert_eq!(
        pair(&runtime, "getEntityWorldPosition", "CHILD"),
        [13.0, 22.0]
    );
    runtime
        .execute_source("AnimationWrapperNative.seek('scene',1)")
        .unwrap();
    assert!(draw(&runtime).is_empty());
    assert_eq!(pair(&runtime, "getEntityPosition", "SLOT_TEST"), [3.0, 4.0]);
    assert_eq!(pair(&runtime, "getEntityScale", "SLOT_TEST"), [2.0, 3.0]);
    assert_eq!(
        pair(&runtime, "getEntityWorldPosition", "CHILD"),
        [13.0, 22.0]
    );
}

#[test]
fn a_zero_basis_cannot_be_resurrected_by_an_ordinary_scale_track() {
    let fixture = Fixture::new(
        serde_json::json!({
            "zero": action(serde_json::json!({"JOINT": {"scale": float2(0.0, 0.0)}})),
            "scale": action(serde_json::json!({"JOINT": {"scale": float2(2.0, 3.0)}}))
        }),
        identity_skin(),
    );
    let runtime = fixture.runtime();
    start(&runtime, "zero");
    assert_eq!(pair(&runtime, "getEntityScale", "JOINT"), [0.0, 0.0]);
    start(&runtime, "scale");
    assert_eq!(pair(&runtime, "getEntityScale", "JOINT"), [0.0, 0.0]);
    assert_eq!(draw(&runtime)[0].state.matrix.unwrap(), [0.0; 4]);
}

#[test]
fn initial_entity_target_application_precedes_the_wrapper_forced_application() {
    let fixture = Fixture::new(
        serde_json::json!({"idle": action(serde_json::json!({"JOINT": {
            "spineEvent": {"type": "DiscreteString", "keyframes": [[0, "zero:::"], [1, ""]]}
        }}))}),
        identity_skin(),
    );
    let runtime = fixture.runtime();
    runtime.execute_source("events={}; AnimationWrapperNative.setPlaybackEvent('scene',function(_,_,event) events[#events+1]=event end)").unwrap();
    start(&runtime, "idle");
    runtime
        .execute_source("AnimationWrapperNative.update(0)")
        .unwrap();
    let env = game_environment(runtime.lua()).unwrap();
    let events = env.get::<mlua::Table>("events").unwrap();
    assert_eq!(events.raw_len(), 2);
    assert_eq!(events.raw_get::<String>(1).unwrap(), "zero");
    assert_eq!(events.raw_get::<String>(2).unwrap(), "zero");
}

#[test]
fn a_valid_skin_writes_its_matrix_even_when_the_live_sprite_provider_is_empty() {
    let fixture = Fixture::new(
        serde_json::json!({"idle": action(serde_json::json!({
            "CHILD": {"translation": float2(5.0, 6.0)}
        }))}),
        serde_json::json!({"name": "TEST_SPRITE", "x": 3, "y": 4, "scaleX": 2, "scaleY": 3}),
    );
    let runtime = StellaLua::new(fixture.0.join("data")).unwrap();
    runtime
        .execute_source(
            "AnimationWrapperNative.loadFromBundle('scene','animations/test.anim.json')",
        )
        .unwrap();
    start(&runtime, "idle");
    assert!(draw(&runtime).is_empty());
    assert_eq!(pair(&runtime, "getEntityPosition", "SLOT_TEST"), [3.0, 4.0]);
    assert_eq!(
        pair(&runtime, "getEntityWorldPosition", "CHILD"),
        [13.0, 22.0]
    );
    runtime
        .execute_source(
            "res.createSpriteSheet('images/SHEET.dat'); AnimationWrapperNative.update(0)",
        )
        .unwrap();
    assert!(draw(&runtime).is_empty());
    runtime
        .execute_source("AnimationWrapperNative.seek('scene',0)")
        .unwrap();
    let commands = draw(&runtime);
    assert_eq!(commands.len(), 1);
    assert_eq!((commands[0].x, commands[0].y), (3.0, 4.0));
    assert_eq!(commands[0].state.matrix.unwrap(), [2.0, 0.0, 0.0, 3.0]);
}
