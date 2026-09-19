//! End-to-end Lua Resources capture registration, independent of GPU pixels.

use super::*;

fn snapshot(runtime: &StellaLua) -> SpriteCatalogSnapshot {
    runtime
        .resource_runtime
        .lock()
        .unwrap()
        .sprite_catalog_snapshot(runtime.data_root())
}

#[test]
fn capture_publishes_a_drawable_full_target_sprite_immediately() {
    let runtime = StellaLua::new_with_resolution("/tmp", 320, 180).unwrap();
    runtime
        .execute_diagnostic_source(
            r#"
                res.drawSprite("CAP", 0, 0)
                res.captureSprite("CAP")
                local width, height = res.getSpriteBounds("CAP")
                local px, py = res.getSpritePivot("CAP")
                assert(width == 320 and height == 180 and px == 0 and py == 0)
                res.drawSprite("CAP", 12, 34)
            "#,
        )
        .unwrap();
    let captures = runtime.take_capture_commands();
    let draws = runtime.take_render_commands();
    assert_eq!(captures.len(), 1);
    assert_eq!(draws.len(), 1);
    assert!(draws[0].order > captures[0].order);
    let catalog = snapshot(&runtime);
    let region = &catalog.regions["CAP"];
    assert_eq!(region.texture_source, captures[0].texture_source);
    assert_eq!(region.sprite.atlas_rotation, 3);
    assert_eq!((region.sprite.width, region.sprite.height), (320, 180));
    assert_eq!(catalog.masked_textures["CAP"], captures[0].texture_source);
    assert!(region.texture_source.starts_with("<capture:"));
}

#[test]
fn recapture_retains_image_identity_and_does_not_reorder_shadowed_sprites() {
    let runtime = StellaLua::new_with_resolution("/tmp", 320, 180).unwrap();
    runtime.execute_source("res.captureSprite('CAP')").unwrap();
    let original = snapshot(&runtime).regions["CAP"].clone();
    let shadow = register_test_sprite_sheet_with_sizes(&runtime, &[("CAP", 7, 9)]);
    let shadowed = snapshot(&runtime);
    runtime
        .execute_diagnostic_source(
            r#"
                res.captureSprite("CAP")
                local width, height = res.getSpriteBounds("CAP")
                assert(width == 7 and height == 9)
                res.drawSprite("CAP", 0, 0)
            "#,
        )
        .unwrap();
    assert_eq!(snapshot(&runtime), shadowed);
    let captures = runtime.take_capture_commands();
    assert_eq!(captures.len(), 2);
    assert_eq!(captures[0].texture_source, captures[1].texture_source);
    game_environment(runtime.lua())
        .unwrap()
        .get::<mlua::Table>("res")
        .unwrap()
        .get::<Function>("releaseSpriteSheet")
        .unwrap()
        .call::<()>((shadow, false))
        .unwrap();
    assert_eq!(snapshot(&runtime).regions["CAP"], original);
}

#[test]
fn capture_resize_failure_preserves_old_image_geometry_and_submission_order() {
    let runtime = StellaLua::new_with_resolution("/tmp", 320, 180).unwrap();
    runtime.execute_source("res.captureSprite('CAP')").unwrap();
    let original = snapshot(&runtime);
    {
        let mut bridge = runtime.render.lock().unwrap();
        bridge.screen_width = 640;
        bridge.screen_height = 360;
    }
    runtime
        .execute_diagnostic_source(
            r#"
                local ok, reason = pcall(res.captureSprite, "CAP")
                assert(not ok and string.find(tostring(reason), "Wrong size capture target image", 1, true))
                local width, height = res.getSpriteBounds("CAP")
                assert(width == 320 and height == 180)
                res.drawSprite("CAP", 0, 0)
            "#,
        )
        .unwrap();
    assert_eq!(snapshot(&runtime), original);
    assert_eq!(runtime.take_capture_commands().len(), 1);
    let draws = runtime.take_render_commands();
    assert_eq!(draws.len(), 1);
    assert_eq!(draws[0].order, 1);
}

#[test]
fn release_capture_then_recreate_uses_a_distinct_image_owner() {
    let runtime = StellaLua::new_with_resolution("/tmp", 320, 180).unwrap();
    runtime
        .execute_diagnostic_source(
            r#"
                res.captureSprite("CAP")
                res.drawSprite("CAP", 0, 0)
                res.releaseSpriteSheet("CAP")
                local width, height = res.getSpriteBounds("CAP")
                assert(width == 0 and height == 0)
                res.drawSprite("CAP", 0, 0)
                res.captureSprite("CAP")
                res.drawSprite("CAP", 0, 0)
            "#,
        )
        .unwrap();
    let captures = runtime.take_capture_commands();
    assert_eq!(captures.len(), 2);
    assert_ne!(captures[0].texture_source, captures[1].texture_source);
    assert_eq!(runtime.take_render_commands().len(), 2);
    assert_eq!(
        snapshot(&runtime).regions["CAP"].texture_source,
        captures[1].texture_source
    );
}

#[test]
fn retained_released_capture_does_not_republish_or_retain_temporary_images() {
    let runtime = StellaLua::new_with_resolution("/tmp", 320, 180).unwrap();
    runtime
        .execute_diagnostic_source(
            r#"
                res.captureSprite("CAP")
                res.releaseSpriteSheet("CAP", true)
                res.captureSprite("CAP")
                res.captureSprite("CAP")
                res.drawSprite("CAP", 0, 0)
                local width, height = res.getSpriteBounds("CAP")
                assert(width == 0 and height == 0)
            "#,
        )
        .unwrap();
    assert!(!snapshot(&runtime).regions.contains_key("CAP"));
    assert!(runtime.take_render_commands().is_empty());
    let captures = runtime.take_capture_commands();
    assert_eq!(captures.len(), 3);
    assert!(!captures[0].temporary);
    assert!(captures[1].temporary);
    assert!(captures[2].temporary);
    assert_ne!(captures[0].texture_source, captures[1].texture_source);
    assert_ne!(captures[1].texture_source, captures[2].texture_source);
    let resources = runtime.resource_runtime.lock().unwrap();
    assert!(resources.sprite_sheets.contains("CAP"));
    assert!(resources.released_sprite_sheet_resources.contains("CAP"));
    assert!(!resources.sprite_sheet_image_dimensions.contains_key("CAP"));
}

#[test]
fn capture_uses_exact_map_key_not_file_stem_or_resource_path() {
    let runtime = StellaLua::new_with_resolution("/tmp", 320, 180).unwrap();
    runtime
        .execute_diagnostic_source(
            r#"
                res.setPath("ignored/")
                res.captureSprite("dir/CAP.dat")
                local width, height = res.getSpriteBounds("dir/CAP.dat")
                assert(width == 320 and height == 180)
                assert(res.getSpriteBounds("CAP") == 0)
                res.releaseSpriteSheet("dir/CAP.dat")
                assert(res.getSpriteBounds("dir/CAP.dat") == 320)
            "#,
        )
        .unwrap();
    let resources = runtime.resource_runtime.lock().unwrap();
    assert!(resources.sprite_sheets.contains("dir/CAP.dat"));
    assert!(!resources.sprite_sheets.contains("CAP"));
}

#[test]
fn capture_existing_sheet_uses_retained_image_size_not_region_size() {
    let data_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../runtime/data");
    let runtime = StellaLua::new(data_root).unwrap();
    runtime
        .execute_source("res.createSpriteSheet('images/1024x768/BUTTONS_SHEET_1.dat')")
        .unwrap();
    let before = snapshot(&runtime);
    let (dimensions, source) = {
        let resources = runtime.resource_runtime.lock().unwrap();
        let sheet = &resources.sprite_sheet_values["BUTTONS_SHEET_1"];
        (
            resources.sprite_sheet_image_dimensions["BUTTONS_SHEET_1"],
            resources.sprite_sheet_texture_sources["BUTTONS_SHEET_1"][sheet.textures.len() - 1]
                .clone(),
        )
    };
    {
        let mut bridge = runtime.render.lock().unwrap();
        bridge.screen_width = dimensions[0];
        bridge.screen_height = dimensions[1];
    }
    runtime
        .execute_source("res.captureSprite('BUTTONS_SHEET_1')")
        .unwrap();
    assert_eq!(snapshot(&runtime), before);
    let captures = runtime.take_capture_commands();
    assert_eq!(captures.len(), 1);
    assert_eq!(captures[0].texture_source, source);
    assert!(!source.starts_with("<capture:"));
    runtime.render.lock().unwrap().screen_width += 1;
    let error = runtime
        .execute_source("res.captureSprite('BUTTONS_SHEET_1')")
        .unwrap_err();
    assert!(
        error
            .to_string()
            .contains("Wrong size capture target image")
    );
    assert!(runtime.take_capture_commands().is_empty());
    assert_eq!(snapshot(&runtime), before);
}

#[test]
fn ordinary_sheet_reload_does_not_reuse_a_captured_image_from_the_same_file() {
    use stella_assets::image_source::image_source_path;

    let data_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../runtime/data");
    let runtime = StellaLua::new(data_root).unwrap();
    let res = game_environment(runtime.lua())
        .unwrap()
        .get::<mlua::Table>("res")
        .unwrap();
    let load = res.get::<Function>("createSpriteSheet").unwrap();
    let capture = res.get::<Function>("captureSprite").unwrap();
    let draw = res.get::<Function>("drawSprite").unwrap();
    let path = "images/1024x768/BUTTONS_SHEET_1.dat";
    load.call::<()>((path, false)).unwrap();
    let before = snapshot(&runtime);
    let (name, first) = before.regions.iter().next().unwrap();
    let dimensions = runtime
        .resource_runtime
        .lock()
        .unwrap()
        .sprite_sheet_image_dimensions["BUTTONS_SHEET_1"];
    {
        let mut bridge = runtime.render.lock().unwrap();
        bridge.screen_width = dimensions[0];
        bridge.screen_height = dimensions[1];
    }
    capture.call::<()>("BUTTONS_SHEET_1").unwrap();
    draw.call::<()>((name.as_str(), 0, 0)).unwrap();
    load.call::<()>((path, true)).unwrap();
    let second = snapshot(&runtime).regions[name].clone();
    draw.call::<()>((name.as_str(), 1, 0)).unwrap();
    capture.call::<()>("BUTTONS_SHEET_1").unwrap();
    res.get::<Function>("releaseSpriteSheet")
        .unwrap()
        .call::<()>(path)
        .unwrap();
    load.call::<()>((path, false)).unwrap();
    let third = snapshot(&runtime).regions[name].clone();
    draw.call::<()>((name.as_str(), 2, 0)).unwrap();
    capture.call::<()>("BUTTONS_SHEET_1").unwrap();

    let images = [
        &first.texture_source,
        &second.texture_source,
        &third.texture_source,
    ];
    assert_ne!(images[0], images[1]);
    assert_ne!(images[1], images[2]);
    assert_ne!(images[0], images[2]);
    let file = image_source_path(images[0]);
    assert!(
        Path::new(file).is_file(),
        "the retained binding must decode a real image"
    );
    assert!(
        images
            .iter()
            .all(|source| image_source_path(source) == file)
    );
    let captures = runtime.take_capture_commands();
    let draws = runtime.take_render_commands();
    assert_eq!(captures.len(), 3);
    assert_eq!(draws.len(), 3);
    for ((capture, draw), source) in captures.iter().zip(&draws).zip(images) {
        assert_eq!(&capture.texture_source, source);
        assert_eq!(&draw.bound_region.as_ref().unwrap().texture_source, source);
    }
}

#[test]
fn duplicate_texture_records_and_other_sheets_have_independent_image_bindings() {
    use stella_assets::{
        image_source::image_source_path,
        ka3d::{SpriteRegion, SpriteSheet},
    };

    let data_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../runtime/data");
    let runtime = StellaLua::new(&data_root).unwrap();
    runtime
        .execute_source("res.createSpriteSheet('images/1024x768/BUTTONS_SHEET_1.dat')")
        .unwrap();
    let file = image_source_path(
        runtime
            .resource_runtime
            .lock()
            .unwrap()
            .sprite_sheet_texture_sources["BUTTONS_SHEET_1"]
            .last()
            .unwrap(),
    )
    .to_owned();
    assert!(Path::new(&file).is_file());
    let sprites = ["FIRST", "SECOND"].map(|name| SpriteRegion {
        name: name.to_owned(),
        x: 0,
        y: 0,
        width: 4,
        height: 4,
        pivot_x: 0,
        pivot_y: 0,
        atlas_rotation: 0,
    });
    let sheet = SpriteSheet {
        textures: vec![file.clone(), file.clone()],
        sprites: sprites.into(),
        sprite_texture_indices: vec![0, 1],
    };
    let mut resources = runtime.resource_runtime.lock().unwrap();
    resources.replace_sprite_sheet_value("A", sheet.clone());
    resources.sprite_sheets.insert("A".to_owned());
    resources.cache_sprite_sheet_host_bindings("A", &data_root);
    let first = resources.sprite_sheet_texture_sources["A"].clone();
    let before = resources.sprite_catalog_snapshot(&data_root);
    assert_eq!(before.regions["FIRST"].texture_source, first[0]);
    assert_eq!(before.regions["SECOND"].texture_source, first[1]);
    let dimensions = resources.sprite_sheet_image_dimensions["A"];
    let (captured, temporary) = resources
        .capture_sprite("A", dimensions, &data_root)
        .unwrap();
    assert_eq!(
        captured, first[1],
        "capture changes the last constructed Image only"
    );
    assert!(!temporary);
    assert_eq!(
        resources.active_masked_texture_source("A", &data_root),
        Some(first[1].clone())
    );
    resources.replace_sprite_sheet_value("B", sheet);
    resources.cache_sprite_sheet_host_bindings("B", &data_root);
    let second = &resources.sprite_sheet_texture_sources["B"];
    let all = [&first[0], &first[1], &second[0], &second[1]];
    assert_eq!(
        all.into_iter()
            .collect::<std::collections::BTreeSet<_>>()
            .len(),
        4
    );
    assert!(
        all.into_iter()
            .all(|source| image_source_path(source) == file)
    );
}
