use super::*;

#[test]
fn dirt_point_query_dispatches_native_shape_test_point_virtuals() {
    let runtime = unlocked_test_runtime();
    runtime
        .execute_source(
            r#"
                createCircle("circle", "", 0, 0, 1, 1, 0, 0, true, false, 1)
                circle_extension = createNativeBlockExtension("dirt", "circle")
                circle_inside = circle_extension.isJointAttached(1, 0)
                circle_outside = circle_extension.isJointAttached(1.001, 0)
                circle_nan = circle_extension.isJointAttached(0 / 0, 0)

                createBox("box", "", 5, -2, 4, 2, 1, 0, 0, true, false, 1)
                setAngle("box", math.pi / 2)
                box_extension = createNativeBlockExtension("dirt", "box")
                box_inside = box_extension.isJointAttached(0, 0)
                box_boundary = box_extension.isJointAttached(-1, 2)
                box_outside = box_extension.isJointAttached(-1.001, 2)
                box_nan = box_extension.isJointAttached(0 / 0, 0)

                clearVertices()
                addVertex(-1, 0)
                addVertex(1, 0)
                createLineShape("line", "", 0, 0, 2, 0, 0, 0, 0, true, false, 1)
                line_extension = createNativeBlockExtension("dirt", "line")
                line_point = line_extension.isJointAttached(0, 0)
                line_nan = line_extension.isJointAttached(0 / 0, 0)
                "#,
        )
        .unwrap();

    let environment = game_environment(runtime.lua()).unwrap();
    assert!(environment.get::<bool>("circle_inside").unwrap());
    assert!(!environment.get::<bool>("circle_outside").unwrap());
    assert!(environment.get::<bool>("circle_nan").unwrap());
    assert!(environment.get::<bool>("box_inside").unwrap());
    assert!(environment.get::<bool>("box_boundary").unwrap());
    assert!(!environment.get::<bool>("box_outside").unwrap());
    assert!(environment.get::<bool>("box_nan").unwrap());
    assert!(!environment.get::<bool>("line_point").unwrap());
    assert!(!environment.get::<bool>("line_nan").unwrap());
}

#[test]
fn dirt_draws_share_cached_native_meshes_until_the_next_cut() {
    let runtime = StellaLua::new("/tmp").unwrap();
    runtime
        .execute_source(
            r#"
                createBox("dirt", "DIRT", 0, 0, 10, 10, 0, 0, 0, true, false, 1)
                blocks = {
                    DIRT_DEF = {
                        components = {
                            dirt = {
                                bgTexture = "DIRT_BACKGROUND",
                                fgTexture = "DIRT_FOREGROUND"
                            }
                        }
                    }
                }
                objects.world.dirt.definition = "DIRT_DEF"
                dirt_extension = createNativeBlockExtension("dirt", "dirt")
                dirt_extension.render()
                drawGameNative()
                drawGameNative()
            "#,
        )
        .unwrap();

    let before_cut = {
        let bridge = runtime.render.lock().unwrap();
        let commands = bridge
            .commands
            .iter()
            .filter_map(|command| command.dirt.as_ref())
            .collect::<Vec<_>>();
        assert!(commands.len() >= 2);
        assert!(Arc::ptr_eq(
            commands[commands.len() - 2],
            commands[commands.len() - 1]
        ));
        Arc::clone(commands[commands.len() - 1])
    };

    runtime
        .execute_source(
            r#"
                dirt_extension.onCollision(0, 0, 0, 1, 2, "impact", 0, 0)
                dirt_extension.checkCollisions()
                drawGameNative()
            "#,
        )
        .unwrap();

    let bridge = runtime.render.lock().unwrap();
    let after_cut = bridge
        .commands
        .iter()
        .rev()
        .find_map(|command| command.dirt.as_ref())
        .unwrap();
    assert!(!Arc::ptr_eq(&before_cut, after_cut));
    assert_ne!(
        before_cut.foreground_triangles,
        after_cut.foreground_triangles
    );
    let cached = bridge.scene["dirt"].dirt.as_ref().unwrap().render_command();
    assert!(Arc::ptr_eq(&cached, after_cut));
}

#[test]
fn native_block_extension_queues_collision_holes_and_exposes_methods() {
    let runtime = StellaLua::new("/tmp").unwrap();
    runtime
        .execute_source(
            r#"
                createBox("dirt_body", "DIRT", 3, 4, 10, 10, 0, 0, 0, true, false, 1)
                blocks = {
                    DIRT_DEF = {
                        components = {
                            dirt = {
                                bgTexture = "DIRT_BACKGROUND",
                                fgTexture = "DIRT_FOREGROUND"
                            }
                        }
                    }
                }
                dirt_extension = createNativeBlockExtension("dirt", "dirt_body")
                dirt_extension.onCollision(3, 4, 0, 1, 2, "collider", 8, 9)
                -- The recovered object loader publishes this after component
                -- construction; collision/render must bind the textures lazily.
                objects.world.dirt_body.definition = "DIRT_DEF"
                dirt_collision_count = dirt_extension.checkCollisions()
                dirt_joint_attached = dirt_extension.isJointAttached(0, 0)
                dirt_outside_fixture = dirt_extension.isJointAttached(6, 0)
                dirt_extension.render()
                createBox("far_dirt_body", "DIRT", 16777216, 0, 2, 2, 0, 0, 0, true, false, 1)
                objects.world.far_dirt_body.definition = "DIRT_DEF"
                far_dirt_extension = createNativeBlockExtension("dirt", "far_dirt_body")
                -- 16777216f + 1f rounds back to 16777216f. A double-precision
                -- addition would put the query outside the float32 fixture.
                dirt_joint_f32_add = far_dirt_extension.isJointAttached(1, 0)
                far_dirt_extension.onCollision(16777217, 0, 0, 1, 0.25, "collider", 0, 0)
                far_dirt_extension.checkCollisions()
                drawGameNative()
                "#,
        )
        .unwrap();

    let environment = game_environment(runtime.lua()).unwrap();
    assert_eq!(environment.get::<i64>("dirt_collision_count").unwrap(), 1);
    // sub_100020D70 has rebuilt the fixture triangles around the cut, so
    // the centre point is no longer attached.
    assert!(!environment.get::<bool>("dirt_joint_attached").unwrap());
    assert!(!environment.get::<bool>("dirt_outside_fixture").unwrap());
    assert!(environment.get::<bool>("dirt_joint_f32_add").unwrap());
    let bridge = runtime.render.lock().unwrap();
    assert_eq!(bridge.scene["dirt_body"].dirt_holes.len(), 1);
    let hole = bridge.scene["dirt_body"].dirt_holes[0];
    assert_eq!((hole.local_x, hole.local_y, hole.radius), (0.0, 0.0, 2.0));
    let far_hole = bridge.scene["far_dirt_body"].dirt_holes[0];
    assert_eq!(far_hole.local_x, 0.0);
    // RayCast now walks the newly created triangle fixtures. Starting in
    // the empty cut and moving right enters solid dirt at radius 2.
    let hits = bridge.scene["dirt_body"].ray_cast_hits("dirt_body", (3.0, 4.0), (10.0, 4.0));
    assert!(!hits.is_empty());
    assert!(hits.iter().all(|hit| hit.point_x >= 4.999));
    assert!(hits.iter().any(|hit| (hit.point_x - 5.0).abs() < 0.01));
    let command = bridge
        .commands
        .iter()
        .find(|command| command.sprite == "DIRT")
        .unwrap();
    let dirt = command.dirt.as_ref().unwrap();
    assert_eq!(dirt.background_texture, "DIRT_BACKGROUND");
    assert_eq!(dirt.foreground_texture, "DIRT_FOREGROUND");
    let triangle_area = |groups: &[Vec<RenderTriangle>]| {
        groups
            .iter()
            .flatten()
            .map(|triangle| {
                let [a, b, c] = triangle.vertices;
                ((b[0] - a[0]) * (c[1] - a[1]) - (b[1] - a[1]) * (c[0] - a[0])).abs() * 0.5
            })
            .sum::<f64>()
    };
    let background_area = triangle_area(&dirt.background_triangles);
    let foreground_area = triangle_area(&dirt.foreground_triangles);
    assert!((background_area - 100.0).abs() < 1e-6);
    assert!(foreground_area > 80.0 && foreground_area < background_area - 10.0);
    let CollisionShape::Polygon { fixtures, .. } = &bridge.scene["dirt_body"].collision_shape
    else {
        panic!("dirt collision should be rebuilt as polygon fixtures");
    };
    assert_eq!(
        fixtures.len(),
        dirt.foreground_triangles
            .iter()
            .map(Vec::len)
            .sum::<usize>()
    );
}

#[test]
fn native_dirt_constructor_retains_both_resolved_image_pointers() {
    let unique = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let root = std::env::temp_dir().join(format!("stella-dirt-pointers-{unique}"));
    let data_root = root.join("data");
    for directory in ["first", "second"] {
        fs::create_dir_all(data_root.join(directory)).unwrap();
    }
    let entries = [("DIRT_BACKGROUND", 10, 20), ("DIRT_FOREGROUND", 30, 40)];
    fs::write(
        data_root.join("first/DIRT.dat"),
        test_textured_sprite_sheet_with_names("first.pvr", &entries),
    )
    .unwrap();
    fs::write(data_root.join("first/first.pvr"), []).unwrap();
    fs::write(
        data_root.join("second/DIRT.dat"),
        test_textured_sprite_sheet_with_names("second.pvr", &entries),
    )
    .unwrap();
    fs::write(data_root.join("second/second.pvr"), []).unwrap();

    let runtime = StellaLua::new(&data_root).unwrap();
    runtime
        .execute_source(
            r#"
                res.createSpriteSheet("first/DIRT.dat")
                createBox("dirt", "DIRT", 0, 0, 10, 10, 0, 0.2, 0, true, false, 1)
                blocks = {
                    DIRT_DEF = {
                        components = {
                            dirt = {
                                bgTexture = "DIRT_BACKGROUND",
                                fgTexture = "DIRT_FOREGROUND"
                            }
                        }
                    }
                }
                objects.world.dirt.definition = "DIRT_DEF"
                dirt_extension = createNativeBlockExtension("dirt", "dirt")
                res.createSpriteSheet("second/DIRT.dat")
                res.releaseSpriteSheet("second/DIRT.dat", false)
                res.releaseSpriteSheet("first/DIRT.dat", false)
                drawGameNative()
                "#,
        )
        .unwrap();

    assert!(
        !runtime
            .sprite_catalog_snapshot_since(0)
            .unwrap()
            .regions
            .contains_key("DIRT_BACKGROUND")
    );
    let bridge = runtime.render.lock().unwrap();
    let dirt = bridge
        .commands
        .iter()
        .find_map(|command| command.dirt.as_ref())
        .unwrap();
    for binding in [
        &dirt.background_texture_binding,
        &dirt.foreground_texture_binding,
    ] {
        match binding {
            MaskedTextureBinding::Source(source) => {
                assert!(source.ends_with("first/first.pvr"));
            }
            MaskedTextureBinding::Missing => panic!("Dirt image resolved during construction"),
        }
    }
    drop(bridge);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn native_dirt_cut_destroys_contacts_and_rebuilds_mass_and_proxies() {
    let runtime = StellaLua::new("/tmp").unwrap();
    runtime
        .execute_source(
            r#"
                createBox("dirt", "DIRT", 0, 0, 10, 10, 1, 0.4, 0.3, true, false, 1)
                createBox("overlap", "", 4, 0, 2, 2, 0, 0.2, 0, true, false, 1)
                blocks = {
                    DIRT_DEF = {
                        components = {
                            dirt = {
                                bgTexture = "DIRT_BACKGROUND",
                                fgTexture = "DIRT_FOREGROUND"
                            }
                        }
                    }
                }
                blockTable = {
                    materials = {
                        dirtMaterial = { density = 2, friction = 0.6, restitution = 0.7 }
                    }
                }
                objects.world.dirt.definition = "DIRT_DEF"
                objects.world.dirt.material = "dirtMaterial"
                dirt_extension = createNativeBlockExtension("dirt", "dirt")
                dirt_exit_count = 0
                exitCollision = function(first, second)
                    dirt_exit_count = dirt_exit_count + 1
                    dirt_exit_first = first
                    dirt_exit_second = second
                end
                "#,
        )
        .unwrap();
    {
        let mut bridge = runtime.render.lock().unwrap();
        assert!(bridge.refresh_contacts().iter().any(|event| event.began));
        assert_eq!(bridge.active_contacts.len(), 1);
        assert_eq!(bridge.scene["dirt"].body_mass, 100.0);
        assert_eq!(bridge.scene["dirt"].fixture_proxy_ids.len(), 1);
    }

    runtime
        .execute_source(
            r#"
                -- The constructor has already copied the material values.
                -- Neither table edits nor setters change its stored fixture def.
                blockTable.materials.dirtMaterial.density = 9
                blockTable.materials.dirtMaterial.friction = 0.1
                blockTable.materials.dirtMaterial.restitution = 0.2
                native_setDensity("dirt", 5)
                setFriction("dirt", 0.15)
                setRestitution("dirt", 0.25)
                dirt_extension.onCollision(0, 0, 0, 1, 2, "overlap", 0, 0)
                dirt_extension.checkCollisions()
                "#,
        )
        .unwrap();

    let environment = game_environment(runtime.lua()).unwrap();
    assert_eq!(environment.get::<i64>("dirt_exit_count").unwrap(), 1);
    assert_eq!(
        environment.get::<String>("dirt_exit_first").unwrap(),
        "dirt"
    );
    assert_eq!(
        environment.get::<String>("dirt_exit_second").unwrap(),
        "overlap"
    );
    let bridge = runtime.render.lock().unwrap();
    assert!(bridge.active_contacts.is_empty());
    let dirt = &bridge.scene["dirt"];
    let CollisionShape::Polygon { fixtures, .. } = &dirt.collision_shape else {
        panic!("dirt cut did not replace the box fixture");
    };
    assert!(fixtures.len() > 1);
    assert_eq!(
        dirt.fixture_densities,
        vec![f64::from(2.0_f32); fixtures.len()]
    );
    assert_eq!(
        dirt.fixture_frictions,
        vec![f64::from(0.6_f32); fixtures.len()]
    );
    assert_eq!(
        dirt.fixture_restitutions,
        vec![f64::from(0.7_f32); fixtures.len()]
    );
    assert_eq!(dirt.fixture_proxy_ids.len(), fixtures.len());
    assert!(dirt.fixture_proxy_ids.iter().all(Option::is_some));
    assert!(dirt.body_mass > 160.0 && dirt.body_mass < 180.0);
    assert!(!dirt.collision_contains_world_point((0.0, 0.0)));
    assert!(dirt.collision_contains_world_point((4.5, 4.5)));
}

#[test]
fn native_dirt_destroys_fixture_heads_before_newer_contacts_on_older_fixtures() {
    let runtime = StellaLua::new("/tmp").unwrap();
    runtime
        .execute_source(
            r#"
                createBox("dirt", "DIRT", 0, 0, 10, 10, 1, 0.4, 0.3, true, false, 1)
                blocks = {
                    DIRT_DEF = {
                        components = {
                            dirt = {
                                bgTexture = "DIRT_BACKGROUND",
                                fgTexture = "DIRT_FOREGROUND"
                            }
                        }
                    }
                }
                objects.world.dirt.definition = "DIRT_DEF"
                dirt_extension = createNativeBlockExtension("dirt", "dirt")
                dirt_exit_order = {}
                exitCollision = function(first, second)
                    table.insert(dirt_exit_order, second)
                end
                "#,
        )
        .unwrap();

    {
        let mut bridge = runtime.render.lock().unwrap();
        bridge.remove_object_broad_phase_proxy_state("dirt");
        let dirt = bridge.scene.get_mut("dirt").unwrap();
        let old_center = dirt.world_center();
        dirt.collision_shape = CollisionShape::Polygon {
            vertices: vec![(-5.0, -5.0), (5.0, -5.0), (5.0, 5.0), (-5.0, 5.0)],
            fixtures: vec![
                vec![(-5.0, -5.0), (0.0, -5.0), (0.0, 5.0), (-5.0, 5.0)],
                vec![(0.0, -5.0), (5.0, -5.0), (5.0, 5.0), (0.0, 5.0)],
            ],
        };
        dirt.fixture_densities = vec![1.0, 1.0];
        dirt.fixture_frictions = vec![0.4, 0.4];
        dirt.fixture_restitutions = vec![0.3, 0.3];
        dirt.fixture_proxy_ids = vec![None, None];
        dirt.reset_native_mass_data(old_center);
        bridge.install_object_broad_phase_proxies("dirt", false);

        // The older fixture owns the globally newer contact. A batch
        // destruction sorted only by contact creation would report tail
        // first; b2Body::DestroyFixture must report the list-head fixture
        // first and only then advance to its saved m_next.
        let head = ("dirt".to_owned(), "z_head".to_owned(), 1, 0);
        let tail = ("dirt".to_owned(), "z_tail".to_owned(), 0, 0);
        bridge.active_contacts.insert(head.clone(), false);
        bridge.active_contacts.insert(tail.clone(), false);
        bridge.broad_phase_contacts.insert(head.clone());
        bridge.broad_phase_contacts.insert(tail.clone());
        bridge.insert_native_contact_order(head, 10);
        bridge.insert_native_contact_order(tail, 100);
    }

    runtime
        .execute_source(
            r#"
                dirt_extension.onCollision(0, 0, 0, 1, 2, "impact", 0, 0)
                dirt_extension.checkCollisions()
                "#,
        )
        .unwrap();
    let order = game_environment(runtime.lua())
        .unwrap()
        .get::<mlua::Table>("dirt_exit_order")
        .unwrap()
        .sequence_values::<String>()
        .collect::<LuaResult<Vec<_>>>()
        .unwrap();
    assert_eq!(order, ["z_head", "z_tail"]);
}

#[test]
fn native_dirt_preserves_source_float_contour_until_first_clipper_cut() {
    let runtime = StellaLua::new("/tmp").unwrap();
    runtime
        .execute_source(
            r#"
                clearVertices()
                addVertex(0.0009, 0.0007)
                addVertex(4.3456, 0.0008)
                addVertex(0.0006, 4.5678)
                createPolygon("dirt", "DIRT", 0, 0, 5, 5, 1, 0.4, 0.3, true, false, 1)
                blocks = {
                    DIRT_DEF = {
                        components = {
                            dirt = {
                                bgTexture = "DIRT_BACKGROUND",
                                fgTexture = "DIRT_FOREGROUND"
                            }
                        }
                    }
                }
                objects.world.dirt.definition = "DIRT_DEF"
                dirt_extension = createNativeBlockExtension("dirt", "dirt")
                "#,
        )
        .unwrap();

    let source = [
        (f64::from(0.0009_f32), f64::from(0.0007_f32)),
        (f64::from(4.3456_f32), f64::from(0.0008_f32)),
        (f64::from(0.0006_f32), f64::from(4.5678_f32)),
    ];
    {
        let bridge = runtime.render.lock().unwrap();
        let dirt = bridge.scene["dirt"].dirt.as_ref().unwrap();
        assert_eq!(dirt.background_paths, [source.to_vec()]);
        assert_eq!(dirt.foreground_paths, [source.to_vec()]);
        assert!((dirt.foreground_paths[0][0].0 * 1000.0 - 0.9).abs() < 1e-4);
    }

    runtime
        .execute_source(
            r#"
                -- A non-intersecting cut still routes the source contour
                -- through Clipper's float32 * 1000 -> int32 conversion.
                dirt_extension.onCollision(100, 100, 0, 1, 1, "impact", 0, 0)
                dirt_extension.checkCollisions()
                "#,
        )
        .unwrap();
    let bridge = runtime.render.lock().unwrap();
    let dirt = bridge.scene["dirt"].dirt.as_ref().unwrap();
    assert_eq!(dirt.background_paths, [source.to_vec()]);
    assert_ne!(dirt.foreground_paths, [source.to_vec()]);
    assert!(dirt.foreground_paths.iter().flatten().all(|&(x, y)| {
        x == native_dirt_output_coord(native_dirt_input_integer(x))
            && y == native_dirt_output_coord(native_dirt_input_integer(y))
    }));
}

#[test]
fn native_dirt_factory_uses_two_strict_slots_and_registered_tag() {
    let runtime = StellaLua::new("/tmp").unwrap();
    runtime
        .execute_source(
            r#"
                createBox("dirt", "DIRT", 0, 0, 10, 10, 0, 0.4, 0.3, true, false, 1)
                dirt_factory_bad_tag_type = pcall(function()
                    createNativeBlockExtension(nil, "dirt")
                end)
                dirt_factory_bad_name_type = pcall(function()
                    createNativeBlockExtension("dirt", nil)
                end)
                dirt_factory_unknown = createNativeBlockExtension("unknown", "dirt")
                dirt_factory_valid = createNativeBlockExtension("dirt", "dirt")
                "#,
        )
        .unwrap();
    let environment = game_environment(runtime.lua()).unwrap();
    assert!(
        !environment
            .get::<bool>("dirt_factory_bad_tag_type")
            .unwrap()
    );
    assert!(
        !environment
            .get::<bool>("dirt_factory_bad_name_type")
            .unwrap()
    );
    assert!(matches!(
        environment.get::<Value>("dirt_factory_unknown").unwrap(),
        Value::Nil
    ));
    assert!(matches!(
        environment.get::<Value>("dirt_factory_valid").unwrap(),
        Value::Table(_)
    ));
}

#[test]
fn native_dirt_collision_adapter_is_strict_and_writes_delayed_velocity() {
    let runtime = StellaLua::new("/tmp").unwrap();
    runtime
        .execute_source(
            r#"
                createBox("dirt", "DIRT", 0, 0, 10, 10, 0, 0.4, 0.3, true, false, 1)
                createBox("collider", "", 20, 0, 2, 2, 1, 0.2, 0, true, false, 1)
                blocks = {
                    DIRT_DEF = {
                        components = {
                            dirt = {
                                bgTexture = "DIRT_BACKGROUND",
                                fgTexture = "DIRT_FOREGROUND"
                            }
                        }
                    }
                }
                objects.world.dirt.definition = "DIRT_DEF"
                dirt_extension = createNativeBlockExtension("dirt", "dirt")
                dirt_bad_collision_ok = pcall(function()
                    -- A filter-map parser would incorrectly shift 77 and 88
                    -- into the missing y/normal slots and queue this record.
                    dirt_extension.onCollision(0, nil, 0, 1, 2, "collider", 77, 88)
                end)
                dirt_bad_joint_ok = pcall(function()
                    dirt_extension.isJointAttached(nil, 0)
                end)
                dirt_colon_collision_ok = pcall(function()
                    dirt_extension:onCollision(0, 0, 0, 1, 2,
                        "collider", 77, 88)
                end)
                dirt_colon_joint_ok = pcall(function()
                    dirt_extension:isJointAttached(0, 0)
                end)
                "#,
        )
        .unwrap();
    let environment = game_environment(runtime.lua()).unwrap();
    assert!(!environment.get::<bool>("dirt_bad_collision_ok").unwrap());
    assert!(!environment.get::<bool>("dirt_bad_joint_ok").unwrap());
    assert!(!environment.get::<bool>("dirt_colon_collision_ok").unwrap());
    assert!(!environment.get::<bool>("dirt_colon_joint_ok").unwrap());
    assert!(
        runtime
            .render
            .lock()
            .unwrap()
            .collision_velocities
            .is_empty()
    );

    runtime
        .execute_source(
            r#"
                dirt_extension.onCollision(100, 100, 0, 1, 1,
                    "collider", 7.1, -3.1)
                dirt_extension.onCollision(100, 100, 0, 1, 1,
                    "missing", 90, 91)
                "#,
        )
        .unwrap();
    let bridge = runtime.render.lock().unwrap();
    assert_eq!(
        bridge.collision_velocities.get("collider"),
        Some(&(f64::from(7.1_f32), f64::from(-3.1_f32)))
    );
    assert!(!bridge.collision_velocities.contains_key("missing"));
    drop(bridge);

    let mut bridge = runtime.render.lock().unwrap();
    bridge.collision_velocities.clear();
    let smallest_subnormal = f32::from_bits(1);
    bridge
        .collision_velocities
        .insert("collider".to_owned(), (f64::from(smallest_subnormal), 0.0));
    {
        let collider = bridge.scene.get_mut("collider").unwrap();
        collider.sleeping = true;
        collider.velocity_x = 0.0;
        collider.velocity_y = 0.0;
    }
    bridge.apply_pending_collision_velocities();
    let collider = &bridge.scene["collider"];
    // Packed float32 square + pairwise add underflows to zero, so
    // b2Body::SetLinearVelocity stores the vector without waking.
    assert!(collider.sleeping);
    assert_eq!(collider.velocity_x, f64::from(smallest_subnormal));
}

#[test]
fn native_dirt_cut_uses_recovered_float_octagon_and_clipper_grid() {
    let vertices = native_dirt_octagon(DirtHole {
        local_x: 0.0,
        local_y: 0.0,
        radius: 2.0,
    });
    assert_eq!(vertices[0], NativeClipperPoint { x: 2000, y: 0 });
    assert_eq!(vertices[1], NativeClipperPoint { x: 1414, y: 1413 });
    assert_eq!(vertices[2], NativeClipperPoint { x: 1, y: 1999 });
    let negative = native_dirt_octagon(DirtHole {
        local_x: 0.0,
        local_y: 0.0,
        radius: -2.0,
    });
    assert_eq!(negative[0], NativeClipperPoint { x: -2000, y: 0 });
    let clipped = native_dirt_difference(
        &[vec![(-5.0, -5.0), (5.0, -5.0), (5.0, 5.0), (-5.0, 5.0)]],
        DirtHole {
            local_x: 0.0,
            local_y: 0.0,
            radius: 2.0,
        },
    );
    let point = |x, y| (native_dirt_output_coord(x), native_dirt_output_coord(y));
    assert_eq!(
        clipped,
        vec![
            vec![
                point(-1, -5000),
                point(-4, -1999),
                point(-1417, -1411),
                point(-1999, 3),
                point(-1412, 1415),
                point(-1, 1998),
                point(-1, 5000),
                point(-5000, 5000),
                point(-5000, -5000),
            ],
            vec![
                point(5000, 5000),
                point(0, 5000),
                point(1, 1999),
                point(1414, 1413),
                point(2000, 0),
                point(1410, -1418),
                point(0, -1997),
                point(0, -5000),
                point(5000, -5000),
            ],
        ],
    );
}

#[test]
fn drawable_polygon_uses_native_reversal_ear_diagonal_and_triangle_order() {
    let triangles =
        triangulate_dirt_paths(&[vec![(-5.0, -5.0), (5.0, -5.0), (5.0, 5.0), (-5.0, 5.0)]]);
    assert_eq!(triangles.len(), 1);
    assert_eq!(
        triangles[0],
        vec![
            RenderTriangle {
                vertices: [[-5.0, 5.0], [-5.0, -5.0], [5.0, 5.0]],
            },
            RenderTriangle {
                vertices: [[5.0, -5.0], [5.0, 5.0], [-5.0, -5.0]],
            },
        ]
    );
}
