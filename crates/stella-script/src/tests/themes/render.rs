use super::super::*;

#[test]
fn background_theme_draw_selects_one_fcvtzs_layer_while_foreground_draws_all() {
    let runtime = StellaLua::new("/tmp").unwrap();
    let sheet = register_test_sprite_sheet(&runtime, &["BG_0", "BG_1", "BG_2", "FG_0", "FG_1"]);
    runtime
        .execute_source(
            r#"
                blockTable = {
                    themes = {
                        split = {
                            bgLayers = {
                                { sprite = "BG_0" },
                                { sprite = "BG_1" },
                                { sprite = "BG_2" }
                            },
                            fgLayers = {
                                { sprite = "FG_0" },
                                { sprite = "FG_1" }
                            }
                        }
                    }
                }
                setWorldScale(20)
                setMaxWorldScale(20)
                setTheme("split")
                bad_background_type = pcall(drawBackgroundNative, "1")
                drawBackgroundNative(1.9)
                "#,
        )
        .unwrap();
    assert!(
        !game_environment(runtime.lua())
            .unwrap()
            .get::<bool>("bad_background_type")
            .unwrap()
    );

    {
        let mut bridge = runtime.render.lock().unwrap();
        let sprites = bridge
            .commands
            .iter()
            .map(|command| command.sprite.as_str())
            .collect::<Vec<_>>();
        assert_eq!(sprites, ["BG_1"]);
        bridge.commands.clear();
    }

    runtime
        .execute_source(
            r#"
                drawBackgroundNative(-1)
                drawForegroundNative(123)
                "#,
        )
        .unwrap();
    {
        let mut bridge = runtime.render.lock().unwrap();
        let sprites = bridge
            .commands
            .iter()
            .map(|command| command.sprite.as_str())
            .collect::<Vec<_>>();
        assert_eq!(sprites, ["BG_0", "BG_1", "BG_2", "FG_0", "FG_1"]);
        bridge.commands.clear();
    }

    // FCVTZS produces INT_MIN for NaN, which enters the native negative
    // "all layers" branch. A positive in-range but out-of-vector index is
    // kept memory-safe as an empty pass by the rehost.
    runtime
        .execute_source(
            r#"
                drawBackgroundNative(99)
                drawBackgroundNative(0 / 0)
                "#,
        )
        .unwrap();
    {
        let mut bridge = runtime.render.lock().unwrap();
        let sprites = bridge
            .commands
            .iter()
            .map(|command| command.sprite.as_str())
            .collect::<Vec<_>>();
        assert_eq!(sprites, ["BG_0", "BG_1", "BG_2"]);
        bridge.commands.clear();
    }

    // The layer dimensions are captured by setTheme, but sub_10009BDB4
    // resolves every tile through ResourceManager::drawSprite. Releasing the
    // sheet therefore suppresses subsequent submissions without rebuilding
    // the theme layer.
    let environment = game_environment(runtime.lua()).unwrap();
    environment
        .get::<mlua::Table>("res")
        .unwrap()
        .get::<Function>("releaseSpriteSheet")
        .unwrap()
        .call::<()>((sheet, true))
        .unwrap();
    runtime
        .execute_source("drawBackgroundNative(-1); drawForegroundNative()")
        .unwrap();
    assert!(runtime.render.lock().unwrap().commands.is_empty());
}

#[test]
fn theme_draw_submission_retains_the_resolved_atlas_across_shadow_and_release() {
    let unique = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let root = std::env::temp_dir().join(format!("stella-theme-pointer-{unique}"));
    let data_root = root.join("data");
    for directory in ["first", "second"] {
        fs::create_dir_all(data_root.join(directory)).unwrap();
    }
    fs::write(
        data_root.join("first/FIRST.dat"),
        test_textured_sprite_sheet("THEME_TILE", "first.pvr", 10, 20),
    )
    .unwrap();
    fs::write(data_root.join("first/first.pvr"), []).unwrap();
    fs::write(
        data_root.join("second/SECOND.dat"),
        test_textured_sprite_sheet("THEME_TILE", "second.pvr", 30, 40),
    )
    .unwrap();
    fs::write(data_root.join("second/second.pvr"), []).unwrap();

    let runtime = StellaLua::new(&data_root).unwrap();
    runtime
        .execute_source(
            r#"
                res.createSpriteSheet("first/FIRST.dat")
                blockTable = {
                    themes = {
                        retained = {
                            bgLayers = { { sprite = "THEME_TILE" } },
                            fgLayers = {}
                        }
                    }
                }
                setWorldScale(20)
                setMaxWorldScale(20)
                setTheme("retained")
                drawBackgroundNative(-1)

                res.createSpriteSheet("second/SECOND.dat")
                res.releaseSpriteSheet("first/FIRST.dat", false)
            "#,
        )
        .unwrap();

    assert_eq!(
        runtime.sprite_catalog_snapshot_since(0).unwrap().regions["THEME_TILE"]
            .sprite
            .width,
        30
    );
    let bridge = runtime.render.lock().unwrap();
    assert_eq!(bridge.commands.len(), 1);
    let retained = bridge.commands[0].bound_region.as_ref().unwrap();
    assert_eq!(retained.sprite.width, 10);
    assert!(retained.texture_source.ends_with("first/first.pvr"));
    drop(bridge);
    fs::remove_dir_all(root).unwrap();
}
