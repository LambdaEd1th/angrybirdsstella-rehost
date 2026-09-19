use super::super::*;
use super::configure_theme_camera_fixture;

fn refresh_theme_system(runtime: &StellaLua) {
    runtime
        .execute_source(
            r#"
                objects = objects or {}
                objects.castleCameraData = {
                    ipad = { sx = 20 },
                    ios = { px = 0, py = 0 }
                }
                native_refreshThemeSystem()
            "#,
        )
        .unwrap();
}

fn install_particle_theme(runtime: &StellaLua) {
    register_test_sprite_sheet(runtime, &["THEME_LAYER", "THEME_PARTICLE"]);
    runtime
        .execute_source(
            r#"
                particleTable = { particles = { themeMist = {
                    amount = 1,
                    sprites = { "THEME_PARTICLE" },
                    lifeTime = 2,
                    gravityX = 0, gravityY = 8,
                    minVel = 0, maxVel = 0,
                    minAngleEmitter = 0, maxAngleEmitter = 0,
                    minAngle = 0, maxAngle = 0,
                    minAngleVel = 0, maxAngleVel = 0,
                    minScaleBegin = 1, maxScaleBegin = 1,
                    minScaleEnd = 3, maxScaleEnd = 3
                } } }
                blockTable = { themes = { particle_theme = {
                    bgLayers = {{
                        sprite = "THEME_LAYER",
                        particles = "themeMist",
                        spawnInterval = 0.5,
                        zDistance = 0.25
                    }},
                    fgLayers = {}
                } } }
                setWorldScale(20)
                setMaxWorldScale(20)
                setTheme("particle_theme")
                update = function() end
            "#,
        )
        .unwrap();
    refresh_theme_system(runtime);
}

#[test]
fn theme_particle_spawner_updates_parallax_fields_and_draws_before_layer() {
    let runtime = unlocked_test_runtime();
    configure_theme_camera_fixture(&runtime);
    install_particle_theme(&runtime);
    {
        let bridge = runtime.render.lock().unwrap();
        assert_eq!(bridge.theme_background_particles.spawners.len(), 1);
        assert!(bridge.theme_background_particles.particles.is_empty());
    }

    runtime.update(0.25).unwrap();
    {
        let bridge = runtime.render.lock().unwrap();
        let particles = &bridge.theme_background_particles.particles[&1];
        assert_eq!(particles.len(), 1);
        let particle = &particles[0];
        assert_eq!(particle.elapsed, 0.25);
        assert_eq!(particle.velocity_y, 1.5);
        assert_eq!(particle.y, 0.28125);
        assert_eq!(particle.current_scale, 1.1875);
        assert_eq!(particle.theme_layer_index, 1);
        assert_eq!(particle.mode, 2);
        assert_eq!(bridge.theme_background_particles.spawners[&1].timer, 0.5);
    }

    runtime.execute_source("drawBackgroundNative(-1)").unwrap();
    let bridge = runtime.render.lock().unwrap();
    let sprites = bridge
        .commands
        .iter()
        .map(|command| command.sprite.as_str())
        .collect::<Vec<_>>();
    assert_eq!(sprites, ["THEME_PARTICLE", "THEME_LAYER"]);
    let particle = &bridge.commands[0];
    assert!(!particle.world_space);
    let projected_x = (particle.state.translate_x + particle.x) * particle.state.scale_x;
    let projected_y = (particle.state.translate_y + particle.y) * particle.state.scale_y;
    // Draw's lazy prelude captures endScale=20; 1x1/pivot0 contributes a
    // half pixel. The particle's 0.28125 local Y contributes 5.625 pixels.
    assert_eq!(projected_x, 128.5_f32);
    assert_eq!(projected_y, 102.125_01_f32);
    assert_eq!(particle.state.scale_x, 23.75);
    assert_eq!(particle.state.scale_y, 23.75);
}

#[test]
fn theme_particles_inherit_then_restore_the_callers_non_transform_state() {
    let runtime = unlocked_test_runtime();
    configure_theme_camera_fixture(&runtime);
    install_particle_theme(&runtime);
    runtime.update(0.25).unwrap();
    runtime
        .execute_source(
            r#"
                setRenderState(11, 12, 2, 3, 0.4, 5, 6, 0.25)
                res.setClipRect(10, 20, 30, 40)
                drawBackgroundNative(-1)
            "#,
        )
        .unwrap();

    let bridge = runtime.render.lock().unwrap();
    let particle = &bridge.commands[0];
    assert_eq!(particle.sprite, "THEME_PARTICLE");
    assert!(!particle.world_space);
    assert_eq!((particle.state.pivot_x, particle.state.pivot_y), (5.0, 6.0));
    assert_eq!(particle.state.alpha, 0.25);
    assert_eq!(particle.state.clip_rect, Some([10, 20, 40, 60]));
    // sub_100096E9C restores the copied 0x9C record after its bucket walk.
    assert_eq!(
        (bridge.state.translate_x, bridge.state.translate_y),
        (11.0, 12.0)
    );
    assert_eq!((bridge.state.scale_x, bridge.state.scale_y), (2.0, 3.0));
    assert_eq!((bridge.state.pivot_x, bridge.state.pivot_y), (5.0, 6.0));
    assert_eq!(bridge.state.angle, f64::from(0.4_f32));
    assert_eq!(bridge.state.alpha, 0.25);
    assert_eq!(bridge.state.clip_rect, Some([10, 20, 40, 60]));
}

#[test]
fn theme_particle_creation_retains_its_atlas_after_theme_resources_are_released() {
    let unique = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let root = std::env::temp_dir().join(format!("stella-theme-particle-pointer-{unique}"));
    let data_root = root.join("data");
    for directory in ["first", "second"] {
        fs::create_dir_all(data_root.join(directory)).unwrap();
    }
    fs::write(
        data_root.join("first/FIRST.dat"),
        test_textured_sprite_sheet_with_names(
            "first.pvr",
            &[("THEME_LAYER", 8, 8), ("THEME_PARTICLE", 10, 20)],
        ),
    )
    .unwrap();
    fs::write(data_root.join("first/first.pvr"), []).unwrap();
    fs::write(
        data_root.join("second/SECOND.dat"),
        test_textured_sprite_sheet("THEME_PARTICLE", "second.pvr", 30, 40),
    )
    .unwrap();
    fs::write(data_root.join("second/second.pvr"), []).unwrap();

    let runtime = StellaLua::new(&data_root).unwrap();
    configure_theme_camera_fixture(&runtime);
    runtime
        .execute_source(
            r#"
                setPhysicsEnabled(true)
                res.createSpriteSheet("first/FIRST.dat")
                particleTable = { particles = { mist = {
                    amount=1, sprites={"THEME_PARTICLE"}, lifeTime=2,
                    gravityX=0, gravityY=0, minVel=0, maxVel=0,
                    minAngleEmitter=0, maxAngleEmitter=0,
                    minAngle=0, maxAngle=0, minAngleVel=0, maxAngleVel=0,
                    minScaleBegin=1, maxScaleBegin=1,
                    minScaleEnd=1, maxScaleEnd=1
                } } }
                blockTable = { themes = { retained = { bgLayers = {{
                    sprite="THEME_LAYER", particles="mist", spawnInterval=1
                }}, fgLayers={} } } }
                objects = { castleCameraData = {
                    ipad={sx=20}, ios={px=0, py=0}
                } }
                setWorldScale(20)
                setMaxWorldScale(20)
                setTheme("retained")
                native_refreshThemeSystem()
                update = function() end
            "#,
        )
        .unwrap();
    runtime.update(0.1).unwrap();
    runtime
        .execute_source(
            r#"
                res.createSpriteSheet("second/SECOND.dat")
                res.releaseSpriteSheet("first/FIRST.dat", false)
                drawBackgroundNative(-1)
            "#,
        )
        .unwrap();

    assert_eq!(
        runtime.sprite_catalog_snapshot_since(0).unwrap().regions["THEME_PARTICLE"]
            .sprite
            .width,
        30
    );
    let bridge = runtime.render.lock().unwrap();
    assert_eq!(bridge.commands.len(), 1);
    assert_eq!(bridge.commands[0].sprite, "THEME_PARTICLE");
    let retained = bridge.commands[0].bound_region.as_ref().unwrap();
    assert_eq!(retained.sprite.width, 10);
    assert!(retained.texture_source.ends_with("first/first.pvr"));
    drop(bridge);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn theme_particle_preroll_and_interval_match_single_spawn_per_update() {
    let runtime = unlocked_test_runtime();
    configure_theme_camera_fixture(&runtime);
    install_particle_theme(&runtime);

    // The first call emits immediately. Even a delta larger than several
    // intervals creates one burst and resets the timer to the authored value.
    runtime
        .execute_source("updateThemeParticlesNative(1.25)")
        .unwrap();
    {
        let bridge = runtime.render.lock().unwrap();
        let particles = &bridge.theme_background_particles.particles[&1];
        assert_eq!(particles.len(), 1);
        assert_eq!(particles[0].elapsed, 1.25);
        assert_eq!(bridge.theme_background_particles.spawners[&1].timer, 0.5);
        // Pre-roll does not advance the containing theme layer.
        assert_eq!(bridge.theme_background_layers[0].animation_timer, 0.0);
        assert_eq!(bridge.theme_background_layers[0].offset_x, 0.0);
    }

    runtime
        .execute_source("updateThemeParticlesNative(0.25)")
        .unwrap();
    assert_eq!(
        runtime
            .render
            .lock()
            .unwrap()
            .theme_background_particles
            .particles[&1]
            .len(),
        1
    );
    runtime
        .execute_source("updateThemeParticlesNative(0.25)")
        .unwrap();
    let bridge = runtime.render.lock().unwrap();
    assert_eq!(bridge.theme_background_particles.particles[&1].len(), 2);
}

#[test]
fn theme_particle_nan_interval_follows_native_unordered_branches() {
    let runtime = unlocked_test_runtime();
    configure_theme_camera_fixture(&runtime);
    runtime
        .execute_source(
            r#"
                particleTable = { particles = { mist = {
                    amount=1, sprites={"NAN_INTERVAL"}, lifeTime=10,
                    gravityX=0, gravityY=0, minVel=0, maxVel=0,
                    minAngleEmitter=0, maxAngleEmitter=0,
                    minAngle=0, maxAngle=0, minAngleVel=0, maxAngleVel=0,
                    minScaleBegin=1, maxScaleBegin=1,
                    minScaleEnd=1, maxScaleEnd=1
                } } }
                blockTable = { themes = { nanTheme = { fgLayers = {}, bgLayers = {{
                    sprite="LAYER", particles="mist", spawnInterval=0/0
                }} } } }
                setTheme("nanTheme")
            "#,
        )
        .unwrap();
    refresh_theme_system(&runtime);

    runtime
        .execute_source(
            r#"
                updateThemeParticlesNative(0.25)
                updateThemeParticlesNative(0.25)
            "#,
        )
        .unwrap();

    let bridge = runtime.render.lock().unwrap();
    let spawner = &bridge.theme_background_particles.spawners[&1];
    assert!(spawner.interval.is_nan());
    assert!(spawner.timer.is_nan());
    assert_eq!(bridge.theme_background_particles.particles[&1].len(), 2);
}

#[test]
fn theme_particle_limits_count_the_empty_native_base_vector() {
    let runtime = unlocked_test_runtime();
    configure_theme_camera_fixture(&runtime);
    runtime
        .execute_source(
            r#"
                particleTable = { particles = { mist = {
                    amount=30, sprites={"UNLIMITED_LAYER"}, lifeTime=10,
                    gravityX=0, gravityY=0, minVel=0, maxVel=0,
                    minAngleEmitter=0, maxAngleEmitter=0,
                    minAngle=0, maxAngle=0, minAngleVel=0, maxAngleVel=0,
                    minScaleBegin=1, maxScaleBegin=1,
                    minScaleEnd=1, maxScaleEnd=1
                } } }
                blockTable = { themes = { limitTheme = { fgLayers = {}, bgLayers = {{
                    sprite="LAYER", particles="mist", spawnInterval=0
                }} } } }
                setTheme("limitTheme")
            "#,
        )
        .unwrap();
    refresh_theme_system(&runtime);

    runtime
        .execute_source(
            r#"
                updateThemeParticlesNative(0.01)
                updateThemeParticlesNative(0.01)
                updateThemeParticlesNative(0.01)
            "#,
        )
        .unwrap();

    // The third native burst still sees Particles+0x40 size zero. Counting
    // the derived +0xB8 bucket would cross the soft limit and emit only 15.
    assert_eq!(
        runtime
            .render
            .lock()
            .unwrap()
            .theme_background_particles
            .particles[&1]
            .len(),
        90
    );
}

#[test]
fn expanded_theme_layers_replace_spawner_by_source_definition_index() {
    let runtime = unlocked_test_runtime();
    configure_theme_camera_fixture(&runtime);
    register_test_sprite_sheet(&runtime, &["EXPANDED"]);
    runtime
        .execute_source(
            r#"
                particleTable = { particles = { mist = {
                    amount=1, sprites={"P"}, lifeTime=1,
                    gravityX=0, gravityY=0, minVel=0, maxVel=0,
                    minAngleEmitter=0, maxAngleEmitter=0,
                    minAngle=0, maxAngle=0, minAngleVel=0, maxAngleVel=0,
                    minScaleBegin=1, maxScaleBegin=1,
                    minScaleEnd=1, maxScaleEnd=1
                } } }
                blockTable = { themes = { expanded = { fgLayers = {}, bgLayers = {{
                    sprite="EXPANDED", particles="mist", spawnInterval=-1,
                    spawnParameters={
                        amount=3,
                        area={ screenX=0, screenY=0, screenW=0, screenH=0 }
                    }
                }} } } }
                setTheme("expanded")
            "#,
        )
        .unwrap();
    refresh_theme_system(&runtime);

    let bridge = runtime.render.lock().unwrap();
    assert_eq!(bridge.theme_background_layers.len(), 3);
    assert!(
        bridge
            .theme_background_layers
            .iter()
            .all(|layer| layer.definition_index == 1)
    );
    assert_eq!(bridge.theme_background_particles.spawners.len(), 1);
    assert_eq!(
        bridge.theme_background_particles.spawners[&1].interval,
        -1.0
    );
}

#[test]
fn theme_system_force_spawn_routes_authored_ids_to_native_layer_buckets() {
    let runtime = unlocked_test_runtime();
    configure_theme_camera_fixture(&runtime);
    runtime
        .execute_source(
            r#"
                particleTable = { particles = {
                    bgMist = {
                        amount=1, sprites={"BG_FORCE"}, lifeTime=1,
                        gravityX=0, gravityY=0, minVel=0, maxVel=0,
                        minAngleEmitter=0, maxAngleEmitter=0,
                        minAngle=0, maxAngle=0, minAngleVel=0, maxAngleVel=0,
                        minScaleBegin=1, maxScaleBegin=1,
                        minScaleEnd=1, maxScaleEnd=1
                    },
                    fgMist = {
                        amount=2, sprites={"FG_FORCE"}, lifeTime=1,
                        gravityX=0, gravityY=0, minVel=0, maxVel=0,
                        minAngleEmitter=0, maxAngleEmitter=0,
                        minAngle=0, maxAngle=0, minAngleVel=0, maxAngleVel=0,
                        minScaleBegin=1, maxScaleBegin=1,
                        minScaleEnd=1, maxScaleEnd=1
                    }
                } }
                blockTable = { themes = { forced = {
                    bgLayers = {{
                        sprite="BG_LAYER", particles="bgMist",
                        spawnInterval=-1, spawnerId=41
                    }},
                    fgLayers = {{
                        sprite="FG_LAYER", particles="fgMist",
                        spawnInterval=-1, spawnerId=73
                    }}
                } } }
                setTheme("forced")
                objects = objects or {}
                objects.castleCameraData = {
                    ipad = { sx = 20 },
                    ios = { px = 0, py = 0 }
                }
                native_refreshThemeSystem()
                assert(type(themeSystem) == "table")
                assert(type(themeSystem.spawnBGLayerParticles) == "function")
                assert(type(themeSystem.spawnFGLayerParticles) == "function")
                themeSystem:spawnBGLayerParticles(999)
                themeSystem:spawnBGLayerParticles(41.9)
                themeSystem.spawnFGLayerParticles(73)
                assert(not pcall(themeSystem.spawnBGLayerParticles, "41"))
            "#,
        )
        .unwrap();

    let bridge = runtime.render.lock().unwrap();
    assert_eq!(bridge.theme_background_layers[0].spawner_id, 41);
    assert_eq!(bridge.theme_foreground_layers[0].spawner_id, 73);
    assert_eq!(bridge.theme_background_particles.particles[&1].len(), 1);
    assert_eq!(bridge.theme_foreground_particles.particles[&1].len(), 2);
    assert_eq!(
        bridge.theme_background_particles.particles[&1][0].sprite,
        "BG_FORCE"
    );
    assert!(
        bridge.theme_foreground_particles.particles[&1]
            .iter()
            .all(|particle| particle.sprite == "FG_FORCE")
    );
}

#[test]
fn theme_particle_systems_are_rebuilt_only_by_native_refresh() {
    let runtime = unlocked_test_runtime();
    configure_theme_camera_fixture(&runtime);
    runtime
        .execute_source(
            r#"
                particleTable = { particles = {
                    oldMist = {
                        amount=1, sprites={"OLD_PARTICLE"}, lifeTime=10,
                        gravityX=0, gravityY=0, minVel=0, maxVel=0,
                        minAngleEmitter=0, maxAngleEmitter=0,
                        minAngle=0, maxAngle=0, minAngleVel=0, maxAngleVel=0,
                        minScaleBegin=1, maxScaleBegin=1,
                        minScaleEnd=1, maxScaleEnd=1
                    },
                    newMist = {
                        amount=1, sprites={"NEW_PARTICLE"}, lifeTime=10,
                        gravityX=0, gravityY=0, minVel=0, maxVel=0,
                        minAngleEmitter=0, maxAngleEmitter=0,
                        minAngle=0, maxAngle=0, minAngleVel=0, maxAngleVel=0,
                        minScaleBegin=1, maxScaleBegin=1,
                        minScaleEnd=1, maxScaleEnd=1
                    }
                } }
                blockTable = { themes = {
                    oldTheme = { fgLayers = {}, bgLayers = {{
                        sprite="OLD_LAYER", particles="oldMist",
                        spawnInterval=0.5, spawnerId=11
                    }} },
                    newTheme = { fgLayers = {}, bgLayers = {{
                        sprite="NEW_LAYER", particles="newMist",
                        spawnInterval=2, spawnerId=22
                    }} }
                } }
                setTheme("oldTheme")
            "#,
        )
        .unwrap();

    // Theme selection replaces the layer vectors but leaves both native
    // ThemeParticleSystems untouched until the explicit refresh call.
    assert!(
        runtime
            .render
            .lock()
            .unwrap()
            .theme_background_particles
            .spawners
            .is_empty()
    );

    refresh_theme_system(&runtime);
    runtime
        .execute_source("themeSystem:spawnBGLayerParticles(11)")
        .unwrap();
    {
        let bridge = runtime.render.lock().unwrap();
        assert_eq!(bridge.theme_background_particles.particles[&1].len(), 1);
        assert_eq!(
            bridge.theme_background_particles.particles[&1][0].sprite,
            "OLD_PARTICLE"
        );
    }

    runtime.execute_source("setTheme('newTheme')").unwrap();
    {
        let bridge = runtime.render.lock().unwrap();
        assert_eq!(bridge.theme_background_layers[0].sprite, "NEW_LAYER");
        assert_eq!(bridge.theme_background_particles.particles[&1].len(), 1);
        assert_eq!(bridge.theme_background_particles.spawners[&1].interval, 0.5);
    }

    refresh_theme_system(&runtime);
    {
        let bridge = runtime.render.lock().unwrap();
        assert!(bridge.theme_background_particles.particles.is_empty());
        assert_eq!(bridge.theme_background_particles.spawners.len(), 1);
        assert_eq!(bridge.theme_background_particles.spawners[&1].interval, 2.0);
        assert_eq!(bridge.theme_background_particles.spawners[&1].timer, 0.0);
    }
    runtime
        .execute_source("themeSystem:spawnBGLayerParticles(22)")
        .unwrap();
    let bridge = runtime.render.lock().unwrap();
    assert_eq!(
        bridge.theme_background_particles.particles[&1][0].sprite,
        "NEW_PARTICLE"
    );
}

#[test]
fn theme_refresh_retains_each_native_particle_definition_cache() {
    let runtime = unlocked_test_runtime();
    configure_theme_camera_fixture(&runtime);
    runtime
        .execute_source(
            r#"
                particleTable = { particles = { mist = {
                    amount=1, sprites={"FIRST_PARTICLE"}, lifeTime=10,
                    gravityX=0, gravityY=0, minVel=0, maxVel=0,
                    minAngleEmitter=0, maxAngleEmitter=0,
                    minAngle=0, maxAngle=0, minAngleVel=0, maxAngleVel=0,
                    minScaleBegin=1, maxScaleBegin=1,
                    minScaleEnd=1, maxScaleEnd=1
                } } }
                blockTable = { themes = { cachedTheme = { fgLayers = {}, bgLayers = {{
                    sprite="LAYER", particles="mist",
                    spawnInterval=-1, spawnerId=17
                }} } } }
                objects = { castleCameraData = {
                    ipad={sx=20}, ios={px=0, py=0}
                } }
                setTheme("cachedTheme")
                native_refreshThemeSystem()
                themeSystem:spawnBGLayerParticles(17)

                particleTable.particles.mist.amount = 3
                particleTable.particles.mist.sprites = {"SECOND_PARTICLE"}
                native_refreshThemeSystem()
                themeSystem:spawnBGLayerParticles(17)
            "#,
        )
        .unwrap();

    let bridge = runtime.render.lock().unwrap();
    assert_eq!(bridge.theme_background_particles.definitions.len(), 1);
    let particles = &bridge.theme_background_particles.particles[&1];
    assert_eq!(particles.len(), 1);
    assert_eq!(particles[0].sprite, "FIRST_PARTICLE");
}
