use super::*;

#[test]
fn flash_animation_draws_same_named_animation_with_native_scene_transform() {
    let runtime = StellaLua::new("/tmp").unwrap();
    runtime
        .execute_source(
            r#"
                setWorldScale(2)
                setTopLeft(4, 6)
                createNonPhysicsObject("animated", "RED_CROSS", 1, 2, 3)
                setObjectParameter("animated", 5, 0.5)
                setObjectParameter("animated", 8, 1)
                setFlashAnimation("animated")
                "#,
        )
        .unwrap();
    {
        runtime
            .render
            .lock()
            .unwrap()
            .scene
            .get_mut("animated")
            .unwrap()
            .angle = 0.25;

        let mut definition = AnimationDefinition::default();
        definition.slots.push("SLOT_BODY".to_owned());
        let mut action = AnimationAction::default();
        action
            .targets
            .entry("SLOT_BODY".to_owned())
            .or_default()
            .sprite
            .push((0.0, "ANIMATED_BODY".to_owned()));
        definition.actions.insert("idle".to_owned(), action);
        let mut animation = runtime._animation_runtime.lock().unwrap();
        animation
            .definitions
            .insert("animated".to_owned(), definition);
        animation.playback.insert(
            "animated".to_owned(),
            AnimationPlayback::active("idle".to_owned(), "repeat".to_owned(), 0.0, 1.0, 1.0),
        );
        bind_test_animation_sprites(&mut animation, "animated", &["ANIMATED_BODY"]);
    }

    runtime.execute_source("drawGameNative()").unwrap();
    {
        let bridge = runtime.render.lock().unwrap();
        assert_eq!(bridge.commands.len(), 1);
        let command = &bridge.commands[0];
        assert_eq!(command.sprite, "ANIMATED_BODY");
        assert_eq!((command.x, command.y), (32.0, 68.0));
        assert_eq!((command.state.scale_x, command.state.scale_y), (-1.0, 1.0));
        assert_eq!(command.state.angle, 0.25);
    }

    runtime
        .execute_source(r#"removeFlashAnimation("animated")"#)
        .unwrap();
    {
        let mut bridge = runtime.render.lock().unwrap();
        bridge.commands.clear();
        bridge.draw_scene_range();
        assert_eq!(bridge.commands.len(), 1);
        assert_eq!(bridge.commands[0].sprite, "RED_CROSS");
    }
}

#[test]
fn held_poppy_and_luca_powers_use_unsolved_physics_time_for_continuous_display_poses() {
    let runtime = StellaLua::new("/tmp").unwrap();
    runtime
        .execute_source(
            r#"
                createNonPhysicsObject("poppy", "RED_CROSS", 1, 2, 3)
                setFlashAnimation("poppy")
                createNonPhysicsObject("luca", "RED_CROSS", 3, 4, 3)
                setFlashAnimation("luca")
                "#,
        )
        .unwrap();
    {
        let mut bridge = runtime.render.lock().unwrap();
        bridge.physics_accumulator = 1.0 / 60.0;
        let object = bridge.scene.get_mut("poppy").unwrap();
        object.velocity_x = 6.0;
        object.velocity_y = -3.0;
        let object = bridge.scene.get_mut("luca").unwrap();
        object.velocity_x = 6.0;
        object.velocity_y = -3.0;
    }
    {
        let mut animation = runtime._animation_runtime.lock().unwrap();
        for (tag, action_name, sprite) in [
            ("poppy", "Poppy_Power", "POPPY_BODY"),
            ("luca", "Luca_ability", "LUCA_BODY"),
        ] {
            let mut definition = AnimationDefinition::default();
            definition.slots.push("SLOT_BODY".to_owned());
            let mut action = AnimationAction::default();
            action
                .targets
                .entry("SLOT_BODY".to_owned())
                .or_default()
                .sprite
                .push((0.0, sprite.to_owned()));
            definition.actions.insert(action_name.to_owned(), action);
            animation.definitions.insert(tag.to_owned(), definition);
            animation.playback.insert(
                tag.to_owned(),
                AnimationPlayback::active(action_name.to_owned(), "once".to_owned(), 0.0, 1.0, 1.0),
            );
            bind_test_animation_sprites(&mut animation, tag, &[sprite]);
        }
    }

    runtime.execute_source("drawGameNative()").unwrap();

    let bridge = runtime.render.lock().unwrap();
    assert_eq!(bridge.commands.len(), 2);
    let residual = 1.0_f32 / 60.0_f32;
    let display_x = 6.0_f32.mul_add(residual, 1.0_f32);
    let display_y = (-3.0_f32).mul_add(residual, 2.0_f32);
    assert_eq!(bridge.commands[0].x, f64::from(display_x * 20.0_f32));
    assert_eq!(bridge.commands[0].y, f64::from(display_y * 20.0_f32));
    let luca_display_x = 6.0_f32.mul_add(residual, 3.0_f32);
    let luca_display_y = (-3.0_f32).mul_add(residual, 4.0_f32);
    assert_eq!(bridge.commands[1].x, f64::from(luca_display_x * 20.0_f32));
    assert_eq!(bridge.commands[1].y, f64::from(luca_display_y * 20.0_f32));
    assert_eq!(
        (bridge.scene["poppy"].x, bridge.scene["poppy"].y),
        (1.0, 2.0)
    );
    assert_eq!((bridge.scene["luca"].x, bridge.scene["luca"].y), (3.0, 4.0));
}

#[test]
fn native_scene_draw_callbacks_wrap_object_draw_and_can_be_cleared() {
    let runtime = StellaLua::new("/tmp").unwrap();
    runtime
        .execute_source(
            r#"
                createNonPhysicsObject("callback_body", "RED_CROSS", 1, 2, 3)
                pre_count = 0
                post_count = 0
                native_setPreDrawFunction("callback_body", function(object, flipped)
                    pre_count = pre_count + 1
                    callback_name = object.name
                    callback_x = object.x
                    callback_flipped = flipped
                end)
                native_setPostDrawFunction("callback_body", function()
                    post_count = post_count + 1
                end)
                missing_callback_name_fails = not pcall(native_setPreDrawFunction)
                invalid_callback_fails = not pcall(
                    native_setPreDrawFunction, "callback_body", 123
                )
                drawGameNative()
                "#,
        )
        .unwrap();
    let environment = game_environment(runtime.lua()).unwrap();
    assert_eq!(environment.get::<i64>("pre_count").unwrap(), 1);
    assert_eq!(environment.get::<i64>("post_count").unwrap(), 1);
    assert!(
        environment
            .get::<bool>("missing_callback_name_fails")
            .unwrap()
    );
    assert!(environment.get::<bool>("invalid_callback_fails").unwrap());
    assert_eq!(
        environment.get::<String>("callback_name").unwrap(),
        "callback_body"
    );
    assert_eq!(environment.get::<f64>("callback_x").unwrap(), 1.0);
    assert!(!environment.get::<bool>("callback_flipped").unwrap());
    assert_eq!(runtime.render.lock().unwrap().commands.len(), 1);

    runtime
        .execute_source(
            r#"
                native_setPreDrawFunction("callback_body", nil)
                native_setPostDrawFunction("callback_body", nil)
                drawGameNative()
                "#,
        )
        .unwrap();
    assert_eq!(environment.get::<i64>("pre_count").unwrap(), 1);
    assert_eq!(environment.get::<i64>("post_count").unwrap(), 1);
}

#[test]
fn native_pre_draw_mutations_feed_the_same_object_draw_and_live_post_flip() {
    let runtime = StellaLua::new("/tmp").unwrap();
    register_test_sprite_sheet(&runtime, &["BODY"]);
    runtime
        .execute_source(
            r#"
                createNonPhysicsObject("live_callback_body", "BODY", 1, 2, 3)
                native_setPreDrawFunction("live_callback_body", function(object, flipped)
                    pre_flipped = flipped
                    setObjectAlpha(object.name, 0.25)
                    setScale(object.name, 2, 3)
                    setObjectParameter(object.name, 8, 1)
                end)
                native_setPostDrawFunction("live_callback_body", function(object, flipped)
                    post_flipped = flipped
                end)
                drawGameNative()
                "#,
        )
        .unwrap();

    let environment = game_environment(runtime.lua()).unwrap();
    assert!(!environment.get::<bool>("pre_flipped").unwrap());
    assert!(environment.get::<bool>("post_flipped").unwrap());
    let bridge = runtime.render.lock().unwrap();
    let body = bridge
        .commands
        .iter()
        .find(|command| command.sprite == "BODY")
        .unwrap();
    assert_eq!(body.state.alpha, f64::from(0.25_f32));
    assert_eq!((body.state.scale_x, body.state.scale_y), (-2.0, 3.0));
}

#[test]
fn native_z_order_range_is_strict_and_fcvtzs_after_float32_rounding() {
    let runtime = StellaLua::new("/tmp").unwrap();
    runtime
        .execute_source(
            r#"
                createNonPhysicsObject("three", "THREE", 0, 0, 3)
                createNonPhysicsObject("four", "FOUR", 0, 0, 4)
                createNonPhysicsObject("one_sixty_nine", "ONE_SIXTY_NINE", 0, 0, 169)
                createNonPhysicsObject("one_seventy", "ONE_SEVENTY", 0, 0, 170)
                native_setZOrderRange(3.99999999, 3.99999999)
                missing_max_fails = not pcall(native_setZOrderRange, 3)
                string_min_fails = not pcall(native_setZOrderRange, "4", 4)
                drawGameNative()
                "#,
        )
        .unwrap();

    let environment = game_environment(runtime.lua()).unwrap();
    assert!(environment.get::<bool>("missing_max_fails").unwrap());
    assert!(environment.get::<bool>("string_min_fails").unwrap());
    let bridge = runtime.render.lock().unwrap();
    assert_eq!((bridge.z_order_min, bridge.z_order_max), (4.0, 4.0));
    assert!(bridge.commands.is_empty());
    drop(bridge);

    runtime
        .execute_source(
            r#"
                native_setZOrderRange(3.99999999, 5)
                drawGameNative()
            "#,
        )
        .unwrap();
    let bridge = runtime.render.lock().unwrap();
    assert_eq!(
        bridge
            .commands
            .iter()
            .map(|command| command.sprite.as_str())
            .collect::<Vec<_>>(),
        ["FOUR"]
    );
    drop(bridge);

    runtime
        .execute_source("native_setZOrderRange(169, -1)")
        .unwrap();
    assert_eq!(
        runtime.render.lock().unwrap().scene_range_names(),
        ["one_sixty_nine"]
    );
}

#[test]
fn native_scene_walk_uses_integer_z_sheet_groups_and_group_insertion_order() {
    let runtime = StellaLua::new("/tmp").unwrap();
    register_test_sprite_sheet(&runtime, &["ZED", "ALPHA"]);
    register_test_sprite_sheet(&runtime, &["BETA"]);
    runtime
        .execute_source(
            r#"
                createNonPhysicsObject("z_name", "ZED", 0, 0, 3.9)
                createNonPhysicsObject("middle_name", "BETA", 0, 0, 3.1)
                createNonPhysicsObject("a_name", "ALPHA", 0, 0, 3.7)
                native_setZOrderRange(3, 4)
                drawGameNative()
                "#,
        )
        .unwrap();

    let bridge = runtime.render.lock().unwrap();
    assert_eq!(
        bridge
            .commands
            .iter()
            .map(|command| command.sprite.as_str())
            .collect::<Vec<_>>(),
        ["ZED", "ALPHA", "BETA"]
    );
    drop(bridge);

    runtime
        .execute_source(
            r#"
                changeZOrder("z_name", 3.2)
                drawGameNative()
                native_setSprite("middle_name", "ALPHA")
                drawGameNative()
                "#,
        )
        .unwrap();
    let bridge = runtime.render.lock().unwrap();
    let sprites = bridge
        .commands
        .iter()
        .map(|command| command.sprite.as_str())
        .collect::<Vec<_>>();
    assert_eq!(&sprites[3..6], ["ALPHA", "ZED", "BETA"]);
    assert_eq!(&sprites[6..9], ["ALPHA", "ZED", "ALPHA"]);
}

#[test]
fn native_scene_walk_drains_z_ordered_lua_draws_before_each_nonempty_bucket() {
    let runtime = StellaLua::new("/tmp").unwrap();
    register_test_sprite_sheet(&runtime, &["BODY_THREE", "BODY_FIVE", "LUA_LAYER"]);
    runtime
        .execute_source(
            r#"
                createNonPhysicsObject("body_three", "BODY_THREE", 0, 0, 3)
                createNonPhysicsObject("body_five", "BODY_FIVE", 0, 0, 5)
                setVisible("body_three", false)
                native_setZOrderRange(3, 6)
                DrawCalls = {
                    hasZOrderedDraws = true,
                    draw = function(z)
                        res.drawSprite("LUA_LAYER", z, 0)
                    end,
                }
                drawGameNative()
            "#,
        )
        .unwrap();

    let bridge = runtime.render.lock().unwrap();
    assert_eq!(
        bridge
            .commands
            .iter()
            .map(|command| (command.sprite.as_str(), command.x))
            .collect::<Vec<_>>(),
        [("LUA_LAYER", 3.0), ("LUA_LAYER", 5.0), ("BODY_FIVE", 0.0),]
    );
}

#[test]
fn native_scene_walk_keeps_removed_empty_z_bucket_observable_to_lua_draws() {
    let runtime = StellaLua::new("/tmp").unwrap();
    register_test_sprite_sheet(&runtime, &["BODY", "LUA_LAYER"]);
    runtime
        .execute_source(
            r#"
                createNonPhysicsObject("removed_body", "BODY", 0, 0, 4)
                removeObject("removed_body")
                native_setZOrderRange(4, 5)
                DrawCalls = {
                    hasZOrderedDraws = true,
                    draw = function(z)
                        res.drawSprite("LUA_LAYER", z, 0)
                    end,
                }
                drawGameNative()
            "#,
        )
        .unwrap();

    let bridge = runtime.render.lock().unwrap();
    assert!(bridge.scene.is_empty());
    assert!(bridge.scene_render_index.contains_z(4));
    assert_eq!(
        bridge
            .commands
            .iter()
            .map(|command| (command.sprite.as_str(), command.x))
            .collect::<Vec<_>>(),
        [("LUA_LAYER", 4.0)]
    );
}

#[test]
fn native_scene_walk_rereads_live_name_vectors_after_callback_z_moves() {
    let runtime = StellaLua::new("/tmp").unwrap();
    register_test_sprite_sheet(&runtime, &["BODY_A", "BODY_B", "LUA_LAYER"]);
    runtime
        .execute_source(
            r#"
                createNonPhysicsObject("body_a", "BODY_A", 0, 0, 3)
                createNonPhysicsObject("body_b", "BODY_B", 0, 0, 3)
                native_setPreDrawFunction("body_a", function(object)
                    changeZOrder(object.name, 4)
                end)
                native_setZOrderRange(3, 5)
                DrawCalls = {
                    hasZOrderedDraws = true,
                    draw = function(z)
                        res.drawSprite("LUA_LAYER", z, 0)
                    end,
                }
                drawGameNative()
            "#,
        )
        .unwrap();

    let bridge = runtime.render.lock().unwrap();
    assert_eq!(
        bridge
            .commands
            .iter()
            .map(|command| command.sprite.as_str())
            .collect::<Vec<_>>(),
        ["LUA_LAYER", "BODY_A", "LUA_LAYER", "BODY_A"]
    );
    assert_eq!(bridge.scene_range_names(), ["body_b", "body_a"]);
}

#[test]
fn native_scene_index_retains_old_sheet_pointer_until_explicit_sprite_rebind() {
    let runtime = StellaLua::new(std::env::temp_dir()).unwrap();
    let owner = register_test_sprite_sheet(&runtime, &["SAME"]);
    runtime
        .execute_source(r#"createNonPhysicsObject("old", "SAME", 0, 0, 3)"#)
        .unwrap();

    let descriptor = runtime.data_root().join(&owner);
    fs::write(
        &descriptor,
        test_textured_sprite_sheet_with_names("replacement.pvr", &[("SAME", 2, 2)]),
    )
    .unwrap();
    let environment = game_environment(runtime.lua()).unwrap();
    environment
        .get::<mlua::Table>("res")
        .unwrap()
        .get::<Function>("createSpriteSheet")
        .unwrap()
        .call::<()>((owner.as_str(), true))
        .unwrap();
    fs::remove_file(descriptor).unwrap();

    runtime
        .execute_source(r#"createNonPhysicsObject("new", "SAME", 0, 0, 3)"#)
        .unwrap();
    assert_eq!(
        runtime.render.lock().unwrap().scene_range_names(),
        ["old", "new"]
    );

    runtime
        .execute_source(r#"native_setSprite("old", "SAME")"#)
        .unwrap();
    assert_eq!(
        runtime.render.lock().unwrap().scene_range_names(),
        ["new", "old"]
    );
}

#[test]
fn native_duplicate_constructor_replaces_name_pointer_but_retains_old_render_leaf() {
    let runtime = StellaLua::new("/tmp").unwrap();
    register_test_sprite_sheet(&runtime, &["OLD", "NEW"]);
    runtime
        .execute_source(
            r#"
                createNonPhysicsObject("duplicate", "OLD", 1, 2, 2)
                previous_table = objects.world.duplicate
                previous_table.only_old = true
                createNonPhysicsObject("duplicate", "NEW", 3, 4, 4)
                duplicate_reused_table = previous_table == objects.world.duplicate
                duplicate_kept_old_field = objects.world.duplicate.only_old
                drawGameNative()
            "#,
        )
        .unwrap();

    let environment = game_environment(runtime.lua()).unwrap();
    assert!(!environment.get::<bool>("duplicate_reused_table").unwrap());
    assert!(matches!(
        environment
            .get::<Value>("duplicate_kept_old_field")
            .unwrap(),
        Value::Nil
    ));
    let bridge = runtime.render.lock().unwrap();
    assert_eq!(bridge.scene_range_names(), ["duplicate", "duplicate"]);
    assert_eq!(
        bridge
            .commands
            .iter()
            .map(|command| command.sprite.as_str())
            .collect::<Vec<_>>(),
        ["NEW", "NEW"]
    );
}

#[test]
fn replacing_objects_world_retires_same_named_native_scene_before_rebuild() {
    let runtime = StellaLua::new("/tmp").unwrap();
    register_test_sprite_sheet(&runtime, &["OLD", "NEW"]);
    runtime
        .execute_source(
            r#"
                createNonPhysicsObject("same", "OLD", 1, 2, 2)
                native_setPreDrawFunction("same", function()
                    stale_callback_count = (stale_callback_count or 0) + 1
                end)
                previous_world = objects.world
                objects.world = {}
                createNonPhysicsObject("same", "NEW", 3, 4, 4)
            "#,
        )
        .unwrap();

    runtime.sync_scene_lifetime().unwrap();
    runtime.execute_source("drawGameNative()").unwrap();

    let environment = game_environment(runtime.lua()).unwrap();
    assert!(
        !environment
            .get::<mlua::Table>("previous_world")
            .unwrap()
            .equals(&object_world(runtime.lua()).unwrap())
            .unwrap()
    );
    assert!(matches!(
        environment.get::<Value>("stale_callback_count").unwrap(),
        Value::Nil
    ));
    let bridge = runtime.render.lock().unwrap();
    assert_eq!(bridge.scene_range_names(), ["same"]);
    assert_eq!(
        bridge
            .commands
            .iter()
            .map(|command| command.sprite.as_str())
            .collect::<Vec<_>>(),
        ["NEW"]
    );
}

#[test]
fn failed_level_load_clears_native_scene_and_callbacks_before_file_open() {
    let runtime = StellaLua::new("/tmp").unwrap();
    runtime
        .execute_source(
            r#"
                createNonPhysicsObject("old", "OLD", 0, 0, 3)
                native_setPreDrawFunction("old", function() end)
                load_succeeded = pcall(loadLevel, "levels/does_not_exist")
            "#,
        )
        .unwrap();

    let environment = game_environment(runtime.lua()).unwrap();
    assert!(!environment.get::<bool>("load_succeeded").unwrap());
    let bridge = runtime.render.lock().unwrap();
    assert!(bridge.scene.is_empty());
    assert!(bridge.scene_range_names().is_empty());
    drop(bridge);
    let callbacks = runtime.draw_callbacks.borrow();
    assert!(callbacks.pre.is_empty());
    assert!(callbacks.post.is_empty());
}

#[test]
fn native_box_and_nonphysics_ground_skip_only_their_initial_render_leaf() {
    for constructor in [
        r#"createBox("ground", "GROUND", 0, 0, 2, 2, 0, 0, 0, true, false, 3)"#,
        r#"createNonPhysicsObject("ground", "GROUND", 0, 0, 3)"#,
    ] {
        let runtime = StellaLua::new("/tmp").unwrap();
        register_test_sprite_sheet(&runtime, &["GROUND"]);
        runtime.execute_source(constructor).unwrap();
        assert!(
            runtime
                .render
                .lock()
                .unwrap()
                .scene_range_names()
                .is_empty()
        );

        runtime
            .execute_source(r#"changeZOrder("ground", 4)"#)
            .unwrap();
        assert_eq!(
            runtime.render.lock().unwrap().scene_range_names(),
            ["ground"]
        );
    }

    let runtime = StellaLua::new("/tmp").unwrap();
    register_test_sprite_sheet(&runtime, &["GROUND"]);
    runtime
        .execute_source(
            r#"
                clearVertices()
                addVertex(-1, 0)
                addVertex(1, 0)
                createLineShape(
                    "ground", "GROUND", 0, 0, 2, 0, 0, 0, 0, true, false, 3
                )
            "#,
        )
        .unwrap();
    assert_eq!(
        runtime.render.lock().unwrap().scene_range_names(),
        ["ground"]
    );
}

#[test]
fn native_scene_callback_state_uses_secondary_pre_scale_translation() {
    let runtime = StellaLua::new("/tmp").unwrap();
    register_test_sprite_sheet(&runtime, &["PRE_DRAW", "BODY"]);
    runtime
        .execute_source(
            r#"
                setWorldScale(3)
                setTopLeft(10, 20)
                createNonPhysicsObject("callback_transform", "BODY", 1, 2, 3)
                setObjectParameter("callback_transform", 5, 2)
                setObjectParameter("callback_transform", 8, 1)
                setAngle("callback_transform", 0.25)
                setPivotOffset("callback_transform", 12, 18)
                native_setPreDrawFunction("callback_transform", function()
                    res.drawSprite("PRE_DRAW", 5, 6)
                end)
                drawGameNative()
                "#,
        )
        .unwrap();

    let bridge = runtime.render.lock().unwrap();
    let callback = bridge
        .commands
        .iter()
        .find(|command| command.sprite == "PRE_DRAW")
        .unwrap();
    assert!(!callback.world_space);
    assert_eq!((callback.x, callback.y), (5.0, 6.0));
    assert_eq!(
        (callback.state.translate_x, callback.state.translate_y),
        (-1.0, -1.0)
    );
    assert_eq!(
        (callback.state.scale_x, callback.state.scale_y),
        (-6.0, 6.0)
    );
    assert_eq!(callback.state.angle, -0.25);
    // BODY's 1x1 test region has an integer pivot of zero. The atlas branch
    // adds the object's two float32 pivot offsets to that native pivot.
    assert_eq!(
        (callback.state.pivot_x, callback.state.pivot_y),
        (12.0, 18.0)
    );

    let body = bridge
        .commands
        .iter()
        .find(|command| command.sprite == "BODY")
        .unwrap();
    assert!(body.world_space);
    assert_eq!(
        (body.state.translate_x, body.state.translate_y),
        (30.0, 60.0)
    );
    assert_eq!((body.state.scale_x, body.state.scale_y), (-6.0, 6.0));
    assert_eq!(body.state.angle, 0.25);
    assert_eq!((body.state.pivot_x, body.state.pivot_y), (0.0, 0.0));
}

#[test]
fn native_scene_composite_callback_uses_integer_bounds_pivot_and_ignores_object_offset() {
    let runtime = StellaLua::new("/tmp").unwrap();
    runtime
        .execute_source(r#"createNonPhysicsObject("composite", "MISSING", 0, 0, 1)"#)
        .unwrap();
    {
        let mut bridge = runtime.render.lock().unwrap();
        let object = bridge.scene.get_mut("composite").unwrap();
        object.sprite_region = None;
        object.pivot_offset_x = 100.0;
        object.pivot_offset_y = 200.0;
        object.composite_sprite = Some(vec![BoundCompositePart {
            part: stella_assets::ka3d::CompositePart {
                sprite: "PART".to_owned(),
                x: 10.0,
                y: -4.0,
                scale_x: 2.0,
                scale_y: 0.5,
                flip_x: -1.0,
                flip_y: 1.0,
                angle: 0.0,
                visible: true,
            },
            region: SpriteCatalogRegion {
                native_sheet_id: 1,
                texture_source: "part.pvr".to_owned(),
                sprite: stella_assets::ka3d::SpriteRegion {
                    name: "PART".to_owned(),
                    x: 0,
                    y: 0,
                    width: 10,
                    height: 20,
                    pivot_x: 3,
                    pivot_y: 7,
                    atlas_rotation: 0,
                },
            },
        }]);
    }

    let bridge = runtime.render.lock().unwrap();
    let object = bridge.scene_draw_object("composite").unwrap();
    let state = bridge.scene_callback_state(&object);
    // Native transformed/truncated X bounds are [-4,16], Y bounds [-7,2],
    // hence CompoSprite+0x68/+0x6C store (4,7). +0xB4/+0xB8 are skipped by
    // the composite branch despite the deliberately large object offsets.
    assert_eq!((state.pivot_x, state.pivot_y), (4.0, 7.0));
}

#[test]
fn native_scene_object_and_callback_transforms_keep_arm64_float_order() {
    let runtime = StellaLua::new("/tmp").unwrap();
    register_test_sprite_sheet(&runtime, &["CIRCLE"]);
    runtime
        .execute_source(
            r#"
                setWorldScale(5.61361031)
                setTopLeft(-163.630294, -100.301506)
                createCircle(
                    "circle", "CIRCLE", 1.23456789, -2.34567891,
                    1, 1, 0, 0, true, false, 2
                )
                setScale("circle", 0.0900000036, 0.123456791)
                setAngle("circle", 0.234567896)
                setSpriteRotation("circle", 0.765432119)
                setPivotOffset("circle", 12.3456789, -18.7654321)
                setObjectParameter("circle", 21, 2)
                drawGameNative()
                "#,
        )
        .unwrap();

    let bridge = runtime.render.lock().unwrap();
    let object = bridge.scene_draw_object("circle").unwrap();
    let command = bridge
        .commands
        .iter()
        .find(|command| command.sprite == "CIRCLE")
        .unwrap();

    let world_scale = bridge.world_scale as f32;
    let top_left_x = bridge.top_left_x as f32;
    let top_left_y = bridge.top_left_y as f32;
    let x = object.x as f32;
    let y = object.y as f32;
    let scale_x = object.scale_x as f32;
    let scale_y = object.scale_y as f32;
    assert_eq!(
        command.state.translate_x,
        f64::from(((x * 20.0_f32) - top_left_x) * world_scale)
    );
    assert_eq!(
        command.state.translate_y,
        f64::from(((y * 20.0_f32) - top_left_y) * world_scale)
    );
    assert_eq!(
        command.state.scale_x,
        f64::from((1.0_f32 * world_scale) * scale_x)
    );
    assert_eq!(command.state.scale_y, f64::from(world_scale * scale_y));
    // sub_10006C838 receives +0xAC. setSpriteRotation writes the distinct
    // +0xB0 field and therefore must not rotate the ordinary sprite twice.
    assert_eq!(command.state.angle, f64::from(object.angle as f32));

    let callback = bridge.scene_callback_state(&object);
    assert_eq!(
        callback.translate_x,
        f64::from((object.pivot_offset_x as f32 - top_left_x) / scale_x)
    );
    assert_eq!(
        callback.translate_y,
        f64::from((object.pivot_offset_y as f32 - top_left_y) / scale_y)
    );
    assert_eq!(
        callback.angle,
        f64::from(object.angle as f32 + object.sprite_rotation as f32)
    );
}

#[test]
fn native_direct_post_draw_sprite_uses_its_own_matrix_inside_flipped_object_context() {
    let runtime = StellaLua::new("/tmp").unwrap();
    register_test_sprite_sheet(&runtime, &["BODY", "PUPIL"]);
    runtime
        .execute_source(
            r#"
                setWorldScale(0.5)
                setTopLeft(40, 60)
                createNonPhysicsObject("pig", "BODY", 3, 4, 5)
                setScale("pig", 2, 3)
                setObjectParameter("pig", 8, 1)
                setAngle("pig", 0.25)
                setObjectAlpha("pig", 0.6)
                native_setPostDrawFunction("pig", function()
                    drawSpriteWithoutShader("PUPIL", 100, 200, 0.5, 0.25, 0.4)
                end)
                drawGameNative()
                "#,
        )
        .unwrap();

    let bridge = runtime.render.lock().unwrap();
    let pupil = bridge
        .commands
        .iter()
        .find(|command| command.sprite == "PUPIL")
        .unwrap();
    assert_eq!((pupil.x, pupil.y), (100.0, 200.0));
    let (sine, cosine) = 0.4_f32.sin_cos();
    assert_eq!(
        pupil.state.matrix.unwrap(),
        [
            f64::from(cosine * 0.5_f32),
            f64::from(-sine * 0.25_f32),
            f64::from(sine * 0.5_f32),
            f64::from(cosine * 0.25_f32),
        ]
    );
    assert_eq!(pupil.state.alpha, f64::from(0.6_f32));
}

#[test]
fn native_audio_handles_are_unique_and_accept_volume_and_stop_updates() {
    let unique = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let root = std::env::temp_dir().join(format!("stella-native-audio-{unique}"));
    fs::create_dir_all(&root).unwrap();
    for name in ["first.wav", "loop.wav", "defaults.wav"] {
        fs::write(root.join(name), test_pcm_wav(32)).unwrap();
    }
    let runtime = StellaLua::new(&root).unwrap();
    runtime
        .execute_source(
            r#"
                res.createAudioOutput(2, 16, 44100)
                res.createAudio("first.wav", "first", false)
                res.createAudio("loop.wav", "loop", false)
                res.createAudio("defaults.wav", "defaults", false)
                res.startAudioOutput()
                setChannelCountLimit(5, 1)
                first_audio_handle = playAudioReturnUniqueHandle("first", 0.5, false, 5)
                limited_audio_handle = playAudioReturnUniqueHandle("first", 0.5, false, 5)
                second_audio_handle = playAudioReturnUniqueHandle("loop", 1, true)
                default_audio_handle = playAudioReturnUniqueHandle("defaults", nil, nil, nil)
                setAudioClipVolume(second_audio_handle, -0.25)
                wrong_name_type_rejected = not pcall(function()
                    playAudioReturnUniqueHandle(nil)
                end)
                wrong_optional_type_rejected = not pcall(function()
                    playAudioReturnUniqueHandle("first", "0.5")
                end)
                fractional_handle_rejected = not pcall(function()
                    setAudioClipVolume(second_audio_handle + 0.5, 1)
                end)
                wrong_limit_type_rejected = not pcall(function()
                    setChannelCountLimit(5, "1")
                end)
                stopAudioWithHandle(first_audio_handle)
                "#,
        )
        .unwrap();
    let environment = game_environment(runtime.lua()).unwrap();
    let first = environment.get::<i64>("first_audio_handle").unwrap();
    let second = environment.get::<i64>("second_audio_handle").unwrap();
    assert_eq!(first, 0);
    assert_eq!(second, 1);
    assert_eq!(environment.get::<i64>("limited_audio_handle").unwrap(), -1);
    assert_eq!(environment.get::<i64>("default_audio_handle").unwrap(), 2);
    assert!(environment.get::<bool>("wrong_name_type_rejected").unwrap());
    assert!(
        environment
            .get::<bool>("wrong_optional_type_rejected")
            .unwrap()
    );
    assert!(
        environment
            .get::<bool>("fractional_handle_rejected")
            .unwrap()
    );
    assert!(
        environment
            .get::<bool>("wrong_limit_type_rejected")
            .unwrap()
    );
    let _ = fs::remove_dir_all(root);
}

#[test]
fn native_texture_state_reaches_scene_render_commands() {
    let runtime = StellaLua::new("/tmp").unwrap();
    runtime
        .execute_source(
            r#"
                createCircle("textured", "RED_CROSS", 1, 2, 1, 0, 0, 0, true, false, 1)
                assert(select('#', native_setSprite("textured", "RED_CROSS")) == 0)
                assert(not pcall(native_setSprite, "textured"))
                objects.world.textured.texture = "lua-texture"
                assert(select('#', setTexture(
                    "textured", "THEME_HOMETREE_BG_TEXTURE_1"
                )) == 0)
                assert(not pcall(setTexture, "textured", nil))
                texture_mirror = objects.world.textured.texture
                texture_name_rejected = not pcall(
                    setTexture, false, "THEME_HOMETREE_BG_TEXTURE_1"
                )
                texture_missing_ok, texture_missing_error = pcall(
                    setTexture, "missing", "THEME_HOMETREE_BG_TEXTURE_1"
                )
                texture_missing_error = tostring(texture_missing_error)
                objects.world.textured.textureScale = 77
                setTextureScale("textured", 0.0932025)
                texture_scale_mirror = objects.world.textured.textureScale
                texture_scale_name_rejected = not pcall(setTextureScale, false, 1)
                texture_scale_value_rejected = not pcall(
                    setTextureScale, "textured", "0.5"
                )
                texture_scale_short_rejected = not pcall(
                    setTextureScale, "textured"
                )
                texture_scale_missing_ok, texture_scale_missing_error = pcall(
                    setTextureScale, "missing", 1
                )
                texture_scale_missing_error = tostring(texture_scale_missing_error)
                drawGameNative()
                "#,
        )
        .unwrap();
    let environment = game_environment(runtime.lua()).unwrap();
    assert_eq!(
        environment.get::<String>("texture_mirror").unwrap(),
        "lua-texture"
    );
    assert!(environment.get::<bool>("texture_name_rejected").unwrap());
    assert!(!environment.get::<bool>("texture_missing_ok").unwrap());
    assert!(
        environment
            .get::<String>("texture_missing_error")
            .unwrap()
            .contains("Missing object: missing")
    );
    assert_eq!(
        environment.get::<f64>("texture_scale_mirror").unwrap(),
        77.0
    );
    for field in [
        "texture_scale_name_rejected",
        "texture_scale_value_rejected",
        "texture_scale_short_rejected",
    ] {
        assert!(environment.get::<bool>(field).unwrap(), "{field}");
    }
    assert!(!environment.get::<bool>("texture_scale_missing_ok").unwrap());
    assert!(
        environment
            .get::<String>("texture_scale_missing_error")
            .unwrap()
            .contains("Missing object: missing")
    );
    let bridge = runtime.render.lock().unwrap();
    assert_eq!(
        bridge.commands[0].texture.as_deref(),
        Some("THEME_HOMETREE_BG_TEXTURE_1")
    );
    assert_eq!(bridge.commands[0].texture_scale, f64::from(0.0932025_f32));
}

#[test]
fn set_texture_retains_its_resolved_native_image_pointer_across_replacement() {
    let unique = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let root = std::env::temp_dir().join(format!("stella-object-texture-pointer-{unique}"));
    let data_root = root.join("data");
    for directory in ["base", "first", "second"] {
        fs::create_dir_all(data_root.join(directory)).unwrap();
    }
    fs::write(
        data_root.join("base/BASE.dat"),
        test_textured_sprite_sheet("BASE_SPRITE", "base.pvr", 12, 18),
    )
    .unwrap();
    fs::write(data_root.join("base/base.pvr"), []).unwrap();
    fs::write(
        data_root.join("first/MASK.dat"),
        test_textured_sprite_sheet("MASK_SPRITE", "first.pvr", 10, 20),
    )
    .unwrap();
    fs::write(data_root.join("first/first.pvr"), []).unwrap();
    fs::write(
        data_root.join("second/MASK.dat"),
        test_textured_sprite_sheet("MASK_SPRITE", "second.pvr", 30, 40),
    )
    .unwrap();
    fs::write(data_root.join("second/second.pvr"), []).unwrap();

    let runtime = StellaLua::new(&data_root).unwrap();
    runtime
        .execute_source(
            r#"
                res.createSpriteSheet("base/BASE.dat")
                res.createSpriteSheet("first/MASK.dat")
                createNonPhysicsObject("textured", "BASE_SPRITE", 0, 0, 1)
                setTexture("textured", "MASK")
                res.createSpriteSheet("second/MASK.dat", true)
                res.releaseSpriteSheet("second/MASK.dat", false)
                drawGameNative()
            "#,
        )
        .unwrap();

    let bridge = runtime.render.lock().unwrap();
    assert_eq!(bridge.commands.len(), 1);
    let command = &bridge.commands[0];
    assert_eq!(command.bound_region.as_ref().unwrap().sprite.width, 12);
    match command.masked_texture_binding.as_ref().unwrap() {
        MaskedTextureBinding::Source(source) => assert!(source.ends_with("first/first.pvr")),
        MaskedTextureBinding::Missing => panic!("setTexture resolved a live image"),
    }
    drop(bridge);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn native_material_enum_is_distinct_from_lua_collision_material() {
    let runtime = StellaLua::new("/tmp").unwrap();
    runtime
        .execute_source(
            r#"
                createBox("body", "", 0, 0, 1, 1, 1, 0, 0, true, false, 1)
                objects.world.body.material = "lua-material"
                setMaterial("body", "wood")
                setMaterial("body", "decoration")
                material_mirror = objects.world.body.material

                material_name_rejected = not pcall(
                    setMaterial, false, "wood"
                )
                material_value_rejected = not pcall(
                    setMaterial, "body", false
                )
                missing_recognized_ok, missing_recognized_error = pcall(
                    setMaterial, "missing", "stone"
                )
                missing_recognized_error = tostring(missing_recognized_error)
                missing_unknown_ok = pcall(
                    setMaterial, "missing", "decoration"
                )
                "#,
        )
        .unwrap();

    let environment = game_environment(runtime.lua()).unwrap();
    assert_eq!(
        environment.get::<String>("material_mirror").unwrap(),
        "lua-material"
    );
    assert!(environment.get::<bool>("material_name_rejected").unwrap());
    assert!(environment.get::<bool>("material_value_rejected").unwrap());
    assert!(!environment.get::<bool>("missing_recognized_ok").unwrap());
    assert!(
        environment
            .get::<String>("missing_recognized_error")
            .unwrap()
            .contains("Missing object: missing")
    );
    assert!(environment.get::<bool>("missing_unknown_ok").unwrap());

    let bridge = runtime.render.lock().unwrap();
    let body = &bridge.scene["body"];
    assert_eq!(body.native_material, 1);
    assert!(body.material.is_empty());
}

#[test]
fn native_scene_draw_uses_game_world_scale_only_for_parameter_21_mode_two() {
    let runtime = StellaLua::new("/tmp").unwrap();
    runtime
            .execute_source(
                r#"
                setGameParameters({ gameWorldScale = 0.1 })
                setWorldScale(5)
                createBox("ordinary_dynamic", "ORDINARY_DYNAMIC", 0, 0, 1, 1, 1, 0, 0, true, false, 1)
                createBox("mode_two", "MODE_TWO", 0, 0, 1, 1, 1, 0, 0, true, false, 1)
                createBox("static", "STATIC", 0, 0, 1, 1, 0, 0, 0, true, false, 0)
                createCircle("circle", "CIRCLE", 0, 0, 1, 1, 0, 0, true, false, 2)
                createNonPhysicsObject("decoration", "DECORATION", 0, 0, 3)
                setObjectParameter("mode_two", 21, 2)
                setObjectParameter("circle", 5, 0.09)
                setObjectParameter("circle", 8, 1)
                setObjectParameter("circle", 21, 2)
                setObjectParameter("decoration", 5, 0.1)
                setObjectParameter("decoration", 21, 2)
                circle_scale_x, circle_scale_y = getScale("circle")
                drawGameNative()
                "#,
            )
            .unwrap();

    let bridge = runtime.render.lock().unwrap();
    let ordinary_dynamic = bridge
        .commands
        .iter()
        .find(|command| command.sprite == "ORDINARY_DYNAMIC")
        .unwrap();
    let mode_two = bridge
        .commands
        .iter()
        .find(|command| command.sprite == "MODE_TWO")
        .unwrap();
    let static_object = bridge
        .commands
        .iter()
        .find(|command| command.sprite == "STATIC")
        .unwrap();
    let circle = bridge
        .commands
        .iter()
        .find(|command| command.sprite == "CIRCLE")
        .unwrap();
    let decoration = bridge
        .commands
        .iter()
        .find(|command| command.sprite == "DECORATION")
        .unwrap();
    assert!((ordinary_dynamic.state.scale_x - 5.0).abs() < 1e-9);
    assert!((ordinary_dynamic.state.scale_y - 5.0).abs() < 1e-9);
    let native_mode_two_scale = f64::from(5.0_f32 * 0.1_f32);
    assert!((mode_two.state.scale_x - native_mode_two_scale).abs() < 1e-9);
    assert!((mode_two.state.scale_y - native_mode_two_scale).abs() < 1e-9);
    assert!((static_object.state.scale_x - 5.0).abs() < 1e-9);
    assert!((static_object.state.scale_y - 5.0).abs() < 1e-9);
    let native_circle_scale = f64::from(5.0_f32 * 0.09_f32);
    assert!((circle.state.scale_x + native_circle_scale).abs() < 1e-9);
    assert!((circle.state.scale_y - native_circle_scale).abs() < 1e-9);
    let native_decoration_scale = f64::from(5.0_f32 * 0.1_f32);
    assert!((decoration.state.scale_x - native_decoration_scale).abs() < 1e-9);
    assert!((decoration.state.scale_y - native_decoration_scale).abs() < 1e-9);
    assert_eq!(bridge.scene["decoration"].sensor_type, -1);
    drop(bridge);
    let environment = game_environment(runtime.lua()).unwrap();
    assert!((environment.get::<f64>("circle_scale_x").unwrap() - f64::from(0.09_f32)).abs() < 1e-9);
    assert!((environment.get::<f64>("circle_scale_y").unwrap() - f64::from(0.09_f32)).abs() < 1e-9);
}
