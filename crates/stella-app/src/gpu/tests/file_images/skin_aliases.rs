//! Real framebuffer checks for exact selected/default animation skin lookup.

use super::*;

fn fixture(default_exact: bool) -> (Files, StellaLua) {
    let files = Files::new();
    files.sheet("A");
    files.sheet("B");
    files.image(OLD);
    std::fs::create_dir(files.data().join("animations")).unwrap();
    let animation = r#"{
        "children":[{"name":"SLOT_TEST","comps":[{"type":"game::SpriteComponentCustom"}]}],
        "comps":[{"type":"game::Animation","data":{"actions":{"idle":{"clips":{"":{
            "targets":{"SLOT_TEST":{"sprite":{"type":"DiscreteString",
                "keyframes":[[0,"namespace/ATTACHMENT"],[1,"ATTACHMENT"]]}}}
        }}}}}}]
    }"#;
    let skins = if default_exact {
        r#"{"default":{"TEST":{"ATTACHMENT":{"name":"B"},"namespace/ATTACHMENT":{"name":"A"}}},"Costume":{"TEST":{"ATTACHMENT":{"name":"B"}}}}"#
    } else {
        r#"{"default":{"TEST":{"ATTACHMENT":{"name":"B"}}},"Costume":{"TEST":{"ATTACHMENT":{"name":"B"}}}}"#
    };
    for (name, value) in [("test.anim.json", animation), ("test.skins.json", skins)] {
        std::fs::write(files.data().join("animations").join(name), value.as_bytes()).unwrap();
    }
    let runtime = StellaLua::new_with_resolution(files.data(), 4, 2).unwrap();
    runtime
        .execute_source("res.createSpriteSheet('A.dat')")
        .unwrap();
    files.image(NEW);
    runtime
        .execute_source(
            "res.createSpriteSheet('B.dat'); \
             AnimationWrapperNative.loadFromBundle('scene','animations/test.anim.json'); \
             AnimationWrapperNative.setSkin('scene','Costume'); \
             AnimationWrapperNative.setTranslation('scene',1,1); \
             AnimationWrapperNative.start('scene','idle','repeat')",
        )
        .unwrap();
    (files, runtime)
}

fn draw(runtime: &StellaLua) {
    runtime
        .execute_source("AnimationWrapperNative.draw('scene'); res.drawSprite('B',2,0)")
        .unwrap();
}

#[test]
fn exact_default_attachment_draws_original_pixels_despite_selected_basename_decoy() {
    let (files, runtime) = fixture(true);
    draw(&runtime);
    assert_halves(&pixels(&runtime, &mut assets(&files)), OLD, NEW);
}

#[test]
fn missing_exact_attachment_clears_pixels_after_valid_unqualified_binding() {
    let (files, runtime) = fixture(false);
    let mut catalog = assets(&files);
    let background = [0, 255, 0, 255];
    draw(&runtime);
    assert_halves(&pixels(&runtime, &mut catalog), background, NEW);
    runtime
        .execute_source("AnimationWrapperNative.seek('scene',1)")
        .unwrap();
    draw(&runtime);
    assert_halves(&pixels(&runtime, &mut catalog), NEW, NEW);
    runtime
        .execute_source("AnimationWrapperNative.seek('scene',0)")
        .unwrap();
    draw(&runtime);
    assert_halves(&pixels(&runtime, &mut catalog), background, NEW);
}
