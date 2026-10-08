//! Exact native attachment names and selected/default skin fallback.

use super::*;

struct Fixture(PathBuf);

impl Fixture {
    fn new(default_exact: bool) -> Self {
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root =
            std::env::temp_dir().join(format!("stella-skin-exact-{}-{unique}", std::process::id()));
        fs::create_dir_all(root.join("data/animations")).unwrap();
        fs::create_dir_all(root.join("data/images")).unwrap();
        fs::create_dir(root.join("appdata")).unwrap();
        fs::write(
            root.join("data/images/SHEET.dat"),
            test_textured_sprite_sheet_with_names(
                "atlas.pvr",
                &[("DEFAULT_SPRITE", 2, 2), ("DECOY_SPRITE", 2, 2)],
            ),
        )
        .unwrap();
        fs::write(
            root.join("data/images/atlas.pvr"),
            test_rgba_pvr(64, 64, [255; 4]),
        )
        .unwrap();
        let animation = serde_json::json!({
            "children": [{
                "name": "SLOT_TEST",
                "comps": [{"type": "game::SpriteComponentCustom"}]
            }],
            "comps": [{
                "type": "game::Animation",
                "data": {"actions": {"idle": {"clips": {"": {"targets": {
                    "SLOT_TEST": {"sprite": {
                        "type": "DiscreteString",
                        "keyframes": [[0, "namespace/ATTACHMENT"], [1, "ATTACHMENT"]]
                    }}
                }}}}}}
            }]
        });
        let mut skins = serde_json::json!({
            "default": {"TEST": {
                "ATTACHMENT": {"name": "DECOY_SPRITE"}
            }},
            "Costume": {"TEST": {
                "ATTACHMENT": {"name": "DECOY_SPRITE"}
            }}
        });
        if default_exact {
            skins["default"]["TEST"]["namespace/ATTACHMENT"] =
                serde_json::json!({"name": "DEFAULT_SPRITE", "x": 3.25, "y": -1.5});
        }
        for (name, value) in [("test.anim.json", animation), ("test.skins.json", skins)] {
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
                 AnimationWrapperNative.loadFromBundle('scene','animations/test.anim.json'); \
                 AnimationWrapperNative.setSkin('scene','Costume'); \
                 AnimationWrapperNative.start('scene','idle','repeat')",
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

fn draw(runtime: &StellaLua) -> Vec<RenderCommand> {
    runtime
        .execute_source("AnimationWrapperNative.draw('scene')")
        .unwrap();
    runtime.take_render_commands()
}

#[test]
fn selected_basename_decoy_does_not_override_exact_default_attachment() {
    let fixture = Fixture::new(true);
    let runtime = fixture.runtime();
    let commands = draw(&runtime);
    assert_eq!(commands.len(), 1);
    let command = &commands[0];
    assert_eq!(command.sprite, "DEFAULT_SPRITE");
    assert!(command.bound_region.is_some());
    assert_eq!(command.x, 3.25_f32);
    assert_eq!(command.y, -1.5_f32);
}

#[test]
fn missing_exact_alias_clears_old_binding_and_never_uses_basename_decoys() {
    let fixture = Fixture::new(false);
    let runtime = fixture.runtime();
    assert!(draw(&runtime).is_empty());
    runtime
        .execute_source("AnimationWrapperNative.seek('scene',1)")
        .unwrap();
    let commands = draw(&runtime);
    assert_eq!(commands.len(), 1);
    assert_eq!(commands[0].sprite, "DECOY_SPRITE");
    assert!(commands[0].bound_region.is_some());
    runtime
        .execute_source("AnimationWrapperNative.seek('scene',0)")
        .unwrap();
    assert!(draw(&runtime).is_empty());
}
