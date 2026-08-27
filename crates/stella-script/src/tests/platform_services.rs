use super::*;

use std::{
    io::{Read, Write},
    net::TcpListener,
    thread,
    time::Duration,
};

fn spawn_url_response(body: Vec<u8>) -> (String, thread::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let worker = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let mut request = [0_u8; 2048];
        let _ = stream.read(&mut request).unwrap();
        write!(
            stream,
            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            body.len()
        )
        .unwrap();
        stream.write_all(&body).unwrap();
    });
    (format!("http://{address}/stella-data"), worker)
}

#[test]
fn native_url_thread_fetches_binary_body_and_dispatches_before_lua_update() {
    let body = vec![b'S', 0, 0x80, b'!'];
    let (url, server) = spawn_url_response(body.clone());
    let runtime = StellaLua::new("/tmp").unwrap();
    runtime
        .execute_source(&format!(
            r#"
                url_callback_count = 0
                url_callback_url = nil
                url_callback_body = nil
                callback_preceded_update = false
                native_startURLThread(
                    "{url}",
                    function(callback_url, callback_body)
                        url_callback_count = url_callback_count + 1
                        url_callback_url = callback_url
                        url_callback_body = callback_body
                    end,
                    true
                )
                update = function()
                    if url_callback_count > 0 then
                        callback_preceded_update = true
                    end
                end
            "#
        ))
        .unwrap();

    let environment = game_environment(runtime.lua()).unwrap();
    assert_eq!(environment.get::<i64>("url_callback_count").unwrap(), 0);
    server.join().unwrap();

    for _ in 0..200 {
        runtime.update(0.0).unwrap();
        if environment.get::<i64>("url_callback_count").unwrap() == 1 {
            break;
        }
        thread::sleep(Duration::from_millis(2));
    }

    assert_eq!(environment.get::<i64>("url_callback_count").unwrap(), 1);
    assert_eq!(environment.get::<String>("url_callback_url").unwrap(), url);
    assert_eq!(
        environment
            .get::<mlua::LuaString>("url_callback_body")
            .unwrap()
            .as_bytes()
            .as_ref(),
        body
    );
    assert!(environment.get::<bool>("callback_preceded_update").unwrap());
}

#[test]
fn native_url_thread_preserves_generated_adapter_argument_contract() {
    let runtime = StellaLua::new("/tmp").unwrap();
    runtime
        .execute_source(
            r#"
                url_missing_fails = not pcall(native_startURLThread)
                url_callback_missing_fails = not pcall(
                    native_startURLThread, "not-a-url"
                )
                url_callback_type_fails = not pcall(
                    native_startURLThread, "not-a-url", false
                )
                url_exact_third_type_fails = not pcall(
                    native_startURLThread, "not-a-url", function() end, 1
                )
                url_fourth_argument_skips_optional_bool = pcall(
                    native_startURLThread,
                    "not-a-url",
                    function() end,
                    "ignored when stack count is not three",
                    true
                )
            "#,
        )
        .unwrap();
    let environment = game_environment(runtime.lua()).unwrap();
    for name in [
        "url_missing_fails",
        "url_callback_missing_fails",
        "url_callback_type_fails",
        "url_exact_third_type_fails",
        "url_fourth_argument_skips_optional_bool",
    ] {
        assert!(environment.get::<bool>(name).unwrap(), "{name}");
    }
}

#[test]
fn external_url_and_store_members_queue_host_actions_in_native_call_order() {
    let runtime = StellaLua::new("/tmp").unwrap();
    runtime
        .execute_source(
            r#"
                open_url_result = res.openURL("https://example.invalid/stella")
                open_url_missing_fails = not pcall(res.openURL)
                open_url_type_fails = not pcall(res.openURL, false)
                ForceUpdate.native_launchAppStore()
            "#,
        )
        .unwrap();

    let environment = game_environment(runtime.lua()).unwrap();
    assert!(environment.get::<bool>("open_url_result").unwrap());
    assert!(environment.get::<bool>("open_url_missing_fails").unwrap());
    assert!(environment.get::<bool>("open_url_type_fails").unwrap());
    assert_eq!(
        runtime.take_platform_action_requests(),
        vec![
            PlatformActionRequest::OpenUrl {
                url: "https://example.invalid/stella".to_owned(),
            },
            PlatformActionRequest::OpenAppStoreProduct {
                product_id: "875251011".to_owned(),
                product_type: 3,
            },
        ]
    );
    assert!(runtime.take_platform_action_requests().is_empty());
}

#[test]
fn screenshot_share_queues_native_temp_names_titles_and_wrap_order() {
    set_screenshot_sequence_for_test(0);
    let runtime = StellaLua::new("/tmp").unwrap();
    runtime
        .execute_source(
            r#"
                native_shareScreenShot("first title")
                native_shareScreenShot("second title", "ignored")
                screenshot_bad_title_fails = not pcall(
                    native_shareScreenShot, 123
                )
            "#,
        )
        .unwrap();
    assert!(
        game_environment(runtime.lua())
            .unwrap()
            .get::<bool>("screenshot_bad_title_fails")
            .unwrap()
    );

    assert_eq!(
        runtime.take_screenshot_share_requests(),
        vec![
            ScreenshotShareRequest {
                sequence: 1,
                filename: "Stella_Screenshot1.png".to_owned(),
                title: "first title".to_owned(),
            },
            ScreenshotShareRequest {
                sequence: 2,
                filename: "Stella_Screenshot2.png".to_owned(),
                title: "second title".to_owned(),
            },
        ]
    );
    assert!(runtime.take_screenshot_share_requests().is_empty());

    let second_runtime = StellaLua::new("/tmp").unwrap();
    second_runtime
        .execute_source(r#"native_shareScreenShot("next runtime")"#)
        .unwrap();
    assert_eq!(
        second_runtime.take_screenshot_share_requests(),
        vec![ScreenshotShareRequest {
            sequence: 3,
            filename: "Stella_Screenshot3.png".to_owned(),
            title: "next runtime".to_owned(),
        }]
    );

    set_screenshot_sequence_for_test(i32::MAX);
    runtime
        .execute_source(r#"native_shareScreenShot("signed wrapped")"#)
        .unwrap();
    assert_eq!(
        runtime.take_screenshot_share_requests(),
        vec![ScreenshotShareRequest {
            sequence: i32::MIN,
            filename: "Stella_Screenshot-2147483648.png".to_owned(),
            title: "signed wrapped".to_owned(),
        }]
    );

    set_screenshot_sequence_for_test(-1);
    runtime
        .execute_source(r#"native_shareScreenShot("zero wrapped")"#)
        .unwrap();
    assert_eq!(
        runtime.take_screenshot_share_requests(),
        vec![ScreenshotShareRequest {
            sequence: 0,
            filename: "Stella_Screenshot0.png".to_owned(),
            title: "zero wrapped".to_owned(),
        }]
    );
}

#[test]
fn native_calendar_bindings_follow_local_mktime_and_float32_contracts() {
    let runtime = StellaLua::new("/tmp").unwrap();
    runtime
        .execute_source(
            r##"
                local current = getCurrentTime("ignored by native adapter")
                local current_epoch = os.time({
                    year = current.year,
                    month = current.month,
                    day = current.day,
                    hour = current.hour,
                    min = current.minutes,
                    sec = current.seconds,
                })
                current_is_local = math.abs(os.time() - current_epoch) <= 1
                current_fields_are_numbers =
                    type(current.year) == "number" and
                    type(current.month) == "number" and
                    type(current.day) == "number" and
                    type(current.hour) == "number" and
                    type(current.minutes) == "number" and
                    type(current.seconds) == "number"

                local midnight = {
                    year = 2024, month = 1, day = 1,
                    hour = 0, minutes = 0, seconds = 0,
                }
                local midnight_without_clock = {
                    year = 2024, month = 1, day = 1,
                }
                omitted_clock_difference = getTimeDifferenceInSeconds(
                    midnight_without_clock, midnight
                )

                local later = {
                    year = 2024, month = 1, day = 2,
                    hour = 1, minutes = 1, seconds = 1,
                }
                decomposed_difference = getTimeDifference(later, midnight)
                decomposed_difference_with_extra = getTimeDifference(
                    later, midnight, "ignored by native member"
                )
                signed_forward_difference = getTimeDifferenceInSeconds(
                    later, midnight, "ignored by native member"
                )
                signed_reverse_difference = getTimeDifferenceInSeconds(
                    midnight, later
                )
                difference_first_tag_fails = not pcall(
                    getTimeDifference, false, midnight
                )
                difference_second_tag_fails = not pcall(
                    getTimeDifferenceInSeconds, midnight, false
                )

                duration_fraction = addDurationToTime({
                    year = 2024, month = 1, day = 1,
                    hour = 0, minutes = 0, seconds = 10.5,
                }, 1.75)
                duration_missing_field_fails = not pcall(
                    addDurationToTime,
                    { year = 2024, month = 1, day = 1 },
                    1
                )
                duration_source_tag_fails = not pcall(
                    addDurationToTime, "2024-01-01", 1
                )
                duration_numeric_string_fails = not pcall(
                    addDurationToTime, midnight, "1"
                )
                duration_ignores_extra = addDurationToTime(
                    midnight, 1, "ignored by native member"
                )

                local long_first = {
                    year = 2030, month = 1, day = 1,
                    hour = 0, minutes = 0, seconds = 1,
                }
                local long_second = {
                    year = 2000, month = 1, day = 1,
                    hour = 0, minutes = 0, seconds = 0,
                }
                long_difference = getTimeDifferenceInSeconds(
                    long_first, long_second
                )
                exact_long_difference = os.difftime(
                    os.time({
                        year = 2030, month = 1, day = 1,
                        hour = 0, min = 0, sec = 1, isdst = false,
                    }),
                    os.time({
                        year = 2000, month = 1, day = 1,
                        hour = 0, min = 0, sec = 0, isdst = false,
                    })
                )
            "##,
        )
        .unwrap();

    let environment = game_environment(runtime.lua()).unwrap();
    assert!(environment.get::<bool>("current_is_local").unwrap());
    assert!(
        environment
            .get::<bool>("current_fields_are_numbers")
            .unwrap()
    );
    assert_eq!(
        environment.get::<f64>("omitted_clock_difference").unwrap(),
        0.0
    );

    let difference: mlua::Table = environment.get("decomposed_difference").unwrap();
    let difference_with_extra: mlua::Table =
        environment.get("decomposed_difference_with_extra").unwrap();
    for (field, expected) in [
        ("days", 1.0),
        ("hours", 1.0),
        ("minutes", 1.0),
        ("seconds", 1.0),
    ] {
        assert_eq!(difference.get::<f64>(field).unwrap(), expected, "{field}");
        assert_eq!(
            difference_with_extra.get::<f64>(field).unwrap(),
            expected,
            "extra {field}"
        );
    }
    assert!(
        environment
            .get::<bool>("difference_first_tag_fails")
            .unwrap()
    );
    assert!(
        environment
            .get::<bool>("difference_second_tag_fails")
            .unwrap()
    );
    assert_eq!(
        environment.get::<f64>("signed_forward_difference").unwrap(),
        90_061.0
    );
    assert_eq!(
        environment.get::<f64>("signed_reverse_difference").unwrap(),
        -90_061.0
    );

    let duration: mlua::Table = environment.get("duration_fraction").unwrap();
    assert_eq!(duration.get::<f64>("year").unwrap(), 2024.0);
    assert_eq!(duration.get::<f64>("month").unwrap(), 1.0);
    assert_eq!(duration.get::<f64>("day").unwrap(), 1.0);
    assert_eq!(duration.get::<f64>("hour").unwrap(), 0.0);
    assert_eq!(duration.get::<f64>("minutes").unwrap(), 0.0);
    assert_eq!(duration.get::<f64>("seconds").unwrap(), 12.0);
    assert!(
        environment
            .get::<bool>("duration_missing_field_fails")
            .unwrap()
    );
    assert!(
        environment
            .get::<bool>("duration_source_tag_fails")
            .unwrap()
    );
    assert!(
        environment
            .get::<bool>("duration_numeric_string_fails")
            .unwrap()
    );
    let duration_extra: mlua::Table = environment.get("duration_ignores_extra").unwrap();
    assert_eq!(duration_extra.get::<f64>("seconds").unwrap(), 1.0);

    let long_difference = environment.get::<f64>("long_difference").unwrap();
    let exact_long_difference = environment.get::<f64>("exact_long_difference").unwrap();
    assert_eq!(long_difference, (exact_long_difference as f32) as f64);
    assert_ne!(long_difference, exact_long_difference);
}

#[test]
fn offline_server_time_exposes_complete_native_utc_local_and_status_contract() {
    let runtime = StellaLua::new("/tmp").unwrap();
    runtime
        .execute_source(
            r##"
                local function same_calendar(left, right)
                    return left.year == right.year and
                        left.month == right.month and
                        left.day == right.day and
                        left.hour == right.hour and
                        left.minutes == right.min and
                        left.seconds == right.sec
                end
                local utc_before = os.date("!*t")
                server_utc = ServerTime.getServerTimeInUTC("ignored")
                local utc_after = os.date("!*t")
                server_utc_matches = same_calendar(server_utc, utc_before) or
                    same_calendar(server_utc, utc_after)

                local local_before = os.date("*t")
                server_local = ServerTime.getServerTimeInLocalTimeZone("ignored")
                local local_after = os.date("*t")
                server_local_matches = same_calendar(server_local, local_before) or
                    same_calendar(server_local, local_after)

                server_status_before = ServerTime.getStatus("ignored")
                server_sync_results = select(
                    "#", ServerTime.synchronizeServerTime("ignored")
                )
                server_status_after = ServerTime.getStatus()
            "##,
        )
        .unwrap();

    let environment = game_environment(runtime.lua()).unwrap();
    assert!(environment.get::<bool>("server_utc_matches").unwrap());
    assert!(environment.get::<bool>("server_local_matches").unwrap());
    assert_eq!(
        environment.get::<String>("server_status_before").unwrap(),
        "STATUS_OK"
    );
    assert_eq!(
        environment.get::<String>("server_status_after").unwrap(),
        "STATUS_OK"
    );
    assert_eq!(environment.get::<i64>("server_sync_results").unwrap(), 0);
    for table_name in ["server_utc", "server_local"] {
        let table: mlua::Table = environment.get(table_name).unwrap();
        for field in ["year", "month", "day", "hour", "minutes", "seconds"] {
            assert!(matches!(
                table.get::<Value>(field).unwrap(),
                Value::Integer(_) | Value::Number(_)
            ));
        }
    }
}

#[test]
fn native_bitwise_helpers_use_signed_fcvtzs_and_float32_results() {
    let runtime = StellaLua::new("/tmp").unwrap();
    runtime
        .execute_source(
            r##"
                fractional_and = performBitwiseAnd(6.9, 3.2)
                signed_or = performBitwiseOr(-8.9, 3.9)
                rounded_or = performBitwiseOr(16777216, 1)
                indefinite_or = performBitwiseOr(2147483648, 0)
                and_missing_fails = not pcall(performBitwiseAnd, 1)
                or_missing_fails = not pcall(performBitwiseOr, 1)
                and_type_fails = not pcall(performBitwiseAnd, false, 1)
                or_type_fails = not pcall(performBitwiseOr, 1, {})
            "##,
        )
        .unwrap();

    let environment = game_environment(runtime.lua()).unwrap();
    assert_eq!(environment.get::<f64>("fractional_and").unwrap(), 2.0);
    assert_eq!(environment.get::<f64>("signed_or").unwrap(), -5.0);
    assert_eq!(environment.get::<f64>("rounded_or").unwrap(), 16_777_216.0);
    assert_eq!(
        environment.get::<f64>("indefinite_or").unwrap(),
        -2_147_483_648.0
    );
    for field in [
        "and_missing_fails",
        "or_missing_fails",
        "and_type_fails",
        "or_type_fails",
    ] {
        assert!(environment.get::<bool>(field).unwrap(), "{field}");
    }
}

#[test]
fn refresh_current_locale_selects_a_supported_texts_basic_language() {
    let sandbox = ShippedDataSandbox::new("native-locale-refresh");
    let bytes = fs::read(sandbox.data_root.join("localization/TEXTS_BASIC.dat")).unwrap();
    let available = stella_assets::ka3d::LocalizationTable::parse(&bytes)
        .unwrap()
        .locales;
    let runtime = StellaLua::new(&sandbox.data_root).unwrap();
    runtime
        .execute_source(
            r##"
                res.setPath("localization")
                res.createTextGroupSet("TEXTS_BASIC.dat")
                g_currentLocale = "unsupported_previous_locale"
                setLocale = function(locale)
                    refreshed_locale = locale
                end
                refresh_locale_results = select("#", refreshCurrentLocale())
            "##,
        )
        .unwrap();

    let environment = game_environment(runtime.lua()).unwrap();
    let selected = environment.get::<String>("refreshed_locale").unwrap();
    assert!(available.contains(&selected), "selected {selected}");
    assert_ne!(selected, "unsupported_previous_locale");
    assert_eq!(environment.get::<i64>("refresh_locale_results").unwrap(), 0);
}

#[test]
fn native_lua_probe_and_bundle_copy_use_strict_paths_and_void_copy_abi() {
    let unique = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let root = std::env::temp_dir().join(format!("stella-native-file-helpers-{unique}"));
    let data_root = root.join("data");
    fs::create_dir_all(data_root.join("scripts")).unwrap();
    fs::create_dir_all(root.join("appdata")).unwrap();
    fs::write(data_root.join("payload.bin"), [0, 1, 2, 0xff]).unwrap();
    fs::write(data_root.join("scripts/probe.lua"), b"return true").unwrap();

    let runtime = StellaLua::new(&data_root).unwrap();
    runtime
        .execute_source(
            r##"
                direct_lua_exists = checkForLuaFile(
                    "scripts/probe.lua", "ignored"
                )
                prefixed_lua_exists = checkForLuaFile("probe.lua")
                missing_lua_exists = checkForLuaFile("missing.lua")
                lua_probe_missing_fails = not pcall(checkForLuaFile)
                lua_probe_type_fails = not pcall(checkForLuaFile, false)
                lua_probe_numeric_fails = not pcall(checkForLuaFile, 123)
                bundle_copy_results = select(
                    "#",
                    copyFileFromBundleToAppData(
                        "payload.bin", "nested/copied.bin", "ignored"
                    )
                )
                bundle_copy_short_fails = not pcall(
                    copyFileFromBundleToAppData, "payload.bin"
                )
                bundle_copy_type_fails = not pcall(
                    copyFileFromBundleToAppData, "payload.bin", false
                )
            "##,
        )
        .unwrap();

    let environment = game_environment(runtime.lua()).unwrap();
    assert!(environment.get::<bool>("direct_lua_exists").unwrap());
    assert!(environment.get::<bool>("prefixed_lua_exists").unwrap());
    assert!(!environment.get::<bool>("missing_lua_exists").unwrap());
    assert!(environment.get::<bool>("lua_probe_missing_fails").unwrap());
    assert!(environment.get::<bool>("lua_probe_type_fails").unwrap());
    assert!(environment.get::<bool>("lua_probe_numeric_fails").unwrap());
    assert_eq!(environment.get::<i64>("bundle_copy_results").unwrap(), 0);
    assert!(environment.get::<bool>("bundle_copy_short_fails").unwrap());
    assert!(environment.get::<bool>("bundle_copy_type_fails").unwrap());
    assert_eq!(
        fs::read(root.join("appdata/nested/copied.bin")).unwrap(),
        [0, 1, 2, 0xff]
    );
    drop(runtime);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn offline_video_member_preserves_native_string_void_request_boundary() {
    let runtime = StellaLua::new("/tmp").unwrap();
    runtime
        .execute_source(
            r##"
                video_results = select("#", playVideo("movies/intro.mp4", "ignored"))
                video_missing_fails = not pcall(playVideo)
                video_type_fails = not pcall(playVideo, false)
            "##,
        )
        .unwrap();

    let environment = game_environment(runtime.lua()).unwrap();
    assert_eq!(environment.get::<i64>("video_results").unwrap(), 0);
    assert!(environment.get::<bool>("video_missing_fails").unwrap());
    assert!(environment.get::<bool>("video_type_fails").unwrap());
    assert_eq!(
        runtime.render.lock().unwrap().requested_video.as_deref(),
        Some("movies/intro.mp4")
    );
}

#[test]
fn native_intersection_query_returns_object_name_array() {
    let runtime = StellaLua::new("/tmp").unwrap();
    runtime
        .execute_source(
            r##"
                createBox("hit", "", 10, 20, 4, 6, 0, 0, 0, true, false, 1)
                intersection_test_hits = getIntersectingObjects({
                    x = 10, y = 20, left = 0, right = 0, down = 0, up = 0
                })
                intersection_test_misses = getIntersectingObjects({
                    x = 30, y = 40, left = 0, right = 0, down = 0, up = 0
                })
                intersection_test_non_finite = getIntersectingObjects({
                    x = 0 / 0, y = 20, left = 0, right = 0, down = 0, up = 0
                })
                createCircle("fat_query", "", 40, 0, 1, 1, 0, 0, true, false, 1)
                intersection_fat_margin = getIntersectingObjects({
                    x = 41.05, y = 0, left = 0, right = 0, down = 0, up = 0
                })
                clearVertices()
                addVertex(2, -2); addVertex(8, -2); addVertex(8, 2)
                addVertex(6, 2); addVertex(6, 0); addVertex(4, 0)
                addVertex(4, 2); addVertex(2, 2)
                createPolygon("concave_query", "", 0, 0, 10, 10, 1, 0, 0, true, false, 1)
                createBox("inactive_query", "", 5, 1, 1, 1, 0, 0, 0, true, false, 1)
                setActive("inactive_query", false)
                intersection_concave_gap = getIntersectingObjects({
                    x = 5, y = 1, left = 0, right = 0, down = 0, up = 0
                })
                intersection_concave_solid = getIntersectingObjects({
                    x = 3, y = 1, left = 0, right = 0, down = 0, up = 0
                })
                createBox("order_a", "", 100, 100, 4, 4, 0, 0, 0, true, false, 1)
                createBox("order_b", "", 100, 100, 4, 4, 0, 0, 0, true, false, 1)
                createBox("order_c", "", 100, 100, 4, 4, 0, 0, 0, true, false, 1)
                intersection_allocation_order_before = getIntersectingObjects({
                    x = 100, y = 100, left = 0, right = 0, down = 0, up = 0
                })
                removeObject("order_b")
                createBox("order_d", "", 100, 100, 4, 4, 0, 0, 0, true, false, 1)
                intersection_allocation_order_after = getIntersectingObjects({
                    x = 100, y = 100, left = 0, right = 0, down = 0, up = 0
                })
                intersection_missing_fails = not pcall(getIntersectingObjects)
                intersection_type_fails = not pcall(getIntersectingObjects, false)
                intersection_top_type_fails = not pcall(
                    getIntersectingObjects,
                    { x = 0, y = 0, left = 0, right = 0, down = 0, up = 0 },
                    false
                )
                intersection_field_missing_fails = not pcall(
                    getIntersectingObjects,
                    { x = 0, y = 0, left = 0, right = 0, down = 0 }
                )
                intersection_field_type_fails = not pcall(
                    getIntersectingObjects,
                    { x = "0", y = 0, left = 0, right = 0, down = 0, up = 0 }
                )
                "##,
        )
        .unwrap();
    let environment = game_environment(runtime.lua()).unwrap();
    let hits: mlua::Table = environment.get("intersection_test_hits").unwrap();
    let misses: mlua::Table = environment.get("intersection_test_misses").unwrap();
    let non_finite: mlua::Table = environment.get("intersection_test_non_finite").unwrap();
    let fat_margin: mlua::Table = environment.get("intersection_fat_margin").unwrap();
    let concave_gap: mlua::Table = environment.get("intersection_concave_gap").unwrap();
    let concave_solid: mlua::Table = environment.get("intersection_concave_solid").unwrap();
    let order_before: mlua::Table = environment
        .get("intersection_allocation_order_before")
        .unwrap();
    let order_after: mlua::Table = environment
        .get("intersection_allocation_order_after")
        .unwrap();
    for field in [
        "intersection_missing_fails",
        "intersection_type_fails",
        "intersection_top_type_fails",
        "intersection_field_missing_fails",
        "intersection_field_type_fails",
    ] {
        assert!(environment.get::<bool>(field).unwrap(), "{field}");
    }
    assert_eq!(hits.raw_get::<String>(1).unwrap(), "hit");
    assert_eq!(misses.raw_len(), 0);
    assert_eq!(non_finite.raw_len(), 0);
    assert_eq!(fat_margin.raw_get::<String>(1).unwrap(), "fat_query");
    assert_eq!(concave_gap.raw_len(), 0);
    assert_eq!(concave_solid.raw_get::<String>(1).unwrap(), "concave_query");
    assert_eq!(concave_solid.raw_len(), 1);
    assert_eq!(
        (1..=3)
            .map(|index| order_before.raw_get::<String>(index).unwrap())
            .collect::<Vec<_>>(),
        ["order_a", "order_b", "order_c"]
    );
    // The block allocator frees into the size-class list head, so the
    // replacement body occupies order_b's middle pointer slot rather than
    // sorting after the monotonically newer order_c allocation.
    assert_eq!(
        (1..=3)
            .map(|index| order_after.raw_get::<String>(index).unwrap())
            .collect::<Vec<_>>(),
        ["order_a", "order_d", "order_c"]
    );
}

#[test]
fn recovered_platform_and_render_utilities_preserve_native_contracts() {
    let runtime = StellaLua::new("/tmp").unwrap();
    register_test_sprite_sheet(&runtime, &["BAND"]);
    for (table_name, methods) in [
        (
            "ForceUpdate",
            &["native_checkForcedUpdate", "native_launchAppStore"][..],
        ),
        (
            "AppStoreLauncher",
            &["updateGameData", "launchAppStore"][..],
        ),
        (
            "Analytics",
            &[
                "logTimerEvent",
                "logEvent",
                "logEventWithParam",
                "logEventWithParams",
            ][..],
        ),
        (
            "FusionGamerServices",
            &[
                "isSupported",
                "getBackendName",
                "isLocalPlayerAuthenticated",
                "login",
                "showAchievements",
                "postAchievement",
                "showLeaderboards",
                "postScore",
            ][..],
        ),
        (
            "SocialManager",
            &[
                "native_isConnectedToSocialNetwork",
                "native_connectToSocialNetwork",
                "native_postScores",
                "native_fetchLeaderboard",
                "native_setProgress",
                "native_getFriendsProgress",
                "native_loadAvatar",
                "native_unloadAvatar",
                "native_unloadAllAvatars",
                "native_getSocialNetworkName",
                "native_getFriendAccountId",
                "native_getLocalUserAccountId",
                "native_getFriends",
            ][..],
        ),
        (
            "SkynestAccount",
            &[
                "native_getServiceName",
                "native_isLoggedIn",
                "native_isLoginInProgress",
                "native_getAccountDetailsUrl",
                "native_login",
                "native_logout",
                "native_loginWithSocialNetwork",
                "native_unRegister",
                "native_hasNickname",
                "native_validateNickname",
            ][..],
        ),
        (
            "SkynestStorage",
            &[
                "native_loadCloudSettings",
                "native_saveCloudSettings",
                "native_setRequestTimeout",
                "native_isTransactionInProcess",
                "native_setKey",
                "native_getKey",
                "native_getKeyForAccountIds",
            ][..],
        ),
        (
            "RovioAds",
            &[
                "refresh",
                "addPlacement",
                "addPlacementWithGeometry",
                "addPlacementNative",
                "show",
                "hide",
                "click",
                "trackConversion",
                "startSession",
            ][..],
        ),
        (
            "Zappar",
            &["native_isZapparSupported", "native_launchZappar"][..],
        ),
        (
            "QrScanner",
            &[
                "isCameraSupported",
                "isFrontCameraSupported",
                "start",
                "stop",
                "setQrRecognizedCallback",
            ][..],
        ),
        (
            "IAP",
            &[
                "native_buyItem",
                "native_restorePurchases",
                "native_getAvailableItems",
                "native_isPaymentInitialized",
                "native_fetchWallet",
                "native_useWalletValidation",
                "native_redeemCode",
                "native_refreshCatalog",
            ][..],
        ),
        ("Assets", &["loadFiles", "createSpriteSheet"][..]),
    ] {
        let table: mlua::Table = runtime.lua().globals().raw_get(table_name).unwrap();
        for method in methods {
            assert!(
                matches!(table.raw_get::<Value>(*method), Ok(Value::Function(_))),
                "Purple service registration {table_name}.{method} is not callable"
            );
        }
    }
    assert!(!runtime.exit_requested());
    runtime
        .execute_source(
            r##"
                orientation = native_getDeviceOrientation()
                device_id = uniqueDeviceId()
                first_shaders = createUniqueShaders("FX_", 2)
                destroy_shader_results = select("#", destroyUniqueShaders(
                    { first_shaders[1], first_shaders[2] }, "ignored"
                ))
                destroy_shader_tag_fails = not pcall(
                    destroyUniqueShaders, first_shaders[1]
                )
                destroy_shader_stringification_ok = pcall(
                    destroyUniqueShaders, { 123, false, "ignored tail" }
                )
                second_shaders = createUniqueShaders("FX_", 1)
                fractional_shaders = createUniqueShaders("FRAC_", 2.9, false)
                negative_shaders = createUniqueShaders("NEG_", -1)
                nan_shaders = createUniqueShaders("NAN_", 0 / 0)
                infinite_shaders = createUniqueShaders("INF_", math.huge)
                shader_base_tag_fails = not pcall(
                    createUniqueShaders, 7, 1
                )
                shader_count_tag_fails = not pcall(
                    createUniqueShaders, "BAD_", "1"
                )
                drawRubberband(0, 0, 10, 0, 2, "BAND")
                gamer_backend = FusionGamerServices.getBackendName()
                gamer_supported = FusionGamerServices.isSupported()
                gamer_authenticated =
                    FusionGamerServices.isLocalPlayerAuthenticated()
                achievement_results = select("#",
                    FusionGamerServices.postAchievement("ACH_TEST"))
                score_results = select("#",
                    FusionGamerServices.postScore("SCORE_TEST", 42))
                analytics_timer_results = select("#",
                    Analytics.logTimerEvent("timer"))
                analytics_event_results = select("#",
                    Analytics.logEvent("event"))
                analytics_param_results = select("#",
                    Analytics.logEventWithParam("event", "key", "value"))
                analytics_params_results = select("#",
                    Analytics.logEventWithParams("event", "json"))
                analytics_short_fails = not pcall(
                    Analytics.logEventWithParam, "event", "key")
                analytics_bad_type_fails = not pcall(
                    Analytics.logEventWithParams, "event", {})
                registration_first, registration_second =
                    checkRegistrationResult()
                register_key_short_fails = not pcall(registerKey, "a", "b")
                register_key_results = select("#", registerKey("a", "b", "c"))
                registration_after_first, registration_after_second =
                    checkRegistrationResult()
                force_update_results = select("#",
                    ForceUpdate.native_checkForcedUpdate("{}", function() end))
                force_update_missing_callback_fails = not pcall(
                    ForceUpdate.native_checkForcedUpdate, "{}")
                force_update_bad_config_fails = not pcall(
                    ForceUpdate.native_checkForcedUpdate, {}, function() end)
                force_update_bad_callback_fails = not pcall(
                    ForceUpdate.native_checkForcedUpdate, "{}", {})
                force_update_store_results = select("#",
                    ForceUpdate.native_launchAppStore())
                setInstalledAppsOffline = function(names)
                    installed_apps_offline = names
                end
                installed_offline_results = select("#",
                    checkInstalledAppsOffline(
                        '{"ttl":60,"gameCount":1,' ..
                        '"game_0":{"name":"AB","scheme":"ab"}}'))
                installed_online_results = select("#",
                    checkInstalledAppsOnline("request"))
                twitter_supported = isTwitterSupported()
                bad_achievement_fails = pcall(
                    FusionGamerServices.postAchievement, {})
                numeric_achievement_fails = pcall(
                    FusionGamerServices.postAchievement, 7)
                bad_score_fails = pcall(
                    FusionGamerServices.postScore, "SCORE_TEST", {})
                numeric_score_name_fails = pcall(
                    FusionGamerServices.postScore, 7, 42)
                string_score_fails = pcall(
                    FusionGamerServices.postScore, "SCORE_TEST", "42")
                trailing_score_ok = pcall(
                    FusionGamerServices.postScore, "SCORE_TEST", 42, false)
                requestExit()
                "##,
        )
        .unwrap();

    let environment = game_environment(runtime.lua()).unwrap();
    assert_eq!(environment.get::<i32>("orientation").unwrap(), 90);
    assert_eq!(
        environment.get::<String>("device_id").unwrap(),
        "unavailable"
    );
    let shader_suffix =
        |name: &str, prefix: &str| name.strip_prefix(prefix).unwrap().parse::<i32>().unwrap();
    let first: mlua::Table = environment.get("first_shaders").unwrap();
    let first_name = first.raw_get::<String>(1).unwrap();
    let first_second_name = first.raw_get::<String>(2).unwrap();
    let first_suffix = shader_suffix(&first_name, "FX_");
    let first_second_suffix = shader_suffix(&first_second_name, "FX_");
    assert_eq!(first_second_suffix, first_suffix.wrapping_add(1));
    let second: mlua::Table = environment.get("second_shaders").unwrap();
    let second_name = second.raw_get::<String>(1).unwrap();
    let second_suffix = shader_suffix(&second_name, "FX_");
    assert!(second_suffix > first_second_suffix);
    let fractional: mlua::Table = environment.get("fractional_shaders").unwrap();
    let fractional_name = fractional.raw_get::<String>(1).unwrap();
    let fractional_second_name = fractional.raw_get::<String>(2).unwrap();
    let fractional_suffix = shader_suffix(&fractional_name, "FRAC_");
    let fractional_second_suffix = shader_suffix(&fractional_second_name, "FRAC_");
    assert!(fractional_suffix > second_suffix);
    assert_eq!(fractional_second_suffix, fractional_suffix.wrapping_add(1));
    for name in ["negative_shaders", "nan_shaders", "infinite_shaders"] {
        assert_eq!(
            environment.get::<mlua::Table>(name).unwrap().raw_len(),
            0,
            "{name}"
        );
    }
    assert_eq!(environment.get::<i64>("destroy_shader_results").unwrap(), 0);
    assert!(environment.get::<bool>("destroy_shader_tag_fails").unwrap());
    assert!(
        environment
            .get::<bool>("destroy_shader_stringification_ok")
            .unwrap()
    );
    assert!(environment.get::<bool>("shader_base_tag_fails").unwrap());
    assert!(environment.get::<bool>("shader_count_tag_fails").unwrap());
    assert_eq!(
        environment.get::<String>("gamer_backend").unwrap(),
        "gamecenter"
    );
    assert!(environment.get::<bool>("gamer_supported").unwrap());
    assert!(!environment.get::<bool>("gamer_authenticated").unwrap());
    assert_eq!(environment.get::<i64>("achievement_results").unwrap(), 0);
    assert_eq!(environment.get::<i64>("score_results").unwrap(), 0);
    for name in [
        "analytics_timer_results",
        "analytics_event_results",
        "analytics_param_results",
        "analytics_params_results",
    ] {
        assert_eq!(environment.get::<i64>(name).unwrap(), 0, "{name}");
    }
    assert!(environment.get::<bool>("analytics_short_fails").unwrap());
    assert!(environment.get::<bool>("analytics_bad_type_fails").unwrap());
    assert!(environment.get::<bool>("registration_first").unwrap());
    assert!(!environment.get::<bool>("registration_second").unwrap());
    assert!(environment.get::<bool>("register_key_short_fails").unwrap());
    assert_eq!(environment.get::<i64>("register_key_results").unwrap(), 0);
    assert!(environment.get::<bool>("registration_after_first").unwrap());
    assert!(
        !environment
            .get::<bool>("registration_after_second")
            .unwrap()
    );
    assert_eq!(environment.get::<i64>("force_update_results").unwrap(), 0);
    assert!(
        environment
            .get::<bool>("force_update_missing_callback_fails")
            .unwrap()
    );
    assert!(
        environment
            .get::<bool>("force_update_bad_config_fails")
            .unwrap()
    );
    assert!(
        environment
            .get::<bool>("force_update_bad_callback_fails")
            .unwrap()
    );
    assert_eq!(
        environment
            .get::<i64>("force_update_store_results")
            .unwrap(),
        0
    );
    assert_eq!(
        runtime
            .render
            .lock()
            .unwrap()
            .requested_app_store_product
            .as_ref(),
        Some(&("875251011".to_owned(), 3))
    );
    assert_eq!(
        environment.get::<String>("installed_apps_offline").unwrap(),
        ""
    );
    assert_eq!(
        environment.get::<i64>("installed_offline_results").unwrap(),
        0
    );
    assert_eq!(
        environment.get::<i64>("installed_online_results").unwrap(),
        0
    );
    assert!(!environment.get::<bool>("twitter_supported").unwrap());
    assert!(!environment.get::<bool>("bad_achievement_fails").unwrap());
    assert!(
        !environment
            .get::<bool>("numeric_achievement_fails")
            .unwrap()
    );
    assert!(!environment.get::<bool>("bad_score_fails").unwrap());
    assert!(!environment.get::<bool>("numeric_score_name_fails").unwrap());
    assert!(!environment.get::<bool>("string_score_fails").unwrap());
    assert!(environment.get::<bool>("trailing_score_ok").unwrap());
    assert!(runtime.exit_requested());

    let second_runtime = StellaLua::new("/tmp").unwrap();
    second_runtime
        .execute_source(r#"cross_runtime_shader = createUniqueShaders("OTHER_", 1)[1]"#)
        .unwrap();
    let cross_runtime_name = game_environment(second_runtime.lua())
        .unwrap()
        .get::<String>("cross_runtime_shader")
        .unwrap();
    assert!(shader_suffix(&cross_runtime_name, "OTHER_") > fractional_second_suffix);

    let commands = runtime.take_render_commands();
    assert_eq!(commands.len(), 1);
    assert_eq!(commands[0].sprite, "BAND");
    let Some(SpriteGeometrySubmission::NativeAtlasQuad(native_quad)) =
        commands[0].geometry.as_ref()
    else {
        panic!("rubber band did not retain its native atlas quad");
    };
    assert_eq!(commands[0].x, native_quad[0][0] as f32);
    assert_eq!(commands[0].y, native_quad[0][1] as f32);
    assert_eq!(
        commands[0].state.matrix,
        Some([f32::from_bits(0xB2CC_DE2E), 10.0, 2.0, 0.0])
    );
}

#[test]
fn qr_scanner_and_app_store_launcher_follow_unsupported_device_branches() {
    let unique = NEXT_TEST_SPRITE_SHEET_ID.fetch_add(1, Ordering::Relaxed);
    let root =
        std::env::temp_dir().join(format!("stella-qr-store-{}-{unique}", std::process::id()));
    let data_root = root.join("data");
    let app_root = root.join("appdata");
    fs::create_dir_all(&data_root).unwrap();
    fs::create_dir_all(&app_root).unwrap();
    fs::write(
        app_root.join("promotion.json"),
        br#"{"launchId":"angrybirds-space","storeId":"123456789"}"#,
    )
    .unwrap();

    let runtime = StellaLua::new(&data_root).unwrap();
    runtime
        .execute_source(
            r##"
                qr_camera_supported = QrScanner.isCameraSupported("ignored")
                qr_front_supported = QrScanner.isFrontCameraSupported("ignored")
                qr_start_results = select("#", QrScanner.start("ignored"))
                qr_stop_results = select("#", QrScanner.stop("ignored"))
                qr_callback_results = select("#",
                    QrScanner.setQrRecognizedCallback(function() end, "ignored"))
                qr_clear_results = select("#",
                    QrScanner.setQrRecognizedCallback(nil, "ignored"))
                qr_non_function_clears = pcall(
                    QrScanner.setQrRecognizedCallback, "clear", "ignored")

                store_update_results = select("#",
                    AppStoreLauncher.updateGameData("promotion.json", "ignored"))
                store_update_missing_fails = not pcall(
                    AppStoreLauncher.updateGameData)
                store_update_tag_fails = not pcall(
                    AppStoreLauncher.updateGameData, false)
                store_launch_results = select("#",
                    AppStoreLauncher.launchAppStore("ignored"))
            "##,
        )
        .unwrap();

    let environment = game_environment(runtime.lua()).unwrap();
    assert!(!environment.get::<bool>("qr_camera_supported").unwrap());
    assert!(!environment.get::<bool>("qr_front_supported").unwrap());
    for field in [
        "qr_start_results",
        "qr_stop_results",
        "qr_callback_results",
        "qr_clear_results",
        "store_update_results",
        "store_launch_results",
    ] {
        assert_eq!(environment.get::<i64>(field).unwrap(), 0, "{field}");
    }
    assert!(environment.get::<bool>("qr_non_function_clears").unwrap());
    assert!(
        environment
            .get::<bool>("store_update_missing_fails")
            .unwrap()
    );
    assert!(environment.get::<bool>("store_update_tag_fails").unwrap());
    assert_eq!(
        runtime
            .render
            .lock()
            .unwrap()
            .requested_app_store_product
            .as_ref(),
        Some(&("123456789".to_owned(), 3))
    );

    drop(runtime);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn shipped_telepods_facade_observes_the_native_no_camera_branch() {
    let sandbox = ShippedDataSandbox::new("telepods-no-camera");
    let runtime = StellaLua::new(&sandbox.data_root).unwrap();
    runtime.boot("scripts/game.lua").unwrap();
    runtime
        .execute_source(
            r##"
                shipped_telepods_supported = Telepods.areSupported()
                shipped_telepods_front_camera = Telepods.hasFrontCamera()
            "##,
        )
        .unwrap();

    let environment = game_environment(runtime.lua()).unwrap();
    assert!(matches!(
        runtime.lua().globals().get::<Value>("QrScanner").unwrap(),
        Value::Table(_)
    ));
    assert!(
        !environment
            .get::<bool>("shipped_telepods_supported")
            .unwrap()
    );
    assert!(
        !environment
            .get::<bool>("shipped_telepods_front_camera")
            .unwrap()
    );
}

#[test]
fn shipped_telepods_ui_snapshots_a_preboot_scanner_capability() {
    let sandbox = ShippedDataSandbox::new("telepods-preboot-ui");
    let runtime = StellaLua::new(&sandbox.data_root).unwrap();
    runtime.set_qr_scanner_available(true).unwrap();
    runtime.boot("scripts/game.lua").unwrap();
    runtime
        .execute_source(
            r##"
                assert(Telepods.areSupported())
                assert(g_showTelepodButtons)

                telepodsUiProbe = ui.TelepodPage:new()
                for _, childName in ipairs({
                    "telepodScanAnimationForeground",
                    "telepodScanAnimationBackground",
                    "telepodsLogo",
                    "telepodPlacementText",
                    "btnBuyTelepods",
                    "btnx",
                    "tpscanbg",
                }) do
                    assert(telepodsUiProbe:getChild(childName), childName)
                end
                notificationsFrame:addChild(telepodsUiProbe)
                assert(ui.TelepodPage.isShown)
            "##,
        )
        .unwrap();

    for _ in 0..180 {
        runtime.update(1.0 / 60.0).unwrap();
        runtime.draw().unwrap();
    }
    let commands = runtime.take_render_commands();
    assert!(
        commands
            .iter()
            .any(|command| command.sprite == "TELEPOD_LOGO_ELECTRIC"),
        "the shipped scan page logo must be submitted"
    );
    assert!(
        commands
            .iter()
            .any(|command| command.sprite.starts_with("TELEPOD_SCAN_")),
        "the shipped background/foreground scan animations must be submitted"
    );
}

#[test]
fn shipped_telepods_in_game_and_reward_wheel_entries_use_the_preboot_capability() {
    let sandbox = ShippedDataSandbox::new("telepods-entry-points");
    let runtime = StellaLua::new(&sandbox.data_root).unwrap();
    runtime.set_qr_scanner_available(true).unwrap();
    runtime.boot("scripts/game.lua").unwrap();
    runtime.execute_source("initializeEventSystem()").unwrap();
    runtime
        .execute_source(
            r##"
                SpriteSheetManager.useGroupSet("INGAME")
                currentFolder = "Chapter02"
                currentPack = "Chapter02"
                currentLevel = 11
                levelFolder = "levels/Chapter02/"
                levelName = "Chapter02_L11"
                loadLevelInternal(levelFolder .. levelName)

                local hud = ui.GameHud:new()
                menuManager:getRoot():addChild(hud)
                local telepodButton = hud:getChild("extraBirdBar"):getChild("telepodButton")
                assert(telepodButton)
                assert(telepodButton.visible)
                assert(telepodButton.returnValue == "OPEN_TELEPOD")

                local rewardWheel = ui.ReSpinWheel:new()
                assert(rewardWheel:getChild("btn3").returnValue == "UNLOCK_SCAN_TELEPOD")
                assert(rewardWheel:getChild("telepodLogo"))
            "##,
        )
        .unwrap();
}

#[test]
fn shipped_telepods_page_consumes_a_code_queued_before_boot() {
    let sandbox = ShippedDataSandbox::new("telepods-preboot-redemption");
    let runtime = StellaLua::new(&sandbox.data_root).unwrap();
    runtime.set_qr_scanner_available(true).unwrap();
    assert!(!runtime.submit_qr_code("hasbro.telepod.020").unwrap());
    runtime.boot("scripts/game.lua").unwrap();
    runtime
        .execute_source(
            r##"
                assert(g_showTelepodButtons)

                -- TelepodPage's shipped success path only adds an extra bird
                -- outside Scrapbook/result screens. Keep this deterministic
                -- UI probe in its original Scrapbook branch.
                local root = menuManager:getRoot()
                local originalGetChild = root.getChild
                root.getChild = function(self, name)
                    if name == "Scrapbook" then
                        return true
                    end
                    return originalGetChild(self, name)
                end
                notificationsFrame:addChild(ui.TelepodPage:new())
                root.getChild = originalGetChild

                assert(SettingsWrapper:isBirdSkinUnlocked("Piano Willow"))
            "##,
        )
        .unwrap();

    for _ in 0..180 {
        runtime.update(1.0 / 60.0).unwrap();
    }
    runtime
        .execute_source("assert(not ui.TelepodPage.isShown)")
        .unwrap();
}

#[test]
fn shipped_telepods_page_adds_the_scanned_bird_during_live_gameplay() {
    let sandbox = ShippedDataSandbox::new("telepods-live-gameplay");
    let runtime = StellaLua::new(&sandbox.data_root).unwrap();
    runtime.set_qr_scanner_available(true).unwrap();
    assert!(!runtime.submit_qr_code("hasbro.telepod.020").unwrap());
    runtime.boot("scripts/game.lua").unwrap();
    runtime.execute_source("initializeEventSystem()").unwrap();
    runtime
        .execute_source(
            r##"
                SpriteSheetManager.useGroupSet("INGAME")
                currentFolder = "Chapter02"
                currentPack = "Chapter02"
                currentLevel = 11
                levelFolder = "levels/Chapter02/"
                levelName = "Chapter02_L11"
                loadLevelInternal(levelFolder .. levelName)
                blocks.BlockComponentManager.triggerGlobalEvent(blocks.events.EID_START)
            "##,
        )
        .unwrap();

    for _ in 0..60 {
        runtime.update(1.0 / 60.0).unwrap();
        runtime.draw().unwrap();
    }
    runtime
        .execute_source(
            r##"
                telepodBirdCountBefore = birdsCounter
                telepodBirdAddFinished = false
                telepodBirdAddListener = {
                    eventTriggered = function(self, event)
                        telepodBirdAddFinished = true
                        telepodBirdAdded = event.bird
                    end,
                }
                eventManager:addEventListener(
                    events.EID_TELEPOD_BIRD_ADD_FINISHED,
                    telepodBirdAddListener
                )
                notificationsFrame:addChild(ui.TelepodPage:new())
            "##,
        )
        .unwrap();

    for _ in 0..180 {
        runtime.update(1.0 / 60.0).unwrap();
        runtime.draw().unwrap();
    }
    runtime
        .execute_source(
            r##"
                assert(SettingsWrapper:isBirdSkinUnlocked("Piano Willow"))
                assert(telepodBirdAddFinished)
                assert(telepodBirdAdded == "Piano Willow")
                assert(birdsCounter == telepodBirdCountBefore + 1)
            "##,
        )
        .unwrap();
}

#[test]
fn shipped_telepods_scanner_and_iap_wallet_complete_all_configured_products() {
    let sandbox = ShippedDataSandbox::new("telepods-wallet");
    let runtime = StellaLua::new(&sandbox.data_root).unwrap();
    runtime.boot("scripts/game.lua").unwrap();

    assert!(!runtime.submit_qr_code("hasbro.telepod.020").unwrap());
    runtime.set_qr_scanner_available(true).unwrap();
    runtime
        .execute_source(
            r##"
                assert(IAP.isPaymentInitialized())
                assert(_G.IAP.native_isPaymentInitialized())
                assert(_G.IAP.native_useWalletValidation())
                assert(#_G.IAP.native_getAvailableItems() == 0)
                assert(_G.IAP.native_buyItem("retired.store.product") == "")
                assert(not pcall(_G.IAP.native_buyItem))
                assert(not pcall(_G.IAP.native_redeemCode, false))
                assert(Telepods.areSupported())
                assert(not Telepods.hasFrontCamera())

                scanner_recognized = nil
                scanner_product = nil
                scanner_status = nil
                QrScanner.start()
                QrScanner.setQrRecognizedCallback(function(code)
                    scanner_recognized = code
                    IAP.redeemCode(code, {
                        onPurchaseDone = function(self, product, status)
                            scanner_product = product
                            scanner_status = status
                        end,
                        onRedeemFailed = function(self, failedCode, status)
                            error(failedCode .. ":" .. status)
                        end,
                    })
                end)

                telepod_product_count = 0
                telepod_success_count = 0
                telepod_mapping_count = 0
                for characterName, configuration in pairs(g_telepodConfiguration) do
                    local product = configuration.productId
                    if product then
                        telepod_product_count = telepod_product_count + 1
                        local mappedName, mappedConfiguration =
                            getCharacterBasedOnProductId(product)
                        if mappedName == characterName and
                           mappedConfiguration == configuration then
                            telepod_mapping_count = telepod_mapping_count + 1
                        end
                        IAP.redeemCode(product, {
                            onPurchaseDone = function(self, delivered, status)
                                if delivered == product and
                                   status == IAP.PaymentStatus.PURCHASE_SUCCEEDED then
                                    telepod_success_count = telepod_success_count + 1
                                end
                            end,
                            onRedeemFailed = function(self, code, status)
                                error(code .. ":" .. status)
                            end,
                        })
                    end
                end

                unknown_code = nil
                unknown_status = nil
                IAP.redeemCode("not-a-shipped-telepod", {
                    onPurchaseDone = function()
                        error("unknown Telepod code was delivered")
                    end,
                    onRedeemFailed = function(self, code, status)
                        unknown_code = code
                        unknown_status = status
                    end,
                })
            "##,
        )
        .unwrap();

    let environment = game_environment(runtime.lua()).unwrap();
    assert_eq!(
        environment.get::<String>("scanner_recognized").unwrap(),
        "hasbro.telepod.020"
    );
    assert_eq!(
        environment.get::<String>("scanner_product").unwrap(),
        "hasbro.telepod.020"
    );
    assert_eq!(
        environment.get::<String>("scanner_status").unwrap(),
        "PURCHASE_SUCCEEDED"
    );
    assert_eq!(environment.get::<i64>("telepod_product_count").unwrap(), 24);
    assert_eq!(environment.get::<i64>("telepod_mapping_count").unwrap(), 24);
    assert_eq!(environment.get::<i64>("telepod_success_count").unwrap(), 24);
    assert_eq!(
        environment.get::<String>("unknown_code").unwrap(),
        "not-a-shipped-telepod"
    );
    assert_eq!(
        environment.get::<String>("unknown_status").unwrap(),
        "CODE_NOT_FOUND"
    );
}

#[test]
fn unsupported_zappar_completes_the_native_close_callback_immediately() {
    let runtime = StellaLua::new("/tmp").unwrap();
    runtime
        .execute_source(
            r##"
                zappar_supported = Zappar.native_isZapparSupported("ignored")
                zappar_close_count = 0
                zappar_launch_results = select("#", Zappar.native_launchZappar(
                    function()
                        zappar_close_count = zappar_close_count + 1
                    end,
                    "ignored tail"
                ))
                zappar_missing_callback_fails = not pcall(
                    Zappar.native_launchZappar
                )
                zappar_bad_callback_fails = not pcall(
                    Zappar.native_launchZappar, false
                )
            "##,
        )
        .unwrap();

    let environment = game_environment(runtime.lua()).unwrap();
    assert!(!environment.get::<bool>("zappar_supported").unwrap());
    assert_eq!(environment.get::<i64>("zappar_close_count").unwrap(), 1);
    assert_eq!(environment.get::<i64>("zappar_launch_results").unwrap(), 0);
    assert!(
        environment
            .get::<bool>("zappar_missing_callback_fails")
            .unwrap()
    );
    assert!(
        environment
            .get::<bool>("zappar_bad_callback_fails")
            .unwrap()
    );
}

#[test]
fn shipped_zappar_handler_restores_audio_after_the_unsupported_launch() {
    let data_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../runtime/data");
    let runtime = StellaLua::new(data_root).unwrap();
    runtime
        .execute_source(
            r##"
                zappar_trace = {}
                local function mark(value)
                    zappar_trace[#zappar_trace + 1] = value
                end
                isIOSVersion = function() return true end
                res = {
                    stopAllAudio = function() mark("stop-all") end,
                    startAudioOutput = function() mark("start-output") end,
                    stopAudioOutput = function() mark("stop-output") end,
                }
                soundManager = {
                    unloadStaticAudioAssets = function() mark("unload-static") end,
                    reloadStaticAudioAssets = function() mark("reload-static") end,
                }
                SettingsWrapper = {
                    audioEnabled = true,
                    isAudioEnabled = function(self) return self.audioEnabled end,
                    setAudioEnabled = function(self, enabled)
                        self.audioEnabled = enabled
                        mark("audio:" .. tostring(enabled))
                    end,
                }
                events = { EID_ZAPPAR_ENTERED = 901 }
                eventManager = {
                    notify = function(self, event)
                        mark("event:" .. tostring(event.id))
                    end,
                }
                setEffectsVolume = function(value)
                    mark("effects:" .. tostring(value))
                end
                setMusicVolume = function(value)
                    mark("music:" .. tostring(value))
                end
                toggleCurrentMusic = function(restart)
                    mark("toggle:" .. tostring(restart))
                end
                previousMusicName = "menu_music"
            "##,
        )
        .unwrap();
    let environment = game_environment(runtime.lua()).unwrap();
    runtime
        .lua()
        .globals()
        .set("res", environment.get::<mlua::Table>("res").unwrap())
        .unwrap();
    runtime.execute("scripts/ZapparHandler.lua").unwrap();
    runtime
        .execute_source(
            r##"
                ZapparHandler.launchZappar()
                zappar_trace_result = table.concat(zappar_trace, ",")
                zappar_audio_restored = SettingsWrapper.audioEnabled
                zappar_music_restored = previousMusicName
            "##,
        )
        .unwrap();

    let environment = game_environment(runtime.lua()).unwrap();
    assert_eq!(
        environment.get::<String>("zappar_trace_result").unwrap(),
        concat!(
            "stop-all,unload-static,audio:false,event:901,reload-static,audio:true,",
            "effects:1,music:1,start-output,toggle:true"
        )
    );
    assert!(environment.get::<bool>("zappar_audio_restored").unwrap());
    assert_eq!(
        environment.get::<String>("zappar_music_restored").unwrap(),
        "menu_music"
    );
}

#[test]
fn rovio_channel_preserves_the_seven_member_native_abi_before_service_enable() {
    let runtime = StellaLua::new("/tmp").unwrap();
    runtime
        .execute_source(
            r##"
                channel_available = RovioChannel.isAvailable("ignored")
                channel_opened = RovioChannel.isChannelViewOpened("ignored")
                channel_new_content = RovioChannel.numOfNewContent("ignored")
                channel_open_results = select("#", RovioChannel.openChannelView(
                    "Purple", "full", "en_EN", 1024, 768,
                    "content/videos", "map_screen", "ignored tail"
                ))
                channel_cancel_results = select(
                    "#", RovioChannel.cancelChannelViewLoading("ignored")
                )
                channel_update_results = select(
                    "#", RovioChannel.updateNewContent("ignored")
                )
                channel_menu_results = select(
                    "#", RovioChannel.onMenuInitialised("ignored")
                )
                channel_open_missing_fails = not pcall(
                    RovioChannel.openChannelView,
                    "Purple", "full", "en_EN", 1024, 768, "content/videos"
                )
                channel_open_string_fails = not pcall(
                    RovioChannel.openChannelView,
                    false, "full", "en_EN", 1024, 768, "", "map_screen"
                )
                channel_open_width_fails = not pcall(
                    RovioChannel.openChannelView,
                    "Purple", "full", "en_EN", false, 768, "", "map_screen"
                )
                channel_open_entry_fails = not pcall(
                    RovioChannel.openChannelView,
                    "Purple", "full", "en_EN", 1024, 768, "", false
                )
            "##,
        )
        .unwrap();

    let environment = game_environment(runtime.lua()).unwrap();
    assert!(!environment.get::<bool>("channel_available").unwrap());
    assert!(!environment.get::<bool>("channel_opened").unwrap());
    assert_eq!(environment.get::<f64>("channel_new_content").unwrap(), 0.0);
    for field in [
        "channel_open_results",
        "channel_cancel_results",
        "channel_update_results",
        "channel_menu_results",
    ] {
        assert_eq!(environment.get::<i64>(field).unwrap(), 0, "{field}");
    }
    for field in [
        "channel_open_missing_fails",
        "channel_open_string_fails",
        "channel_open_width_fails",
        "channel_open_entry_fails",
    ] {
        assert!(environment.get::<bool>(field).unwrap(), "{field}");
    }
}

#[test]
fn retired_channel_sprite_names_fall_back_to_bundled_toons_art() {
    let runtime = StellaLua::new("/tmp").unwrap();
    register_test_sprite_sheet_with_sizes(
        &runtime,
        &[
            ("BTN_PLAY_BG", 148, 146),
            ("ICON_TOONS", 109, 72),
            ("BTN_BG_SMALL", 67, 67),
            ("ICON_PUSH", 48, 48),
            ("ICON_TOONS_TV", 202, 189),
            ("ICON_X", 58, 51),
        ],
    );

    let resources = runtime.resource_runtime.lock().unwrap();
    for (alias, target) in [
        ("toonsBackgroundButton", "BTN_PLAY_BG"),
        ("BUTTON_TOONS_NORMAL", "ICON_TOONS"),
        ("BUTTON_TOONS_LOOKLEFT", "ICON_TOONS"),
        ("BUTTON_TOONS_LOOKRIGHT", "ICON_TOONS"),
        ("BUTTON_TOONS_BLINK", "ICON_TOONS"),
        ("BUTTON_TOONS_AMOUNT", "BTN_BG_SMALL"),
        ("BUTTON_TOONS_AMOUNT_SMALL", "BTN_BG_SMALL"),
        ("BUTTON_TOONS_AMOUNT_MEDIUM", "BTN_BG_SMALL"),
        ("BUTTON_TOONS_AMOUNT_LARGE", "BTN_BG_SMALL"),
        ("BUTTON_TOONS_AMOUNT_PLUS", "ICON_PUSH"),
        ("toonsBanner", "ICON_TOONS_TV"),
        ("CHANNEL_INTRO_CLOSE", "ICON_X"),
    ] {
        assert_eq!(
            resources.active_native_sprite_metrics(alias),
            resources.active_native_sprite_metrics(target),
            "{alias}"
        );
        assert!(
            resources
                .active_atlas_catalog_region(alias, runtime.data_root())
                .is_some(),
            "{alias}"
        );
    }
}

#[test]
fn social_manager_preserves_native_table_and_disconnected_abi() {
    let runtime = StellaLua::new("/tmp").unwrap();
    runtime
        .execute_source(
            r##"
                native_social_connected = _G.SocialManager.native_isConnectedToSocialNetwork()
                native_social_network = _G.SocialManager.native_getSocialNetworkName()
                native_social_local_id = _G.SocialManager.native_getLocalUserAccountId()
                native_social_friend_id = _G.SocialManager.native_getFriendAccountId("friend")
                native_social_friends = _G.SocialManager.native_getFriends()
                native_social_post_results = select("#",
                    _G.SocialManager.native_postScores("Level1", 42, "1"))
                native_social_fetch_results = select("#",
                    _G.SocialManager.native_fetchLeaderboard("Level1", "2"))
                native_social_bad_post = pcall(
                    _G.SocialManager.native_postScores, "Level1", {}, "1")
                native_social_bad_friend = pcall(
                    _G.SocialManager.native_getFriendAccountId, {})
                native_social_noarg_tail = pcall(function()
                    _G.SocialManager.native_connectToSocialNetwork({}, "ignored")
                    _G.SocialManager.native_getFriendsProgress({}, "ignored")
                    _G.SocialManager.native_unloadAllAvatars({}, "ignored")
                    _G.SocialManager.native_isConnectedToSocialNetwork({}, "ignored")
                    _G.SocialManager.native_getSocialNetworkName({}, "ignored")
                    _G.SocialManager.native_getLocalUserAccountId({}, "ignored")
                    _G.SocialManager.native_getFriends({}, "ignored")
                end)
                native_social_strict_tails = pcall(function()
                    _G.SocialManager.native_postScores("Level1", 42, "1", "ignored")
                    _G.SocialManager.native_fetchLeaderboard("Level1", "2", "ignored")
                    _G.SocialManager.native_setProgress("progress", "ignored")
                    _G.SocialManager.native_loadAvatar("friend", "ignored")
                    _G.SocialManager.native_unloadAvatar("friend", "ignored")
                    _G.SocialManager.native_getFriendAccountId("friend", "ignored")
                end)
                native_social_string_tags_strict =
                    not pcall(_G.SocialManager.native_postScores, 1, 42, "1") and
                    not pcall(_G.SocialManager.native_postScores, "Level1", 42, 1) and
                    not pcall(_G.SocialManager.native_fetchLeaderboard, 1, "2") and
                    not pcall(_G.SocialManager.native_fetchLeaderboard, "Level1", 2) and
                    not pcall(_G.SocialManager.native_setProgress, 1) and
                    not pcall(_G.SocialManager.native_loadAvatar, 1) and
                    not pcall(_G.SocialManager.native_unloadAvatar, 1)
                native_social_number_tag_strict = not pcall(
                    _G.SocialManager.native_postScores, "Level1", "42", "1")
                "##,
        )
        .unwrap();
    let environment = game_environment(runtime.lua()).unwrap();
    assert!(!environment.get::<bool>("native_social_connected").unwrap());
    assert_eq!(
        environment.get::<String>("native_social_network").unwrap(),
        "facebook"
    );
    assert_eq!(
        environment.get::<String>("native_social_local_id").unwrap(),
        ""
    );
    assert_eq!(
        environment
            .get::<String>("native_social_friend_id")
            .unwrap(),
        ""
    );
    assert_eq!(
        environment
            .get::<mlua::Table>("native_social_friends")
            .unwrap()
            .raw_len(),
        0
    );
    assert_eq!(
        environment
            .get::<i64>("native_social_post_results")
            .unwrap(),
        0
    );
    assert_eq!(
        environment
            .get::<i64>("native_social_fetch_results")
            .unwrap(),
        0
    );
    assert!(!environment.get::<bool>("native_social_bad_post").unwrap());
    assert!(!environment.get::<bool>("native_social_bad_friend").unwrap());
    for name in [
        "native_social_noarg_tail",
        "native_social_strict_tails",
        "native_social_string_tags_strict",
        "native_social_number_tag_strict",
    ] {
        assert!(environment.get::<bool>(name).unwrap(), "{name}");
    }
}

#[test]
fn skynest_native_account_and_storage_complete_retired_backend_calls_locally() {
    let runtime = StellaLua::new("/tmp").unwrap();
    runtime
        .execute_source(
            r##"
                local account = _G.SkynestAccount
                local storage = _G.SkynestStorage

                skynest_service_name = account.native_getServiceName("ignored")
                skynest_account_url = account.native_getAccountDetailsUrl("ignored")
                skynest_logged_in = account.native_isLoggedIn("ignored")
                skynest_login_in_progress = account.native_isLoginInProgress("ignored")
                skynest_has_nickname_before = account.native_hasNickname("ignored")

                skynest_validate_results = select("#", account.native_validateNickname(
                    "testplayer", function(...)
                        skynest_valid_count = select("#", ...)
                        skynest_valid_ok, skynest_valid_value = ...
                    end, "ignored"
                ))
                account.native_validateNickname("   ", function(...)
                    skynest_invalid_count = select("#", ...)
                    skynest_invalid_ok, skynest_invalid_value = ...
                end)

                storage.native_getKey("nickname", function(...)
                    skynest_missing_count = select("#", ...)
                    skynest_missing_value = ...
                end)
                skynest_set_results = select("#", storage.native_setKey(
                    "nickname", "testplayer", function(...)
                        skynest_set_callback_count = select("#", ...)
                    end, "ignored"
                ))
                skynest_has_nickname_after = account.native_hasNickname()
                storage.native_getKey("nickname", function(...)
                    skynest_found_count = select("#", ...)
                    skynest_found_value = ...
                end, "ignored")
                storage.native_getKeyForAccountIds(
                    "nickname", { "friend-a", "friend-b", false, "ignored" },
                    function(...)
                        skynest_batch_count = select("#", ...)
                        skynest_batch_value = ...
                    end, "ignored"
                )

                skynest_load_started = storage.native_loadCloudSettings("ignored")
                skynest_save_started = storage.native_saveCloudSettings({}, "ignored")
                skynest_transaction = storage.native_isTransactionInProcess("ignored")
                skynest_timeout_results = select("#",
                    storage.native_setRequestTimeout(12.9, "ignored"))

                account.onLoginFailure = function(...)
                    skynest_login_failure_count = select("#", ...)
                    skynest_login_failure_code, skynest_login_failure_message = ...
                end
                skynest_login_results = select("#",
                    account.native_login(true, false, true, "ignored"))
                skynest_login_in_progress_after =
                    account.native_isLoginInProgress()
                skynest_login_argument_tags_strict =
                    not pcall(account.native_login, 1, false, true) and
                    not pcall(account.native_login, true, 0, true) and
                    not pcall(account.native_login, true, false, "true")
                skynest_validate_argument_tags_strict =
                    not pcall(account.native_validateNickname, false, function() end) and
                    not pcall(account.native_validateNickname, "name", false)
                skynest_storage_argument_tags_strict =
                    not pcall(storage.native_saveCloudSettings, false) and
                    not pcall(storage.native_setRequestTimeout, "12") and
                    not pcall(storage.native_setKey, 1, "value", function() end) and
                    not pcall(storage.native_setKey, "key", 1, function() end) and
                    not pcall(storage.native_setKey, "key", "value", false) and
                    not pcall(storage.native_getKey, 1, function() end) and
                    not pcall(storage.native_getKey, "key", false) and
                    not pcall(storage.native_getKeyForAccountIds,
                        "key", false, function() end)
            "##,
        )
        .unwrap();

    let environment = game_environment(runtime.lua()).unwrap();
    assert_eq!(
        environment.get::<String>("skynest_service_name").unwrap(),
        "identityLevel2"
    );
    assert_eq!(
        environment.get::<String>("skynest_account_url").unwrap(),
        "https://account.rovio.com"
    );
    assert!(
        environment
            .get::<bool>("skynest_login_in_progress")
            .unwrap()
    );
    for name in [
        "skynest_logged_in",
        "skynest_login_in_progress_after",
        "skynest_load_started",
        "skynest_save_started",
        "skynest_transaction",
    ] {
        assert!(!environment.get::<bool>(name).unwrap(), "{name}");
    }
    // Purple exposes `nickname.empty()` under this inverted native name.
    assert!(
        environment
            .get::<bool>("skynest_has_nickname_before")
            .unwrap()
    );
    assert!(
        !environment
            .get::<bool>("skynest_has_nickname_after")
            .unwrap()
    );
    assert_eq!(
        environment.get::<i64>("skynest_validate_results").unwrap(),
        0
    );
    assert_eq!(environment.get::<i64>("skynest_valid_count").unwrap(), 2);
    assert!(environment.get::<bool>("skynest_valid_ok").unwrap());
    assert!(environment.get::<bool>("skynest_valid_value").unwrap());
    assert_eq!(environment.get::<i64>("skynest_invalid_count").unwrap(), 2);
    assert!(environment.get::<bool>("skynest_invalid_ok").unwrap());
    assert!(!environment.get::<bool>("skynest_invalid_value").unwrap());
    assert_eq!(environment.get::<i64>("skynest_missing_count").unwrap(), 0);
    assert!(matches!(
        environment.get::<Value>("skynest_missing_value").unwrap(),
        Value::Nil
    ));
    assert_eq!(environment.get::<i64>("skynest_set_results").unwrap(), 0);
    assert_eq!(
        environment
            .get::<i64>("skynest_set_callback_count")
            .unwrap(),
        0
    );
    assert_eq!(environment.get::<i64>("skynest_found_count").unwrap(), 1);
    assert_eq!(
        environment.get::<String>("skynest_found_value").unwrap(),
        "testplayer"
    );
    assert_eq!(environment.get::<i64>("skynest_batch_count").unwrap(), 1);
    assert_eq!(
        environment
            .get::<mlua::Table>("skynest_batch_value")
            .unwrap()
            .raw_len(),
        0
    );
    assert_eq!(
        environment.get::<i64>("skynest_timeout_results").unwrap(),
        0
    );
    assert_eq!(environment.get::<i64>("skynest_login_results").unwrap(), 0);
    assert_eq!(
        environment
            .get::<i64>("skynest_login_failure_count")
            .unwrap(),
        2
    );
    assert_eq!(
        environment
            .get::<String>("skynest_login_failure_code")
            .unwrap(),
        "ERROR_OTHER"
    );
    assert!(
        environment
            .get::<String>("skynest_login_failure_message")
            .unwrap()
            .contains("offline")
    );
    for name in [
        "skynest_login_argument_tags_strict",
        "skynest_validate_argument_tags_strict",
        "skynest_storage_argument_tags_strict",
    ] {
        assert!(environment.get::<bool>(name).unwrap(), "{name}");
    }
}

#[test]
fn rovio_ads_native_table_preserves_null_provider_abi() {
    let runtime = StellaLua::new("/tmp").unwrap();
    runtime
        .execute_source(
            r##"
                local ads = _G.RovioAds
                ads_refresh_results = select("#", ads.refresh("Banner", "ignored"))
                ads_add_results = select("#", ads.addPlacement("Banner", "ignored"))
                ads_geometry_results = select("#", ads.addPlacementWithGeometry(
                    "Banner", 1.9, 2.9, 3.9, 4.9, "ignored"
                ))
                ads_native_results = select("#",
                    ads.addPlacementNative("Native", "ignored"))
                ads_show_result = ads.show("Banner", "ignored")
                ads_hide_results = select("#", ads.hide("Banner", "ignored"))
                ads_click_results = select("#", ads.click("Banner", "ignored"))
                ads_conversion_results = select("#", ads.trackConversion("ignored"))
                ads_session_results = select("#", ads.startSession("ignored"))
                ads_tags_strict =
                    not pcall(ads.refresh, false) and
                    not pcall(ads.addPlacement, false) and
                    not pcall(ads.addPlacementWithGeometry,
                        "Banner", false, 2, 3, 4) and
                    not pcall(ads.addPlacementWithGeometry,
                        "Banner", 1, false, 3, 4) and
                    not pcall(ads.addPlacementWithGeometry,
                        "Banner", 1, 2, false, 4) and
                    not pcall(ads.addPlacementWithGeometry,
                        "Banner", 1, 2, 3, false) and
                    not pcall(ads.addPlacementNative, false) and
                    not pcall(ads.show, false) and
                    not pcall(ads.hide, false) and
                    not pcall(ads.click, false)
            "##,
        )
        .unwrap();
    let environment = game_environment(runtime.lua()).unwrap();
    for name in [
        "ads_refresh_results",
        "ads_add_results",
        "ads_geometry_results",
        "ads_native_results",
        "ads_hide_results",
        "ads_click_results",
        "ads_conversion_results",
        "ads_session_results",
    ] {
        assert_eq!(environment.get::<i64>(name).unwrap(), 0, "{name}");
    }
    assert!(!environment.get::<bool>("ads_show_result").unwrap());
    assert!(environment.get::<bool>("ads_tags_strict").unwrap());
}

#[test]
fn boot_announces_social_service_and_loads_shipped_facade() {
    let sandbox = ShippedDataSandbox::new("social-boot");
    let runtime = StellaLua::new(&sandbox.data_root).unwrap();
    runtime.boot("scripts/game.lua").unwrap();
    assert!(runtime.gamelogic_loaded());

    let environment = game_environment(runtime.lua()).unwrap();
    let wrapper = environment.raw_get::<mlua::Table>("SocialManager").unwrap();
    assert!(wrapper.get::<Function>("isConnected").is_ok());
    assert!(
        !wrapper
            .get::<Function>("isConnected")
            .unwrap()
            .call::<bool>(())
            .unwrap()
    );

    let native = runtime
        .lua()
        .globals()
        .raw_get::<mlua::Table>("SocialManager")
        .unwrap();
    assert_eq!(
        native
            .get::<Function>("native_getSocialNetworkName")
            .unwrap()
            .call::<String>(())
            .unwrap(),
        "facebook"
    );
    let cloud = environment.get::<mlua::Table>("RovioCloudManager").unwrap();
    assert!(
        cloud
            .get::<Function>("isServiceAvailable")
            .unwrap()
            .call::<bool>("social")
            .unwrap()
    );
}

#[test]
fn frame_delays_round_each_subtraction_like_purples_float32_lua_vm() {
    let sandbox = ShippedDataSandbox::new("frame-delay-float32");
    let runtime = StellaLua::new(&sandbox.data_root).unwrap();
    runtime.boot("scripts/game.lua").unwrap();
    runtime
        .execute_source(
            r#"
                gamelua.g_realDt = nil
                native_delay_probe = ui.Frame:new({})
                native_delay_probe_frames = {}
                native_delay_probe_frame = 0
                native_delay_probe:doDelayed(0.8, function()
                    table.insert(native_delay_probe_frames, native_delay_probe_frame)
                    native_delay_probe:doDelayed(0.5, function()
                        table.insert(native_delay_probe_frames, native_delay_probe_frame)
                        native_delay_probe:doDelayed(0.3, function()
                            table.insert(native_delay_probe_frames, native_delay_probe_frame)
                        end)
                    end)
                end)
                local displayDelta = 0.01666666753590107
                for frame = 1, 48 do
                    native_delay_probe_frame = frame
                    native_delay_probe:update(displayDelta, displayDelta)
                end
                native_delay_probe_after_48 = native_delay_probe.delayedCalls[1].timeLeft
                for frame = 49, 97 do
                    native_delay_probe_frame = frame
                    native_delay_probe:update(displayDelta, displayDelta)
                end

                native_tween_probe = TweenSubsystem:add({
                    start = 1,
                    change = -1,
                    duration = 1,
                    func = tweenEaseCubicInOut
                })
                for _ = 1, 60 do
                    native_tween_probe:update(displayDelta)
                end
                native_tween_timer_after_60 = native_tween_probe.timer
                native_tween_done_after_60 = native_tween_probe.done
                native_tween_probe:update(displayDelta)
                native_tween_done_after_61 = native_tween_probe.done
                native_cubic_probe = tweenEaseCubicInOut(0.4, 1, -1, 1)
                native_tween_curve_probes = {
                    tweenLinear("0.4", "1", "-1", "1"),
                    tweenEaseCubicIn(0.4, 1, -1, 1),
                    tweenEaseCubicOut(0.4, 1, -1, 1),
                    tweenEaseCubicInOut(0.4, 1, -1, 1),
                    tweenEaseQuadraticIn(0.4, 1, -1, 1),
                    tweenEaseQuadraticOut(0.4, 1, -1, 1),
                    tweenEaseSineIn(0.4, 1, -1, 1),
                    tweenEaseSineOut(0.4, 1, -1, 1),
                    tweenEaseSineInOut(0.4, 1, -1, 1),
                    tweenEaseBounceOut(0.2, 1, -1, 1),
                    tweenEaseBounceOut(0.5, 1, -1, 1),
                    tweenEaseBounceOut(0.8, 1, -1, 1),
                    tweenEaseBounceOut(0.95, 1, -1, 1)
                }
                native_tween_namespace_matches =
                    gamelua.tweenLinear == tweenLinear and
                    gamelua.tweenEaseCubicInOut == tweenEaseCubicInOut and
                    gamelua.tweenEaseSineInOut == tweenEaseSineInOut
            "#,
        )
        .unwrap();

    let environment = game_environment(runtime.lua()).unwrap();
    let callback_frames = environment
        .get::<mlua::Table>("native_delay_probe_frames")
        .unwrap();
    assert_eq!(callback_frames.raw_get::<i64>(1).unwrap(), 49);
    assert_eq!(callback_frames.raw_get::<i64>(2).unwrap(), 79);
    assert_eq!(callback_frames.raw_get::<i64>(3).unwrap(), 97);
    assert_eq!(
        environment
            .get::<f64>("native_delay_probe_after_48")
            .unwrap(),
        f64::from(f32::from_bits(0x33d8_0000))
    );
    assert_eq!(
        environment
            .get::<f64>("native_tween_timer_after_60")
            .unwrap(),
        f64::from(f32::from_bits(0x3f7f_fffb))
    );
    assert!(
        !environment
            .get::<bool>("native_tween_done_after_60")
            .unwrap()
    );
    assert!(
        environment
            .get::<bool>("native_tween_done_after_61")
            .unwrap()
    );
    assert_eq!(
        environment.get::<f64>("native_cubic_probe").unwrap(),
        f64::from(f32::from_bits(0x3f3e_76c8))
    );
    let probes = environment
        .get::<mlua::Table>("native_tween_curve_probes")
        .unwrap();
    assert!(
        environment
            .get::<bool>("native_tween_namespace_matches")
            .unwrap()
    );
    for (index, bits) in [
        0x3f19_999a,
        0x3f6f_9db2,
        0x3e5d_2f1c,
        0x3f3e_76c8,
        0x3f57_0a3d,
        0x3eb8_51ec,
        0x3f4f_1bbd,
        0x3ed3_0dd0,
        0x3f27_8dde,
        0x3f32_8f5c,
        0x3e70_0000,
        0x3d75_c290,
        0x3c7d_70c0,
    ]
    .into_iter()
    .enumerate()
    {
        assert_eq!(
            probes.raw_get::<f64>(index + 1).unwrap(),
            f64::from(f32::from_bits(bits)),
            "tween curve probe {}",
            index + 1
        );
    }
}

#[test]
fn gamelogic_loader_invokes_update_values_then_publishes_loaded_state() {
    let sandbox = ShippedDataSandbox::new("gamelogic-load-completion");
    let runtime = StellaLua::new(&sandbox.data_root).unwrap();
    assert!(!runtime.gamelogic_loaded());
    runtime.execute("scripts_common/gamelogic.lua").unwrap();
    runtime
        .execute_source(
            r#"
                gamelogic_update_probe_calls = 0
                local shippedUpdateValues = updateValues
                updateValues = function()
                    gamelogic_update_probe_calls = gamelogic_update_probe_calls + 1
                    shippedUpdateValues()
                end
            "#,
        )
        .unwrap();
    runtime.finish_gamelogic_load().unwrap();

    assert!(runtime.gamelogic_loaded());
    let environment = game_environment(runtime.lua()).unwrap();
    assert_eq!(
        environment
            .get::<u32>("gamelogic_update_probe_calls")
            .unwrap(),
        1
    );
    assert_eq!(
        environment
            .get::<i64>("birdCollisionSoundForceThreshold")
            .unwrap(),
        40
    );
    assert_eq!(
        environment
            .get::<i64>("hardLimitSimultaneousParticles")
            .unwrap(),
        150
    );
}

#[test]
fn failed_gamelogic_update_values_does_not_publish_loaded_state() {
    let runtime = StellaLua::new("/tmp").unwrap();
    runtime
        .execute_source(
            r#"
                updateValues = function()
                    error("recovered load completion failure")
                end
            "#,
        )
        .unwrap();

    let error = runtime.finish_gamelogic_load().unwrap_err();
    assert!(
        error
            .to_string()
            .contains("recovered load completion failure")
    );
    assert!(!runtime.gamelogic_loaded());
}

#[test]
fn boot_repairs_the_shipped_challenge_result_edge_strip_layout() {
    let sandbox = ShippedDataSandbox::new("challenge-result-layout");
    let runtime = StellaLua::new(&sandbox.data_root).unwrap();
    runtime.boot("scripts/game.lua").unwrap();
    runtime
        .execute_source(
            r##"
                local left = {
                    scaleY = 0.75,
                    setNonUniformScale = function(self, x, y)
                        self.scaleX = x
                        self.scaleY = y
                    end
                }
                local right = {
                    scaleY = 0.5,
                    setNonUniformScale = function(self, x, y)
                        self.scaleX = x
                        self.scaleY = y
                    end
                }
                local probe = {
                    children = {},
                    bgStripLeft = left,
                    bgStripRight = right,
                    _doLayout = function() end,
                    getChild = function(self, name)
                        return self[name]
                    end
                }
                ui.FrenemiesChallengeLevelCompleted.layout(probe)
                challenge_result_left_scale_x = left.scaleX
                challenge_result_left_scale_y = left.scaleY
                challenge_result_right_scale_x = right.scaleX
                challenge_result_right_scale_y = right.scaleY
            "##,
        )
        .unwrap();

    let environment = game_environment(runtime.lua()).unwrap();
    assert_eq!(
        environment
            .get::<f64>("challenge_result_left_scale_x")
            .unwrap(),
        1_000.0
    );
    assert_eq!(
        environment
            .get::<f64>("challenge_result_left_scale_y")
            .unwrap(),
        0.5
    );
    assert_eq!(
        environment
            .get::<f64>("challenge_result_right_scale_x")
            .unwrap(),
        1_000.0
    );
    assert_eq!(
        environment
            .get::<f64>("challenge_result_right_scale_y")
            .unwrap(),
        0.5
    );
}

#[test]
fn offline_challenge_replay_preserves_the_shipped_async_result_route() {
    let sandbox = ShippedDataSandbox::new("offline-challenge-replay");
    let runtime = StellaLua::new(&sandbox.data_root).unwrap();
    runtime.boot("scripts/game.lua").unwrap();
    runtime
        .execute_source(
            r##"
                initDelayedCallbacks()
                PlayerState:setCurrentChallenge({
                    competitionId = "competition-probe",
                    levelId = "2352589225",
                    players = {{
                        currentPlayer = true,
                        score = 17,
                        stars = 1
                    }}
                })
                challenge_replay_called = false
                GameServerConnection.replayCompetitionLevel(
                    "competition-probe",
                    function(payload)
                        challenge_replay_called = true
                        challenge_replay_level_id = payload.levelId
                        challenge_replay_player_count = #payload.players
                        challenge_replay_current = payload.players[1].currentPlayer
                        challenge_replay_session = payload.players[1].gameSessionId
                    end,
                    function()
                        challenge_replay_failed = true
                    end
                )
            "##,
        )
        .unwrap();

    let environment = game_environment(runtime.lua()).unwrap();
    assert!(!environment.get::<bool>("challenge_replay_called").unwrap());
    runtime
        .execute_source("updateDelayedCallbacks(1 / 60, 1 / 60)")
        .unwrap();
    runtime
        .execute_source("updateDelayedCallbacks(1 / 60, 1 / 60)")
        .unwrap();
    assert!(environment.get::<bool>("challenge_replay_called").unwrap());
    assert_eq!(
        environment
            .get::<String>("challenge_replay_level_id")
            .unwrap(),
        "2352589225"
    );
    assert_eq!(
        environment
            .get::<i64>("challenge_replay_player_count")
            .unwrap(),
        1
    );
    assert!(environment.get::<bool>("challenge_replay_current").unwrap());
    assert!(
        environment
            .get::<String>("challenge_replay_session")
            .unwrap()
            .starts_with("offline-session-")
    );
    assert!(matches!(
        environment.get::<Value>("challenge_replay_failed").unwrap(),
        Value::Nil
    ));
}

#[test]
fn challenge_result_restart_reaches_the_original_start_challenge_callback() {
    let sandbox = ShippedDataSandbox::new("challenge-result-restart");
    let runtime = StellaLua::new(&sandbox.data_root).unwrap();
    runtime.boot("scripts/game.lua").unwrap();
    runtime
        .execute_source(
            r##"
                initDelayedCallbacks()
                PlayerState:setCurrentChallenge({
                    competitionId = "competition-result-probe",
                    levelId = "987654321",
                    players = {{ currentPlayer = true, score = 23 }}
                })
                local originalPointerEvent = ui.ScalableLayout.onPointerEvent
                local originalGetLevelMetadata = getLevelMetadata
                local originalStartChallengeLevel = IslandEventManager.startChallengeLevel
                ui.ScalableLayout.onPointerEvent = function()
                    return "RESTART"
                end
                getLevelMetadata = function()
                    return { islandEvent = { levels = {{}} } }
                end
                IslandEventManager.startChallengeLevel = function(self, event, payload)
                    challenge_result_restart_called = true
                    challenge_result_restart_event = event
                    challenge_result_restart_level_id = payload.levelId
                end
                ui.FrenemiesChallengeLevelCompleted.onPointerEvent({}, 0, 0, 0)
                ui.ScalableLayout.onPointerEvent = originalPointerEvent
                challenge_result_restart_restore = function()
                    getLevelMetadata = originalGetLevelMetadata
                    IslandEventManager.startChallengeLevel = originalStartChallengeLevel
                end
            "##,
        )
        .unwrap();

    let environment = game_environment(runtime.lua()).unwrap();
    assert!(matches!(
        environment
            .get::<Value>("challenge_result_restart_called")
            .unwrap(),
        Value::Nil
    ));
    runtime
        .execute_source("updateDelayedCallbacks(1 / 60, 1 / 60)")
        .unwrap();
    runtime
        .execute_source("updateDelayedCallbacks(1 / 60, 1 / 60)")
        .unwrap();
    assert!(
        environment
            .get::<bool>("challenge_result_restart_called")
            .unwrap()
    );
    assert_eq!(
        environment
            .get::<String>("challenge_result_restart_level_id")
            .unwrap(),
        "987654321"
    );
    let event = environment
        .get::<mlua::Table>("challenge_result_restart_event")
        .unwrap();
    assert_eq!(event.get::<mlua::Table>("levels").unwrap().raw_len(), 1);
    environment
        .get::<Function>("challenge_result_restart_restore")
        .unwrap()
        .call::<()>(())
        .unwrap();
}

#[test]
fn shipped_settings_popup_preserves_ios_gamer_service_state_and_base_tween() {
    let sandbox = ShippedDataSandbox::new("settings-popup");
    let runtime = StellaLua::new_with_resolution(&sandbox.data_root, 1429, 768).unwrap();
    runtime.boot("scripts/game.lua").unwrap();
    runtime
        .execute_source(
            r##"
                settings_probe = ui.SettingsPopup:new()
                settings_probe.skipAnimation = false
                settings_probe:onEntry()

                local services = SubsystemManager.get("FusionGamerServices")
                settings_backend = services:getBackendName()
                settings_available = services:isAvailable()
                settings_authenticated = services:isPlayerAuthenticated()

                local achievements = settings_probe:getChild("btnAchievements")
                local leaderboards = settings_probe:getChild("btnLeaderboards")
                settings_achievements_visible = achievements.visible
                settings_achievements_enabled = achievements.enabled
                settings_achievements_image = achievements.image
                settings_leaderboards_visible = leaderboards.visible
                settings_leaderboards_enabled = leaderboards.enabled
                settings_leaderboards_image = leaderboards.image
            "##,
        )
        .unwrap();

    let environment = game_environment(runtime.lua()).unwrap();
    assert_eq!(
        environment.get::<String>("settings_backend").unwrap(),
        "gamecenter"
    );
    assert!(environment.get::<bool>("settings_available").unwrap());
    assert!(!environment.get::<bool>("settings_authenticated").unwrap());
    assert!(
        environment
            .get::<bool>("settings_achievements_visible")
            .unwrap()
    );
    assert!(
        !environment
            .get::<bool>("settings_achievements_enabled")
            .unwrap()
    );
    assert_eq!(
        environment
            .get::<String>("settings_achievements_image")
            .unwrap(),
        "BTN_TROPHY_GREY"
    );
    assert!(
        environment
            .get::<bool>("settings_leaderboards_visible")
            .unwrap()
    );
    assert!(
        !environment
            .get::<bool>("settings_leaderboards_enabled")
            .unwrap()
    );
    assert_eq!(
        environment
            .get::<String>("settings_leaderboards_image")
            .unwrap(),
        "BTN_WINNERS_GREY"
    );

    let popup = environment.get::<mlua::Table>("settings_probe").unwrap();
    assert!(popup.get::<bool>("opening").unwrap());
    assert_eq!(popup.get::<f64>("bgDarkening").unwrap(), 0.0);

    for _ in 0..21 {
        runtime.update(1.0 / 60.0).unwrap();
    }
    assert!(popup.get::<bool>("opening").unwrap());
    assert!((popup.get::<f64>("bgDarkening").unwrap() - 0.5).abs() < 1.0e-5);
    assert!(popup.get::<f64>("y").unwrap().abs() < 1.0e-3);
    assert!((popup.get::<f64>("scaleX").unwrap() - 0.975).abs() < 1.0e-5);
    assert!((popup.get::<f64>("scaleY").unwrap() - 1.1).abs() < 1.0e-5);

    for _ in 0..21 {
        runtime.update(1.0 / 60.0).unwrap();
    }
    assert!(!popup.get::<bool>("opening").unwrap());
    assert!(popup.get::<bool>("visible").unwrap());
    assert!((popup.get::<f64>("bgDarkening").unwrap() - 1.0).abs() < 1.0e-5);
    assert!(popup.get::<f64>("y").unwrap().abs() < 1.0e-5);
    assert!((popup.get::<f64>("scaleX").unwrap() - 1.0).abs() < 1.0e-5);
    assert!((popup.get::<f64>("scaleY").unwrap() - 1.0).abs() < 1.0e-5);

    runtime
        .execute_source(
            r##"
                settings_removed = false
                settings_probe.parent = {
                    removeChild = function(_, child)
                        settings_removed = child == settings_probe
                    end
                }
                settings_probe:close()
            "##,
        )
        .unwrap();
    for _ in 0..12 {
        runtime.update(1.0 / 60.0).unwrap();
    }
    assert!(popup.get::<bool>("closing").unwrap());
    assert!((popup.get::<f64>("bgDarkening").unwrap() - 0.875).abs() < 1.0e-5);
    assert!((popup.get::<f64>("x").unwrap() - 35.725).abs() < 1.0e-3);
    assert!((popup.get::<f64>("y").unwrap() + 144.0).abs() < 1.0e-3);
    assert!((popup.get::<f64>("scaleX").unwrap() - 0.95).abs() < 1.0e-5);
    assert!((popup.get::<f64>("scaleY").unwrap() - 1.05).abs() < 1.0e-5);
    assert!(!environment.get::<bool>("settings_removed").unwrap());

    for _ in 0..13 {
        runtime.update(1.0 / 60.0).unwrap();
    }
    assert!(environment.get::<bool>("settings_removed").unwrap());
    assert!(popup.get::<f64>("bgDarkening").unwrap().abs() < 1.0e-5);
    assert!((popup.get::<f64>("y").unwrap() + 1152.0).abs() < 1.0e-3);
    assert!((popup.get::<f64>("scaleX").unwrap() - 0.6).abs() < 1.0e-5);
    assert!((popup.get::<f64>("scaleY").unwrap() - 1.4).abs() < 1.0e-5);
}

#[test]
fn native_resolution_is_published_before_boot_and_updates_through_script_callback() {
    let sandbox = ShippedDataSandbox::new("native-resolution");
    let runtime = StellaLua::new_with_resolution(&sandbox.data_root, 2009, 1080).unwrap();
    assert_eq!(
        runtime.lua().globals().get::<u32>("screenWidth").unwrap(),
        2009
    );
    assert_eq!(
        runtime.lua().globals().get::<u32>("screenHeight").unwrap(),
        1080
    );
    let resources = runtime.lua().globals().get::<mlua::Table>("res").unwrap();
    assert_eq!(
        resources
            .get::<Function>("getClipRect")
            .unwrap()
            .call::<(f64, f64, f64, f64)>(())
            .unwrap(),
        (0.0, 0.0, 2009.0, 1080.0)
    );
    assert_eq!(
        runtime
            .lua()
            .globals()
            .get::<u32>("g_startingResolutionWidth")
            .unwrap(),
        2009
    );

    runtime.boot("scripts/game.lua").unwrap();
    assert!(runtime.set_screen_resolution(1429, 768).unwrap());
    assert!(!runtime.set_screen_resolution(1429, 768).unwrap());
    assert_eq!(
        resources
            .get::<Function>("getClipRect")
            .unwrap()
            .call::<(f64, f64, f64, f64)>(())
            .unwrap(),
        (0.0, 0.0, 1429.0, 768.0)
    );
    let environment = game_environment(runtime.lua()).unwrap();
    let screen = environment.get::<mlua::Table>("screen").unwrap();
    assert_eq!(screen.get::<f64>("x").unwrap(), 714.5);
    assert_eq!(screen.get::<f64>("y").unwrap(), 384.0);
    assert_eq!(screen.get::<f64>("right").unwrap(), 1429.0);
    assert_eq!(screen.get::<f64>("bottom").unwrap(), 768.0);
    assert_eq!(
        runtime
            .lua()
            .globals()
            .get::<u32>("g_startingResolutionWidth")
            .unwrap(),
        2009
    );
}

#[test]
fn native_device_orientation_preserves_context_enum_mapping_and_host_rotation() {
    let runtime = StellaLua::new_with_resolution("/tmp", 1024, 768).unwrap();
    let orientation = runtime
        .lua()
        .globals()
        .get::<Function>("native_getDeviceOrientation")
        .unwrap();
    assert_eq!(orientation.call::<i32>(()).unwrap(), 90);

    for (native_index, degrees) in [(0, 0), (1, 90), (2, 180), (3, 270), (4, -1)] {
        runtime.render.lock().unwrap().device_orientation_index = native_index;
        assert_eq!(orientation.call::<i32>(()).unwrap(), degrees);
    }

    assert!(runtime.set_screen_resolution(768, 1024).unwrap());
    assert_eq!(orientation.call::<i32>(()).unwrap(), 0);
    assert!(runtime.set_screen_resolution(1024, 768).unwrap());
    assert_eq!(orientation.call::<i32>(()).unwrap(), 90);
}

#[test]
fn resolution_change_latches_old_corrected_camera_scale_before_callback() {
    let runtime = StellaLua::new_with_resolution("/tmp", 1024, 768).unwrap();
    runtime
        .execute_source(
            r#"
                gameCamera = {
                    endCameraIndex = 2.9,
                    resolutionCorrectedCameras = {
                        [2] = { sx = 3.25 }
                    }
                }
                resolutionChanged = function()
                    gameCamera.resolutionCorrectedCameras[2].sx = 9.5
                end
            "#,
        )
        .unwrap();

    assert!(runtime.set_screen_resolution(1280, 720).unwrap());
    let bridge = runtime.render.lock().unwrap();
    assert_eq!((bridge.screen_width, bridge.screen_height), (1280, 720));
    assert_eq!(bridge.resolution_camera_scale, 3.25_f32);
}

#[test]
fn startup_device_info_model_comes_from_the_native_platform_query() {
    let runtime = StellaLua::new("/tmp").unwrap();
    assert_eq!(
        runtime
            .lua()
            .globals()
            .get::<String>("deviceInfoModel")
            .unwrap(),
        native_device_info_model()
    );
}

#[test]
fn safe_to_quit_is_lua_truthy_and_latched_before_script_update() {
    let runtime = StellaLua::new("/tmp").unwrap();
    assert!(
        !runtime
            .lua()
            .globals()
            .get::<bool>("g_mouseAvailable")
            .unwrap()
    );
    runtime
        .execute_source(
            r#"
                g_safeToQuit = 0
                update = function()
                    g_safeToQuit = false
                end
            "#,
        )
        .unwrap();

    runtime.update(1.0 / 60.0).unwrap();
    assert!(runtime.safe_to_quit());
    runtime.update(1.0 / 60.0).unwrap();
    assert!(!runtime.safe_to_quit());
}

#[test]
fn safe_to_quit_is_latched_before_the_zoom_callback() {
    let _pinch_guard = lock_native_pinch_for_test();
    let runtime = StellaLua::new("/tmp").unwrap();
    runtime
        .execute_source(
            r#"
                g_safeToQuit = false
                applyUserZoom = function()
                    g_safeToQuit = true
                end
                setWorldScale(2)
            "#,
        )
        .unwrap();

    runtime.set_touches(&[(1, 0, 0), (2, 3, 4)]).unwrap();
    runtime.update(0.0).unwrap();
    runtime.set_touches(&[(1, 0, 0), (2, 6, 8)]).unwrap();
    runtime.update(0.0).unwrap();
    // applyUserZoom has already made the Lua value true, but the native byte
    // was captured at 0x10005EAC8..0x10005EB14 before that callback.
    assert!(
        game_environment(runtime.lua())
            .unwrap()
            .get::<bool>("g_safeToQuit")
            .unwrap()
    );
    assert!(!runtime.safe_to_quit());

    runtime.update(0.0).unwrap();
    assert!(runtime.safe_to_quit());
}

#[test]
fn drawable_resize_preserves_an_explicit_resource_clip() {
    let runtime = StellaLua::new_with_resolution("/tmp", 1024, 768).unwrap();
    runtime
        .execute_source("res.setClipRect(10, 20, 30, 40)")
        .unwrap();
    assert!(runtime.set_screen_resolution(1429, 768).unwrap());
    let resources = runtime.lua().globals().get::<mlua::Table>("res").unwrap();
    assert_eq!(
        resources
            .get::<Function>("getClipRect")
            .unwrap()
            .call::<(f64, f64, f64, f64)>(())
            .unwrap(),
        (10.0, 20.0, 30.0, 40.0)
    );
}

#[test]
fn downloadable_assets_match_native_load_callbacks_and_sheet_abi() {
    let unique = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let root = std::env::temp_dir().join(format!("stella-assets-{unique}"));
    let data_root = root.join("data");
    let app_root = root.join("appdata");
    fs::create_dir_all(data_root.join("config")).unwrap();
    fs::create_dir_all(app_root.join("assets_service")).unwrap();
    fs::write(
        app_root.join("cached.json"),
        r#"{
            "meta":{"app":"Adobe Animate"},
            "frames":[{
                "filename":"DYNAMIC_SPRITE",
                "frame":{"x":0,"y":0,"w":4,"h":6},
                "pivot":{"x":1,"y":2}
            }]
        }"#,
    )
    .unwrap();
    fs::write(app_root.join("invalid.json"), b"not-json").unwrap();

    let runtime = StellaLua::new(&data_root).unwrap();
    runtime
        .execute_source(
            r##"
                native_asset_helpers_absent =
                    Assets.getAssetFilename == nil and
                    Assets.haveBeenDownloaded == nil
                Assets.onLoadSuccess = function(files)
                    load_success = files["cached.json"]
                end
                Assets.onLoadError = function(files, code, message)
                    load_failure = files[1]
                    load_failure_code = code
                    load_failure_message = message
                end
                load_success_results = select("#",
                    Assets.loadFiles({ [4] = "cached.json" }))
                trailing_load_ok = pcall(
                    Assets.loadFiles, { [4] = "cached.json" }, false
                )
                missing_load_table_fails = not pcall(Assets.loadFiles)
                non_table_load_fails = not pcall(Assets.loadFiles, false)
                numeric_load_value_fails = not pcall(
                    Assets.loadFiles, { [4] = 123 }
                )
                load_error_results = select("#",
                    Assets.loadFiles({ "missing.dat" }))
                sheet_results = select("#",
                    Assets.createSpriteSheet("DYNAMIC", "cached.json", "cached.pvr"))
                dynamic_width, dynamic_height = res.getSpriteBounds("DYNAMIC_SPRITE")
                invalid_sheet_fails = not pcall(
                    Assets.createSpriteSheet,
                    "DYNAMIC", "invalid.json", "invalid.pvr"
                )
                dynamic_width_after_failure, dynamic_height_after_failure =
                    res.getSpriteBounds("DYNAMIC_SPRITE")
                bad_sheet_fails = pcall(
                    Assets.createSpriteSheet, "DYNAMIC", "cached.json")
                numeric_sheet_name_fails = pcall(
                    Assets.createSpriteSheet, 1, "cached.json", "cached.pvr")
                numeric_sheet_descriptor_fails = pcall(
                    Assets.createSpriteSheet, "DYNAMIC", 2, "cached.pvr")
                numeric_sheet_texture_fails = pcall(
                    Assets.createSpriteSheet, "DYNAMIC", "cached.json", 3)
                trailing_sheet_ok = pcall(
                    Assets.createSpriteSheet,
                    "DYNAMIC", "cached.json", "cached.pvr", false)
                "##,
        )
        .unwrap();
    let environment = game_environment(runtime.lua()).unwrap();
    assert!(
        environment
            .get::<bool>("native_asset_helpers_absent")
            .unwrap()
    );
    assert_eq!(environment.get::<i64>("load_success_results").unwrap(), 0);
    assert!(environment.get::<bool>("trailing_load_ok").unwrap());
    assert!(environment.get::<bool>("missing_load_table_fails").unwrap());
    assert!(environment.get::<bool>("non_table_load_fails").unwrap());
    assert!(environment.get::<bool>("numeric_load_value_fails").unwrap());
    assert_eq!(
        environment.get::<String>("load_success").unwrap(),
        "cached.json"
    );
    assert_eq!(environment.get::<i64>("load_error_results").unwrap(), 0);
    assert_eq!(
        environment.get::<String>("load_failure").unwrap(),
        "missing.dat"
    );
    assert_eq!(environment.get::<i32>("load_failure_code").unwrap(), 1);
    assert_eq!(
        environment.get::<String>("load_failure_message").unwrap(),
        "offline asset unavailable"
    );
    assert_eq!(environment.get::<i64>("sheet_results").unwrap(), 0);
    assert_eq!(environment.get::<f64>("dynamic_width").unwrap(), 4.0);
    assert_eq!(environment.get::<f64>("dynamic_height").unwrap(), 6.0);
    assert!(environment.get::<bool>("invalid_sheet_fails").unwrap());
    assert_eq!(
        environment
            .get::<f64>("dynamic_width_after_failure")
            .unwrap(),
        4.0
    );
    assert_eq!(
        environment
            .get::<f64>("dynamic_height_after_failure")
            .unwrap(),
        6.0
    );
    assert!(!environment.get::<bool>("bad_sheet_fails").unwrap());
    assert!(!environment.get::<bool>("numeric_sheet_name_fails").unwrap());
    assert!(
        !environment
            .get::<bool>("numeric_sheet_descriptor_fails")
            .unwrap()
    );
    assert!(
        !environment
            .get::<bool>("numeric_sheet_texture_fails")
            .unwrap()
    );
    assert!(environment.get::<bool>("trailing_sheet_ok").unwrap());
    assert!(
        runtime
            .resource_runtime
            .lock()
            .unwrap()
            .sprite_sheets
            .contains("DYNAMIC")
    );

    fs::remove_dir_all(root).unwrap();
}

#[test]
fn boot_announces_all_nine_native_cloud_services_before_menu_updates() {
    let sandbox = ShippedDataSandbox::new("cloud-service-announcement");
    let runtime = StellaLua::new(&sandbox.data_root).unwrap();
    let native_assets = runtime
        .lua()
        .globals()
        .get::<mlua::Table>("Assets")
        .unwrap();
    assert!(matches!(
        native_assets.get::<Value>("haveBeenDownloaded").unwrap(),
        Value::Nil
    ));

    runtime.boot("scripts/game.lua").unwrap();
    let booted_assets = game_environment(runtime.lua())
        .unwrap()
        .get::<mlua::Table>("Assets")
        .unwrap();
    assert_ne!(native_assets.to_pointer(), booted_assets.to_pointer());
    for name in ["loadFiles", "createSpriteSheet"] {
        assert!(native_assets.get::<Function>(name).is_ok(), "{name}");
    }
    runtime
        .execute_source(
            r#"
                cloud_social_available =
                    RovioCloudManager.isServiceAvailable("social")
                cloud_analytics_available =
                    RovioCloudManager.isServiceAvailable("analytics")
                cloud_push_available =
                    RovioCloudManager.isServiceAvailable("push")
                cloud_identity_available =
                    RovioCloudManager.isServiceAvailable("identityLevel2")
                cloud_storage_available =
                    RovioCloudManager.isServiceAvailable("storage")
                cloud_ads_available =
                    RovioCloudManager.isServiceAvailable("ads")
                cloud_assets_available =
                    RovioCloudManager.isServiceAvailable("assets")
                cloud_channel_available =
                    RovioCloudManager.isServiceAvailable("channel")
                cloud_time_available =
                    RovioCloudManager.isServiceAvailable("time")
                channel_facade_loaded = rovioChannel ~= nil
                skynest_account_facade_loaded =
                    type(SkynestAccount.isAccountLoggedIn) == "function" and
                    type(_G.SkynestAccount.onLoginFailure) == "function"
                skynest_storage_facade_loaded =
                    type(SkynestStorage.syncWithCloud) == "function" and
                    type(_G.SkynestStorage.onEnableService) == "function"
                skynest_account_starts_signed_out =
                    SkynestAccount.isNotLoggedIn() and
                    not SkynestAccount.isAccountLoggedIn()
                skynest_initial_autologin_completed =
                    SkynestAccount.initialAutologinDone and
                    not _G.SkynestAccount.native_isLoginInProgress() and
                    notificationsFrame:getChild("initialLoadingScreen") == nil
                ads_callbacks_loaded =
                    type(_G.RovioAds.adStateChanged) == "function"
                ads_provider_unavailable =
                    not _G.RovioAds.show("InGameBanner")
                channel_sdk_available = rovioChannel:isAvailable()
                channel_native_callbacks_loaded =
                    type(RovioChannel.onChannelShown) == "function" and
                    type(RovioChannel.onChannelClosed) == "function" and
                    type(RovioChannel.onNewChannelContentUpdated) == "function"
                local original_channel_failure =
                    RovioChannel.onChannelLoadingFailed
                local channel_failure_calls = 0
                RovioChannel.onChannelLoadingFailed = function()
                    channel_failure_calls = channel_failure_calls + 1
                end
                RovioChannel.openChannelView(
                    "Purple", "full", "en_EN", 1024, 768, "", "map_screen"
                )
                RovioChannel.onChannelLoadingFailed = original_channel_failure
                retired_channel_request_finishes =
                    channel_failure_calls == 1 and
                    not RovioChannel.isChannelViewOpened()
                assets_have_been_downloaded_loaded =
                    type(Assets.haveBeenDownloaded) == "function"
                assets_filename_loaded =
                    type(Assets.getAssetFilename) == "function"
                assets_download_probe_callable, assets_download_probe_error = pcall(
                    Assets.haveBeenDownloaded, { "telepod_configuration.dat" }
                )
            "#,
        )
        .unwrap();
    let environment = game_environment(runtime.lua()).unwrap();
    for name in [
        "cloud_social_available",
        "cloud_analytics_available",
        "cloud_push_available",
        "cloud_identity_available",
        "cloud_storage_available",
        "cloud_ads_available",
        "cloud_assets_available",
        "cloud_channel_available",
        "cloud_time_available",
        "channel_facade_loaded",
        "skynest_account_facade_loaded",
        "skynest_storage_facade_loaded",
        "skynest_account_starts_signed_out",
        "skynest_initial_autologin_completed",
        "ads_callbacks_loaded",
        "ads_provider_unavailable",
        "channel_sdk_available",
        "channel_native_callbacks_loaded",
        "retired_channel_request_finishes",
        "assets_have_been_downloaded_loaded",
        "assets_filename_loaded",
    ] {
        assert!(environment.get::<bool>(name).unwrap(), "{name}");
    }
    assert!(
        environment
            .get::<bool>("assets_download_probe_callable")
            .unwrap(),
        "{}",
        environment
            .get::<String>("assets_download_probe_error")
            .unwrap()
    );

    // Native registration is a one-shot service-map insertion. Replaying the
    // desktop boundary must therefore leave both script facades intact.
    announce_cloud_service_registrations(runtime.lua()).unwrap();
    runtime
        .execute_source(
            r#"
                assert(RovioCloudManager.isServiceAvailable("social"))
                assert(RovioCloudManager.isServiceAvailable("analytics"))
                assert(RovioCloudManager.isServiceAvailable("push"))
                assert(RovioCloudManager.isServiceAvailable("identityLevel2"))
                assert(RovioCloudManager.isServiceAvailable("storage"))
                assert(RovioCloudManager.isServiceAvailable("ads"))
                assert(RovioCloudManager.isServiceAvailable("assets"))
                assert(RovioCloudManager.isServiceAvailable("channel"))
                assert(RovioCloudManager.isServiceAvailable("time"))
                assert(rovioChannel ~= nil)
                assert(rovioChannel:isAvailable())
                assert(type(Assets.haveBeenDownloaded) == "function")
            "#,
        )
        .unwrap();
}

#[test]
fn simple_random_native_matches_cmwc_seed_and_msvc_lcg_contracts() {
    let mut seed_source = NativeSeedRandom::new();
    assert_eq!(seed_source.next_word(), 0x6d16_d313);
    let mut seed_source = NativeSeedRandom::new();
    assert_eq!(seed_source.new_seed(), 0x6d16_d312);

    let runtime = StellaLua::new("/tmp").unwrap();
    runtime
        .execute_source(
            r##"
                parsed_seed = SimpleRandomNative.newSeedFromString("  123tail")
                invalid_seed_count = select(
                    "#", SimpleRandomNative.newSeedFromString("not-a-seed")
                )
                overflow_seed_count = select(
                    "#", SimpleRandomNative.newSeedFromString("4294967296")
                )
                numeric_seed = SimpleRandomNative.newSeedFromNumber(4294967296)
                negative_seed = SimpleRandomNative.newSeedFromNumber(-10)
                nan_seed = SimpleRandomNative.newSeedFromNumber(0 / 0)
                infinite_seed = SimpleRandomNative.newSeedFromNumber(math.huge)
                seed_text = SimpleRandomNative.seedToString(4294967295)
                random_seed, random_value = SimpleRandomNative.random(1, 3, 9)
                corrected_seed, corrected_value = SimpleRandomNative.random(
                    3887973612, 0, 10
                )
                generated_seed_text = SimpleRandomNative.newSeedString()
                generated_calls_ignore_extra =
                    pcall(SimpleRandomNative.newSeed, "ignored") and
                    pcall(SimpleRandomNative.newSeedString, "ignored")
                parsed_extra = SimpleRandomNative.newSeedFromString(
                    "77", "ignored"
                )
                numeric_extra = SimpleRandomNative.newSeedFromNumber(
                    88, "ignored"
                )
                random_extra_seed, random_extra_value =
                    SimpleRandomNative.random(1, 3, 9, "ignored")
                seed_text_extra = SimpleRandomNative.seedToString(
                    4294967295, "ignored"
                )
                string_tag_fails = not pcall(
                    SimpleRandomNative.newSeedFromString, 123
                )
                number_tag_fails = not pcall(
                    SimpleRandomNative.newSeedFromNumber, "123"
                )
                random_seed_tag_fails = not pcall(
                    SimpleRandomNative.random, 1.5, 3, 9
                )
                random_bound_tag_fails = not pcall(
                    SimpleRandomNative.random, 1, "3", 9
                )
                seed_to_string_tag_fails = not pcall(
                    SimpleRandomNative.seedToString, 1.5
                )
                "##,
        )
        .unwrap();
    let environment = game_environment(runtime.lua()).unwrap();
    assert_eq!(environment.get::<u32>("parsed_seed").unwrap(), 123);
    assert_eq!(environment.get::<i64>("invalid_seed_count").unwrap(), 0);
    assert_eq!(environment.get::<i64>("overflow_seed_count").unwrap(), 0);
    assert_eq!(environment.get::<u32>("numeric_seed").unwrap(), u32::MAX);
    assert_eq!(environment.get::<u32>("negative_seed").unwrap(), 0);
    assert_eq!(environment.get::<u32>("nan_seed").unwrap(), 0);
    assert_eq!(environment.get::<u32>("infinite_seed").unwrap(), u32::MAX);
    assert_eq!(
        environment.get::<String>("seed_text").unwrap(),
        u32::MAX.to_string()
    );
    assert_eq!(environment.get::<u32>("random_seed").unwrap(), 2_745_024);
    assert_eq!(environment.get::<f64>("random_value").unwrap(), 9.0);
    assert_eq!(environment.get::<u32>("corrected_seed").unwrap(), 0);
    assert_eq!(environment.get::<f64>("corrected_value").unwrap(), 0.0);
    assert!(
        environment
            .get::<bool>("generated_calls_ignore_extra")
            .unwrap()
    );
    assert_eq!(environment.get::<u32>("parsed_extra").unwrap(), 77);
    assert_eq!(environment.get::<u32>("numeric_extra").unwrap(), 88);
    assert_eq!(
        environment.get::<u32>("random_extra_seed").unwrap(),
        2_745_024
    );
    assert_eq!(environment.get::<f64>("random_extra_value").unwrap(), 9.0);
    assert_eq!(
        environment.get::<String>("seed_text_extra").unwrap(),
        u32::MAX.to_string()
    );
    for name in [
        "string_tag_fails",
        "number_tag_fails",
        "random_seed_tag_fails",
        "random_bound_tag_fails",
        "seed_to_string_tag_fails",
    ] {
        assert!(environment.get::<bool>(name).unwrap(), "{name}");
    }
    environment
        .get::<String>("generated_seed_text")
        .unwrap()
        .parse::<u32>()
        .unwrap();
}

#[test]
fn align_utility_matches_fixed_aspect_scale_permissions_and_anchor_math() {
    let runtime = StellaLua::new("/tmp").unwrap();
    runtime
        .execute_source(
            r#"
                left_x, top_y, fit_sx, fit_sy = Align.getPositionAndScale({
                    alignH = "LEFT", alignV = "TOP",
                    scaleH = "TRUE", scaleV = "TRUE",
                    posx = 10, posy = 20, scalex = 2, scaley = 3
                }, 100, 100, 200, 300, "ignored")
                right_x, bottom_y = Align.getPositionAndScale({
                    alignH = "RIGHT", alignV = "BOTTOM",
                    scaleH = "TRUE", scaleV = "TRUE",
                    posx = 10, posy = 20, scalex = 1, scaley = 1
                }, 100, 100, 200, 300)
                center_x, center_y = Align.getPositionAndScale({
                    alignH = "CENTER", alignV = "CENTER",
                    scaleH = "TRUE", scaleV = "TRUE",
                    posx = 10, posy = 20, scalex = 1, scaley = 1
                }, 100, 100, 200, 300)
                clamp_x, clamp_y, clamp_sx, clamp_sy = Align.getPositionAndScale({
                    alignH = "LEFT", alignV = "TOP",
                    scaleH = "DOWN", scaleV = "UP",
                    posx = 10, posy = 20, scalex = 2, scaley = 3
                }, 100, 100, 200, 50)
                raw_x, raw_y = Align.getPositionAndScale({
                    alignH = "UNKNOWN", alignV = "UNKNOWN",
                    scaleH = "TRUE", scaleV = "TRUE",
                    posx = 10, posy = 20, scalex = 1, scaley = 1
                }, 100, 100, 200, 300)
                bad_align_arity_fails = not pcall(
                    Align.getPositionAndScale, {}, 100, 100
                )
                bad_align_table_tag_fails = not pcall(
                    Align.getPositionAndScale, "layout", 100, 100, 200, 300
                )
                bad_align_reference_width_tag_fails = not pcall(
                    Align.getPositionAndScale, {}, "100", 100, 200, 300
                )
                bad_align_reference_height_tag_fails = not pcall(
                    Align.getPositionAndScale, {}, 100, "100", 200, 300
                )
                bad_align_target_width_tag_fails = not pcall(
                    Align.getPositionAndScale, {}, 100, 100, "200", 300
                )
                bad_align_target_height_tag_fails = not pcall(
                    Align.getPositionAndScale, {}, 100, 100, 200, "300"
                )
                zero_x, zero_y, zero_sx, zero_sy = Align.getPositionAndScale({
                    alignH = "LEFT", alignV = "TOP",
                    scaleH = "TRUE", scaleV = "TRUE",
                    posx = 10, posy = 20, scalex = 0, scaley = 0
                }, 100, 100, 200, 300)
                zero_position_ratios_are_nan = zero_x ~= zero_x and zero_y ~= zero_y
                "#,
        )
        .unwrap();
    let environment = game_environment(runtime.lua()).unwrap();
    for (name, expected) in [
        ("left_x", 20.0),
        ("top_y", 40.0),
        ("fit_sx", 4.0),
        ("fit_sy", 6.0),
        ("right_x", 20.0),
        ("bottom_y", 140.0),
        ("center_x", 20.0),
        ("center_y", 90.0),
        ("clamp_x", 10.0),
        ("clamp_y", 20.0),
        ("clamp_sx", 2.0),
        ("clamp_sy", 3.0),
        ("raw_x", 10.0),
        ("raw_y", 20.0),
    ] {
        assert_eq!(environment.get::<f64>(name).unwrap(), expected, "{name}");
    }
    for name in [
        "bad_align_arity_fails",
        "bad_align_table_tag_fails",
        "bad_align_reference_width_tag_fails",
        "bad_align_reference_height_tag_fails",
        "bad_align_target_width_tag_fails",
        "bad_align_target_height_tag_fails",
        "zero_position_ratios_are_nan",
    ] {
        assert!(environment.get::<bool>(name).unwrap(), "{name}");
    }
    assert_eq!(environment.get::<f64>("zero_sx").unwrap(), 0.0);
    assert_eq!(environment.get::<f64>("zero_sy").unwrap(), 0.0);
}

#[test]
fn notification_adapters_enforce_native_types_and_keyed_removal() {
    let runtime = StellaLua::new("/tmp").unwrap();
    runtime
        .execute_source(
            r#"
                notification_added = addNotificationAfter("daily", 30, "Come back")
                notification_removed = removeNotification("daily")
                notification_removed_twice = removeNotification("daily")
                notification_bad_arity = pcall(function()
                    addNotificationAfter("missing-delay")
                end)
                notification_bad_delay = pcall(function()
                    addNotificationAfter("bad-delay", {}, "message")
                end)
                notification_numeric_id_rejected = not pcall(
                    addNotificationAfter, 123, 1, "message")
                notification_string_delay_rejected = not pcall(
                    addNotificationAfter, "string-delay", "1", "message")
                notification_numeric_message_rejected = not pcall(
                    addNotificationAfter, "numeric-message", 1, 123)
                notification_trailing_added = addNotificationAfter(
                    "trailing", 2, "message", "ignored")
                notification_trailing_removed = removeNotification(
                    "trailing", "ignored")
                notification_numeric_remove_rejected = not pcall(
                    removeNotification, 123)
                notification_numeric_enable_rejected = not pcall(
                    setNotificationsEnabled, 1)
                notification_trailing_enable_accepted = pcall(
                    setNotificationsEnabled, true, "ignored")
                setNotificationsEnabled(false)
                notification_added_while_disabled =
                    addNotificationAfter("disabled", 1, "message")
                notification_remove_all_trailing = pcall(
                    removeAllNotifications, "ignored")
                "#,
        )
        .unwrap();

    let environment = game_environment(runtime.lua()).unwrap();
    assert!(environment.get::<bool>("notification_added").unwrap());
    assert!(environment.get::<bool>("notification_removed").unwrap());
    assert!(
        !environment
            .get::<bool>("notification_removed_twice")
            .unwrap()
    );
    assert!(!environment.get::<bool>("notification_bad_arity").unwrap());
    assert!(!environment.get::<bool>("notification_bad_delay").unwrap());
    for name in [
        "notification_numeric_id_rejected",
        "notification_string_delay_rejected",
        "notification_numeric_message_rejected",
        "notification_trailing_added",
        "notification_trailing_removed",
        "notification_numeric_remove_rejected",
        "notification_numeric_enable_rejected",
        "notification_trailing_enable_accepted",
        "notification_remove_all_trailing",
    ] {
        assert!(environment.get::<bool>(name).unwrap(), "{name}");
    }
    assert!(
        !environment
            .get::<bool>("notification_added_while_disabled")
            .unwrap()
    );
}

#[test]
fn recovered_resource_lifecycle_bindings_return_no_lua_values() {
    let unique = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let root = std::env::temp_dir().join(format!("stella-resource-lifecycle-{unique}"));
    let data_root = root.join("data");
    let app_root = root.join("appdata");
    fs::create_dir_all(&data_root).unwrap();
    fs::create_dir_all(&app_root).unwrap();
    fs::create_dir_all(data_root.join("fonts")).unwrap();
    fs::create_dir_all(data_root.join("images")).unwrap();
    fs::create_dir_all(data_root.join("localization")).unwrap();
    fs::write(data_root.join("fonts/FONT.dat"), test_bitmap_font()).unwrap();
    fs::write(data_root.join("fonts/OPTIONAL.dat"), test_bitmap_font()).unwrap();
    fs::write(data_root.join("images/SHEET.dat"), test_sprite_sheet()).unwrap();
    fs::write(
        data_root.join("images/OPTIONAL.dat"),
        test_composite_set(Some("OPTIONAL")),
    )
    .unwrap();
    fs::write(
        data_root.join("images/UI.dat"),
        test_composite_set(Some("UI")),
    )
    .unwrap();
    fs::write(
        data_root.join("localization/TEXTS.dat"),
        test_localization_table("zz_ZZ", "TEXT", "VALUE"),
    )
    .unwrap();
    fs::write(data_root.join("SHEET.dat"), test_sprite_sheet()).unwrap();
    fs::write(data_root.join("clip.wav"), test_pcm_wav(32)).unwrap();
    fs::write(app_root.join("cached.wav"), test_pcm_wav(32)).unwrap();
    let runtime = StellaLua::new(&data_root).unwrap();
    runtime
        .execute_source(
            r##"
                bitmap_results = select("#", res.createBitmapFont("fonts/FONT.dat"))
                sprite_optional_types_are_probes = pcall(
                    res.createSpriteSheet, "images/SHEET.dat", 1
                ) and pcall(
                    res.createSpriteSheet, "images/SHEET.dat", true, 1
                )
                compo_optional_type_is_probe = pcall(
                    res.createCompoSpriteSet, "images/OPTIONAL.dat", 1
                )
                bitmap_optional_type_is_probe = pcall(
                    res.createBitmapFont, "fonts/OPTIONAL.dat", 1
                )
                release_sprite_bool_is_strict = not pcall(
                    res.releaseSpriteSheet, "images/SHEET.dat", 1
                )
                fonts_before = res.getAvailableSystemFonts()
                fonts_with_extra = res.getAvailableSystemFonts(false)
                locale_before = res.getLocale()
                locale_with_extra = res.getLocale(false)
                system_results = select("#", res.createSystemFont(
                    "SYSTEM_FONT", "Arial", 12, 255, 255, 255, 255
                ))
                res.useFont("SYSTEM_FONT")
                system_width = res.getStringWidth("AVATAR")
                system_ascending = res.getFontMaxAscending()
                system_descending = res.getFontMaxDescending()
                system_leading = res.getFontLeading()
                system_tracking = res.getFontTracking()
                system_height = res.getFontHeight()
                duplicate_system_results = select("#", res.createSystemFont(
                    "SYSTEM_FONT", "FONT_THAT_DOES_NOT_EXIST", 99,
                    255, 255, 255, 255
                ))
                preserved_system_width = res.getStringWidth("AVATAR")
                system_optional_types_are_probes = pcall(
                    res.createSystemFont,
                    "PROBED_SYSTEM_FONT", "Arial", 12,
                    255, 255, 255, 255, "not-a-style"
                )
                forced_missing_system_fails = not pcall(
                    res.createSystemFont, "SYSTEM_FONT", "FONT_THAT_DOES_NOT_EXIST", 99,
                    255, 255, 255, 255, 0, true
                )
                unavailable_style_fails = not pcall(
                    res.createSystemFont, "BAD_STYLE", "Arial", 12,
                    255, 255, 255, 255, 1
                )
                fonts_after = res.getAvailableSystemFonts()
                created_alias_listed = false
                for _, font_name in ipairs(fonts_after) do
                    if font_name == "SYSTEM_FONT" then
                        created_alias_listed = true
                    end
                end
                stroked_system_results = select("#", res.createSystemFontWithStroke(
                    "SYSTEM_FONT_STROKE", "Arial", 12,
                    255, 255, 255, 255, 0, 2, 0, 0, 255, 0
                ))
                short_system_font_fails = not pcall(
                    res.createSystemFont, "BAD_SYSTEM_FONT", "Arial", 12
                )
                text_group_results = select("#", res.createTextGroupSet("localization/TEXTS.dat"))
                compo_results = select("#", res.createCompoSpriteSet("images/UI.dat"))
                release_compo_results = select("#", res.releaseCompoSpriteSet("images/UI.dat"))
                output_results = select("#", res.createAudioOutput(1, 16, 16000))
                short_audio_output_fails = not pcall(res.createAudioOutput, 1, 16)
                input_results = select("#", res.createAudioInput(1, 16, 16000))
                start_input_results = select("#", res.startAudioInput())
                start_output_results = select("#", res.startAudioOutput())
                start_output_value = res.startAudioOutput()
                locale_results = select("#", res.loadLocale("TEXTS_BASIC", "zz_ZZ"))
                use_locale_results = select("#", res.useLocale("zz_ZZ"))
                sheet_results = select("#", ResourceManager.native_createSpriteSheet("SHEET.dat"))
                audio_results = select("#", ResourceManager.native_createAudio("clip.wav", "CLIP"))
                appdata_audio_results = select("#",
                    ResourceManager.native_createAudioFromAppData(
                        "cached.wav", "CACHED_CLIP", false
                    )
                )
                short_manager_audio_fails = not pcall(
                    ResourceManager.native_createAudio, "clip.wav"
                )
                bad_manager_audio_flag_fails = not pcall(
                    ResourceManager.native_createAudio, "clip.wav", "BAD_CLIP", 1
                )
                manager_play_results = select("#",
                    ResourceManager.native_playAudio("CLIP", 0.25, true, 3.9)
                )
                manager_play_is_active = res.isAudioPlaying("CLIP")
                bad_manager_play_fails = not pcall(
                    ResourceManager.native_playAudio, 123
                )
                bad_manager_play_volume_fails = not pcall(
                    ResourceManager.native_playAudio, "CLIP", "0.5"
                )
                bad_manager_play_loop_fails = not pcall(
                    ResourceManager.native_playAudio, "CLIP", 0.5, 1
                )
                bad_manager_play_channel_fails = not pcall(
                    ResourceManager.native_playAudio, "CLIP", 0.5, false, "3"
                )
                bad_manager_play_range_fails = not pcall(
                    ResourceManager.native_playAudio, "CLIP", 0.5, false, 8
                )
                bad_audio_flag_fails = not pcall(
                    res.createAudio, "clip.wav", "BAD_CLIP", 1
                )
                release_audio_results = select("#", ResourceManager.native_releaseAudio("CLIP"))
                manager_release_stops_active = not res.isAudioPlaying("CLIP")
                fallback_locale_string = res.getString("TEXTS_BASIC", "MISSING_KEY")
                "##,
        )
        .unwrap();
    let environment = game_environment(runtime.lua()).unwrap();
    for name in [
        "bitmap_results",
        "system_results",
        "duplicate_system_results",
        "stroked_system_results",
        "text_group_results",
        "compo_results",
        "release_compo_results",
        "output_results",
        "input_results",
        "start_input_results",
        "locale_results",
        "use_locale_results",
        "sheet_results",
        "audio_results",
        "appdata_audio_results",
        "manager_play_results",
        "release_audio_results",
    ] {
        assert_eq!(environment.get::<i64>(name).unwrap(), 0, "{name}");
    }
    for name in [
        "short_system_font_fails",
        "forced_missing_system_fails",
        "unavailable_style_fails",
        "short_manager_audio_fails",
        "bad_manager_audio_flag_fails",
        "bad_manager_play_fails",
        "bad_manager_play_volume_fails",
        "bad_manager_play_loop_fails",
        "bad_manager_play_channel_fails",
        "bad_manager_play_range_fails",
        "short_audio_output_fails",
        "bad_audio_flag_fails",
    ] {
        assert!(environment.get::<bool>(name).unwrap(), "{name}");
    }
    assert!(environment.get::<bool>("manager_play_is_active").unwrap());
    assert!(
        environment
            .get::<bool>("manager_release_stops_active")
            .unwrap()
    );
    assert_eq!(
        runtime
            .resource_runtime
            .lock()
            .expect("resource runtime lock poisoned")
            .legacy_audio_play_counts
            .get("CLIP"),
        Some(&1)
    );
    for name in [
        "sprite_optional_types_are_probes",
        "compo_optional_type_is_probe",
        "bitmap_optional_type_is_probe",
        "release_sprite_bool_is_strict",
        "system_optional_types_are_probes",
    ] {
        assert!(environment.get::<bool>(name).unwrap(), "{name}");
    }
    assert_eq!(environment.get::<i64>("start_output_results").unwrap(), 1);
    assert!(environment.get::<bool>("start_output_value").unwrap());
    assert!(environment.get::<f64>("system_width").unwrap() > 0.0);
    assert_eq!(
        environment.get::<f64>("system_width").unwrap(),
        environment.get::<f64>("preserved_system_width").unwrap()
    );
    assert_eq!(environment.get::<f64>("system_tracking").unwrap(), 0.0);
    assert_eq!(
        environment.get::<f64>("system_height").unwrap(),
        environment.get::<f64>("system_ascending").unwrap()
            + environment.get::<f64>("system_descending").unwrap()
    );
    assert!(environment.get::<f64>("system_leading").unwrap() >= 0.0);
    let fonts_before: mlua::Table = environment.get("fonts_before").unwrap();
    let fonts_with_extra: mlua::Table = environment.get("fonts_with_extra").unwrap();
    let fonts_after: mlua::Table = environment.get("fonts_after").unwrap();
    assert_eq!(fonts_before.raw_len(), fonts_after.raw_len());
    assert_eq!(fonts_before.raw_len(), fonts_with_extra.raw_len());
    assert_eq!(
        environment.get::<String>("locale_before").unwrap(),
        environment.get::<String>("locale_with_extra").unwrap()
    );
    assert!(!environment.get::<bool>("created_alias_listed").unwrap());
    assert_eq!(
        environment.get::<String>("fallback_locale_string").unwrap(),
        "MISSING_KEY"
    );
    assert!(
        runtime
            .missing_globals()
            .iter()
            .all(|name| !name.starts_with("res.") && !name.starts_with("ResourceManager."))
    );
    let _ = fs::remove_dir_all(root);
}

#[test]
fn recovered_resource_lifecycle_uses_native_filepath_keys_for_create_and_release() {
    let unique = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let root = std::env::temp_dir().join(format!("stella-resource-keys-{unique}"));
    let data_root = root.join("data");
    let base = data_root.join("bundle/profile");
    for directory in [
        base.join("images"),
        base.join("fonts"),
        base.join("localization"),
        base.join("replacement"),
        base.join("reactivated"),
    ] {
        fs::create_dir_all(directory).unwrap();
    }
    fs::create_dir_all(root.join("appdata")).unwrap();
    fs::write(base.join("images/MENU.PROFILE.dat"), test_sprite_sheet()).unwrap();
    fs::write(
        base.join("images/MENU.PROFILE.json"),
        r#"{"meta":{"app":"Adobe Animate"},"compo":[{"name":"MENU","sprites":[]}]}"#,
    )
    .unwrap();
    fs::write(base.join("fonts/FONT.PROFILE.dat"), test_bitmap_font()).unwrap();
    fs::write(
        base.join("localization/TEXTS.PROFILE.dat"),
        test_localization_table("en_EN", "TEXT", "VALUE"),
    )
    .unwrap();
    fs::write(
        base.join("replacement/MENU.OVERRIDE.dat"),
        test_sprite_sheet(),
    )
    .unwrap();
    fs::write(
        base.join("replacement/MENU.OVERRIDE.json"),
        r#"{"meta":{"app":"ArtPacker"},"compo":[{"name":"MENU","sprites":[]}]}"#,
    )
    .unwrap();
    fs::write(
        base.join("replacement/FONT.PROFILE.dat"),
        test_bitmap_font(),
    )
    .unwrap();
    fs::write(
        base.join("replacement/TEXTS.PROFILE.dat"),
        test_localization_table("en_EN", "TEXT", "REPLACED"),
    )
    .unwrap();
    fs::write(base.join("reactivated/MENU.AGAIN.dat"), test_sprite_sheet()).unwrap();

    let runtime = StellaLua::new(&data_root).unwrap();
    runtime
        .execute_source(
            r#"
                res.setPath([[bundle\profile]])
                res.createSpriteSheet("images/MENU.PROFILE.dat")
                res.createCompoSpriteSet("images/MENU.PROFILE.json")
                res.createBitmapFont("fonts/FONT.PROFILE.dat")
                res.createTextGroupSet("localization/TEXTS.PROFILE.dat")
                res.createSpriteSheet("ignored/MENU.EXTRA.dat")
                res.createCompoSpriteSet("ignored/MENU.EXTRA.json")
                res.createBitmapFont("ignored/FONT.PROFILE.dat")
                res.createTextGroupSet("ignored/TEXTS.PROFILE.dat")
                res.useFont("FONT.PROFILE")
            "#,
        )
        .unwrap();

    {
        let resources = runtime
            .resource_runtime
            .lock()
            .expect("resource runtime lock poisoned");
        // Sprite sheets and composite sets parse the FilePath stem twice;
        // fonts and text groups parse it once.
        assert!(resources.sprite_sheets.contains("MENU"));
        assert!(resources.composite_sets.contains("MENU"));
        assert!(resources.bitmap_fonts.contains("FONT.PROFILE"));
        assert!(resources.text_group_sets.contains("TEXTS.PROFILE"));
        assert_eq!(resources.current_font.as_deref(), Some("FONT.PROFILE"));
        assert_eq!(
            resources.sprite_sheet_paths.get("MENU").map(String::as_str),
            Some("bundle/profile/images/MENU.PROFILE.dat")
        );
        assert_eq!(
            resources
                .composite_set_paths
                .get("MENU")
                .map(String::as_str),
            Some("bundle/profile/images/MENU.PROFILE.json")
        );
        assert_eq!(
            resources
                .bitmap_font_paths
                .get("FONT.PROFILE")
                .map(String::as_str),
            Some("bundle/profile/fonts/FONT.PROFILE.dat")
        );
        assert_eq!(
            resources
                .text_group_set_paths
                .get("TEXTS.PROFILE")
                .map(String::as_str),
            Some("bundle/profile/localization/TEXTS.PROFILE.dat")
        );
        assert!(!resources.sprite_sheets.contains("images/MENU.PROFILE.dat"));
        assert!(
            !resources
                .composite_sets
                .contains("images/MENU.PROFILE.json")
        );
    }

    runtime
        .execute_source(
            r#"
                res.createSpriteSheet("replacement/MENU.OVERRIDE.dat", true)
                res.createCompoSpriteSet("replacement/MENU.OVERRIDE.json", true)
                res.createBitmapFont("replacement/FONT.PROFILE.dat", true)
                res.createTextGroupSet("replacement/TEXTS.PROFILE.dat", true)
            "#,
        )
        .unwrap();
    {
        let resources = runtime
            .resource_runtime
            .lock()
            .expect("resource runtime lock poisoned");
        assert_eq!(
            resources.sprite_sheet_paths.get("MENU").map(String::as_str),
            Some("bundle/profile/replacement/MENU.OVERRIDE.dat")
        );
        assert_eq!(
            resources
                .composite_set_paths
                .get("MENU")
                .map(String::as_str),
            Some("bundle/profile/replacement/MENU.OVERRIDE.json")
        );
        assert_eq!(
            resources
                .bitmap_font_paths
                .get("FONT.PROFILE")
                .map(String::as_str),
            Some("bundle/profile/replacement/FONT.PROFILE.dat")
        );
        assert_eq!(
            resources
                .text_group_set_paths
                .get("TEXTS.PROFILE")
                .map(String::as_str),
            Some("bundle/profile/replacement/TEXTS.PROFILE.dat")
        );
    }

    runtime
        .execute_source(r#"res.releaseSpriteSheet("images/MENU.PROFILE.dat", true)"#)
        .unwrap();
    {
        let resources = runtime
            .resource_runtime
            .lock()
            .expect("resource runtime lock poisoned");
        assert!(resources.sprite_sheets.contains("MENU"));
        assert!(resources.sprite_sheet_paths.contains_key("MENU"));
        assert!(resources.released_sprite_sheet_resources.contains("MENU"));
    }
    runtime
        .execute_source(
            r#"
                res.createSpriteSheet("ignored_after_release/MENU.AGAIN.dat")
                res.createSpriteSheet("reactivated/MENU.AGAIN.dat", true)
            "#,
        )
        .unwrap();
    {
        let resources = runtime
            .resource_runtime
            .lock()
            .expect("resource runtime lock poisoned");
        assert!(!resources.released_sprite_sheet_resources.contains("MENU"));
        assert_eq!(
            resources.sprite_sheet_paths.get("MENU").map(String::as_str),
            Some("bundle/profile/reactivated/MENU.AGAIN.dat")
        );
    }

    runtime
        .execute_source(
            r#"
                res.releaseSpriteSheet([[images\MENU.PROFILE.dat]], false)
                res.releaseCompoSpriteSet([[images\MENU.PROFILE.json]])
                res.releaseFont([[fonts\FONT.PROFILE.dat]])
                res.releaseTextGroupSet([[localization\TEXTS.PROFILE.dat]])
            "#,
        )
        .unwrap();

    let resources = runtime
        .resource_runtime
        .lock()
        .expect("resource runtime lock poisoned");
    assert!(!resources.sprite_sheets.contains("MENU"));
    assert!(!resources.composite_sets.contains("MENU"));
    assert!(!resources.bitmap_fonts.contains("FONT.PROFILE"));
    assert!(!resources.text_group_sets.contains("TEXTS.PROFILE"));
    assert!(!resources.sprite_sheet_paths.contains_key("MENU"));
    assert!(!resources.released_sprite_sheet_resources.contains("MENU"));
    assert!(!resources.composite_set_paths.contains_key("MENU"));
    assert!(!resources.bitmap_font_paths.contains_key("FONT.PROFILE"));
    assert!(!resources.text_group_set_paths.contains_key("TEXTS.PROFILE"));
    assert_eq!(resources.current_font, None);
    drop(resources);
    let _ = fs::remove_dir_all(root);
}

#[test]
fn recovered_resource_loader_dispatch_and_failure_commit_order_match_native() {
    let unique = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let root = std::env::temp_dir().join(format!("stella-resource-loading-{unique}"));
    let data_root = root.join("data");
    let assets = data_root.join("assets");
    for child in [
        "initial",
        "invalid",
        "empty",
        "case",
        "bad_family",
        "texture_packer_object",
    ] {
        fs::create_dir_all(assets.join(child)).unwrap();
    }
    fs::create_dir_all(root.join("appdata")).unwrap();

    fs::write(assets.join("initial/SHEET.dat"), test_sprite_sheet()).unwrap();
    fs::write(
        assets.join("initial/COMPO.dat"),
        test_composite_set(Some("BUTTON")),
    )
    .unwrap();
    fs::write(assets.join("initial/FONT.dat"), test_bitmap_font()).unwrap();
    fs::write(
        assets.join("initial/TEXT.dat"),
        test_localization_table("en_EN", "GREETING", "HELLO"),
    )
    .unwrap();
    for name in ["SHEET.dat", "COMPO.dat", "FONT.dat", "TEXT.dat"] {
        fs::write(assets.join("invalid").join(name), b"not a KA3D resource").unwrap();
    }
    fs::write(assets.join("empty/COMPO.dat"), test_composite_set(None)).unwrap();
    fs::write(assets.join("case/SHEET.DAT"), test_sprite_sheet()).unwrap();
    fs::write(
        assets.join("case/COMPO.DAT"),
        test_composite_set(Some("BUTTON")),
    )
    .unwrap();
    fs::write(
        assets.join("bad_family/SHEET.json"),
        r#"{"meta":{"app":"Unknown Exporter"},"frames":[]}"#,
    )
    .unwrap();
    fs::write(
        assets.join("bad_family/COMPO.json"),
        r#"{"meta":{"app":"Unknown Exporter"},"compo":[]}"#,
    )
    .unwrap();
    fs::write(
        assets.join("texture_packer_object/SHEET.json"),
        r#"{"meta":{"app":"http://www.texturepacker.com"},"frames":{}}"#,
    )
    .unwrap();

    let runtime = StellaLua::new(&data_root).unwrap();
    runtime
        .execute_source(
            r##"
                res.setPath("assets")
                res.createSpriteSheet("initial/SHEET.dat")
                res.createCompoSpriteSet("initial/COMPO.dat")
                res.createBitmapFont("initial/FONT.dat")
                res.createTextGroupSet("initial/TEXT.dat")
                res.loadLocale("TEXT", "en_EN")
                res.useLocale("en_EN")
                initial_text = res.getString("TEXT", "GREETING")

                duplicate_sprite_ok = pcall(
                    res.createSpriteSheet, "missing/SHEET.dat"
                )
                duplicate_compo_ok = pcall(
                    res.createCompoSpriteSet, "missing/COMPO.dat"
                )
                duplicate_font_ok = pcall(
                    res.createBitmapFont, "missing/FONT.dat"
                )
                duplicate_text_ok = pcall(
                    res.createTextGroupSet, "missing/TEXT.dat"
                )

                invalid_sprite_ok = pcall(
                    res.createSpriteSheet, "invalid/SHEET.dat", true
                )
                invalid_compo_ok = pcall(
                    res.createCompoSpriteSet, "invalid/COMPO.dat", true
                )
                empty_compo_ok = pcall(
                    res.createCompoSpriteSet, "empty/COMPO.dat", true
                )
                invalid_font_ok = pcall(
                    res.createBitmapFont, "invalid/FONT.dat", true
                )
                invalid_text_ok = pcall(
                    res.createTextGroupSet, "invalid/TEXT.dat", true
                )
                text_after_failure_ok, text_after_failure_error = pcall(
                    res.getString, "TEXT", "GREETING"
                )
                text_after_failure_error = tostring(text_after_failure_error)

                case_sprite_ok, case_sprite_error = pcall(
                    res.createSpriteSheet, "case/SHEET.DAT", true
                )
                case_sprite_error = tostring(case_sprite_error)
                case_compo_ok, case_compo_error = pcall(
                    res.createCompoSpriteSet, "case/COMPO.DAT", true
                )
                case_compo_error = tostring(case_compo_error)
                bad_sheet_ok, bad_sheet_error = pcall(
                    res.createSpriteSheet, "bad_family/SHEET.json", true
                )
                bad_sheet_error = tostring(bad_sheet_error)
                bad_compo_ok, bad_compo_error = pcall(
                    res.createCompoSpriteSet, "bad_family/COMPO.json", true
                )
                bad_compo_error = tostring(bad_compo_error)
                object_sheet_ok, object_sheet_error = pcall(
                    res.createSpriteSheet,
                    "texture_packer_object/SHEET.json", true
                )
                object_sheet_error = tostring(object_sheet_error)
            "##,
        )
        .unwrap();

    let environment = game_environment(runtime.lua()).unwrap();
    assert_eq!(environment.get::<String>("initial_text").unwrap(), "HELLO");
    for name in [
        "duplicate_sprite_ok",
        "duplicate_compo_ok",
        "duplicate_font_ok",
        "duplicate_text_ok",
        "empty_compo_ok",
    ] {
        assert!(environment.get::<bool>(name).unwrap(), "{name}");
    }
    for name in [
        "invalid_sprite_ok",
        "invalid_compo_ok",
        "invalid_font_ok",
        "invalid_text_ok",
        "text_after_failure_ok",
        "case_sprite_ok",
        "case_compo_ok",
        "bad_sheet_ok",
        "bad_compo_ok",
        "object_sheet_ok",
    ] {
        assert!(!environment.get::<bool>(name).unwrap(), "{name}");
    }
    assert!(
        environment
            .get::<String>("text_after_failure_error")
            .unwrap()
            .contains("not present in data file")
    );
    assert!(
        environment
            .get::<String>("case_sprite_error")
            .unwrap()
            .contains("Unsupported SpriteSheet file extension")
    );
    assert!(
        environment
            .get::<String>("case_compo_error")
            .unwrap()
            .contains("Unsupported CompoSpriteSet file extension")
    );
    assert!(
        environment
            .get::<String>("bad_sheet_error")
            .unwrap()
            .contains("Unsupported JSON sheet format")
    );
    assert!(
        environment
            .get::<String>("bad_compo_error")
            .unwrap()
            .contains("Unsupported JSON composprite format")
    );
    assert!(
        environment
            .get::<String>("object_sheet_error")
            .unwrap()
            .contains("use JSON Array format instead")
    );

    let resources = runtime
        .resource_runtime
        .lock()
        .expect("resource runtime lock poisoned");
    assert_eq!(
        resources
            .sprite_sheet_paths
            .get("SHEET")
            .map(String::as_str),
        Some("assets/initial/SHEET.dat")
    );
    assert_eq!(
        resources
            .composite_set_paths
            .get("COMPO")
            .map(String::as_str),
        Some("assets/initial/COMPO.dat")
    );
    assert_eq!(
        resources.bitmap_font_paths.get("FONT").map(String::as_str),
        Some("assets/initial/FONT.dat")
    );
    assert!(resources.bitmap_font_values.contains_key("FONT"));
    assert_eq!(
        resources
            .text_group_set_paths
            .get("TEXT")
            .map(String::as_str),
        Some("assets/invalid/TEXT.dat")
    );
    assert!(matches!(
        resources.text_group_set_tables.get("TEXT"),
        Some(None)
    ));
    drop(resources);
    let _ = fs::remove_dir_all(root);
}

#[test]
fn legacy_sprite_manager_forwards_to_filepath_keyed_lua_resources_member() {
    let unique = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let root = std::env::temp_dir().join(format!("stella-legacy-sheet-forward-{unique}"));
    let data_root = root.join("data");
    fs::create_dir_all(data_root.join("images")).unwrap();
    fs::create_dir_all(root.join("appdata")).unwrap();
    fs::write(
        data_root.join("images/MENU.PROFILE.dat"),
        test_sprite_sheet(),
    )
    .unwrap();
    fs::write(data_root.join("images/BROKEN.dat"), b"not KA3D").unwrap();

    let runtime = StellaLua::new(&data_root).unwrap();
    runtime
        .execute_source(
            r##"
                legacy_create_results = select(
                    "#", ResourceManager.native_createSpriteSheet(
                        "images/MENU.PROFILE.dat", "ignored"
                    )
                )
                legacy_bad_create_tag = pcall(
                    ResourceManager.native_createSpriteSheet, 123
                )
                legacy_duplicate_missing_ok = pcall(
                    ResourceManager.native_createSpriteSheet,
                    "missing/MENU.EXTRA.dat"
                )
                legacy_broken_ok = pcall(
                    ResourceManager.native_createSpriteSheet,
                    "images/BROKEN.dat"
                )
            "##,
        )
        .unwrap();
    let environment = game_environment(runtime.lua()).unwrap();
    assert_eq!(environment.get::<i64>("legacy_create_results").unwrap(), 0);
    assert!(!environment.get::<bool>("legacy_bad_create_tag").unwrap());
    assert!(
        environment
            .get::<bool>("legacy_duplicate_missing_ok")
            .unwrap()
    );
    assert!(!environment.get::<bool>("legacy_broken_ok").unwrap());
    {
        let resources = runtime
            .resource_runtime
            .lock()
            .expect("resource runtime lock poisoned");
        assert!(resources.sprite_sheets.contains("MENU"));
        assert!(!resources.sprite_sheets.contains("images/MENU.PROFILE.dat"));
        assert_eq!(
            resources.sprite_sheet_paths.get("MENU").map(String::as_str),
            Some("images/MENU.PROFILE.dat")
        );
        assert!(!resources.sprite_sheets.contains("BROKEN"));
    }

    runtime
        .execute_source(
            r#"
                legacy_bad_release_tag = pcall(
                    ResourceManager.native_releaseSpriteSheet, 123
                )
                ResourceManager.native_releaseSpriteSheet(
                    "images/MENU.PROFILE.dat", "ignored"
                )
            "#,
        )
        .unwrap();
    assert!(
        !game_environment(runtime.lua())
            .unwrap()
            .get::<bool>("legacy_bad_release_tag")
            .unwrap()
    );
    let resources = runtime
        .resource_runtime
        .lock()
        .expect("resource runtime lock poisoned");
    assert!(!resources.sprite_sheets.contains("MENU"));
    assert!(!resources.sprite_sheet_paths.contains_key("MENU"));
    drop(resources);
    let _ = fs::remove_dir_all(root);
}

#[test]
fn sprite_resource_name_vectors_use_last_entry_and_release_fallback() {
    let unique = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let root = std::env::temp_dir().join(format!("stella-resource-name-stack-{unique}"));
    let data_root = root.join("data");
    for directory in ["initial", "replacement", "shadow"] {
        fs::create_dir_all(data_root.join(directory)).unwrap();
    }
    fs::create_dir_all(root.join("appdata")).unwrap();
    fs::write(
        data_root.join("initial/FIRST.dat"),
        test_named_sprite_sheet("SHARED", 10, 20),
    )
    .unwrap();
    fs::write(
        data_root.join("initial/SECOND.dat"),
        test_named_sprite_sheet("SHARED", 30, 40),
    )
    .unwrap();
    fs::write(
        data_root.join("replacement/FIRST.dat"),
        test_named_sprite_sheet("SHARED", 50, 60),
    )
    .unwrap();
    fs::write(
        data_root.join("shadow/SHADOW_SHEET.dat"),
        test_named_sprite_sheet("SHADOW", 12, 14),
    )
    .unwrap();
    fs::write(
        data_root.join("shadow/SHADOW_COMPO.dat"),
        test_composite_set(Some("SHADOW")),
    )
    .unwrap();

    let runtime = StellaLua::new(&data_root).unwrap();
    runtime
        .execute_source(
            r#"
                res.createSpriteSheet("initial/FIRST.dat")
                first_w, first_h = res.getSpriteBounds("SHARED")
                res.createSpriteSheet("initial/SECOND.dat")
                second_w, second_h = res.getSpriteBounds("SHARED")
                res.releaseSpriteSheet("initial/SECOND.dat", false)
                fallback_w, fallback_h = res.getSpriteBounds("SHARED")

                res.createSpriteSheet("replacement/FIRST.dat", true)
                replaced_w, replaced_h = res.getSpriteBounds("SHARED")
                res.releaseSpriteSheet("initial/FIRST.dat", false)
                removed_w, removed_h = res.getSpriteBounds("SHARED")

                res.createSpriteSheet("shadow/SHADOW_SHEET.dat")
                shadow_sheet_w, shadow_sheet_h = res.getSpriteBounds("SHADOW")
                res.createCompoSpriteSet("shadow/SHADOW_COMPO.dat")
                shadow_compo_w, shadow_compo_h = res.getSpriteBounds("SHADOW")
                shadow_compo_data = res.getCompoSpriteData("SHADOW")
                res.releaseCompoSpriteSet("shadow/SHADOW_COMPO.dat")
                shadow_fallback_w, shadow_fallback_h = res.getSpriteBounds("SHADOW")
                res.releaseSpriteSheet("shadow/SHADOW_SHEET.dat", false)
                shadow_removed_w, shadow_removed_h = res.getSpriteBounds("SHADOW")
            "#,
        )
        .unwrap();
    let environment = game_environment(runtime.lua()).unwrap();
    for (prefix, expected) in [
        ("first", (10.0, 20.0)),
        ("second", (30.0, 40.0)),
        ("fallback", (10.0, 20.0)),
        ("replaced", (50.0, 60.0)),
        ("removed", (0.0, 0.0)),
        ("shadow_sheet", (12.0, 14.0)),
        ("shadow_compo", (0.0, 0.0)),
        ("shadow_fallback", (12.0, 14.0)),
        ("shadow_removed", (0.0, 0.0)),
    ] {
        assert_eq!(
            (
                environment.get::<f64>(format!("{prefix}_w")).unwrap(),
                environment.get::<f64>(format!("{prefix}_h")).unwrap(),
            ),
            expected,
            "{prefix}"
        );
    }
    assert_eq!(
        environment
            .get::<mlua::Table>("shadow_compo_data")
            .unwrap()
            .raw_len(),
        0
    );
    let resources = runtime
        .resource_runtime
        .lock()
        .expect("resource runtime lock poisoned");
    assert!(!resources.sprite_entries.contains_key("SHARED"));
    assert!(!resources.sprite_entries.contains_key("SHADOW"));
    drop(resources);
    let _ = fs::remove_dir_all(root);
}

#[test]
fn deferred_host_sprite_catalog_tracks_native_shadow_and_release_lifetimes() {
    let unique = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let root = std::env::temp_dir().join(format!("stella-host-catalog-{unique}"));
    let data_root = root.join("data");
    for directory in ["first", "second"] {
        fs::create_dir_all(data_root.join(directory)).unwrap();
    }
    fs::write(
        data_root.join("first/FIRST.dat"),
        test_textured_sprite_sheet("SHARED", "first.pvr", 10, 20),
    )
    .unwrap();
    fs::write(data_root.join("first/first.pvr"), []).unwrap();
    fs::write(
        data_root.join("second/SECOND.dat"),
        test_textured_sprite_sheet("SHARED", "second.pvr", 30, 40),
    )
    .unwrap();
    fs::write(data_root.join("second/second.pvr"), []).unwrap();

    let runtime = StellaLua::new(&data_root).unwrap();
    runtime
        .execute_source(
            r#"
                res.createSpriteSheet("first/FIRST.dat")
                drawSpriteWithoutShader("SHARED", 0, 0, 1, 1, 0)
            "#,
        )
        .unwrap();
    let first = runtime.sprite_catalog_snapshot_since(0).unwrap();
    let first_revision = first.revision;
    assert_eq!(first.regions["SHARED"].sprite.width, 10);
    assert!(
        first.regions["SHARED"]
            .texture_source
            .ends_with("first/first.pvr")
    );
    assert!(
        runtime
            .sprite_catalog_snapshot_since(first_revision)
            .is_none()
    );
    {
        let resources = runtime.resource_runtime.lock().unwrap();
        let cached = &resources.sprite_sheet_catalog_regions["FIRST"]["SHARED"];
        let first_lookup = resources
            .active_atlas_catalog_region("SHARED", runtime.data_root())
            .unwrap();
        let repeated_lookup = resources
            .active_atlas_catalog_region("SHARED", runtime.data_root())
            .unwrap();
        let active_entry = resources.sprite_entries["SHARED"].last().unwrap();
        let retained_entry_region = active_entry.atlas_region.as_ref().unwrap();
        assert!(Arc::ptr_eq(cached, &first_lookup));
        assert!(Arc::ptr_eq(&first_lookup, &repeated_lookup));
        assert!(Arc::ptr_eq(&first_lookup, retained_entry_region));
        let bridge = runtime.render.lock().unwrap();
        let command = &bridge.commands[0];
        assert!(Arc::ptr_eq(
            command.sprite.as_arc(),
            active_entry.name.as_arc()
        ));
        assert!(Arc::ptr_eq(
            command.bound_region.as_ref().unwrap(),
            retained_entry_region
        ));
        assert_eq!(cached.sprite.width, 10);
        assert!(cached.texture_source.ends_with("first/first.pvr"));
    }

    runtime
        .execute_source(r#"res.createSpriteSheet("second/SECOND.dat")"#)
        .unwrap();
    let second = runtime
        .sprite_catalog_snapshot_since(first_revision)
        .unwrap();
    assert_eq!(second.regions["SHARED"].sprite.width, 30);
    assert!(
        second.regions["SHARED"]
            .texture_source
            .ends_with("second/second.pvr")
    );

    runtime
        .execute_source(r#"res.releaseSpriteSheet("second/SECOND.dat", false)"#)
        .unwrap();
    let fallback = runtime
        .sprite_catalog_snapshot_since(second.revision)
        .unwrap();
    assert_eq!(fallback.regions["SHARED"].sprite.width, 10);
    {
        let resources = runtime.resource_runtime.lock().unwrap();
        assert!(
            !resources
                .sprite_sheet_catalog_regions
                .contains_key("SECOND")
        );
        assert!(resources.sprite_sheet_catalog_regions.contains_key("FIRST"));
    }

    runtime
        .execute_source(r#"res.releaseSpriteSheet("first/FIRST.dat", true)"#)
        .unwrap();
    let released = runtime
        .sprite_catalog_snapshot_since(fallback.revision)
        .unwrap();
    assert!(!released.regions.contains_key("SHARED"));
    let resources = runtime.resource_runtime.lock().unwrap();
    assert!(resources.sprite_sheets.contains("FIRST"));
    assert!(!resources.sprite_sheet_catalog_regions.contains_key("FIRST"));
    drop(resources);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn composite_loader_freezes_the_first_ordered_sheet_pointer_across_shadow_and_release() {
    let unique = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let root = std::env::temp_dir().join(format!("stella-composite-pointer-{unique}"));
    let data_root = root.join("data");
    for directory in ["first", "second", "composite"] {
        fs::create_dir_all(data_root.join(directory)).unwrap();
    }
    fs::write(
        data_root.join("first/FIRST.dat"),
        test_textured_sprite_sheet("SHARED", "first.pvr", 10, 20),
    )
    .unwrap();
    fs::write(data_root.join("first/first.pvr"), []).unwrap();
    fs::write(
        data_root.join("second/SECOND.dat"),
        test_textured_sprite_sheet("SHARED", "second.pvr", 30, 40),
    )
    .unwrap();
    fs::write(data_root.join("second/second.pvr"), []).unwrap();
    fs::write(
        data_root.join("composite/COMPOSITE.dat"),
        test_composite_set_with_part("FROZEN", "SHARED"),
    )
    .unwrap();

    let runtime = StellaLua::new(&data_root).unwrap();
    runtime
        .execute_source(
            r#"
                res.createSpriteSheet("first/FIRST.dat")
                res.createCompoSpriteSet("composite/COMPOSITE.dat")
                res.createSpriteSheet("second/SECOND.dat")
                res.releaseSpriteSheet("first/FIRST.dat", false)
                drawCompoSprite("FROZEN", 0, 0, 1, 1)
            "#,
        )
        .unwrap();

    let snapshot = runtime.sprite_catalog_snapshot_since(0).unwrap();
    assert_eq!(snapshot.regions["SHARED"].sprite.width, 30);
    let alias = &snapshot.composites["FROZEN"][0].sprite;
    assert_eq!(snapshot.regions[alias].sprite.width, 10);
    assert!(
        snapshot.regions[alias]
            .texture_source
            .ends_with("first/first.pvr")
    );
    let bridge = runtime.render.lock().unwrap();
    let command = &bridge.commands[0];
    let retained = command.bound_region.as_ref().unwrap();
    assert_eq!(retained.sprite.width, 10);
    assert!(retained.texture_source.ends_with("first/first.pvr"));
    let resources = runtime.resource_runtime.lock().unwrap();
    let owner = resources.sprite_entries["FROZEN"]
        .last()
        .unwrap()
        .composite_sprite
        .as_ref()
        .unwrap();
    let parts = owner.snapshot();
    assert!(Arc::ptr_eq(
        command.sprite.as_arc(),
        parts[0].sprite.as_arc()
    ));
    assert!(Arc::ptr_eq(retained, &parts[0].region));
    drop(resources);
    drop(bridge);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn scene_objects_retain_assigned_atlas_composite_and_null_resource_pointers() {
    let unique = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let root = std::env::temp_dir().join(format!("stella-object-sprite-pointer-{unique}"));
    let data_root = root.join("data");
    for directory in ["first", "second", "late", "composite"] {
        fs::create_dir_all(data_root.join(directory)).unwrap();
    }
    fs::write(
        data_root.join("first/FIRST.dat"),
        test_textured_sprite_sheet("SHARED", "first.pvr", 10, 20),
    )
    .unwrap();
    fs::write(data_root.join("first/first.pvr"), []).unwrap();
    fs::write(
        data_root.join("second/SECOND.dat"),
        test_textured_sprite_sheet("SHARED", "second.pvr", 30, 40),
    )
    .unwrap();
    fs::write(data_root.join("second/second.pvr"), []).unwrap();
    fs::write(
        data_root.join("late/LATE.dat"),
        test_textured_sprite_sheet("LATE", "late.pvr", 50, 60),
    )
    .unwrap();
    fs::write(data_root.join("late/late.pvr"), []).unwrap();
    fs::write(
        data_root.join("composite/COMPOSITE.dat"),
        test_composite_set_with_part("FROZEN", "SHARED"),
    )
    .unwrap();

    let runtime = StellaLua::new(&data_root).unwrap();
    runtime
        .execute_source(
            r#"
                res.createSpriteSheet("first/FIRST.dat")
                res.createCompoSpriteSet("composite/COMPOSITE.dat")
                createNonPhysicsObject("atlas", "SHARED", 0, 0, 1)
                createNonPhysicsObject("composite", "FROZEN", 0, 0, 2)
                createNonPhysicsObject("missing", "LATE", 0, 0, 3)
                createNonPhysicsObject("retargeted", "SHARED", 0, 0, 4)
                native_setSprite("retargeted", "FROZEN")

                res.createSpriteSheet("second/SECOND.dat")
                res.releaseSpriteSheet("first/FIRST.dat", false)
                res.createSpriteSheet("late/LATE.dat")
                drawGameNative()
            "#,
        )
        .unwrap();

    let snapshot = runtime.sprite_catalog_snapshot_since(0).unwrap();
    assert_eq!(snapshot.regions["SHARED"].sprite.width, 30);
    assert_eq!(snapshot.regions["LATE"].sprite.width, 50);

    let bridge = runtime.render.lock().unwrap();
    assert_eq!(bridge.commands.len(), 4);
    let atlas = &bridge.commands[0];
    assert_eq!(atlas.sprite, "SHARED");
    assert_eq!(atlas.bound_region.as_ref().unwrap().sprite.width, 10);
    assert!(atlas.bound_composite.is_none());

    for command in [&bridge.commands[1], &bridge.commands[3]] {
        assert_eq!(command.sprite, "FROZEN");
        let bound = command.bound_composite.as_ref().unwrap();
        assert_eq!(bound.len(), 1);
        assert_eq!(bound[0].region.sprite.width, 10);
        assert!(bound[0].region.texture_source.ends_with("first/first.pvr"));
    }

    let missing = &bridge.commands[2];
    assert_eq!(missing.sprite, "LATE");
    assert!(missing.bound_region.is_none());
    assert!(missing.bound_composite.as_ref().unwrap().is_empty());
    drop(bridge);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn text_group_locale_lifecycle_distinguishes_absent_unloaded_and_loaded_languages() {
    let data_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../runtime/data");
    let bytes = fs::read(data_root.join("localization/TEXTS_BASIC.dat")).unwrap();
    let table = stella_assets::ka3d::LocalizationTable::parse(&bytes).unwrap();
    let key = table.ids[0].clone();
    let en_index = table
        .locales
        .iter()
        .position(|locale| locale == "en_EN")
        .unwrap();
    let fr_index = table
        .locales
        .iter()
        .position(|locale| locale == "fr_FR")
        .unwrap();
    let en_value = table.translations[en_index][0].clone();
    let fr_value = table.translations[fr_index][0].clone();

    let runtime = StellaLua::new(&data_root).unwrap();
    runtime
        .lua()
        .globals()
        .set("locale_test_key", key.clone())
        .unwrap();
    runtime
        .execute_source(
            r##"
                res.setPath("localization")
                res.createTextGroupSet("TEXTS_BASIC.dat")

                before_loaded_ok, before_loaded_error = pcall(
                    res.getString, "TEXTS_BASIC", locale_test_key
                )
                before_loaded_error = tostring(before_loaded_error)
                missing_load_results = select(
                    "#", res.loadLocale("ABSENT_GROUP", "zz_ZZ")
                )
                missing_group_value = res.getString("ABSENT_GROUP", locale_test_key)
                invalid_load_ok, invalid_load_error = pcall(
                    res.loadLocale, "TEXTS_BASIC", "zz_ZZ"
                )
                invalid_load_error = tostring(invalid_load_error)

                res.loadLocale("TEXTS_BASIC", "en_EN")
                res.useLocale("en_EN")
                en_value = res.getString("TEXTS_BASIC", locale_test_key)
                res.useLocale("fr_FR")
                unloaded_ok, unloaded_error = pcall(
                    res.getString, "TEXTS_BASIC", locale_test_key
                )
                unloaded_error = tostring(unloaded_error)
                res.loadLocale("TEXTS_BASIC", "fr_FR")
                fr_value = res.getString("TEXTS_BASIC", locale_test_key)

                res.useLocale("en_EN")
                res.loadLocale("TEXTS_BASIC", "en_EN")
                invalid_after_loaded_ok = pcall(
                    res.loadLocale, "TEXTS_BASIC", "zz_ZZ"
                )
                after_invalid_get_ok, after_invalid_get_error = pcall(
                    res.getString, "TEXTS_BASIC", locale_test_key
                )
                after_invalid_get_error = tostring(after_invalid_get_error)

                res.loadLocale("TEXTS_BASIC", "ALL")
                res.useLocale("en_EN")
                all_en_value = res.getString("TEXTS_BASIC", locale_test_key)
                res.useLocale("fr_FR")
                all_fr_value = res.getString("TEXTS_BASIC", locale_test_key)
                res.releaseTextGroupSet("localization/TEXTS_BASIC.dat")
                released_value = res.getString("TEXTS_BASIC", locale_test_key)
            "##,
        )
        .unwrap();

    let environment = game_environment(runtime.lua()).unwrap();
    assert!(!environment.get::<bool>("before_loaded_ok").unwrap());
    assert!(
        environment
            .get::<String>("before_loaded_error")
            .unwrap()
            .contains("which is not loaded")
    );
    assert_eq!(environment.get::<i64>("missing_load_results").unwrap(), 0);
    assert_eq!(
        environment.get::<String>("missing_group_value").unwrap(),
        key
    );
    assert!(!environment.get::<bool>("invalid_load_ok").unwrap());
    assert!(
        environment
            .get::<String>("invalid_load_error")
            .unwrap()
            .contains("not present in data file")
    );
    assert_eq!(environment.get::<String>("en_value").unwrap(), en_value);
    assert!(!environment.get::<bool>("unloaded_ok").unwrap());
    assert!(
        environment
            .get::<String>("unloaded_error")
            .unwrap()
            .contains("which is not loaded")
    );
    assert_eq!(environment.get::<String>("fr_value").unwrap(), fr_value);
    assert!(!environment.get::<bool>("invalid_after_loaded_ok").unwrap());
    assert!(!environment.get::<bool>("after_invalid_get_ok").unwrap());
    assert!(
        environment
            .get::<String>("after_invalid_get_error")
            .unwrap()
            .contains("which is not loaded")
    );
    assert_eq!(environment.get::<String>("all_en_value").unwrap(), en_value);
    assert_eq!(environment.get::<String>("all_fr_value").unwrap(), fr_value);
    assert_eq!(environment.get::<String>("released_value").unwrap(), key);
}

#[test]
fn legacy_resource_memory_globals_follow_native_upload_and_decode_counters() {
    let unique = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let root = std::env::temp_dir().join(format!("stella-resource-memory-{unique}"));
    let data_root = root.join("data");
    let image_root = data_root.join("images");
    let audio_root = data_root.join("audio");
    let app_root = root.join("appdata");
    fs::create_dir_all(&image_root).unwrap();
    fs::create_dir_all(&audio_root).unwrap();
    fs::create_dir_all(&app_root).unwrap();

    let mut sheet_payload = Vec::new();
    sheet_payload.extend_from_slice(&1u16.to_be_bytes());
    sheet_payload.extend_from_slice(&9u16.to_be_bytes());
    sheet_payload.extend_from_slice(b"atlas.pvr");
    sheet_payload.extend_from_slice(&0u16.to_be_bytes());
    let mut sheet = b"KA3D".to_vec();
    sheet.extend_from_slice(&(sheet_payload.len() as u32 + 8).to_be_bytes());
    sheet.extend_from_slice(b"SPRT");
    sheet.extend_from_slice(&(sheet_payload.len() as u32).to_be_bytes());
    sheet.extend_from_slice(&sheet_payload);
    fs::write(image_root.join("sheet.dat"), sheet).unwrap();

    let mut pvr = Vec::new();
    for word in [
        52u32,
        1,
        4,
        0,
        stella_assets::pvr::OGL_RGBA_4444 as u32,
        8,
        16,
        0xf000,
        0x0f00,
        0x00f0,
        0x000f,
        u32::from_le_bytes(*b"PVR!"),
        1,
    ] {
        pvr.extend_from_slice(&word.to_le_bytes());
    }
    pvr.extend_from_slice(&[0; 8]);
    fs::write(image_root.join("atlas.pvr"), pvr).unwrap();

    fs::write(audio_root.join("bundle.wav"), test_pcm_wav(12)).unwrap();
    fs::write(app_root.join("cached.wav"), test_pcm_wav(20)).unwrap();

    let runtime = StellaLua::new(&data_root).unwrap();
    runtime
        .execute_source(
            r#"
                texture_before = g_usedTextureMemory
                ResourceManager.native_createSpriteSheet("images/sheet.dat")
                texture_after_create = g_usedTextureMemory
                ResourceManager.native_createSpriteSheet("images/sheet.dat")
                texture_after_duplicate = g_usedTextureMemory
                ResourceManager.native_releaseSpriteSheet("images/sheet.dat")
                texture_after_release = g_usedTextureMemory

                audio_before = g_usedAudioMemory
                ResourceManager.native_createAudio("audio/bundle.wav", "BUNDLE", false)
                audio_after_bundle = g_usedAudioMemory
                ResourceManager.native_createAudioFromAppData(
                    "cached.wav", "CACHED", false
                )
                audio_after_appdata = g_usedAudioMemory
            "#,
        )
        .unwrap();

    let environment = game_environment(runtime.lua()).unwrap();
    assert!(matches!(
        environment.get::<Value>("texture_before").unwrap(),
        Value::Nil
    ));
    assert_eq!(environment.get::<f64>("texture_after_create").unwrap(), 8.0);
    assert_eq!(
        environment.get::<f64>("texture_after_duplicate").unwrap(),
        8.0
    );
    assert_eq!(
        environment.get::<f64>("texture_after_release").unwrap(),
        0.0
    );
    assert!(matches!(
        environment.get::<Value>("audio_before").unwrap(),
        Value::Nil
    ));
    assert_eq!(environment.get::<f64>("audio_after_bundle").unwrap(), 12.0);
    assert_eq!(environment.get::<f64>("audio_after_appdata").unwrap(), 32.0);
}

#[test]
fn fallback_audit_distinguishes_missing_data_reads_from_invoked_native_methods() {
    let runtime = StellaLua::new_with_missing_global_diagnostics("/tmp").unwrap();
    assert_eq!(runtime.compatibility_bindings(), Vec::<String>::new());
    runtime
        .execute_source(
            r#"
                local absent_data = absentGameData
                goToTaskSwitcherLua()
                local generated = res.__auditMissingResourceMethod
                missing_resource_is_nil = generated == nil
                local animation_generated =
                    AnimationWrapperNative.__auditMissingAnimationMethod
                missing_animation_is_nil = animation_generated == nil
                local manager_generated =
                    ResourceManager.__auditMissingManagerMethod
                missing_manager_is_nil = manager_generated == nil
            "#,
        )
        .unwrap();

    let environment = game_environment(runtime.lua()).unwrap();
    assert!(environment.get::<bool>("missing_resource_is_nil").unwrap());
    assert!(environment.get::<bool>("missing_animation_is_nil").unwrap());
    assert!(environment.get::<bool>("missing_manager_is_nil").unwrap());

    assert!(
        runtime
            .missing_globals()
            .contains(&"absentGameData".to_owned())
    );
    assert!(runtime.fallback_calls().is_empty());
}
