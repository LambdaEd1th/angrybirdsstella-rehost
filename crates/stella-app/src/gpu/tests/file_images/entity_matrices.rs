//! Literal Metal framebuffer checks for skin writes and descendant matrices.

use super::*;

fn fixture(children: &str, targets: &str, skins: &str) -> (Files, StellaLua) {
    let files = Files::new();
    files.sheet("A");
    files.sheet("B");
    files.image(OLD);
    std::fs::create_dir(files.data().join("animations")).unwrap();
    let animation = [
        r#"{"children": "#,
        children,
        r#", "comps": [{"type":"game::Animation","data":{"actions":{"idle":{"clips":{"":{"targets": "#,
        targets,
        r#"}}}}}}]}"#,
    ].concat();
    for (name, value) in [
        ("test.anim.json", animation.as_str()),
        ("test.skins.json", skins),
    ] {
        std::fs::write(files.data().join("animations").join(name), value.as_bytes()).unwrap();
    }
    let runtime = StellaLua::new_with_resolution(files.data(), 4, 2).unwrap();
    runtime
        .execute_source("res.createSpriteSheet('A.dat')")
        .unwrap();
    files.image(NEW);
    runtime.execute_source("res.createSpriteSheet('B.dat'); AnimationWrapperNative.loadFromBundle('scene','animations/test.anim.json'); AnimationWrapperNative.start('scene','idle','once')").unwrap();
    (files, runtime)
}

#[test]
fn a_later_translation_track_replaces_the_skin_offset_in_the_actual_framebuffer() {
    let (files, runtime) = fixture(
        r#"[{"name":"SLOT_TEST","comps":[{"type":"game::SpriteComponentCustom"}]}]"#,
        r#"{"SLOT_TEST":{"sprite":{"type":"DiscreteString","keyframes":[[0,"ATTACHMENT"]]},"translation":{"type":"LinearFloat2","keyframes":[[0,[1,1]]]}}}"#,
        r#"{"default":{"TEST":{"ATTACHMENT":{"name":"A","x":40,"y":40}}}}"#,
    );
    runtime
        .execute_source("AnimationWrapperNative.draw('scene')")
        .unwrap();
    assert_halves(
        &pixels(&runtime, &mut assets(&files)),
        OLD,
        [0, 255, 0, 255],
    );
}

#[test]
fn a_child_sprite_inherits_its_parent_slots_applied_skin_matrix() {
    let (files, runtime) = fixture(
        r#"[{"name":"SLOT_PARENT","comps":[{"type":"game::SpriteComponentCustom"}],"children":[{"name":"SLOT_CHILD","comps":[{"type":"game::SpriteComponentCustom"}]}]}]"#,
        r#"{"SLOT_PARENT":{"sprite":{"type":"DiscreteString","keyframes":[[0,"PARENT"]]}},"SLOT_CHILD":{"sprite":{"type":"DiscreteString","keyframes":[[0,"CHILD"]]}}}"#,
        r#"{"default":{"PARENT":{"PARENT":{"name":"A","x":1,"y":1}},"CHILD":{"CHILD":{"name":"B","x":1,"y":0}}}}"#,
    );
    runtime
        .execute_source("AnimationWrapperNative.draw('scene')")
        .unwrap();
    let background = [0, 255, 0, 255];
    let expected = [OLD, NEW, NEW, background, OLD, NEW, NEW, background].concat();
    assert_eq!(pixels(&runtime, &mut assets(&files)), expected);
}
