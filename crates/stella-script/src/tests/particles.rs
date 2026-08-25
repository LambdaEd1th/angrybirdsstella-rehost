use super::*;

#[test]
fn native_particle_table_spawns_updates_draws_and_clears() {
    let runtime = unlocked_test_runtime();
    let globals = runtime.lua().globals();
    let particle_api: mlua::Table = globals.raw_get("particles").unwrap();
    assert!(matches!(
        particle_api.raw_get::<mlua::Value>("native_addParticlesWithMode"),
        Ok(mlua::Value::Function(_))
    ));
    assert!(matches!(
        particle_api.raw_get::<mlua::Value>("addParticles"),
        Ok(mlua::Value::Nil)
    ));
    assert!(matches!(
        globals.raw_get::<mlua::Value>("native_addParticlesWithMode"),
        Ok(mlua::Value::Nil)
    ));
    assert!(matches!(
        globals.raw_get::<mlua::Value>("addParticles"),
        Ok(mlua::Value::Nil)
    ));
    runtime
        .execute_source(
            r#"
                particleTable = {
                    particles = {
                        testBurst = {
                            amount = 2,
                            sprites = { "TEST_PARTICLE" },
                            lifeTime = 1,
                            gravityX = 0,
                            gravityY = 0,
                            minVel = 0,
                            maxVel = 0,
                            minAngleEmitter = 0,
                            maxAngleEmitter = 0,
                            minAngle = 0,
                            maxAngle = 0,
                            minAngleVel = 0,
                            maxAngleVel = 0,
                            minScaleBegin = 1,
                            maxScaleBegin = 1,
                            minScaleEnd = 0,
                            maxScaleEnd = 0
                        }
                    }
                }
                particles.native_addParticlesWithMode({
                    definitionName = "testBurst", amount = 2,
                    x = 10, y = 20, w = 0, h = 0, angle = 0, mode = 0
                })
                update = function() end
                "#,
        )
        .unwrap();

    assert_eq!(
        runtime
            .render
            .lock()
            .unwrap()
            .particle_system
            .particles
            .len(),
        2
    );
    runtime
        .execute_source("native_drawForegroundParticles()")
        .unwrap();
    {
        let bridge = runtime.render.lock().unwrap();
        assert_eq!(bridge.commands.len(), 2);
        assert!(
            bridge
                .commands
                .iter()
                .all(|command| command.sprite == "TEST_PARTICLE")
        );
    }
    runtime.update(0.5).unwrap();
    assert!(
        runtime
            .render
            .lock()
            .unwrap()
            .particle_system
            .particles
            .iter()
            .all(|particle| (particle.elapsed - 0.5).abs() < 1e-9)
    );
    runtime
        .execute_source("clearParticlesWithTagNative('INGAME_FOREGROUND')")
        .unwrap();
    assert!(
        runtime
            .render
            .lock()
            .unwrap()
            .particle_system
            .particles
            .is_empty()
    );

    runtime
        .execute_source(
            r#"
                particles.native_addParticlesWithMode({
                    definitionName = "testBurst", amount = 1001,
                    x = 0, y = 0, w = 0, h = 0, angle = 0, mode = 1,
                    ignoreLimits = true
                })
                "#,
        )
        .unwrap();
    // The query override was false when this definition was first cached.
    // Later calls reuse ParticleSystemData+0x78, so changing it to true does
    // not bypass the soft limit and the 1001 request is halved.
    assert_eq!(
        runtime
            .render
            .lock()
            .unwrap()
            .particle_system
            .particles
            .len(),
        500
    );
}

#[test]
fn particle_spawned_by_lua_update_integrates_in_the_same_native_frame() {
    let runtime = unlocked_test_runtime();
    runtime
        .execute_source(
            r#"
                particleTable = {
                    particles = {
                        sameFrame = {
                            amount = 1,
                            sprites = { "PARTICLE" },
                            lifeTime = 1,
                            gravityX = 0, gravityY = 0,
                            minVel = 0, maxVel = 0,
                            minAngleEmitter = 0, maxAngleEmitter = 0,
                            minAngle = 0, maxAngle = 0,
                            minAngleVel = 0, maxAngleVel = 0,
                            minScaleBegin = 1, maxScaleBegin = 1,
                            minScaleEnd = 1, maxScaleEnd = 1
                        }
                    }
                }
                spawned = false
                update = function()
                    if not spawned then
                        spawned = true
                        particles.native_addParticlesWithMode({
                            definitionName = "sameFrame", amount = 1,
                            x = 0, y = 0, w = 0, h = 0, angle = 0, mode = 0
                        })
                    end
                end
            "#,
        )
        .unwrap();

    runtime.update(0.25).unwrap();
    let bridge = runtime.render.lock().unwrap();
    assert_eq!(bridge.particle_system.particles.len(), 1);
    assert_eq!(bridge.particle_system.particles[0].elapsed, 0.25_f32);
}

#[test]
fn particle_creation_retains_the_resolved_atlas_across_shadow_and_release() {
    let unique = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let root = std::env::temp_dir().join(format!("stella-particle-pointer-{unique}"));
    let data_root = root.join("data");
    for directory in ["first", "second"] {
        fs::create_dir_all(data_root.join(directory)).unwrap();
    }
    fs::write(
        data_root.join("first/FIRST.dat"),
        test_textured_sprite_sheet("PARTICLE", "first.pvr", 10, 20),
    )
    .unwrap();
    fs::write(data_root.join("first/first.pvr"), []).unwrap();
    fs::write(
        data_root.join("second/SECOND.dat"),
        test_textured_sprite_sheet("PARTICLE", "second.pvr", 30, 40),
    )
    .unwrap();
    fs::write(data_root.join("second/second.pvr"), []).unwrap();

    let runtime = StellaLua::new(&data_root).unwrap();
    runtime
        .execute_source(
            r#"
                res.createSpriteSheet("first/FIRST.dat")
                particleTable = { particles = { retained = {
                    amount=1, sprites={"PARTICLE"}, lifeTime=-1,
                    gravityX=0, gravityY=0, minVel=0, maxVel=0,
                    minAngleEmitter=0, maxAngleEmitter=0,
                    minAngle=0, maxAngle=0, minAngleVel=0, maxAngleVel=0,
                    minScaleBegin=1, maxScaleBegin=1,
                    minScaleEnd=1, maxScaleEnd=1
                } } }
                particles.native_addParticlesWithMode({
                    definitionName="retained", x=0, y=0, w=0, h=0,
                    angle=0, mode=3
                })
                res.createSpriteSheet("second/SECOND.dat")
                res.releaseSpriteSheet("first/FIRST.dat", false)
                drawMenuParticlesNative()
            "#,
        )
        .unwrap();

    assert_eq!(
        runtime.sprite_catalog_snapshot_since(0).unwrap().regions["PARTICLE"]
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

#[test]
fn lifetime_particle_frame_change_rebinds_once_then_retains_that_atlas() {
    let unique = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let root = std::env::temp_dir().join(format!("stella-particle-frame-pointer-{unique}"));
    let data_root = root.join("data");
    for directory in ["initial", "replacement"] {
        fs::create_dir_all(data_root.join(directory)).unwrap();
    }
    fs::write(
        data_root.join("initial/INITIAL.dat"),
        test_textured_sprite_sheet_with_names(
            "initial.pvr",
            &[("FRAME_A", 10, 20), ("FRAME_B", 30, 40)],
        ),
    )
    .unwrap();
    fs::write(data_root.join("initial/initial.pvr"), []).unwrap();
    fs::write(
        data_root.join("replacement/REPLACEMENT.dat"),
        test_textured_sprite_sheet("FRAME_B", "replacement.pvr", 50, 60),
    )
    .unwrap();
    fs::write(data_root.join("replacement/replacement.pvr"), []).unwrap();

    let runtime = StellaLua::new(&data_root).unwrap();
    runtime
        .execute_source(
            r#"
                setPhysicsEnabled(true)
                res.createSpriteSheet("initial/INITIAL.dat")
                particleTable = { particles = { animated = {
                    amount=1, sprites={"FRAME_A", "FRAME_B"},
                    animation="lifeTime", lifeTime=1,
                    gravityX=0, gravityY=0, minVel=0, maxVel=0,
                    minAngleEmitter=0, maxAngleEmitter=0,
                    minAngle=0, maxAngle=0, minAngleVel=0, maxAngleVel=0,
                    minScaleBegin=1, maxScaleBegin=1,
                    minScaleEnd=1, maxScaleEnd=1
                } } }
                particles.native_addParticlesWithMode({
                    definitionName="animated", x=0, y=0, w=0, h=0,
                    angle=0, mode=3
                })
                update = function() end
            "#,
        )
        .unwrap();
    {
        let bridge = runtime.render.lock().unwrap();
        let initial = bridge.particle_system.particles[0]
            .bound_region
            .as_ref()
            .unwrap();
        assert_eq!(initial.sprite.width, 10);
    }

    runtime.update(0.6).unwrap();
    runtime
        .execute_source(
            r#"
                res.createSpriteSheet("replacement/REPLACEMENT.dat")
                res.releaseSpriteSheet("initial/INITIAL.dat", false)
                drawMenuParticlesNative()
            "#,
        )
        .unwrap();

    assert_eq!(
        runtime.sprite_catalog_snapshot_since(0).unwrap().regions["FRAME_B"]
            .sprite
            .width,
        50
    );
    let bridge = runtime.render.lock().unwrap();
    assert_eq!(bridge.particle_system.particles[0].sprite, "FRAME_B");
    assert_eq!(bridge.commands.len(), 1);
    let retained = bridge.commands[0].bound_region.as_ref().unwrap();
    assert_eq!(retained.sprite.width, 30);
    assert!(retained.texture_source.ends_with("initial/initial.pvr"));
    drop(bridge);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn lifetime_particle_atlas_frame_draws_before_retained_composite_slot() {
    let unique = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let root = std::env::temp_dir().join(format!("stella-particle-binding-priority-{unique}"));
    let data_root = root.join("data");
    fs::create_dir_all(&data_root).unwrap();
    fs::write(
        data_root.join("PART.dat"),
        test_textured_sprite_sheet("PART", "part.pvr", 12, 14),
    )
    .unwrap();
    fs::write(data_root.join("part.pvr"), []).unwrap();
    fs::write(
        data_root.join("COMPOSITE.dat"),
        test_composite_set_with_part("FRAME_A", "PART"),
    )
    .unwrap();
    fs::write(
        data_root.join("ATLAS.dat"),
        test_textured_sprite_sheet("FRAME_B", "atlas.pvr", 30, 40),
    )
    .unwrap();
    fs::write(data_root.join("atlas.pvr"), []).unwrap();

    let runtime = StellaLua::new(&data_root).unwrap();
    runtime
        .execute_source(
            r#"
                setPhysicsEnabled(true)
                res.createSpriteSheet("PART.dat")
                res.createCompoSpriteSet("COMPOSITE.dat")
                res.createSpriteSheet("ATLAS.dat")
                particleTable = { particles = { animated = {
                    amount=1, sprites={"FRAME_A", "FRAME_B"},
                    animation="lifeTime", lifeTime=1,
                    gravityX=0, gravityY=0, minVel=0, maxVel=0,
                    minAngleEmitter=0, maxAngleEmitter=0,
                    minAngle=0, maxAngle=0, minAngleVel=0, maxAngleVel=0,
                    minScaleBegin=1, maxScaleBegin=1,
                    minScaleEnd=1, maxScaleEnd=1
                } } }
                particles.native_addParticlesWithMode({
                    definitionName="animated", x=0, y=0, w=0, h=0,
                    angle=0, mode=3
                })
                update = function() end
            "#,
        )
        .unwrap();
    {
        let bridge = runtime.render.lock().unwrap();
        let particle = &bridge.particle_system.particles[0];
        assert!(particle.bound_region.is_none());
        assert_eq!(particle.bound_composite.as_ref().unwrap().len(), 1);
    }

    runtime.update(0.6).unwrap();
    runtime.execute_source("drawMenuParticlesNative()").unwrap();
    let bridge = runtime.render.lock().unwrap();
    let particle = &bridge.particle_system.particles[0];
    assert_eq!(particle.sprite, "FRAME_B");
    assert_eq!(particle.bound_region.as_ref().unwrap().sprite.width, 30);
    // Purple retains +0x28 but branches on non-null +0x20 first.
    assert_eq!(particle.bound_composite.as_ref().unwrap().len(), 1);
    assert_eq!(bridge.commands.len(), 1);
    assert!(bridge.commands[0].bound_region.is_some());
    assert!(bridge.commands[0].bound_composite.is_none());
    drop(bridge);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn native_particle_entry_uses_stack_top_and_strict_required_fields() {
    let runtime = unlocked_test_runtime();
    runtime
        .execute_source(
            r#"
                particleTable = { particles = {
                    strict = {
                        amount=1, sprites={"STRICT"}, lifeTime=1,
                        gravityX=0, gravityY=0,
                        minVel=0, maxVel=0,
                        minAngleEmitter=0, maxAngleEmitter=0,
                        minAngle=0, maxAngle=0,
                        minAngleVel=0, maxAngleVel=0,
                        minScaleBegin=1, maxScaleBegin=1,
                        minScaleEnd=1, maxScaleEnd=1
                    },
                    malformed = {
                        amount=1, sprites={"BAD"}, lifeTime=1,
                        gravityY=0,
                        minVel=0, maxVel=0,
                        minAngleEmitter=0, maxAngleEmitter=0,
                        minAngle=0, maxAngle=0,
                        minAngleVel=0, maxAngleVel=0,
                        minScaleBegin=1, maxScaleBegin=1,
                        minScaleEnd=1, maxScaleEnd=1
                    }
                } }
                local valid = {
                    definitionName="strict", x=2, y=3, w=0, h=0,
                    angle=0, mode=1,
                    amount="wrong optional type", z="wrong optional type",
                    themeLayerIndex={}, ignoreDeltaTimeMultiplier=7
                }
                particle_no_args_fails = not pcall(
                    particles.native_addParticlesWithMode
                )
                particle_wrong_top_fails = not pcall(
                    particles.native_addParticlesWithMode, valid, false
                )
                local missing_angle = {
                    definitionName="strict", x=0, y=0, w=0, h=0, mode=1
                }
                particle_missing_angle_fails = not pcall(
                    particles.native_addParticlesWithMode, missing_angle
                )
                local wrong_mode = {
                    definitionName="strict", x=0, y=0, w=0, h=0,
                    angle=0, mode="foreground"
                }
                particle_wrong_mode_fails = not pcall(
                    particles.native_addParticlesWithMode, wrong_mode
                )
                local malformed = {
                    definitionName="malformed", x=0, y=0, w=0, h=0,
                    angle=0, mode=1
                }
                particle_malformed_definition_fails = not pcall(
                    particles.native_addParticlesWithMode, malformed
                )
                -- `sub_10008E524` wraps -1: the leading value is immaterial.
                particles.native_addParticlesWithMode("ignored", valid)
                particles.native_addParticlesWithMode({
                    definitionName="strict", x=4, y=5, w=0, h=0,
                    angle=0, amount=1, mode=0/0, themeLayerIndex=0/0
                })
            "#,
        )
        .unwrap();

    let environment = game_environment(runtime.lua()).unwrap();
    for flag in [
        "particle_no_args_fails",
        "particle_wrong_top_fails",
        "particle_missing_angle_fails",
        "particle_wrong_mode_fails",
        "particle_malformed_definition_fails",
    ] {
        assert!(environment.get::<bool>(flag).unwrap(), "{flag}");
    }
    let bridge = runtime.render.lock().unwrap();
    assert_eq!(bridge.particle_system.particles.len(), 2);
    let particle = &bridge.particle_system.particles[0];
    assert_eq!(particle.sprite, "STRICT");
    assert_eq!((particle.x, particle.y), (2.0, 3.0));
    assert_eq!(particle.z, 0.0);
    assert_eq!(particle.theme_layer_index, -1);
    assert!(!particle.ignore_time_multiplier);
    let indefinite = &bridge.particle_system.particles[1];
    assert_eq!(indefinite.mode, i32::MIN);
    assert_eq!(indefinite.theme_layer_index, i32::MIN);
}

#[test]
fn native_particle_definition_and_limit_flag_are_cached_on_first_use() {
    let runtime = unlocked_test_runtime();
    runtime
        .execute_source(
            r#"
                particleTable = { particles = { cached = {
                    amount=1, sprites={"FIRST"}, lifeTime=2,
                    gravityX=0, gravityY=0,
                    minVel=1, maxVel=1,
                    minAngleEmitter=0, maxAngleEmitter=0,
                    minAngleEmitter=0, maxAngleEmitter=0,
                    minAngle=0, maxAngle=0,
                    minAngleVel=0, maxAngleVel=0,
                    minScaleBegin=1, maxScaleBegin=1,
                    minScaleEnd=1, maxScaleEnd=1
                } } }
                particles.native_addParticlesWithMode({
                    definitionName="cached", x=0, y=0, w=0, h=0,
                    angle=0, mode=1, ignoreLimits=true
                })
                particleTable.particles.cached.sprites = {"SECOND"}
                particleTable.particles.cached.minVel = 9
                particleTable.particles.cached.amount = 70
                particles.native_addParticlesWithMode({
                    definitionName="cached", x=0, y=0, w=0, h=0,
                    angle=0, mode=1, ignoreLimits=false
                })
            "#,
        )
        .unwrap();

    let bridge = runtime.render.lock().unwrap();
    assert_eq!(bridge.particle_system.definitions.len(), 1);
    assert_eq!(bridge.particle_system.particles.len(), 2);
    assert!(
        bridge
            .particle_system
            .particles
            .iter()
            .all(|particle| particle.sprite == "FIRST" && particle.velocity_x == 1.0)
    );
}

#[test]
fn native_particle_signed_amount_runs_through_unsigned_hard_limit_check() {
    let runtime = unlocked_test_runtime();
    runtime
        .execute_source(
            r#"
                particleTable = { particles = { p = {
                    amount=1, sprites={"P"}, lifeTime=1,
                    gravityX=0, gravityY=0,
                    minVel=0, maxVel=0,
                    minAngleEmitter=0, maxAngleEmitter=0,
                    minAngle=0, maxAngle=0,
                    minAngleVel=0, maxAngleVel=0,
                    minScaleBegin=1, maxScaleBegin=1,
                    minScaleEnd=1, maxScaleEnd=1
                } } }
                particles.native_addParticlesWithMode({
                    definitionName="p", amount=-1,
                    x=0, y=0, w=0, h=0, angle=0, mode=1
                })
            "#,
        )
        .unwrap();

    // `(size + amount)` is compared as uint64 in `sub_10008E524`; -1 at an
    // empty vector trips the hard-limit branch and becomes 1000-size.
    assert_eq!(
        runtime
            .render
            .lock()
            .unwrap()
            .particle_system
            .particles
            .len(),
        1_000
    );
}

#[test]
fn native_particle_draw_modes_and_enable_gate_match_game_lua() {
    let runtime = unlocked_test_runtime();
    runtime
        .execute_source(
            r#"
                particleTable = {
                    particles = {
                        p = {
                            amount = 1, sprites = { "P" }, lifeTime = -1,
                            gravityX = 0, gravityY = 0,
                            minVel = 0, maxVel = 0,
                            minAngleEmitter = 0, maxAngleEmitter = 0,
                            minAngle = 0, maxAngle = 0,
                            minAngleVel = 0, maxAngleVel = 0,
                            minScaleBegin = 1, maxScaleBegin = 1,
                            minScaleEnd = 1, maxScaleEnd = 1
                        }
                    }
                }
                levelLeftEdgePhysics, levelRightEdgePhysics = -2, 3
                levelTopEdgePhysics, levelBottomEdgePhysics = -4, 5
                oldLevelLeftEdgePhysics, oldLevelRightEdgePhysics = -2, 3
                oldLevelTopEdgePhysics, oldLevelBottomEdgePhysics = -4, 5
                setLevelLimits(-2, -4, 3, 5)
                particles.native_addParticlesWithMode({ definitionName="p", x=10, y=0, w=0, h=0, angle=0, mode=1 })
                particles.native_addParticlesWithMode({ definitionName="p", x=20, y=0, w=0, h=0, angle=0, mode=2 })
                particles.native_addParticlesWithMode({ definitionName="p", x=30, y=0, w=0, h=0, angle=0, mode=3 })
                particles.native_addParticlesWithMode({ definitionName="p", x=40, y=0, w=0, h=0, angle=0, mode=4 })
                enableInGameParticlesNative(false, true)
                particle_enable_number_fails = not pcall(enableInGameParticlesNative, 1)
                particle_enable_missing_fails = not pcall(enableInGameParticlesNative)
                native_drawForegroundParticles()
                native_drawBackgroundParticles()
                drawMenuParticlesNative()
                native_drawNotificationParticles()
                "#,
        )
        .unwrap();

    {
        runtime.render.lock().unwrap().update_particles(0.25, 0.5);
        let bridge = runtime.render.lock().unwrap();
        assert_eq!(bridge.commands.len(), 2);
        assert_eq!(bridge.commands[0].x, 30.0);
        assert_eq!(bridge.commands[1].x, 40.0);
        assert_eq!(bridge.particle_system.particles[0].elapsed, 0.25);
        assert_eq!(bridge.particle_system.particles[1].elapsed, 0.25);
        assert_eq!(bridge.particle_system.particles[2].elapsed, 0.5);
        assert_eq!(bridge.particle_system.particles[3].elapsed, 0.5);
    }
    let environment = game_environment(runtime.lua()).unwrap();
    assert!(
        environment
            .get::<bool>("particle_enable_number_fails")
            .unwrap()
    );
    assert!(
        environment
            .get::<bool>("particle_enable_missing_fails")
            .unwrap()
    );

    runtime
        .execute_source("setPhysicsEnabled(false, 'particle-update-test')")
        .unwrap();
    runtime.render.lock().unwrap().update_particles(0.25, 0.5);
    {
        let bridge = runtime.render.lock().unwrap();
        assert_eq!(bridge.particle_system.particles[0].elapsed, 0.25);
        assert_eq!(bridge.particle_system.particles[1].elapsed, 0.25);
        assert_eq!(bridge.particle_system.particles[2].elapsed, 1.0);
        assert_eq!(bridge.particle_system.particles[3].elapsed, 1.0);
    }

    runtime
        .execute_source(
            r#"
                setPhysicsEnabled(true, "particle-update-test")
                enableInGameParticlesNative(true)
                native_drawForegroundParticles()
                native_drawBackgroundParticles()
                clearParticlesWithTagNative("INGAME_FOREGROUND")
                clearParticlesWithTagNative("INGAME_BACKGROUND")
                clearParticlesWithTagNative("MENU")
                "#,
        )
        .unwrap();
    let bridge = runtime.render.lock().unwrap();
    assert_eq!(bridge.commands.len(), 4);
    assert_eq!(bridge.commands[2].x, 10.0);
    assert_eq!(bridge.commands[3].x, 20.0);
    assert_eq!(bridge.particle_system.particles.len(), 1);
    assert_eq!(bridge.particle_system.particles[0].mode, 4);
    drop(bridge);

    runtime
        .execute_source(
            r##"
                clear_all_particle_results = select(
                    "#", clearParticlesNative("ignored")
                )
            "##,
        )
        .unwrap();
    assert!(
        runtime
            .render
            .lock()
            .unwrap()
            .particle_system
            .particles
            .is_empty()
    );
    let environment = game_environment(runtime.lua()).unwrap();
    assert_eq!(
        environment
            .get::<i64>("clear_all_particle_results")
            .unwrap(),
        0
    );
}

#[test]
fn ordinary_particle_draw_resets_renderer_state_but_a_gated_pass_does_not() {
    let runtime = unlocked_test_runtime();
    runtime
        .execute_source(
            r#"
                enableInGameParticlesNative(false)
                setRenderState(11, 12, 2, 3, 0.4, 5, 6, 0.25)
                res.setClipRect(10, 20, 30, 40)
                native_drawForegroundParticles()
            "#,
        )
        .unwrap();
    {
        let state = runtime.render.lock().unwrap().state;
        assert_eq!((state.translate_x, state.translate_y), (11.0, 12.0));
        assert_eq!((state.scale_x, state.scale_y), (2.0, 3.0));
        assert_eq!((state.pivot_x, state.pivot_y), (5.0, 6.0));
        assert_eq!(state.alpha, 0.25);
        assert_eq!(state.clip_rect, Some([10, 20, 40, 60]));
    }

    // Modes 3/4 always enter sub_100091D90. Its empty-vector path still
    // constructs and copies the default render-state record before return.
    runtime.execute_source("drawMenuParticlesNative()").unwrap();
    let state = runtime.render.lock().unwrap().state;
    assert_eq!((state.translate_x, state.translate_y), (0.0, 0.0));
    assert_eq!((state.scale_x, state.scale_y), (1.0, 1.0));
    assert_eq!((state.pivot_x, state.pivot_y), (0.0, 0.0));
    assert_eq!(state.angle, 0.0);
    assert_eq!(state.alpha, 1.0);
    assert_eq!(state.clip_rect, None);
}

#[test]
fn ordinary_particle_draw_keeps_native_divide_add_multiply_float_boundaries() {
    let runtime = unlocked_test_runtime();
    runtime
        .execute_source(
            r#"
                particleTable = { particles = { exact = {
                    amount=1, sprites={"P"}, lifeTime=-1,
                    gravityX=0, gravityY=0, minVel=0, maxVel=0,
                    minAngleEmitter=0, maxAngleEmitter=0,
                    minAngle=0, maxAngle=0, minAngleVel=0, maxAngleVel=0,
                    minScaleBegin=1, maxScaleBegin=1,
                    minScaleEnd=1, maxScaleEnd=1
                } } }
                particles.native_addParticlesWithMode({
                    definitionName="exact", x=0, y=0, w=0, h=0,
                    angle=0, mode=1
                })
            "#,
        )
        .unwrap();

    let mut bridge = runtime.render.lock().unwrap();
    let world_position = f32::from_bits(0x4880_9d64);
    let camera = f32::from_bits(0xc806_cee8);
    let particle_scale = f32::from_bits(0x3eca_a3b3);
    let world_scale = f32::from_bits(0x4211_8321);
    {
        let particle = &mut bridge.particle_system.particles[0];
        particle.x = world_position;
        particle.y = world_position;
        particle.current_scale = particle_scale;
        particle.mode = 1;
    }
    bridge.top_left_x = f64::from(camera);
    bridge.top_left_y = f64::from(camera);
    bridge.world_scale = f64::from(world_scale);
    bridge.draw_particles(1);
    let command = &bridge.commands[0];
    assert!(!command.world_space);
    let projected =
        (command.state.translate_x as f32 + command.x as f32) * command.state.scale_x as f32;
    assert_eq!(projected.to_bits(), 0x4b5e_d64d);
    // Algebraically collapsing the native operations and rounding only once
    // lands one pixel lower for this exact ARM-float boundary fixture.
    assert_eq!(
        ((world_position - camera) * world_scale).to_bits(),
        0x4b5e_d64c
    );

    bridge.commands.clear();
    let framebuffer_position = f32::from_bits(0x4850_1c9b);
    let framebuffer_scale = f32::from_bits(0x4041_38ec);
    let menu_scale = f32::from_bits(0x3ed7_6162);
    {
        let particle = &mut bridge.particle_system.particles[0];
        particle.x = framebuffer_position;
        particle.y = framebuffer_position;
        particle.current_scale = framebuffer_scale;
        particle.mode = 3;
    }
    bridge.particle_system.scale = menu_scale;
    bridge.draw_particles(3);
    let command = &bridge.commands[0];
    let projected =
        (command.state.translate_x as f32 + command.x as f32) * command.state.scale_x as f32;
    assert_eq!(projected.to_bits(), 0x4850_1c9a);
    assert_ne!(projected, framebuffer_position);
}

#[test]
fn native_infinite_particles_wrap_at_strict_framebuffer_edges() {
    let runtime = unlocked_test_runtime();
    runtime
        .execute_source(
            r#"
                particleTable = { particles = { p = {
                    amount=1, sprites={"P"}, lifeTime=-1,
                    gravityX=0, gravityY=0,
                    minVel=0, maxVel=0, minAngle=0, maxAngle=0,
                    minAngleEmitter=0, maxAngleEmitter=0,
                    minAngleVel=0, maxAngleVel=0,
                    minScaleBegin=1, maxScaleBegin=1,
                    minScaleEnd=1, maxScaleEnd=1
                } } }
                levelLeftEdgePhysics, levelRightEdgePhysics = -2, 3
                levelTopEdgePhysics, levelBottomEdgePhysics = -4, 5
                oldLevelLeftEdgePhysics, oldLevelRightEdgePhysics = -2, 3
                oldLevelTopEdgePhysics, oldLevelBottomEdgePhysics = -4, 5
                setLevelLimits(-2, -4, 3, 5)
                particles.native_addParticlesWithMode({definitionName="p", mode=3, x=61, y=61, w=0, h=0, angle=0})
                particles.native_addParticlesWithMode({definitionName="p", mode=3, x=-41, y=-41, w=0, h=0, angle=0})
                particles.native_addParticlesWithMode({definitionName="p", mode=3, x=-40, y=60, w=0, h=0, angle=0})
                "#,
        )
        .unwrap();
    let mut bridge = runtime.render.lock().unwrap();
    assert_eq!(bridge.particle_wrap_limits, [-40, 60, -40, 60]);
    bridge.update_particles(0.0, 0.0);
    assert_eq!(
        (
            bridge.particle_system.particles[0].x,
            bridge.particle_system.particles[0].y
        ),
        (-40.0, -40.0)
    );
    assert_eq!(
        (
            bridge.particle_system.particles[1].x,
            bridge.particle_system.particles[1].y
        ),
        (60.0, 60.0)
    );
    assert_eq!(
        (
            bridge.particle_system.particles[2].x,
            bridge.particle_system.particles[2].y
        ),
        (-40.0, 60.0)
    );
}

#[test]
fn native_level_limit_change_remaps_infinite_particles_about_old_viewport_center() {
    let runtime = unlocked_test_runtime();
    runtime
        .execute_source(
            r#"
                particleTable = { particles = { p = {
                    amount=1, sprites={"P"}, lifeTime=-1,
                    gravityX=0, gravityY=0,
                    minVel=0, maxVel=0, minAngle=0, maxAngle=0,
                    minAngleEmitter=0, maxAngleEmitter=0,
                    minAngleVel=0, maxAngleVel=0,
                    minScaleBegin=1, maxScaleBegin=1,
                    minScaleEnd=1, maxScaleEnd=1
                } } }
                levelLeftEdgePhysics, levelRightEdgePhysics = -2, 3
                levelTopEdgePhysics, levelBottomEdgePhysics = -4, 5
                oldLevelLeftEdgePhysics, oldLevelRightEdgePhysics = -2, 3
                oldLevelTopEdgePhysics, oldLevelBottomEdgePhysics = -4, 5
                setLevelLimits(-2, -4, 3, 5)
                particles.native_addParticlesWithMode({definitionName="p", mode=3, x=20, y=20, w=0, h=0, angle=0})

                oldLevelLeftEdgePhysics, oldLevelRightEdgePhysics = -2, 3
                oldLevelTopEdgePhysics, oldLevelBottomEdgePhysics = -4, 5
                levelLeftEdgePhysics, levelRightEdgePhysics = -4, 6
                levelTopEdgePhysics, levelBottomEdgePhysics = -6, 8
                setLevelLimits(-4, -6, 6, 8)
                "#,
        )
        .unwrap();

    let bridge = runtime.render.lock().unwrap();
    assert_eq!(bridge.particle_wrap_limits, [-80, 120, -80, 120]);
    assert_eq!(bridge.particle_system.particles[0].x, 30.0);
    assert_eq!(bridge.particle_system.particles[0].y, 30.0);
}

#[test]
fn native_particle_definition_units_area_and_zero_amount_match_parser() {
    let runtime = unlocked_test_runtime();
    runtime
        .execute_source(
            r#"
                particleTable = { particles = { exact = {
                    amount = 1, sprites = { "P" }, lifeTime = 1,
                    areaW = 4, areaH = 6,
                    emitAreaScaleX = 0.5, emitAreaScaleY = 0.25,
                    gravityX = 0, gravityY = 0,
                    minVel = 2, maxVel = 2,
                    minAngleEmitter = 180, maxAngleEmitter = 180,
                    minAngle = 180, maxAngle = 180,
                    minAngleVel = 2, maxAngleVel = 2,
                    minScaleBegin = 1, maxScaleBegin = 1,
                    minScaleEnd = 1, maxScaleEnd = 1
                } } }
                particles.native_addParticlesWithMode({
                    definitionName="exact", amount=0,
                    x=10, y=20, w=2, h=2, angle=0, mode=0,
                    z=1.25, themeLayerIndex=7.9
                })
                "#,
        )
        .unwrap();

    let mut random = NativeParticleRandom::default();
    let offset_x = (((random.next() as f32) - 0.5_f32) * 6.0_f32) * 0.5_f32;
    let offset_y = (((random.next() as f32) - 0.5_f32) * 8.0_f32) * 0.25_f32;
    let radians = 180.0_f32 * f32::from_bits(0x3c8e_fa35);
    let (sine, cosine) = radians.sin_cos();
    let bridge = runtime.render.lock().unwrap();
    assert_eq!(bridge.particle_system.particles.len(), 1);
    let particle = &bridge.particle_system.particles[0];
    assert_eq!(particle.x.to_bits(), offset_x.mul_add(1.0, 10.0).to_bits());
    assert_eq!(particle.y.to_bits(), offset_y.mul_add(1.0, 20.0).to_bits());
    assert_eq!(particle.velocity_x.to_bits(), (2.0 * cosine).to_bits());
    assert_eq!(particle.velocity_y.to_bits(), (2.0 * sine).to_bits());
    assert_eq!(particle.angle.to_bits(), radians.to_bits());
    assert_eq!(particle.angular_velocity, 2.0);
    assert_eq!(particle.z, 1.25);
    assert_eq!(particle.theme_layer_index, 7);
}

#[test]
fn native_particle_query_angles_are_raw_and_menu_time_default_is_unscaled() {
    let runtime = unlocked_test_runtime();
    runtime
        .execute_source(
            r#"
                particleTable = { particles = { exact = {
                    amount = 1, sprites = { "P" }, lifeTime = 1,
                    gravityX = 0, gravityY = 0,
                    minVel = 2, maxVel = 2,
                    minAngleEmitter = 180, maxAngleEmitter = 180,
                    minAngle = 180, maxAngle = 180,
                    minAngleVel = 2, maxAngleVel = 2,
                    minScaleBegin = 1, maxScaleBegin = 1,
                    minScaleEnd = 1, maxScaleEnd = 1,
                    useAngleFromSpawner = true
                } } }
                particles.native_addParticlesWithMode({
                    definitionName="exact", x=0, y=0, w=0, h=0,
                    mode=3, angle=0.25,
                    minAngleEmitter=1, maxAngleEmitter=1,
                    minAngle=2, maxAngle=2, minAngleVel=3, maxAngleVel=3
                })
                particles.native_addParticlesWithMode({
                    definitionName="exact", x=0, y=0, w=0, h=0,
                    angle=0, mode=4,
                    ignoreDeltaTimeMultiplier=false
                })
                "#,
        )
        .unwrap();

    let bridge = runtime.render.lock().unwrap();
    let first = &bridge.particle_system.particles[0];
    let (velocity_sine, velocity_cosine) = 1.25_f32.sin_cos();
    assert_eq!(
        first.velocity_x.to_bits(),
        (2.0 * velocity_cosine).to_bits()
    );
    assert_eq!(first.velocity_y.to_bits(), (2.0 * velocity_sine).to_bits());
    assert_eq!(first.angle, 2.25);
    assert_eq!(first.angular_velocity, 3.0);
    assert!(first.ignore_time_multiplier);
    assert!(!bridge.particle_system.particles[1].ignore_time_multiplier);
}

#[test]
fn lifetime_particle_animation_uses_first_sprite_and_one_based_ceil_frames() {
    let runtime = unlocked_test_runtime();
    runtime
        .execute_source(
            r#"
                particleTable = {
                    particles = {
                        animated = {
                            amount = 1,
                            sprites = { "FRAME_1", "FRAME_2", "FRAME_3", "FRAME_4" },
                            animation = "lifeTime",
                            lifeTime = 1,
                            gravityX = 0, gravityY = 0,
                            minVel = 0, maxVel = 0,
                            minAngleEmitter = 0, maxAngleEmitter = 0,
                            minAngle = 0, maxAngle = 0,
                            minAngleVel = 0, maxAngleVel = 0,
                            minScaleBegin = 1, maxScaleBegin = 1,
                            minScaleEnd = 0, maxScaleEnd = 0
                        }
                    }
                }
                particles.native_addParticlesWithMode({
                    definitionName = "animated", amount = 1,
                    x = 0, y = 0, w = 0, h = 0, angle = 0, mode = 3
                })
                setMenuParticlesScale(2.123456789)
                drawMenuParticlesNative()
                update = function() end
                "#,
        )
        .unwrap();

    {
        let bridge = runtime.render.lock().unwrap();
        let particle = &bridge.particle_system.particles[0];
        // The animated branch bypasses the ninth, sprite-selection RNG
        // call and directly installs sprites[0].
        assert_eq!(bridge.particle_random.index, 7);
        assert_eq!(particle.sprite, "FRAME_1");
        assert_eq!(particle.animation_frame, 0);
        assert_eq!(bridge.commands.len(), 1);
        assert_eq!(bridge.commands[0].state.scale_x, f64::from(2.123_456_7_f32));
    }

    runtime.update(0.26).unwrap();
    {
        let bridge = runtime.render.lock().unwrap();
        assert_eq!(bridge.particle_system.particles[0].sprite, "FRAME_2");
        assert_eq!(bridge.particle_system.particles[0].animation_frame, 2);
        assert_eq!(bridge.particle_system.particles[0].current_scale, 0.74_f32);
    }
    runtime.update(0.25).unwrap();
    assert_eq!(
        runtime.render.lock().unwrap().particle_system.particles[0].sprite,
        "FRAME_3"
    );
    runtime.update(0.49).unwrap();
    assert_eq!(
        runtime.render.lock().unwrap().particle_system.particles[0].sprite,
        "FRAME_4"
    );
    runtime.update(0.01).unwrap();
    assert!(
        runtime
            .render
            .lock()
            .unwrap()
            .particle_system
            .particles
            .is_empty()
    );
}

#[test]
fn menu_particle_scale_preserves_generated_number_adapter_and_float32_store() {
    let runtime = unlocked_test_runtime();
    runtime
        .execute_source(
            r#"
                missing_ok = pcall(setMenuParticlesScale)
                string_ok = pcall(setMenuParticlesScale, "2.5")
                boolean_ok = pcall(setMenuParticlesScale, true)
                extra_ok, extra_error = pcall(setMenuParticlesScale, 2.123456789, "ignored")
            "#,
        )
        .unwrap();

    let environment = game_environment(runtime.lua()).unwrap();
    assert!(!environment.get::<bool>("missing_ok").unwrap());
    assert!(!environment.get::<bool>("string_ok").unwrap());
    assert!(!environment.get::<bool>("boolean_ok").unwrap());
    assert!(
        environment.get::<bool>("extra_ok").unwrap(),
        "{:?}",
        environment.get::<Value>("extra_error").unwrap()
    );
    assert_eq!(
        runtime.render.lock().unwrap().particle_system.scale,
        2.123_456_7_f32
    );
}

#[test]
fn native_particle_random_matches_fixed_xorshift_cmwc_sequence() {
    let mut random = NativeParticleRandom::default();
    for expected in [
        0x6d16_d313_u32,
        0xd8b1_ba7b,
        0x9f03_4706,
        0x4707_2697,
        0xce91_0c99,
        0x23ee_e0dd,
        0x5b21_1f60,
        0x8926_a92d,
        0x1b20_3314,
        0x2c0f_c956,
    ] {
        let generated = (random.next() * 4_294_967_296.0) as u32;
        assert_eq!(generated, expected);
    }
    assert_eq!(random.index, 9);
}
