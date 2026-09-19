use super::*;
use crate::game_lua::deliver_persistent_load_messages;

#[test]
fn persistent_load_open_failure_returns_table_and_keeps_existing_directory() {
    let sandbox = ShippedDataSandbox::new("persistent-open-failure");
    let unreadable_file = sandbox.root.join("appdata/directory.lua");
    fs::create_dir(&unreadable_file).unwrap();
    let runtime = StellaLua::new(&sandbox.data_root).unwrap();
    runtime
        .execute_diagnostic_source(
            r#"
        result = {old = true}
        loadTableFromFile('directory.lua', 'result')
        assert(type(result) == 'table' and next(result) == nil)
    "#,
        )
        .unwrap();
    assert_eq!(
        &runtime.pending_persistent_load_messages()[3..],
        ["File Created:directory.lua"]
    );
    assert!(unreadable_file.is_dir());
}

#[test]
fn persistent_load_relay_without_script_callback_reports_error() {
    let sandbox = ShippedDataSandbox::new("persistent-relay-uninitialized");
    let runtime = StellaLua::new(&sandbox.data_root).unwrap();
    runtime
        .execute_diagnostic_source(
            r#"
        local ok = pcall(_G.onLoadLuaFileFail, 'uninitialized')
        assert(not ok)
        local relay = _G.onLoadLuaFileFail
        _G.onLoadLuaFileFail = function(message) fallbackMessage = message end
        relay('metatable callback')
        assert(fallbackMessage == 'metatable callback')
        onLoadLuaFileFail = relay
        assert(not pcall(relay, 'recursive callback'))
    "#,
        )
        .unwrap();
    assert_eq!(runtime.pending_persistent_load_messages().len(), 3);
}

#[test]
fn persistent_load_retains_runtime_assignments_but_compile_failure_executes_nothing() {
    let sandbox = ShippedDataSandbox::new("persistent-load-recovery");
    let fixtures = [
        ("syntax.lua", b"before = 17; this is invalid !!!".to_vec()),
        (
            "runtime.lua",
            b"before = {score = 17}; missing(); after = 99".to_vec(),
        ),
        (
            "encrypted.lua",
            stella_assets::encrypt_persistent_lua(b"before = {score = 23}; missing(); after = 99"),
        ),
        ("binary.lua", b"\x1bLuaQ\0truncated".to_vec()),
        ("valid.lua", b"before = {score = 42}; after = 99".to_vec()),
    ];
    for (name, bytes) in &fixtures {
        fs::write(sandbox.root.join("appdata").join(name), bytes).unwrap();
    }
    let runtime = StellaLua::new(&sandbox.data_root).unwrap();
    runtime
        .execute_diagnostic_source(
            r#"
        previous = { untouched = {score = 12} }
        result = previous
        assert(select('#', loadTableFromFile('syntax.lua', 'result')) == 0)
        assert(type(result) == 'table' and next(result) == nil)
        assert(not rawequal(previous, result))
        empty = result
        loadTableFromFile('runtime.lua', 'result')
        assert(result.before.score == 17 and result.after == nil)
        assert(not rawequal(result, empty) and previous.untouched.score == 12)
        assert(rawequal(_G.result, result))
        partial = result
        loadTableFromFile('encrypted.lua', 'result')
        assert(result.before.score == 23 and result.after == nil)
        assert(not rawequal(result, partial) and partial.before.score == 17)
        loadTableFromFile('binary.lua', 'result')
        assert(type(result) == 'table' and next(result) == nil)
        loadTableFromFile('valid.lua', 'result')
        assert(result.before.score == 42 and result.after == 99)
        assert(before == nil and after == nil)
        -- Level loading belongs to a different native exception boundary.
        assert(not pcall(loadLevelFromAppData, 'runtime'))
    "#,
        )
        .unwrap();
    assert_eq!(
        &runtime.pending_persistent_load_messages()[3..],
        [
            "Persistent file loading failed syntax.lua",
            "Persistent file loading failed runtime.lua",
            "Persistent file loading failed encrypted.lua",
            "Persistent file loading failed binary.lua",
        ]
    );
    for (name, bytes) in fixtures {
        assert_eq!(
            fs::read(sandbox.root.join("appdata").join(name)).unwrap(),
            bytes
        );
    }
}

#[test]
fn persistent_constructor_tables_share_recovery_and_preserve_disk_bytes() {
    let sandbox = ShippedDataSandbox::new("persistent-constructor-recovery");
    let fixtures = [
        ("highscores.lua", "before = 1; invalid !!!"),
        (
            "settings.lua",
            "volume = 0.35; marker = {score = 19}; missing(); after = 1",
        ),
        ("bi_data.lua", "counter = 4"),
    ];
    for (name, text) in fixtures {
        fs::write(sandbox.root.join("appdata").join(name), text).unwrap();
    }
    let runtime = StellaLua::new(&sandbox.data_root).unwrap();
    runtime
        .execute_diagnostic_source(
            r#"
        assert(type(highscores) == 'table' and next(highscores) == nil)
        assert(settings.volume == 0.35 and settings.marker.score == 19 and settings.after == nil)
        assert(bi_data.counter == 4)
    "#,
        )
        .unwrap();
    assert_eq!(
        runtime.pending_persistent_load_messages(),
        [
            "Persistent file loading failed highscores.lua",
            "Persistent file loading failed settings.lua",
        ]
    );
    for (name, text) in fixtures {
        assert_eq!(
            fs::read_to_string(sandbox.root.join("appdata").join(name)).unwrap(),
            text
        );
    }
}

#[test]
fn persistent_load_delivery_reloads_callback_and_includes_messages_appended_by_it() {
    let sandbox = ShippedDataSandbox::new("persistent-load-delivery");
    let runtime = StellaLua::new(&sandbox.data_root).unwrap();
    runtime
        .execute_diagnostic_source(
            r#"
        received = {}
        onLoadLuaFileFail = function(message)
            table.insert(received, 'first:' .. message)
            loadTableFromFile('callback_created.lua', 'callbackTable')
            onLoadLuaFileFail = function(nextMessage)
                table.insert(received, 'next:' .. nextMessage)
            end
        end
        assert(#received == 0)
    "#,
        )
        .unwrap();
    deliver_persistent_load_messages(runtime.lua()).unwrap();
    assert!(runtime.pending_persistent_load_messages().is_empty());
    runtime
        .execute_diagnostic_source(
            r#"
        assert(#received == 4)
        assert(received[1] == 'first:File Created:highscores.lua')
        assert(received[2] == 'next:File Created:settings.lua')
        assert(received[3] == 'next:File Created:bi_data.lua')
        assert(received[4] == 'next:File Created:callback_created.lua')
        assert(select('#', _G.onLoadLuaFileFail('direct', 'ignored')) == 0)
        assert(received[5] == 'next:direct')
        assert(not pcall(_G.onLoadLuaFileFail))
        assert(not pcall(_G.onLoadLuaFileFail, false))
        assert(#received == 5)
    "#,
        )
        .unwrap();
    deliver_persistent_load_messages(runtime.lua()).unwrap();
    runtime
        .execute_diagnostic_source("assert(#received == 5)")
        .unwrap();
    assert!(!sandbox.root.join("appdata/callback_created.lua").exists());
}

#[test]
fn persistent_load_delivery_failure_keeps_entire_queue_and_propagates_error() {
    let sandbox = ShippedDataSandbox::new("persistent-load-delivery-failure");
    let runtime = StellaLua::new(&sandbox.data_root).unwrap();
    let original = runtime.pending_persistent_load_messages();
    runtime
        .execute_source(
            r#"
        received = {}
        onLoadLuaFileFail = function(message)
            table.insert(received, message)
            if #received == 2 then error('synthetic load callback failure') end
        end
    "#,
        )
        .unwrap();
    let error = deliver_persistent_load_messages(runtime.lua()).unwrap_err();
    assert!(
        error
            .to_string()
            .contains("synthetic load callback failure")
    );
    assert_eq!(runtime.pending_persistent_load_messages(), original);
    runtime
        .execute_diagnostic_source(
            r#"
        assert(#received == 2)
        onLoadLuaFileFail = function(message) table.insert(received, message) end
    "#,
        )
        .unwrap();
    deliver_persistent_load_messages(runtime.lua()).unwrap();
    assert!(runtime.pending_persistent_load_messages().is_empty());
    runtime
        .execute_diagnostic_source(
            r#"
        assert(#received == 5 and received[1] == received[3] and received[2] == received[4])
        onLoadLuaFileFail = function() error('direct relay failure') end
        local ok, message = pcall(_G.onLoadLuaFileFail, 'direct')
        assert(not ok and string.find(tostring(message), 'direct relay failure', 1, true))
    "#,
        )
        .unwrap();
}

#[test]
fn persistent_load_original_startup_drains_queue_and_later_loads_remain_queued() {
    let sandbox = ShippedDataSandbox::new("persistent-load-original-startup");
    let runtime = StellaLua::new(&sandbox.data_root).unwrap();
    fs::write(
        sandbox.root.join("appdata/broken.lua"),
        "marker = 7; missing()",
    )
    .unwrap();
    runtime
        .execute_source("loadTableFromFile('broken.lua', 'brokenTable')")
        .unwrap();
    assert_eq!(runtime.pending_persistent_load_messages().len(), 4);
    runtime.boot("scripts/game.lua").unwrap();
    assert!(runtime.gamelogic_loaded());
    assert!(runtime.pending_persistent_load_messages().is_empty());
    runtime
        .execute_diagnostic_source(
            r#"
        assert(brokenTable.marker == 7)
        loadTableFromFile('later_missing.lua', 'laterTable')
        -- Execute the real shipped callback's non-release warning branch.
        releaseBuild = false
        _G.onLoadLuaFileFail('synthetic visible warning')
        assert(g_warningMessages[#g_warningMessages] == 'synthetic visible warning')
    "#,
        )
        .unwrap();
    assert_eq!(
        runtime.pending_persistent_load_messages(),
        ["File Created:later_missing.lua"]
    );
    for _ in 0..3 {
        runtime.update(1.0 / 60.0).unwrap();
    }
    assert_eq!(
        runtime.pending_persistent_load_messages(),
        ["File Created:later_missing.lua"]
    );
}
