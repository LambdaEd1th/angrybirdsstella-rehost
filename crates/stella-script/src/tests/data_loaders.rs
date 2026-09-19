use super::*;

#[test]
fn decompose_polygon_reads_argument_table_and_returns_native_convex_parts() {
    let runtime = StellaLua::new("/tmp").unwrap();
    runtime
        .execute_source(
            r#"
                clearVertices()
                addVertex(999, 999)
                decomposed = decomposePolygon({
                    { x = 0, y = 0 },
                    { x = 4, y = 0 },
                    { x = 4, y = 4 },
                    { x = 2, y = 2 },
                    { x = 0, y = 4 }
                })
                decomposition_missing_fails = not pcall(decomposePolygon)
                decomposition_type_fails = not pcall(decomposePolygon, false)
                decomposition_point_type_fails = not pcall(
                    decomposePolygon, { { x = 0, y = 0 }, false, { x = 1, y = 1 } }
                )
                decomposition_x_type_fails = not pcall(
                    decomposePolygon, { { x = "0", y = 0 }, { x = 1, y = 0 }, { x = 0, y = 1 } }
                )
                decomposition_y_missing_fails = not pcall(
                    decomposePolygon, { { x = 0 }, { x = 1, y = 0 }, { x = 0, y = 1 } }
                )
                "#,
        )
        .unwrap();

    let environment = game_environment(runtime.lua()).unwrap();
    for field in [
        "decomposition_missing_fails",
        "decomposition_type_fails",
        "decomposition_point_type_fails",
        "decomposition_x_type_fails",
        "decomposition_y_missing_fails",
    ] {
        assert!(environment.get::<bool>(field).unwrap(), "{field}");
    }
    let decomposed: mlua::Table = environment.get("decomposed").unwrap();
    assert_eq!(decomposed.raw_len(), 2);
    let mut total_area = 0.0;
    for polygon in decomposed.sequence_values::<mlua::Table>() {
        let polygon = polygon.unwrap();
        assert!((3..=8).contains(&polygon.raw_len()));
        let points = polygon
            .sequence_values::<mlua::Table>()
            .map(|point| {
                let point = point.unwrap();
                (
                    point.get::<f64>("x").unwrap(),
                    point.get::<f64>("y").unwrap(),
                )
            })
            .collect::<Vec<_>>();
        assert!(points.iter().all(|&(x, y)| x != 999.0 && y != 999.0));
        total_area += polygon_area(&points);
    }
    assert!((total_area - 12.0).abs() < 1.0e-9);
}

#[test]
fn native_polygon_decomposition_merges_convex_triangles_and_rounds_to_float32() {
    let square = decompose_native_polygon(&[(0.0, 0.0), (4.0, 0.0), (4.0, 4.0), (0.0, 4.0)]);
    assert_eq!(square.len(), 1);
    assert_eq!(square[0].len(), 4);
    assert_eq!(polygon_area(&square[0]), 16.0);

    let quantized = decompose_native_polygon(&[
        (16_777_217.0, 0.0),
        (16_777_217.0, 4.0),
        (16_777_213.0, 0.0),
    ]);
    assert_eq!(quantized.len(), 1);
    assert!(quantized[0].iter().any(|point| point.0 == 16_777_216.0));
    assert!(quantized[0].iter().all(|point| point.0 != 16_777_217.0));

    let self_touching = decompose_native_polygon(&[
        (0.0, 0.0),
        (-2.0, 0.0),
        (-2.0, 2.0),
        (0.0, 2.0),
        (0.000_5, 0.000_5),
        (2.0, 0.0),
        (2.0, -2.0),
        (0.0, -2.0),
    ]);
    assert_eq!(self_touching.len(), 2);
    assert!(self_touching.iter().all(|polygon| polygon.len() == 4));
    assert!(
        (self_touching
            .iter()
            .map(|polygon| polygon_area(polygon))
            .sum::<f64>()
            - 8.0)
            .abs()
            < 0.01
    );
}

#[test]
fn block_editor_loader_uses_recovered_script_path_and_module_names() {
    let unique = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let root = std::env::temp_dir().join(format!(
        "stella-block-editor-{}-{unique}",
        std::process::id()
    ));
    fs::create_dir_all(root.join("editor-defs")).unwrap();
    fs::write(
        root.join("editor-defs/blocks_wood.lua"),
        br#"
            marker = 'wood-loaded'
            gamelua.retainedEditor = blockEditorTable
            gamelua.blockEditorTable = { shadow = true }
        "#,
    )
    .unwrap();
    fs::write(
        root.join("editor-defs/groups.lua"),
        b"marker = 'groups-loaded'",
    )
    .unwrap();

    let runtime = StellaLua::new(&root).unwrap();
    runtime
        .execute_source(
            r#"
                scriptPath = "editor-defs"
                loadBlocksForEditing()
                editor_marker = retainedEditor.blocks_wood.marker
                editor_group_marker = retainedEditor.groups.marker
                editor_shadow_untouched = blockEditorTable.shadow == true
                    and blockEditorTable.blocks_wood == nil
                    and blockEditorTable.groups == nil
                "#,
        )
        .unwrap();
    let environment = game_environment(runtime.lua()).unwrap();
    assert_eq!(
        environment.get::<String>("editor_marker").unwrap(),
        "wood-loaded"
    );
    assert_eq!(
        environment.get::<String>("editor_group_marker").unwrap(),
        "groups-loaded"
    );
    assert!(environment.get::<bool>("editor_shadow_untouched").unwrap());
    drop(runtime);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn native_text_table_loader_matches_recovered_five_slot_contract() {
    let unique = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let root = std::env::temp_dir().join(format!(
        "stella-native-text-loader-{}-{unique}",
        std::process::id()
    ));
    let data_root = root.join("data");
    let app_root = root.join("appdata");
    fs::create_dir_all(data_root.join("config")).unwrap();
    fs::create_dir_all(app_root.join("assets_service")).unwrap();
    fs::write(
        app_root.join("table.lua"),
        b"return { marker = loaderGlobal, nested = { value = 7 } }",
    )
    .unwrap();
    fs::write(
        app_root.join("document.json"),
        br#"{"marker":116,"items":[true,3]}"#,
    )
    .unwrap();
    fs::write(app_root.join("empty.lua"), b"").unwrap();
    fs::write(app_root.join("invalid.lua"), b"return {").unwrap();
    fs::write(
        data_root.join("config/fallback.json"),
        br#"{"source":"bundle-fallback"}"#,
    )
    .unwrap();
    fs::write(
        app_root.join("assets_service/downloaded.dat"),
        br#"{"source":"app-data-resource"}"#,
    )
    .unwrap();

    let runtime = StellaLua::new(&data_root).unwrap();
    runtime
        .execute_source(
            r#"
                loaderGlobal = 116
                local from_lua = native_loadTextFileToLuaTable(
                    "table.lua", false
                )
                assert(from_lua.marker == 116)
                assert(from_lua.nested.value == 7)

                local from_json = native_loadTextFileToLuaTable(
                    "document.json", false, true
                )
                assert(from_json.marker == 116)
                assert(from_json.items[1] == true)
                assert(from_json.items[2] == 3)

                assert(native_loadTextFileToLuaTable(
                    "empty.lua", false
                ) == nil)
                local fallback = native_loadTextFileToLuaTable(
                    "assets_service/fallback.dat", true, true, true
                )
                assert(fallback.source == "bundle-fallback")
                local downloaded = native_loadTextFileToLuaTable(
                    "assets_service/downloaded.dat", true, true, true
                )
                assert(downloaded.source == "app-data-resource")
                assert(native_loadTextFileToLuaTable(
                    "missing.lua", false
                ) == nil)
                assert(native_loadTextFileToLuaTable(
                    "missing.dat", true
                ) == nil)
                assert(not pcall(native_loadTextFileToLuaTable, "table.lua"))
                assert(not pcall(
                    native_loadTextFileToLuaTable, "table.lua", 0
                ))
                assert(not pcall(
                    native_loadTextFileToLuaTable, "table.lua", false, nil
                ))
                assert(not pcall(
                    native_loadTextFileToLuaTable,
                    "table.lua", false, false, "no"
                ))
                assert(not pcall(
                    native_loadTextFileToLuaTable,
                    "table.lua", false, false, false, 1
                ))
                assert(not pcall(
                    native_loadTextFileToLuaTable, "invalid.lua", false
                ))
                "#,
        )
        .unwrap();

    drop(runtime);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn load_table_from_missing_account_file_publishes_fresh_empty_table_without_writing() {
    let unique = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let root = std::env::temp_dir().join(format!(
        "stella-missing-account-{}-{unique}",
        std::process::id()
    ));
    let data_root = root.join("data");
    let app_root = root.join("appdata");
    fs::create_dir_all(&data_root).unwrap();
    fs::create_dir_all(&app_root).unwrap();
    fs::write(
        app_root.join("corrupt.lua"),
        b"this is not a valid Lua chunk !!!",
    )
    .unwrap();
    let runtime = StellaLua::new(&data_root).unwrap();
    runtime.execute_source(r#"
        oldAccountSettings = {root = {score = 17}}
        localAccountSettings = oldAccountSettings
        if select('#', loadTableFromFile('settings_synthetic.lua', 'localAccountSettings')) ~= 0 then error('native member must return zero values') end
        if type(localAccountSettings) ~= 'table' or next(localAccountSettings) ~= nil then error('missing account must publish empty table') end
        if rawequal(localAccountSettings, oldAccountSettings) then error('previous account settings were reused') end
        if oldAccountSettings.root.score ~= 17 then error('old account table mutated') end
        firstMissing = localAccountSettings
        loadTableFromFile('settings_synthetic.lua', 'localAccountSettings')
        if rawequal(firstMissing, localAccountSettings) then error('missing loads must create fresh tables') end
        if not rawequal(_G.localAccountSettings, localAccountSettings) then error('global publication mismatch') end
        if pcall(loadTableFromFile, 'settings_synthetic.lua') then error('missing filename bypassed arity check') end
        if pcall(loadTableFromFile, '../outside.lua', 'localAccountSettings') then error('unsafe path accepted') end
        if pcall(loadTableFromFile, '', 'localAccountSettings') then error('empty path accepted') end
        loadTableFromFile('corrupt.lua', 'localAccountSettings')
        if next(localAccountSettings) ~= nil then error('compile failure must return a fresh empty table') end
    "#).unwrap();
    assert!(!app_root.join("settings_synthetic.lua").exists());
    assert!(!app_root.join("settings_synthetic.lua.json").exists());
    assert_eq!(
        &runtime.pending_persistent_load_messages()[3..],
        [
            "File Created:settings_synthetic.lua",
            "File Created:settings_synthetic.lua",
            "Persistent file loading failed corrupt.lua",
        ]
    );
    assert_eq!(
        fs::read(app_root.join("corrupt.lua")).unwrap(),
        b"this is not a valid Lua chunk !!!"
    );
    drop(runtime);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn app_data_lua_serializer_and_loaders_round_trip_native_table_shape() {
    let unique = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let root = std::env::temp_dir().join(format!(
        "stella-appdata-roundtrip-{}-{unique}",
        std::process::id()
    ));
    let data_root = root.join("data");
    let app_root = root.join("appdata");
    fs::create_dir_all(&data_root).unwrap();
    fs::create_dir_all(&app_root).unwrap();
    fs::write(data_root.join("bundle-only.lua"), b"bundle_marker = true").unwrap();
    fs::write(
        app_root.join("persistent-old.json"),
        br#"{"legacy":true,"count":3}"#,
    )
    .unwrap();
    fs::write(
        app_root.join("legacy-level"),
        br#"{"world":{"legacy_block":{"x":9}}}"#,
    )
    .unwrap();
    fs::write(app_root.join("plain.txt"), b"plain\ntext").unwrap();
    fs::create_dir_all(app_root.join("existing-directory")).unwrap();

    let runtime = StellaLua::new(&data_root).unwrap();
    runtime
        .execute_source(
            r#"
                persisted = {
                    answer = 42,
                    enabled = true,
                    text = "line\nquote\"slash\\",
                    nested = { "first", "second", ["bad-key"] = 7 },
                    _G = "must-not-be-serialized",
                    this = "must-not-be-serialized"
                }
                local bad_save_bool = pcall(
                    saveLuaFile, "bad.lua", "persisted", 1
                )
                assert(not bad_save_bool)
                local bad_load_arity = pcall(loadTableFromFile, "roundtrip.lua")
                assert(not bad_load_arity)
                plain_text = loadTextFileToString(
                    "plain.txt", false, false, false
                )
                assert(plain_text == "plain\ntext")
                assert(loadTextFileToString(
                    "missing.txt", false, false, false
                ) == "")
                assert(loadTextFileToString(
                    "missing.dat", true, false, false
                ) == "")
                assert(not pcall(
                    loadTextFileToString, "plain.txt", false, false
                ))
                assert(select('#', saveLuaFile(
                    "roundtrip.lua", "persisted", false
                )) == 0)
                assert(fileExistsInAppData("roundtrip.lua", "ignored"))
                assert(fileExistsInAppData("existing-directory"))
                assert(not fileExistsInAppData("bundle-only.lua"))
                assert(not pcall(fileExistsInAppData, 123))

                persisted = nil
                assert(select('#', loadTableFromFile(
                    "roundtrip.lua", "restored"
                )) == 0)
                roundtrip_answer = restored.answer
                roundtrip_first = restored.nested[1]
                roundtrip_bad_key = restored.nested["bad-key"]
                roundtrip_text = restored.text

                holder = {}
                assert(select('#', loadLuaFileFromAppDataToObject(
                    "roundtrip.lua", holder, "child"
                )) == 0)
                object_loader_answer = holder.child.answer
                plain_holder = {}
                assert(select('#', loadLuaFileFromAppDataToObject(
                    "roundtrip.lua", plain_holder, "child",
                    false, false, false
                )) == 0)
                assert(plain_holder.child.answer == 42)
                assert(not pcall(loadLuaFileFromAppDataToObject,
                    "roundtrip.lua", {}, "bad", false, "true"
                ))

                secure = { token = "native-aes", number = 81 }
                assert(select('#', savePersistentLuaFile(
                    "secure.dat", "secure"
                )) == 0)
                secure = nil
                assert(select('#', loadTableFromFile(
                    "secure.dat", "secure_restored"
                )) == 0)
                secure_number = secure_restored.number
                secure_holder = {}
                assert(select('#', loadLuaFileFromAppDataToObject(
                    "secure.dat", secure_holder, "child",
                    false, true, false
                )) == 0)
                secure_object_token = secure_holder.child.token

                secure_via_save_lua = { marker = 27 }
                assert(select('#', saveLuaFile(
                    "secure-via-save-lua.dat", "secure_via_save_lua", true
                )) == 0)
                assert(select('#', loadTableFromFile(
                    "secure-via-save-lua.dat", "secure_via_save_lua_restored"
                )) == 0)
                secure_save_lua_marker = secure_via_save_lua_restored.marker

                assert(select('#', loadTableFromFile(
                    "persistent-old", "legacy_restored"
                )) == 0)
                legacy_count = legacy_restored.count

                assert(select('#', loadLevelFromAppData("legacy-level")) == 0)
                legacy_level_x = loadedObjects.world.legacy_block.x

                objects = {
                    filename = "level-copy",
                    world = { block = { x = 12, y = 34 } },
                    gravityForceMultiplier = 0.75
                }
                assert(select('#', saveLevel("level-copy")) == 0)
                assert(select('#', loadLevelFromAppData("level-copy")) == 0)
                level_block_x = loadedObjects.world.block.x
                level_gravity = loadedObjects.gravityForceMultiplier
                level_native_gravity = getGravityForceMultiplier()
                "#,
        )
        .unwrap();

    let environment = game_environment(runtime.lua()).unwrap();
    assert_eq!(environment.get::<i64>("roundtrip_answer").unwrap(), 42);
    assert_eq!(
        environment.get::<String>("roundtrip_first").unwrap(),
        "first"
    );
    assert_eq!(environment.get::<i64>("roundtrip_bad_key").unwrap(), 7);
    assert_eq!(
        environment.get::<String>("roundtrip_text").unwrap(),
        "line\nquote\"slash\\"
    );
    assert_eq!(environment.get::<i64>("object_loader_answer").unwrap(), 42);
    assert_eq!(environment.get::<i64>("secure_number").unwrap(), 81);
    assert_eq!(
        environment.get::<String>("secure_object_token").unwrap(),
        "native-aes"
    );
    assert_eq!(
        environment.get::<i64>("secure_save_lua_marker").unwrap(),
        27
    );
    assert_eq!(environment.get::<i64>("legacy_count").unwrap(), 3);
    assert_eq!(environment.get::<i64>("legacy_level_x").unwrap(), 9);
    assert_eq!(environment.get::<i64>("level_block_x").unwrap(), 12);
    assert_eq!(environment.get::<f64>("level_gravity").unwrap(), 0.75);
    assert_eq!(
        environment.get::<f64>("level_native_gravity").unwrap(),
        0.75
    );

    let saved = fs::read_to_string(app_root.join("roundtrip.lua")).unwrap();
    assert!(saved.contains("answer = 42"));
    assert!(saved.contains("[\"bad-key\"] = 7"));
    assert!(!saved.contains("must-not-be-serialized"));
    assert!(app_root.join("level-copy.lua").is_file());
    let encrypted = fs::read(app_root.join("secure.dat")).unwrap();
    assert!(encrypted.len().is_multiple_of(16));
    let decrypted = stella_assets::decrypt_persistent_lua(&encrypted).unwrap();
    let token_assignment = b"token = \"native-aes\"";
    assert!(
        decrypted
            .windows(token_assignment.len())
            .any(|bytes| bytes == token_assignment)
    );

    drop(runtime);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn save_level_applies_recovered_root_object_and_sensor_field_whitelists() {
    let unique = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let root = std::env::temp_dir().join(format!(
        "stella-level-whitelist-{}-{unique}",
        std::process::id()
    ));
    let data_root = root.join("data");
    fs::create_dir_all(&data_root).unwrap();

    let runtime = StellaLua::new(&data_root).unwrap();
    runtime
        .execute_source(
            r#"
                objects = {
                    theme = "forest",
                    physicsToWorld = 16,
                    doNotWaitForMovingObjects = true,
                    variantGroups = { "day", "night" },
                    variantProbabilities = { 0.75, 0.25 },
                    runtimeRoot = "drop-me",
                    world = {
                        excludedA = {
                            angle = 0, x = 0, y = 0, name = "excludedA",
                            definition = "BLOCK_SENSOR_PIG_A", z_order = 0
                        },
                        excludedB = {
                            angle = 0, x = 0, y = 0, name = "excludedB",
                            definition = "BLOCK_SENSOR_PIG_B", z_order = 0
                        },
                        gravity = {
                            angle = 1.25, x = 2, y = 3, name = "gravity",
                            definition = "GRAVITY_DEF", z_order = 7,
                            active = false,
                            gravityFilterCategory = "NONE",
                            triggerEvents = { enter = "start" },
                            scale = 1.5, scaleX = 2, scaleY = 3,
                            horFlip = true, themeTexture = "THEME",
                            startNumber = 4, startNumberDecimal = 5,
                            episodeType = 6, pageNumber = 7,
                            shotPattern = 8, levelNumber = 9,
                            area = "tree", groupingIndex = 10,
                            groupVariantIndex = 11,
                            sensorType = "gravitation",
                            gravitationMinForce = 12,
                            gravitationMaxForce = 13,
                            isWater = true, waterDensityZeroLevel = 14,
                            radius = 15,
                            canBeEdited = true,
                            explosionRadius = 16, explosionForce = 17,
                            explosionDamageRadius = 18, explosionDamage = 19,
                            startingForce = 20, forceAngle = 21,
                            suckerTransmitSpeed = 22, suckerExitSpeed = 23,
                            editableAttributes = {
                                "customNumber", "customBool", "customString",
                                "customTable", "missing"
                            },
                            customNumber = 24.5,
                            customBool = true,
                            customString = "kept",
                            customTable = { "first", nested = { value = 25 } },
                            runtimeOnly = "drop-me"
                        },
                        stream = {
                            angle = 0, x = 1, y = 2, name = "stream",
                            definition = "STREAM_DEF", z_order = 3,
                            sensorType = "stream", radius = 4, force = 5,
                            nodes = { { x = 1, y = 2 } },
                            vertices = { { x = 3, y = 4 } }
                        },
                        killing = {
                            angle = 0, x = 1, y = 2, name = "killing",
                            definition = "KILL_DEF", z_order = 3,
                            sensorType = "killing", width = 6, height = 7
                        },
                        collectible = {
                            angle = 0, x = 1, y = 2, name = "collectible",
                            definition = "COLLECT_DEF", z_order = 3,
                            sensorType = "collectible", width = 99
                        }
                    }
                }
                assert(select('#', saveLevel("filtered")) == 0)
                assert(select('#', loadLevelFromAppData("filtered")) == 0)

                whitelist_ok =
                    loadedObjects.theme == "forest" and
                    loadedObjects.physicsToWorld == 16 and
                    loadedObjects.doNotWaitForMovingObjects == true and
                    loadedObjects.runtimeRoot == nil and
                    loadedObjects.world.excludedA == nil and
                    loadedObjects.world.excludedB == nil and
                    loadedObjects.world.gravity.angle == 1.25 and
                    loadedObjects.world.gravity.customNumber == 24.5 and
                    loadedObjects.world.gravity.customBool == true and
                    loadedObjects.world.gravity.customString == "kept" and
                    loadedObjects.world.gravity.customTable[1] == "first" and
                    loadedObjects.world.gravity.customTable.nested.value == 25 and
                    loadedObjects.world.gravity.runtimeOnly == nil and
                    loadedObjects.world.gravity.sensorType == nil and
                    loadedObjects.world.gravity.isWater == nil and
                    loadedObjects.world.gravity.canBeEdited == nil and
                    loadedObjects.world.gravity.editableAttributes == nil and
                    loadedObjects.world.gravity.gravityFilterCategory == nil and
                    loadedObjects.world.gravity.gravitationMinForce == 12 and
                    loadedObjects.world.gravity.gravitationMaxForce == 13 and
                    loadedObjects.world.gravity.waterDensityZeroLevel == 14 and
                    loadedObjects.world.gravity.radius == 15 and
                    loadedObjects.world.gravity.explosionDamage == 19 and
                    loadedObjects.world.stream.radius == 4 and
                    loadedObjects.world.stream.force == 5 and
                    loadedObjects.world.stream.nodes[1].x == 1 and
                    loadedObjects.world.stream.vertices[1].y == 4 and
                    loadedObjects.world.killing.width == 6 and
                    loadedObjects.world.killing.height == 7 and
                    loadedObjects.world.collectible.width == nil

                objects.world.bad = {
                    angle = 0, x = 0, y = 0, name = "bad",
                    definition = "BAD_DEF", z_order = 0,
                    editableAttributes = { "callback" },
                    callback = function() end
                }
                unsupported_ok, unsupported_error = pcall(saveLevel, "unsupported")
                unsupported_failed = not unsupported_ok
                unsupported_message = tostring(unsupported_error)
                "#,
        )
        .unwrap();

    let environment = game_environment(runtime.lua()).unwrap();
    assert!(environment.get::<bool>("whitelist_ok").unwrap());
    assert!(environment.get::<bool>("unsupported_failed").unwrap());
    let message = environment.get::<String>("unsupported_message").unwrap();
    assert!(message.contains(
        "Attribute callback of block bad can't be saved because it's of an unsupported type"
    ));

    let saved = fs::read_to_string(root.join("appdata/filtered.lua")).unwrap();
    assert!(!saved.contains("runtimeRoot"));
    assert!(!saved.contains("runtimeOnly"));
    assert!(!saved.contains("sensorType"));
    assert!(!saved.contains("editableAttributes"));
    drop(runtime);
    fs::remove_dir_all(root).unwrap();
}
