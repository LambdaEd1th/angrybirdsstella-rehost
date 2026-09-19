//! Native cloud Lua payload and staged conflict delivery, using loopback only.

use super::{identity_routes::accept_request, storage_session::*, *};

fn login(runtime: &StellaLua) {
    runtime
        .execute_source(
            r#"
        login_done=false
        _G.SkynestAccount.onLoginSuccess=function() login_done=true end
        _G.SkynestAccount.onLoginFailure=function() error("fixture login failed") end
        _G.SkynestAccount.native_login(false,false,false)
    "#,
        )
        .unwrap();
    wait_for(runtime, "login_done");
}

fn cloud_state(hash: &str, value: &str) -> serde_json::Value {
    serde_json::json!([{"hash":hash,"value":value,"encoding":"SDKv1"}])
}

fn assert_cloud_get(request: &str) {
    assert_eq!(
        request.lines().next(),
        Some("GET /storage/1.0/state?key=%5Bmy%5D%2F%5Bclient%5D%2FPurpleState HTTP/1.1")
    );
    assert!(body(request).is_empty());
}

fn cloud_fields(request: &str) -> BTreeMap<String, String> {
    assert_eq!(
        request.lines().next(),
        Some("POST /storage/1.0/state HTTP/1.1")
    );
    let fields = form_fields(body(request));
    assert_eq!(fields["key"], "[my]/[client]/PurpleState");
    assert_eq!(fields["encoding"], "SDKv2");
    assert_eq!(fields["force"], "false");
    fields
}

#[test]
fn cloud_payload_wire_preserves_lua_tables_binary_strings_and_nonfinite_numbers() {
    let listener = listener();
    let sandbox = ShippedDataSandbox::new("cloud-lua-wire");
    let runtime = StellaLua::new(&sandbox.data_root).unwrap();
    configure(&runtime, &listener);
    runtime
        .execute_source(
            r#"
        notifyEventManager=function(name)
            if name=="EID_SYNC_CLOUD_COMPLETED" then save_done=true end
        end
        _G.SkynestStorage.cloudDataSync=function(document)
            assert(document.coins==222)
            assert(document.nested[2]=="second" and document.nested[9]==false)
            assert(document.raw=="a\000\255z")
            assert(document.nan~=document.nan and document.inf==1/0)
            assert(document.remote_only==123)
            assert(remote_only==nil)
            load_done=true
        end
    "#,
        )
        .unwrap();
    let server = thread::spawn(move || {
        acquire(&listener, None, "wire-access");
        let (mut save, request) = accept_request(&listener);
        let fields = cloud_fields(&request);
        let source = decode_sdkv2(&fields["value"]);
        assert!(
            !source.trim_start().starts_with('{'),
            "wire is assignments, not JSON"
        );
        // Independently execute the synthetic serialized result: ordering of
        // top-level table fields is not a wire invariant.
        let lua = mlua::Lua::new();
        let document = lua.create_table().unwrap();
        lua.load(&source)
            .set_environment(document.clone())
            .exec()
            .unwrap();
        assert_eq!(document.get::<i64>("coins").unwrap(), 111);
        assert_eq!(
            document
                .get::<mlua::LuaString>("raw")
                .unwrap()
                .as_bytes()
                .as_ref(),
            b"a\0\xffz"
        );
        assert!(document.get::<f64>("nan").unwrap().is_nan());
        assert_eq!(document.get::<f64>("inf").unwrap(), f64::INFINITY);
        let nested = document.get::<mlua::Table>("nested").unwrap();
        assert_eq!(nested.get::<String>(2).unwrap(), "second");
        assert!(!nested.get::<bool>(9).unwrap());
        respond(&mut save, 200, &serde_json::json!([{"hash":"wire-hash"}]));
        let (mut load, request) = accept_request(&listener);
        assert_cloud_get(&request);
        respond(
            &mut load,
            200,
            &cloud_state(
                "remote-wire-hash",
                r#"coins = 222
nested = {[2] = "second", [9] = false}
raw = "a\000\255z"
nan = 0/0
inf = 1/0
remote_only = 123
"#,
            ),
        );
    });
    login(&runtime);
    runtime
        .execute_source(
            r#"
        assert(_G.SkynestStorage.native_saveCloudSettings({
            coins=111,nested={[2]="second",[9]=false},raw="a\000\255z",nan=0/0,inf=1/0
        }))
    "#,
        )
        .unwrap();
    wait_for(&runtime, "save_done");
    runtime
        .execute_source("assert(_G.SkynestStorage.native_loadCloudSettings())")
        .unwrap();
    wait_for(&runtime, "load_done");
    server.join().unwrap();
}

#[test]
fn cloud_payload_409_fetches_once_then_merges_before_completed_and_allows_explicit_resave() {
    for resave in [false, true] {
        let listener = listener();
        let sandbox = ShippedDataSandbox::new("cloud-conflict-order");
        let runtime = StellaLua::new(&sandbox.data_root).unwrap();
        configure(&runtime, &listener);
        runtime
            .execute_source(
                r#"
            cloud_events={}
            notifyEventManager=function(name)
                if not string.match(name,"^EID_SYNC_CLOUD") then return end
                table.insert(cloud_events,name)
                completed_count=(completed_count or 0)+1
                conflict_done=true
            end
            _G.SkynestStorage.cloudDataNewDataAvailable=function(document)
                assert(not _G.SkynestStorage.native_isTransactionInProcess())
                assert(document.coins==222)
                table.insert(cloud_events,"remote")
                if resave_from_callback then
                    resave_started=_G.SkynestStorage.native_saveCloudSettings({coins=333})
                    assert(resave_started)
                end
            end
        "#,
            )
            .unwrap();
        game_environment(runtime.lua())
            .unwrap()
            .set("resave_from_callback", resave)
            .unwrap();
        let (get_seen_tx, get_seen_rx) = mpsc::channel();
        let (release_get_tx, release_get_rx) = mpsc::channel();
        let (save_seen_tx, save_seen_rx) = mpsc::channel();
        let (release_save_tx, release_save_rx) = mpsc::channel();
        let server = thread::spawn(move || {
            acquire(&listener, None, "conflict-access");
            let (mut save, request) = accept_request(&listener);
            let fields = cloud_fields(&request);
            assert_eq!(fields["hash"], "");
            assert_eq!(decode_sdkv2(&fields["value"]), "coins = 111\n");
            respond(&mut save, 409, &serde_json::json!({}));
            let (mut get, request) = accept_request(&listener);
            assert_cloud_get(&request);
            get_seen_tx.send(()).unwrap();
            release_get_rx.recv_timeout(Duration::from_secs(8)).unwrap();
            respond(&mut get, 200, &cloud_state("remote-hash", "coins = 222\n"));
            if resave {
                let (mut save, request) = accept_request(&listener);
                let fields = cloud_fields(&request);
                assert_eq!(fields["hash"], "remote-hash");
                assert_eq!(decode_sdkv2(&fields["value"]), "coins = 333\n");
                save_seen_tx.send(()).unwrap();
                release_save_rx
                    .recv_timeout(Duration::from_secs(8))
                    .unwrap();
                respond(
                    &mut save,
                    200,
                    &serde_json::json!([{"hash":"explicit-save-hash"}]),
                );
            }
            listener
        });
        login(&runtime);
        runtime
            .execute_source("assert(_G.SkynestStorage.native_saveCloudSettings({coins=111}))")
            .unwrap();
        wait_pending(&runtime);
        runtime
            .execute_source(
                "assert(#cloud_events==0 and _G.SkynestStorage.native_isTransactionInProcess())",
            )
            .unwrap();
        dispatch_registered_application_events(runtime.lua()).unwrap();
        get_seen_rx.recv_timeout(Duration::from_secs(8)).unwrap();
        runtime
            .execute_source(
                "assert(#cloud_events==0 and _G.SkynestStorage.native_isTransactionInProcess())",
            )
            .unwrap();
        release_get_tx.send(()).unwrap();
        wait_pending(&runtime);
        runtime.execute_source("assert(#cloud_events==0)").unwrap();
        dispatch_registered_application_events(runtime.lua()).unwrap();
        runtime
            .execute_source(
                r#"
            assert(cloud_events[1]=="remote")
            assert(cloud_events[2]=="EID_SYNC_CLOUD_COMPLETED")
            assert(#cloud_events==2)
        "#,
            )
            .unwrap();
        if resave {
            save_seen_rx.recv_timeout(Duration::from_secs(8)).unwrap();
            runtime
                .execute_source("assert(_G.SkynestStorage.native_isTransactionInProcess())")
                .unwrap();
            release_save_tx.send(()).unwrap();
            wait_pending(&runtime);
            dispatch_registered_application_events(runtime.lua()).unwrap();
            runtime.execute_source("assert(completed_count==2 and not _G.SkynestStorage.native_isTransactionInProcess())").unwrap();
        } else {
            runtime
                .execute_source("assert(not _G.SkynestStorage.native_isTransactionInProcess())")
                .unwrap();
        }
        assert!(
            matches!(server.join().unwrap().accept(),Err(error) if error.kind()==std::io::ErrorKind::WouldBlock)
        );
    }
}

#[test]
fn cloud_payload_conflict_get_failure_mapping_is_bounded_and_never_auto_overwrites() {
    // None is a malformed HTTP response, producing the native unavailable
    // category without reflecting any private server body into diagnostics.
    for status in [
        None,
        Some(400),
        Some(404),
        Some(401),
        Some(403),
        Some(500),
        Some(409),
    ] {
        let listener = listener();
        let sandbox = ShippedDataSandbox::new("cloud-conflict-get-errors");
        let runtime = StellaLua::new(&sandbox.data_root).unwrap();
        configure(&runtime, &listener);
        // Frozen explicit credentials keep a 401 GET from exercising the
        // unrelated, already-tested account renewal branch.
        runtime
            .set_storage_credentials(Some("explicit-access"), Some("explicit-segment"))
            .unwrap();
        runtime.execute_source(r#"
            cloud_events={}
            notifyEventManager=function(name)
                if string.match(name,"^EID_SYNC_CLOUD") then table.insert(cloud_events,name) end
            end
            _G.SkynestStorage.cloudDataFirstSync=function() error("save conflict is not first sync") end
            _G.SkynestStorage.cloudDataNewDataAvailable=function(document)
                assert(next(document)==nil)
                table.insert(cloud_events,"empty-remote")
            end
        "#).unwrap();
        let (get_seen_tx, get_seen_rx) = mpsc::channel();
        let (release_get_tx, release_get_rx) = mpsc::channel();
        let server = thread::spawn(move || {
            acquire(&listener, None, "account-access");
            let (mut save, request) = accept_request(&listener);
            cloud_fields(&request);
            respond(&mut save, 409, &serde_json::json!({}));
            let (mut get, request) = accept_request(&listener);
            assert_cloud_get(&request);
            get_seen_tx.send(()).unwrap();
            release_get_rx.recv_timeout(Duration::from_secs(8)).unwrap();
            if let Some(status) = status {
                respond(&mut get, status, &serde_json::json!({}));
            } else {
                get.write_all(b"not an HTTP response\r\n\r\n").unwrap();
            }
            listener
        });
        login(&runtime);
        runtime
            .execute_source("assert(_G.SkynestStorage.native_saveCloudSettings({coins=111}))")
            .unwrap();
        wait_pending(&runtime);
        dispatch_registered_application_events(runtime.lua()).unwrap();
        get_seen_rx.recv_timeout(Duration::from_secs(8)).unwrap();
        release_get_tx.send(()).unwrap();
        wait_pending(&runtime);
        dispatch_registered_application_events(runtime.lua()).unwrap();
        runtime
            .execute_source("assert(not _G.SkynestStorage.native_isTransactionInProcess())")
            .unwrap();
        let events = game_environment(runtime.lua())
            .unwrap()
            .get::<mlua::Table>("cloud_events")
            .unwrap();
        if status == Some(409) {
            assert_eq!(events.raw_len(), 2);
            assert_eq!(events.get::<String>(1).unwrap(), "empty-remote");
            assert_eq!(events.get::<String>(2).unwrap(), "EID_SYNC_CLOUD_COMPLETED");
        } else if status.is_some() {
            assert_eq!(events.raw_len(), 1);
            assert_eq!(events.get::<String>(1).unwrap(), "EID_SYNC_CLOUD_COMPLETED");
        } else {
            assert_eq!(events.raw_len(), 0);
        }
        assert!(
            matches!(server.join().unwrap().accept(),Err(error) if error.kind()==std::io::ErrorKind::WouldBlock)
        );
    }
}

#[test]
fn cloud_payload_ordinary_set_key_conflict_retains_zero_argument_adapter() {
    let listener = listener();
    let sandbox = ShippedDataSandbox::new("storage-key-conflict-adapter");
    let runtime = StellaLua::new(&sandbox.data_root).unwrap();
    configure(&runtime, &listener);
    runtime
        .skynest_account
        .seed_test_tokens(false, "key-access", "key-refresh", "key-segment");
    runtime.execute_source(r##"
        notifyEventManager=function(name)
            if string.match(name,"^EID_SYNC_CLOUD") then error("setKey must not emit cloud events") end
        end
        _G.SkynestStorage.cloudDataNewDataAvailable=function() error("setKey must not merge cloud") end
        _G.SkynestStorage.native_setKey("plain","local-value",function(...)
            assert(select("#",...)==0); key_done=true
        end)
    "##).unwrap();
    let server = thread::spawn(move || {
        let (mut save, request) = accept_request(&listener);
        assert_eq!(
            decode_sdkv2(&form_fields(body(&request))["value"]),
            "local-value"
        );
        respond(&mut save, 409, &serde_json::json!({}));
        let (mut get, request) = accept_request(&listener);
        assert_eq!(
            request.lines().next(),
            Some("GET /storage/1.0/state?key=%5Bmy%5D%2F%5Bclient%5D%2Fplain HTTP/1.1")
        );
        respond(
            &mut get,
            200,
            &cloud_state("plain-remote-hash", "remote-value"),
        );
        listener
    });
    wait_for(&runtime, "key_done");
    assert!(
        matches!(server.join().unwrap().accept(),Err(error) if error.kind()==std::io::ErrorKind::WouldBlock)
    );
}

#[test]
fn cloud_payload_conflict_scope_change_discards_pending_fetch_and_late_remote_result() {
    for identity_logout in [false, true] {
        for fetch_started in [false, true] {
            let listener = listener();
            let origin = format!("http://{}", listener.local_addr().unwrap());
            let sandbox = ShippedDataSandbox::new("cloud-conflict-scope-change");
            let runtime = StellaLua::new(&sandbox.data_root).unwrap();
            configure(&runtime, &listener);
            runtime
                .execute_source(
                    r#"
                merged_count=0; completed_count=0
                _G.SkynestStorage.cloudDataNewDataAvailable=function()
                    merged_count=merged_count+1
                end
                notifyEventManager=function(name)
                    if not string.match(name,"^EID_SYNC_CLOUD") then return end
                    assert(name=="EID_SYNC_CLOUD_COMPLETED")
                    completed_count=completed_count+1; cloud_done=true
                end
            "#,
                )
                .unwrap();
            let (old_seen_tx, old_seen_rx) = mpsc::channel();
            let (new_seen_tx, new_seen_rx) = mpsc::channel();
            let (release_old_tx, release_old_rx) = mpsc::channel();
            let (release_new_tx, release_new_rx) = mpsc::channel();
            let server = thread::spawn(move || {
                acquire(&listener, None, "old-account-access");
                let (mut save, request) = accept_request(&listener);
                cloud_fields(&request);
                respond(&mut save, 409, &serde_json::json!({}));
                let mut pending_get = if fetch_started {
                    let (get, request) = accept_request(&listener);
                    assert_cloud_get(&request);
                    Some(get)
                } else {
                    None
                };
                old_seen_tx.send(()).unwrap();
                if identity_logout {
                    acquire(&listener, None, "replacement-access");
                }
                let (mut new, request) = accept_request(&listener);
                let expected = if identity_logout {
                    "POST /storage/1.0/state HTTP/1.1"
                } else {
                    "POST /replacement-storage/1.0/state HTTP/1.1"
                };
                assert_eq!(request.lines().next(), Some(expected));
                let fields = form_fields(body(&request));
                assert_eq!(fields["hash"], "");
                assert_eq!(decode_sdkv2(&fields["value"]), "coins = 222\n");
                new_seen_tx.send(()).unwrap();
                release_old_rx.recv_timeout(Duration::from_secs(8)).unwrap();
                if let Some(get) = pending_get.as_mut() {
                    respond(get, 200, &cloud_state("late-old-hash", "coins = 999\n"));
                }
                release_new_rx.recv_timeout(Duration::from_secs(8)).unwrap();
                respond(
                    &mut new,
                    200,
                    &serde_json::json!([{"hash":"current-cloud-hash"}]),
                );
                let (mut probe, request) = accept_request(&listener);
                assert_eq!(request.lines().next(), Some(expected));
                assert_eq!(form_fields(body(&request))["hash"], "current-cloud-hash");
                respond(
                    &mut probe,
                    200,
                    &serde_json::json!([{"hash":"probe-cloud-hash"}]),
                );
                listener
            });
            login(&runtime);
            runtime
                .execute_source("assert(_G.SkynestStorage.native_saveCloudSettings({coins=111}))")
                .unwrap();
            wait_pending(&runtime);
            if fetch_started {
                dispatch_registered_application_events(runtime.lua()).unwrap();
            }
            old_seen_rx.recv_timeout(Duration::from_secs(8)).unwrap();
            if identity_logout {
                runtime
                    .execute_source("_G.SkynestAccount.native_logout()")
                    .unwrap();
                login(&runtime);
            } else {
                runtime
                    .set_storage_url(&format!("{origin}/replacement-storage/1.0"))
                    .unwrap();
            }
            runtime
                .execute_source("assert(_G.SkynestStorage.native_saveCloudSettings({coins=222}))")
                .unwrap();
            new_seen_rx.recv_timeout(Duration::from_secs(8)).unwrap();
            release_old_tx.send(()).unwrap();
            if fetch_started {
                wait_pending(&runtime);
            }
            dispatch_registered_application_events(runtime.lua()).unwrap();
            runtime
                .execute_source(
                    r#"
                assert(merged_count==0 and completed_count==0)
                assert(_G.SkynestStorage.native_isTransactionInProcess())
            "#,
                )
                .unwrap();
            release_new_tx.send(()).unwrap();
            wait_for(&runtime, "cloud_done");
            runtime
                .execute_source(
                    r#"
                assert(merged_count==0 and completed_count==1)
                assert(not _G.SkynestStorage.native_isTransactionInProcess())
                cloud_done=false
                assert(_G.SkynestStorage.native_saveCloudSettings({coins=333}))
            "#,
                )
                .unwrap();
            wait_for(&runtime, "cloud_done");
            assert!(
                matches!(server.join().unwrap().accept(),Err(error) if error.kind()==std::io::ErrorKind::WouldBlock)
            );
        }
    }
}

#[test]
fn cloud_payload_merge_callback_owner_change_suppresses_only_the_old_completed_event() {
    for change_url in [false, true] {
        let listener = listener();
        let origin = format!("http://{}", listener.local_addr().unwrap());
        let sandbox = ShippedDataSandbox::new("cloud-merge-reentrant-owner");
        let runtime = StellaLua::new(&sandbox.data_root).unwrap();
        configure(&runtime, &listener);
        let storage = runtime.skynest_storage.clone();
        let replacement = format!("{origin}/replacement-storage/1.0");
        let environment = game_environment(runtime.lua()).unwrap();
        environment.set("change_url", change_url).unwrap();
        environment
            .set(
                "replace_storage",
                runtime
                    .lua()
                    .create_function(move |_, ()| storage.set_compatible_url(&replacement))
                    .unwrap(),
            )
            .unwrap();
        runtime.execute_source(r#"
            completed_count=0
            notifyEventManager=function(name)
                if name=="EID_SYNC_CLOUD_COMPLETED" then completed_count=completed_count+1 end
            end
            _G.SkynestStorage.cloudDataNewDataAvailable=function(document)
                assert(document.coins==222 and not _G.SkynestStorage.native_isTransactionInProcess())
                if change_url then replace_storage() else _G.SkynestAccount.native_logout() end
                merged=true
            end
        "#).unwrap();
        let server = thread::spawn(move || {
            acquire(&listener, None, "merge-access");
            let (mut save, request) = accept_request(&listener);
            cloud_fields(&request);
            respond(&mut save, 409, &serde_json::json!({}));
            let (mut get, request) = accept_request(&listener);
            assert_cloud_get(&request);
            respond(
                &mut get,
                200,
                &cloud_state("old-remote-hash", "coins = 222\n"),
            );
            let (mut probe, request) = accept_request(&listener);
            let expected = if change_url {
                "POST /replacement-storage/1.0/state HTTP/1.1"
            } else {
                "POST /storage/1.0/state HTTP/1.1"
            };
            assert_eq!(request.lines().next(), Some(expected));
            let fields = form_fields(body(&request));
            assert_eq!(fields["hash"], "");
            assert_eq!(decode_sdkv2(&fields["value"]), "probe");
            respond(
                &mut probe,
                200,
                &serde_json::json!([{"hash":"new-owner-hash"}]),
            );
        });
        login(&runtime);
        runtime
            .execute_source("assert(_G.SkynestStorage.native_saveCloudSettings({coins=111}))")
            .unwrap();
        wait_for(&runtime, "merged");
        runtime.execute_source("assert(completed_count==0 and not _G.SkynestStorage.native_isTransactionInProcess())").unwrap();
        if !change_url {
            runtime.skynest_account.seed_test_tokens(
                false,
                "new-owner-access",
                "new-owner-refresh",
                "new-owner-segment",
            );
        }
        runtime
            .execute_source(
                r#"
            _G.SkynestStorage.native_setKey("PurpleState","probe",function() probe_done=true end)
        "#,
            )
            .unwrap();
        wait_for(&runtime, "probe_done");
        assert_eq!(environment.get::<i64>("completed_count").unwrap(), 0);
        server.join().unwrap();
    }
}

#[test]
fn cloud_payload_invalid_remote_delivery_clears_busy_without_merging_or_caching_hash() {
    for conflict in [false, true] {
        for source in ["while true do end", "coins = function", r#"{"coins":999}"#] {
            let listener = listener();
            let sandbox = ShippedDataSandbox::new("cloud-invalid-delivery");
            let runtime = StellaLua::new(&sandbox.data_root).unwrap();
            configure(&runtime, &listener);
            runtime.execute_source(r#"
                completed_count=0
                notifyEventManager=function(name)
                    if name=="EID_SYNC_CLOUD_COMPLETED" then completed_count=completed_count+1 end
                end
                _G.SkynestStorage.cloudDataSync=function() error("invalid cloud data was loaded") end
                _G.SkynestStorage.cloudDataNewDataAvailable=function() error("invalid cloud data was merged") end
            "#).unwrap();
            let server = thread::spawn(move || {
                acquire(&listener, None, "invalid-source-access");
                if conflict {
                    let (mut save, request) = accept_request(&listener);
                    cloud_fields(&request);
                    respond(&mut save, 409, &serde_json::json!({}));
                }
                let (mut get, request) = accept_request(&listener);
                assert_cloud_get(&request);
                respond(&mut get, 200, &cloud_state("must-not-publish-hash", source));
                let (mut probe, request) = accept_request(&listener);
                let fields = form_fields(body(&request));
                assert_eq!(fields["key"], "[my]/[client]/PurpleState");
                assert_eq!(fields["hash"], "");
                assert_eq!(decode_sdkv2(&fields["value"]), "probe");
                respond(
                    &mut probe,
                    200,
                    &serde_json::json!([{"hash":"safe-probe-hash"}]),
                );
            });
            login(&runtime);
            runtime
                .execute_source(if conflict {
                    "assert(_G.SkynestStorage.native_saveCloudSettings({coins=111}))"
                } else {
                    "assert(_G.SkynestStorage.native_loadCloudSettings())"
                })
                .unwrap();
            if conflict {
                wait_pending(&runtime);
                dispatch_registered_application_events(runtime.lua()).unwrap();
            }
            wait_pending(&runtime);
            dispatch_registered_application_events(runtime.lua()).unwrap();
            runtime.execute_source(r#"
                assert(completed_count==0)
                assert(not _G.SkynestStorage.native_isTransactionInProcess())
                _G.SkynestStorage.native_setKey("PurpleState","probe",function() probe_done=true end)
            "#).unwrap();
            wait_for(&runtime, "probe_done");
            server.join().unwrap();
        }
    }
}
