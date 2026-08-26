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
    runtime
        .execute_source(r#"setRotation("animated", 0.25)"#)
        .unwrap();
    {
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
fn native_fixed_step_interpolation_is_action_independent_for_flash_birds() {
    let runtime = unlocked_test_runtime();
    runtime
        .execute_source(
            r#"
                createCircle("bird", "RED_CROSS", 1, 2, 0.25, 1, 0, 0, true, false, 3)
                setFlashAnimation("bird")
                setWorldGravity(0, 0)
                setVelocity("bird", 3, -1.5)
                "#,
        )
        .unwrap();
    {
        let mut definition = AnimationDefinition::default();
        definition.slots.push("SLOT_BODY".to_owned());
        let mut action = AnimationAction::default();
        action
            .targets
            .entry("SLOT_BODY".to_owned())
            .or_default()
            .sprite
            .push((0.0, "BIRD_BODY".to_owned()));
        // Deliberately not one of the bird ability action names: native
        // interpolation belongs to RenderObjectData, not the animation state.
        definition
            .actions
            .insert("arbitrary_action".to_owned(), action);
        let mut animation = runtime._animation_runtime.lock().unwrap();
        animation.definitions.insert("bird".to_owned(), definition);
        animation.playback.insert(
            "bird".to_owned(),
            AnimationPlayback::active(
                "arbitrary_action".to_owned(),
                "once".to_owned(),
                0.0,
                1.0,
                1.0,
            ),
        );
        bind_test_animation_sprites(&mut animation, "bird", &["BIRD_BODY"]);
    }

    let step = f64::from(f32::from_bits(0x3D08_8889));
    runtime.update(step).unwrap();
    runtime.update(f64::from(1.0_f32 / 60.0_f32)).unwrap();
    runtime.execute_source("drawGameNative()").unwrap();

    let bridge = runtime.render.lock().unwrap();
    assert_eq!(bridge.commands.len(), 1);
    let object = &bridge.scene["bird"];
    let alpha = bridge.physics_accumulator * f32::from_bits(0x41EF_FFFF);
    let expected_x = alpha.mul_add(object.x as f32, (1.0_f32 - alpha) * 1.0_f32);
    let expected_y = alpha.mul_add(object.y as f32, (1.0_f32 - alpha) * 2.0_f32);
    assert_eq!(object.render_x.to_bits(), f64::from(expected_x).to_bits());
    assert_eq!(object.render_y.to_bits(), f64::from(expected_y).to_bits());
    let transform = runtime._animation_runtime.lock().unwrap().transforms["bird"];
    assert_eq!(transform.x, f64::from(expected_x) * 20.0);
    assert_eq!(transform.y, f64::from(expected_y) * 20.0);
    assert_ne!(object.render_x, object.x);
}

#[test]
fn flying_bird_visual_angle_interpolates_the_velocity_overwritten_by_lua() {
    let runtime = unlocked_test_runtime();
    runtime
        .execute_source(
            r#"
                createCircle("stella", "RED_CROSS", 1, 2, 0.25, 1, 0, 0, true, false, 3)
                setFlashAnimation("stella")
                setRotation("stella", 0.7853982)
                "#,
        )
        .unwrap();
    {
        let mut definition = AnimationDefinition::default();
        definition.slots.push("SLOT_BODY".to_owned());
        let mut action = AnimationAction::default();
        action
            .targets
            .entry("SLOT_BODY".to_owned())
            .or_default()
            .sprite
            .push((0.0, "STELLA_BODY".to_owned()));
        definition
            .actions
            .insert("Stella_Flying".to_owned(), action);
        let mut animation = runtime._animation_runtime.lock().unwrap();
        animation
            .definitions
            .insert("stella".to_owned(), definition);
        animation.playback.insert(
            "stella".to_owned(),
            AnimationPlayback::active(
                "Stella_Flying".to_owned(),
                "repeat".to_owned(),
                0.0,
                1.0,
                1.0,
            ),
        );
        bind_test_animation_sprites(&mut animation, "stella", &["STELLA_BODY"]);
    }
    {
        let mut bridge = runtime.render.lock().unwrap();
        bridge.physics_interpolation_slot = 1;
        bridge.physics_accumulator = 1.0_f32 / 60.0_f32;
        let body = bridge.scene.get_mut("stella").unwrap();
        body.display_interpolation_velocities[0] = DisplayInterpolationVelocity { x: 4.0, y: 0.0 };
        body.display_interpolation_velocities[1] = DisplayInterpolationVelocity { x: 4.0, y: 4.0 };
    }

    runtime.execute_source("drawGameNative()").unwrap();

    let alpha = (1.0_f32 / 60.0_f32) * f32::from_bits(0x41EF_FFFF);
    let previous_weight = 1.0_f32 - alpha;
    let velocity_x = alpha.mul_add(4.0, previous_weight * 4.0);
    let velocity_y = alpha.mul_add(4.0, previous_weight * 0.0);
    let target = f64::from(velocity_y).atan2(f64::from(velocity_x));
    let expected = f64::from(target as f32);
    let transform = runtime._animation_runtime.lock().unwrap().transforms["stella"];
    assert_eq!(transform.angle.to_bits(), expected.to_bits());
    assert_ne!(transform.angle, f64::from(std::f32::consts::FRAC_PI_4));
    // The compatibility sample is submission-only. Native/Lua pose state
    // still contains the exact setRotation result recovered from Purple.
    assert_eq!(
        runtime.render.lock().unwrap().scene["stella"].render_angle,
        f64::from(std::f32::consts::FRAC_PI_4)
    );
}

#[test]
fn flying_bird_low_speed_replays_angle_lerp_with_interpolated_velocity() {
    let runtime = unlocked_test_runtime();
    runtime
        .execute_source(
            r#"
                createCircle("luca", "RED_CROSS", 1, 2, 0.25, 1, 0, 0, true, false, 3)
                setFlashAnimation("luca")
                setObjectParameter("luca", 8, 1)
                setRotation("luca", 0.25)
                "#,
        )
        .unwrap();
    {
        let mut definition = AnimationDefinition::default();
        definition.slots.push("SLOT_BODY".to_owned());
        let mut action = AnimationAction::default();
        action
            .targets
            .entry("SLOT_BODY".to_owned())
            .or_default()
            .sprite
            .push((0.0, "LUCA_BODY".to_owned()));
        definition.actions.insert("Luca_Flying".to_owned(), action);
        let mut animation = runtime._animation_runtime.lock().unwrap();
        animation.definitions.insert("luca".to_owned(), definition);
        animation.playback.insert(
            "luca".to_owned(),
            AnimationPlayback::active("Luca_Flying".to_owned(), "repeat".to_owned(), 0.0, 1.0, 1.0),
        );
        bind_test_animation_sprites(&mut animation, "luca", &["LUCA_BODY"]);
    }
    {
        let mut bridge = runtime.render.lock().unwrap();
        bridge.physics_interpolation_slot = 1;
        bridge.physics_accumulator = 1.0_f32 / 60.0_f32;
        let body = bridge.scene.get_mut("luca").unwrap();
        body.display_interpolation_velocities = [
            DisplayInterpolationVelocity { x: -0.25, y: -1.0 },
            DisplayInterpolationVelocity { x: -0.25, y: -0.5 },
        ];
    }

    runtime.execute_source("drawGameNative()").unwrap();

    let alpha = (1.0_f32 / 60.0_f32) * f32::from_bits(0x41EF_FFFF);
    let velocity_x = f64::from(alpha.mul_add(-0.25, (1.0_f32 - alpha) * -0.25));
    let velocity_y = f64::from(alpha.mul_add(-0.5, -(1.0_f32 - alpha)));
    let target = velocity_y.atan2(velocity_x);
    let speed = (velocity_x * velocity_x + velocity_y * velocity_y).sqrt();
    let mut difference = std::f64::consts::PI - target;
    if difference > std::f64::consts::PI {
        difference -= std::f64::consts::TAU;
    }
    let flight_angle = std::f64::consts::PI - difference * (0.5 * speed);
    let normalized_flight = f64::from(flight_angle as f32);
    let vector_angle = normalized_flight.sin().atan2(normalized_flight.cos());
    let adjusted = vector_angle - std::f64::consts::PI;
    let tau = std::f32::consts::PI + std::f32::consts::PI;
    let mut native_expected = adjusted as f32 % tau;
    if native_expected < 0.0 {
        native_expected += tau;
    }
    let expected = f64::from(native_expected);
    let angle = runtime._animation_runtime.lock().unwrap().transforms["luca"].angle;
    assert_eq!(angle.to_bits(), expected.to_bits());
    assert_ne!(angle, 0.25);
}

#[test]
fn released_poppy_power_keeps_the_lua_pinned_pose_at_normal_time() {
    let runtime = StellaLua::new("/tmp").unwrap();
    runtime
        .execute_source(
            r#"
                createNonPhysicsObject("poppy", "RED_CROSS", 1, 2, 3)
                setFlashAnimation("poppy")
                "#,
        )
        .unwrap();
    {
        let mut bridge = runtime.render.lock().unwrap();
        bridge.physics_accumulator = 1.0 / 60.0;
        assert_eq!(bridge.delta_time_multiplier, 1.0);
        let object = bridge.scene.get_mut("poppy").unwrap();
        // Poppy's release state retains the incoming body velocity while Lua
        // pins the body to poppyStartX/Y until the drill begins.
        object.velocity_x = 6.0;
        object.velocity_y = -3.0;
    }
    {
        let mut definition = AnimationDefinition::default();
        definition.slots.push("SLOT_BODY".to_owned());
        let mut action = AnimationAction::default();
        action
            .targets
            .entry("SLOT_BODY".to_owned())
            .or_default()
            .sprite
            .push((0.0, "POPPY_BODY".to_owned()));
        definition.actions.insert("Poppy_Power".to_owned(), action);
        let mut animation = runtime._animation_runtime.lock().unwrap();
        animation.definitions.insert("poppy".to_owned(), definition);
        animation.playback.insert(
            "poppy".to_owned(),
            AnimationPlayback::active("Poppy_Power".to_owned(), "once".to_owned(), 0.0, 1.0, 1.0),
        );
        bind_test_animation_sprites(&mut animation, "poppy", &["POPPY_BODY"]);
    }

    runtime.execute_source("drawGameNative()").unwrap();

    let bridge = runtime.render.lock().unwrap();
    assert_eq!(bridge.commands.len(), 1);
    assert_eq!((bridge.commands[0].x, bridge.commands[0].y), (20.0, 40.0));
    assert_eq!(
        (bridge.scene["poppy"].x, bridge.scene["poppy"].y),
        (1.0, 2.0)
    );
}

#[test]
fn ordinary_willow_action_keeps_its_solved_physics_pose_and_authored_angle() {
    let runtime = StellaLua::new("/tmp").unwrap();
    runtime
        .execute_source(
            r#"
                createNonPhysicsObject("willow", "RED_CROSS", 1, 2, 3)
                setFlashAnimation("willow")
                "#,
        )
        .unwrap();
    {
        let mut bridge = runtime.render.lock().unwrap();
        bridge.physics_accumulator = 1.0 / 60.0;
        let object = bridge.scene.get_mut("willow").unwrap();
        object.velocity_x = 6.0;
        object.velocity_y = -3.0;
    }
    runtime
        .execute_source(r#"setRotation("willow", 0.75)"#)
        .unwrap();
    {
        let mut definition = AnimationDefinition::default();
        definition.slots.push("SLOT_BODY".to_owned());
        let mut action = AnimationAction::default();
        action
            .targets
            .entry("SLOT_BODY".to_owned())
            .or_default()
            .sprite
            .push((0.0, "WILLOW_BODY".to_owned()));
        definition
            .actions
            .insert("Willow_Flying".to_owned(), action);
        let mut animation = runtime._animation_runtime.lock().unwrap();
        animation
            .definitions
            .insert("willow".to_owned(), definition);
        animation.playback.insert(
            "willow".to_owned(),
            AnimationPlayback::active(
                "Willow_Flying".to_owned(),
                "repeat".to_owned(),
                0.0,
                1.0,
                1.0,
            ),
        );
        bind_test_animation_sprites(&mut animation, "willow", &["WILLOW_BODY"]);
    }

    runtime.execute_source("drawGameNative()").unwrap();

    assert_eq!(
        runtime._animation_runtime.lock().unwrap().transforms["willow"].angle,
        0.75
    );
    let bridge = runtime.render.lock().unwrap();
    assert_eq!(bridge.commands.len(), 1);
    assert_eq!((bridge.commands[0].x, bridge.commands[0].y), (20.0, 40.0));
    assert_eq!(
        (bridge.scene["willow"].x, bridge.scene["willow"].y),
        (1.0, 2.0)
    );
}

#[test]
fn scripted_stella_ability_keeps_its_exact_per_frame_lua_pose() {
    let runtime = StellaLua::new("/tmp").unwrap();
    runtime
        .execute_source(
            r#"
                createNonPhysicsObject("stella", "RED_CROSS", 1, 2, 3)
                setFlashAnimation("stella")
                "#,
        )
        .unwrap();
    {
        let mut bridge = runtime.render.lock().unwrap();
        bridge.physics_accumulator = 1.0 / 60.0;
        let object = bridge.scene.get_mut("stella").unwrap();
        object.velocity_x = 6.0;
        object.velocity_y = -3.0;
    }
    runtime
        .execute_source(r#"setRotation("stella", 0.75)"#)
        .unwrap();
    {
        let mut definition = AnimationDefinition::default();
        definition.slots.push("SLOT_BODY".to_owned());
        let mut action = AnimationAction::default();
        action
            .targets
            .entry("SLOT_BODY".to_owned())
            .or_default()
            .sprite
            .push((0.0, "STELLA_BODY".to_owned()));
        definition.actions.insert("Ability".to_owned(), action);
        let mut animation = runtime._animation_runtime.lock().unwrap();
        animation
            .definitions
            .insert("stella".to_owned(), definition);
        animation.playback.insert(
            "stella".to_owned(),
            AnimationPlayback::active("Ability".to_owned(), "repeat".to_owned(), 0.0, 1.0, 1.0),
        );
        bind_test_animation_sprites(&mut animation, "stella", &["STELLA_BODY"]);
    }

    runtime.execute_source("drawGameNative()").unwrap();

    assert_eq!(
        runtime._animation_runtime.lock().unwrap().transforms["stella"].angle,
        0.75
    );
    let bridge = runtime.render.lock().unwrap();
    assert_eq!(bridge.commands.len(), 1);
    assert_eq!((bridge.commands[0].x, bridge.commands[0].y), (20.0, 40.0));
    assert_eq!(
        (bridge.scene["stella"].x, bridge.scene["stella"].y),
        (1.0, 2.0)
    );
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
                unknown_callback_ok, unknown_callback_error = pcall(
                    native_setPreDrawFunction, "does_not_exist", function() end
                )
                unknown_callback_error = tostring(unknown_callback_error)
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
    assert!(!environment.get::<bool>("unknown_callback_ok").unwrap());
    assert!(
        environment
            .get::<String>("unknown_callback_error")
            .unwrap()
            .contains("Missing object: does_not_exist")
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
fn native_rectangular_water_replaces_editor_sprite_before_callbacks() {
    let runtime = StellaLua::new("/tmp").unwrap();
    register_test_sprite_sheet(&runtime, &["RED_CROSS", "WATER_PRE"]);
    runtime
        .execute_source(
            r#"
                setWorldScale(2)
                setTopLeft(4, 6)
                createBox("water", "RED_CROSS", 10, 20, 4, 6, 0, 0, 0, true, false, 3)
                native_setIsWater("water", true)
                native_setWaterColor(0.1, 0.2, 0.3, 0.4)
                pre_count = 0
                post_count = 0
                native_setPreDrawFunction("water", function()
                    pre_count = pre_count + 1
                    res.drawSprite("WATER_PRE", 0, 0)
                end)
                native_setPostDrawFunction("water", function()
                    post_count = post_count + 1
                end)
                drawGameNative()
                "#,
        )
        .unwrap();

    let environment = game_environment(runtime.lua()).unwrap();
    assert_eq!(environment.get::<i64>("pre_count").unwrap(), 0);
    assert_eq!(environment.get::<i64>("post_count").unwrap(), 0);
    {
        let bridge = runtime.render.lock().unwrap();
        assert!(bridge.commands.is_empty());
        assert_eq!(bridge.rect_commands.len(), 1);
        let water = &bridge.rect_commands[0];
        assert_eq!(water.order, 0);
        assert_eq!(
            (water.left, water.top, water.right, water.bottom),
            (312.0, 668.0, 472.0, 908.0)
        );
        assert_eq!(
            (water.red, water.green, water.blue, water.alpha),
            (25.0 / 255.0, 51.0 / 255.0, 76.0 / 255.0, f64::from(0.4_f32),)
        );
        assert_eq!(water.color_program, ColorProgram::PlainAlpha);
        assert!(water.clip_rect.is_none());
    }

    {
        let mut bridge = runtime.render.lock().unwrap();
        bridge.commands.clear();
        bridge.rect_commands.clear();
        bridge.next_draw_order = 0;
    }
    runtime
        .execute_source(
            r#"
                setEditing(true)
                drawGameNative()
                "#,
        )
        .unwrap();
    assert_eq!(environment.get::<i64>("pre_count").unwrap(), 1);
    assert_eq!(environment.get::<i64>("post_count").unwrap(), 1);
    let bridge = runtime.render.lock().unwrap();
    assert_eq!(bridge.rect_commands.len(), 1);
    assert_eq!(bridge.commands.len(), 2);
    assert_eq!(bridge.rect_commands[0].order, 0);
    assert_eq!(bridge.commands[0].order, 1);
    assert_eq!(bridge.commands[0].sprite, "WATER_PRE");
    assert_eq!(
        (
            bridge.commands[0].state.translate_x,
            bridge.commands[0].state.translate_y,
            bridge.commands[0].state.scale_x,
            bridge.commands[0].state.scale_y,
            bridge.commands[0].state.angle,
            bridge.commands[0].state.alpha,
        ),
        (0.0, 0.0, 1.0, 1.0, 0.0, 1.0)
    );
    assert_eq!(bridge.commands[1].order, 2);
    assert_eq!(bridge.commands[1].sprite, "RED_CROSS");
}

#[test]
fn chapter02_l16_water_draws_native_fill_without_editor_cross() {
    let sandbox = ShippedDataSandbox::new("chapter02-l16-water");
    let runtime = StellaLua::new(&sandbox.data_root).unwrap();
    runtime.boot("scripts/game.lua").unwrap();
    runtime.execute_source("initializeEventSystem()").unwrap();
    runtime
        .execute_source(
            r#"
                SpriteSheetManager.useGroupSet('INGAME')
                currentFolder = 'Chapter02'
                currentPack = 'Chapter02'
                currentLevel = 16
                levelFolder = 'levels/Chapter02/'
                levelName = 'Chapter02_L16'
                loadLevelInternal(levelFolder .. levelName)
                blocks.BlockComponentManager.triggerGlobalEvent(blocks.events.EID_START)
                drawGameNative()
            "#,
        )
        .unwrap();

    let bridge = runtime.render.lock().unwrap();
    let water_count = bridge
        .scene
        .values()
        .filter(|object| object.visible && object.is_water)
        .count();
    assert!(
        water_count > 0,
        "Chapter02_L16 lost its authored water body"
    );
    assert_eq!(bridge.rect_commands.len(), water_count);
    assert!(bridge.rect_commands.iter().all(|command| {
        command.color_program == ColorProgram::PlainAlpha
            && command.alpha > 0.0
            && command.clip_rect.is_none()
    }));
    assert!(
        bridge
            .commands
            .iter()
            .all(|command| command.sprite != "RED_CROSS"),
        "Chapter02_L16 leaked the water editor placeholder"
    );
}

#[test]
fn native_scene_draw_retains_the_constructor_object_table() {
    let runtime = StellaLua::new("/tmp").unwrap();
    runtime
        .execute_source(
            r#"
                createNonPhysicsObject("retained", "RED_CROSS", 1, 2, 3)
                original = objects.world.retained
                original.marker = "constructor-table"
                objects.world.retained = { name = "retained", marker = "replacement" }
                native_setPreDrawFunction("retained", function(object)
                    callback_marker = object.marker
                    callback_is_original = object == original
                end)
                drawGameNative()
                "#,
        )
        .unwrap();

    let environment = game_environment(runtime.lua()).unwrap();
    assert_eq!(
        environment.get::<String>("callback_marker").unwrap(),
        "constructor-table"
    );
    assert!(environment.get::<bool>("callback_is_original").unwrap());
}

#[test]
fn native_scene_draw_reaches_callbacks_through_the_retained_object_slot() {
    let runtime = StellaLua::new("/tmp").unwrap();
    runtime
        .execute_source(
            r#"
                createNonPhysicsObject("slotted", "RED_CROSS", 1, 2, 3)
                slot_callback_count = 0
                native_setPreDrawFunction("slotted", function()
                    slot_callback_count = slot_callback_count + 1
                end)
            "#,
        )
        .unwrap();

    // RenderObjectData already owns the Lua holders after construction. Drop
    // only the host-side name index used by setter/removal adapters: Purple's
    // hot draw loop must still reach +0x158 through its retained object
    // pointer, and the Rust path must therefore reach the stable slot too.
    assert!(
        runtime
            .draw_callbacks
            .borrow_mut()
            .records
            .remove("slotted")
            .is_some()
    );
    runtime.execute_source("drawGameNative()").unwrap();
    let environment = game_environment(runtime.lua()).unwrap();
    assert_eq!(environment.get::<i64>("slot_callback_count").unwrap(), 1);
}

#[test]
fn native_remove_object_releases_its_retained_lua_draw_state_immediately() {
    let runtime = StellaLua::new("/tmp").unwrap();
    runtime
        .execute_source(
            r#"
                createNonPhysicsObject("removed_ref", "RED_CROSS", 1, 2, 3)
                native_setPreDrawFunction("removed_ref", function() end)
                native_setPostDrawFunction("removed_ref", function() end)
                removeObject("removed_ref")
                "#,
        )
        .unwrap();

    let callbacks = runtime.draw_callbacks.borrow();
    assert!(!callbacks.records.contains_key("removed_ref"));
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
    assert_eq!(body.state.alpha, 0.25_f32);
    assert_eq!((body.state.scale_x, body.state.scale_y), (-2.0, 3.0));
}

#[test]
fn native_pre_draw_replacement_of_post_callback_is_live_for_same_visit() {
    let runtime = StellaLua::new("/tmp").unwrap();
    runtime
        .execute_source(
            r#"
                createNonPhysicsObject("live_post_holder", "RED_CROSS", 0, 0, 3)
                old_post_count = 0
                new_post_count = 0
                native_setPostDrawFunction("live_post_holder", function()
                    old_post_count = old_post_count + 1
                end)
                native_setPreDrawFunction("live_post_holder", function(object)
                    native_setPostDrawFunction(object.name, function()
                        new_post_count = new_post_count + 1
                    end)
                end)
                drawGameNative()
                "#,
        )
        .unwrap();

    let environment = game_environment(runtime.lua()).unwrap();
    assert_eq!(environment.get::<i64>("old_post_count").unwrap(), 0);
    assert_eq!(environment.get::<i64>("new_post_count").unwrap(), 1);
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
fn native_scene_draw_command_retains_the_same_name_and_atlas_pointers() {
    let runtime = StellaLua::new(std::env::temp_dir()).unwrap();
    register_test_sprite_sheet(&runtime, &["BODY"]);
    runtime
        .execute_source(
            r#"
                createNonPhysicsObject("body", "BODY", 0, 0, 3)
                setTextureScale("body", 0.25)
                setTexture("body", "FILL")
            "#,
        )
        .unwrap();

    let bridge = runtime.render.lock().unwrap();
    let scene_object = bridge.scene.get("body").unwrap();
    let retained = scene_object.sprite_region.as_ref().unwrap();
    let first_leaf = bridge
        .scene_render_index
        .name_at(3, retained.native_sheet_id, 0)
        .unwrap();
    let second_leaf = bridge
        .scene_render_index
        .name_at(3, retained.native_sheet_id, 0)
        .unwrap();
    assert!(Arc::ptr_eq(&first_leaf, &second_leaf));
    let snapshot = bridge.scene_draw_object("body").unwrap();
    assert!(Arc::ptr_eq(&scene_object.sprite, &snapshot.sprite));
    let snapshot_region = snapshot.sprite_region.as_ref().unwrap();
    assert!(Arc::ptr_eq(retained, snapshot_region));
    let retained_texture = scene_object.texture.as_ref().unwrap();
    let snapshot_texture = snapshot.texture.as_ref().unwrap();
    assert!(Arc::ptr_eq(retained_texture, snapshot_texture));

    let command = bridge.scene_object_command(&snapshot).unwrap();
    assert!(Arc::ptr_eq(&snapshot.sprite, command.sprite.as_arc()));
    let command_region = command.bound_region.as_ref().unwrap();
    assert!(Arc::ptr_eq(retained, command_region));
    let command_texture = command.texture.as_ref().unwrap();
    assert!(Arc::ptr_eq(retained_texture, command_texture));
    assert_eq!(command.texture_name(), Some("FILL"));
    assert_eq!(command.texture_scale(), 0.25);
}

#[test]
fn native_scene_composite_command_retains_the_same_part_vector_pointer() {
    let unique = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let root = std::env::temp_dir().join(format!("stella-scene-composite-pointer-{unique}"));
    fs::create_dir_all(&root).unwrap();
    fs::write(
        root.join("SHEET.dat"),
        test_textured_sprite_sheet("PART", "sheet.pvr", 12, 14),
    )
    .unwrap();
    fs::write(root.join("sheet.pvr"), []).unwrap();
    fs::write(
        root.join("COMPOSITE.dat"),
        test_composite_set_with_part("BODY", "PART"),
    )
    .unwrap();

    let runtime = StellaLua::new(&root).unwrap();
    runtime
        .execute_source(
            r#"
                res.createSpriteSheet("SHEET.dat")
                res.createCompoSpriteSet("COMPOSITE.dat")
                createNonPhysicsObject("body", "BODY", 0, 0, 3)
            "#,
        )
        .unwrap();

    let bridge = runtime.render.lock().unwrap();
    let scene_object = bridge.scene.get("body").unwrap();
    let retained = scene_object.composite_sprite.as_ref().unwrap();
    let snapshot = bridge.scene_draw_object("body").unwrap();
    let snapshot_parts = snapshot.composite_sprite.as_ref().unwrap();
    assert!(Arc::ptr_eq(retained, snapshot_parts));
    let command = bridge.scene_object_command(&snapshot).unwrap();
    let command_parts = command.bound_composite.as_ref().unwrap();
    assert!(Arc::ptr_eq(retained, command_parts));

    drop(bridge);
    fs::remove_dir_all(root).unwrap();
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
    assert!(callbacks.records.is_empty());
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
fn native_scene_pre_observes_prior_context_and_post_observes_object_context() {
    let runtime = StellaLua::new("/tmp").unwrap();
    register_test_sprite_sheet(&runtime, &["PRE_DRAW", "BODY", "POST_DRAW"]);
    runtime
        .execute_source(
            r#"
                setWorldScale(3)
                setTopLeft(10, 20)
                setRenderState(7, 8, 9, 10, 0.5, 11, 12, 0.75)
                createNonPhysicsObject("callback_transform", "BODY", 1, 2, 3)
                setObjectParameter("callback_transform", 5, 2)
                setObjectParameter("callback_transform", 8, 1)
                setAngle("callback_transform", 0.25)
                setPivotOffset("callback_transform", 12, 18)
                native_setPreDrawFunction("callback_transform", function()
                    res.drawSprite("PRE_DRAW", 5, 6)
                end)
                native_setPostDrawFunction("callback_transform", function()
                    res.drawSprite("POST_DRAW", 7, 8)
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
        (7.0, 8.0)
    );
    assert_eq!(
        (callback.state.scale_x, callback.state.scale_y),
        (9.0, 10.0)
    );
    assert_eq!(callback.state.angle, 0.5);
    assert_eq!(
        (callback.state.pivot_x, callback.state.pivot_y),
        (11.0, 12.0)
    );
    assert_eq!(callback.state.alpha, 0.75);

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

    let post = bridge
        .commands
        .iter()
        .find(|command| command.sprite == "POST_DRAW")
        .unwrap();
    assert!(!post.world_space);
    assert_eq!((post.x, post.y), (7.0, 8.0));
    assert_eq!(
        (post.state.translate_x, post.state.translate_y),
        (-1.0, -1.0)
    );
    assert_eq!((post.state.scale_x, post.state.scale_y), (-6.0, 6.0));
    assert_eq!(post.state.angle, -0.25);
    // BODY's 1x1 test region has an integer pivot of zero. The atlas branch
    // adds the object's two float32 pivot offsets to that native pivot.
    assert_eq!((post.state.pivot_x, post.state.pivot_y), (12.0, 18.0));

    // 0x10004C350 restores only scale after the SpriteSheet vector ends.
    assert_eq!((bridge.state.scale_x, bridge.state.scale_y), (3.0, 3.0));
    assert_eq!(
        (bridge.state.translate_x, bridge.state.translate_y),
        (-1.0, -1.0)
    );
    assert_eq!(bridge.state.angle, -0.25);
    assert_eq!((bridge.state.pivot_x, bridge.state.pivot_y), (12.0, 18.0));
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
        object.composite_sprite = Some(Arc::new(vec![BoundCompositePart {
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
        }]));
    }

    let bridge = runtime.render.lock().unwrap();
    let object = bridge.scene_draw_object("composite").unwrap();
    let state = bridge.scene_post_draw_state(&object);
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
        ((x * 20.0_f32) - top_left_x) * world_scale
    );
    assert_eq!(
        command.state.translate_y,
        ((y * 20.0_f32) - top_left_y) * world_scale
    );
    assert_eq!(command.state.scale_x, (1.0_f32 * world_scale) * scale_x);
    assert_eq!(command.state.scale_y, world_scale * scale_y);
    // sub_10006C838 receives +0xAC. setSpriteRotation writes the distinct
    // +0xB0 field and therefore must not rotate the ordinary sprite twice.
    assert_eq!(command.state.angle, object.angle as f32);

    let callback = bridge.scene_post_draw_state(&object);
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
            cosine * 0.5_f32,
            -sine * 0.25_f32,
            sine * 0.5_f32,
            cosine * 0.25_f32,
        ]
    );
    assert_eq!(pupil.state.alpha, 0.6_f32);
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
                pre_output_channel_rejected = not pcall(
                    setChannelCountLimit, 5, 1
                )
                pre_output_volume_rejected = not pcall(
                    setAudioClipVolume, 0, 1
                )
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
    assert!(
        environment
            .get::<bool>("pre_output_channel_rejected")
            .unwrap()
    );
    assert!(
        environment
            .get::<bool>("pre_output_volume_rejected")
            .unwrap()
    );
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
                setScale("textured", 2, 3)
                setRotation("textured", 0.4)
                setSpriteRotation("textured", 0.2)
                setPivotOffset("textured", 4, 5)
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
        bridge.commands[0].texture_name(),
        Some("THEME_HOMETREE_BG_TEXTURE_1")
    );
    assert_eq!(bridge.commands[0].texture_scale(), f64::from(0.0932025_f32));
    let expected_matrix = RenderState::native_masked_texture_matrix(
        20.0,
        40.0,
        2.0,
        3.0,
        0.4_f32 + 0.2_f32,
        4.0,
        5.0,
    );
    assert_eq!(
        bridge.commands[0].state.masked_texture_matrix,
        Some(expected_matrix.map(|value| value as f32))
    );
    let retained_texture = bridge.commands[0].texture.as_ref().unwrap().clone();
    let original_screen_x = bridge.commands[0].state.translate_x;
    drop(bridge);

    runtime.render.lock().unwrap().commands.clear();
    runtime
        .execute_source(
            r#"
                setTopLeft(300, 400)
                setWorldScale(0.5)
                drawGameNative()
            "#,
        )
        .unwrap();
    let bridge = runtime.render.lock().unwrap();
    assert!(Arc::ptr_eq(
        &retained_texture,
        bridge.commands[0].texture.as_ref().unwrap()
    ));
    assert_ne!(bridge.commands[0].state.translate_x, original_screen_x);
    assert_eq!(
        bridge.commands[0].state.masked_texture_matrix,
        Some(expected_matrix.map(|value| value as f32)),
        "camera movement must not make the terrain texture swim"
    );
}

#[test]
fn texture_scale_update_preserves_an_already_queued_native_submission() {
    let runtime = StellaLua::new("/tmp").unwrap();
    runtime
        .execute_source(
            r#"
                createNonPhysicsObject("textured", "RED_CROSS", 0, 0, 1)
                setTexture("textured", "FILL")
                drawGameNative()
                setTextureScale("textured", 0.25)
                drawGameNative()
            "#,
        )
        .unwrap();

    let bridge = runtime.render.lock().unwrap();
    assert_eq!(bridge.commands.len(), 2);
    assert_eq!(bridge.commands[0].texture_scale(), 1.0);
    assert_eq!(bridge.commands[1].texture_scale(), 0.25);
    assert!(!Arc::ptr_eq(
        bridge.commands[0].texture.as_ref().unwrap(),
        bridge.commands[1].texture.as_ref().unwrap()
    ));
}

#[test]
fn chapter02_themed_terrain_keeps_native_world_anchored_fill_matrices() {
    let sandbox = ShippedDataSandbox::new("chapter02-themed-terrain-fill");
    let runtime = StellaLua::new(&sandbox.data_root).unwrap();
    runtime.boot("scripts/game.lua").unwrap();
    runtime.execute_source("initializeEventSystem()").unwrap();
    runtime
        .execute_source(
            r#"
                SpriteSheetManager.useGroupSet('INGAME')
                currentFolder = 'Chapter02'
                currentPack = 'Chapter02'
                currentLevel = 1
                levelFolder = 'levels/Chapter02/'
                levelName = 'Chapter02_L01'
                loadLevelInternal(levelFolder .. levelName)
                blocks.BlockComponentManager.triggerGlobalEvent(blocks.events.EID_START)
                drawGameNative()
            "#,
        )
        .unwrap();

    let bridge = runtime.render.lock().unwrap();
    let terrain = bridge
        .commands
        .iter()
        .filter(|command| command.texture_name() == Some("THEME_HOMETREE_BOTTOM_TEXTURE_1"))
        .collect::<Vec<_>>();
    assert!(terrain.len() >= 2, "Chapter02_L01 lost its themed terrain");
    assert!(terrain.iter().all(|command| {
        command.texture_scale() == f64::from(0.0932025_f32)
            && command.state.masked_texture_matrix.is_some()
    }));
    let first_origin = terrain[0].state.masked_texture_matrix.unwrap()[..2].to_vec();
    assert!(
        terrain
            .iter()
            .skip(1)
            .any(|command| { command.state.masked_texture_matrix.unwrap()[..2] != first_origin })
    );
}

#[test]
fn native_scene_draw_reads_live_object_shader_for_golden_pollen() {
    let runtime = StellaLua::new("/tmp").unwrap();
    runtime
        .execute_source(
            r#"
                createNonPhysicsObject("golden", "RED_CROSS", 1, 2, 3)
                objects.world.golden.shader = {
                    name = "2d-sprite-gold",
                    params = {
                        { name = "DIFFUSEC", type = "vector", value = { 1, 0.6, 0 } },
                        { name = "LIGHTNESS", type = "float", value = 0.4 },
                        { name = "HIGHLIGHT", type = "float", value = 0.5 },
                    },
                }
                drawGameNative()
                "#,
        )
        .unwrap();

    {
        let bridge = runtime.render.lock().unwrap();
        assert_eq!(bridge.commands.len(), 1);
        let shader = bridge.commands[0].shader.as_ref().unwrap();
        assert_eq!(shader.name, "2d-sprite-gold");
        assert_eq!(shader.diffuse, [1.0, f64::from(0.6_f32), 0.0, 1.0]);
        assert_eq!(shader.lightness, f64::from(0.4_f32));
        assert_eq!(shader.highlight, 0.5);
    }

    runtime.render.lock().unwrap().commands.clear();
    runtime
        .execute_source(
            r#"
                objects.world.golden.shader = nil
                drawGameNative()
                "#,
        )
        .unwrap();
    let bridge = runtime.render.lock().unwrap();
    assert_eq!(bridge.commands.len(), 1);
    assert!(bridge.commands[0].shader.is_none());
}

#[test]
fn chapter01_finale_gold_transformer_reaches_native_scene_submission() {
    let sandbox = ShippedDataSandbox::new("chapter01-l61-gold-shader");
    let runtime = StellaLua::new(&sandbox.data_root).unwrap();
    runtime.boot("scripts/game.lua").unwrap();
    runtime.execute_source("initializeEventSystem()").unwrap();
    runtime
        .execute_source(
            r#"
                SpriteSheetManager.useGroupSet('INGAME')
                currentFolder = 'Chapter01'
                currentPack = 'Chapter01'
                currentLevel = 61
                levelFolder = 'levels/Chapter01/'
                levelName = 'Chapter01_L61'
                loadLevelInternal(levelFolder .. levelName)
                blocks.BlockComponentManager.triggerGlobalEvent(blocks.events.EID_START)
                setPhysicsEnabled(true)
                -- GaleSlice paints the ray-cast Lua object through
                -- `makeGolden`; do not bypass that gameplay gate by calling
                -- its lower-level shader helper directly.
                local painted = objects.world.BLOCK_WOOD_1X10_1_1
                assert(makeGolden(painted))
                assert(painted.shader.name == '2d-sprite-gold')
                drawGameNative()
            "#,
        )
        .unwrap();

    let bridge = runtime.render.lock().unwrap();
    let golden = bridge
        .commands
        .iter()
        .filter_map(|command| command.shader.as_ref())
        .collect::<Vec<_>>();
    assert_eq!(golden.len(), 1);
    assert_eq!(golden[0].name, "2d-sprite-gold");
    assert_eq!(golden[0].diffuse, [1.0, f64::from(0.6_f32), 0.0, 1.0]);
    assert_eq!(golden[0].lightness, f64::from(0.4_f32));
    assert_eq!(golden[0].highlight, 0.5);
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
    match command.masked_texture_binding().unwrap() {
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
    let native_mode_two_scale = 5.0_f32 * 0.1_f32;
    assert!((mode_two.state.scale_x - native_mode_two_scale).abs() < 1e-6);
    assert!((mode_two.state.scale_y - native_mode_two_scale).abs() < 1e-6);
    assert!((static_object.state.scale_x - 5.0).abs() < 1e-9);
    assert!((static_object.state.scale_y - 5.0).abs() < 1e-9);
    let native_circle_scale = 5.0_f32 * 0.09_f32;
    assert!((circle.state.scale_x + native_circle_scale).abs() < 1e-6);
    assert!((circle.state.scale_y - native_circle_scale).abs() < 1e-6);
    let native_decoration_scale = 5.0_f32 * 0.1_f32;
    assert!((decoration.state.scale_x - native_decoration_scale).abs() < 1e-6);
    assert!((decoration.state.scale_y - native_decoration_scale).abs() < 1e-6);
    assert_eq!(bridge.scene["decoration"].sensor_type, -1);
    drop(bridge);
    let environment = game_environment(runtime.lua()).unwrap();
    assert!((environment.get::<f64>("circle_scale_x").unwrap() - f64::from(0.09_f32)).abs() < 1e-9);
    assert!((environment.get::<f64>("circle_scale_y").unwrap() - f64::from(0.09_f32)).abs() < 1e-9);
}
