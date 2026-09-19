use super::*;

#[test]
fn native_platform_new_account_session_retires_old_pending_connection_without_stalling() {
    let sandbox = ShippedDataSandbox::new("native-platform-account-replacement");
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let origin = format!("http://{}", listener.local_addr().unwrap());
    let (arrived, phase) = std::sync::mpsc::channel();
    let server = thread::spawn(move || {
        let (mut stream, _) = identity_routes::accept_request_including_friends(&listener);
        raw_reply(&mut stream, 200, &linked_session());
        drop(stream);
        reply_initial_profiles(&listener, r#"{"id":"platform-own","name":"Own"}"#);
        let (mut old_ids, request) = identity_routes::accept_request_including_friends(&listener);
        assert!(request.starts_with("GET /v2.0//me/friends?"));
        arrived.send(()).unwrap();
        let (mut stream, request) = identity_routes::accept_request_including_friends(&listener);
        assert!(
            request.starts_with("POST /identity/3.0/abid/login "),
            "{request}"
        );
        storage_session::assert_auth(&request, "synthetic-friend-access", "8, 2");
        raw_reply(
            &mut stream,
            200,
            r#"{"accessToken":"replacement-access","refreshToken":"replacement-refresh","expiresIn":3600,"segment":"replacement-segment"}"#,
        );
        drop(stream);
        let mut replacement = linked_profile();
        replacement["publicAccountId"] = json!("replacement");
        let (mut stream, request) = identity_routes::accept_request_including_friends(&listener);
        assert!(request.starts_with("GET /identity/3.0/profile/own "));
        raw_reply(&mut stream, 200, &replacement.to_string());
        drop(stream);
        // This must arrive while the first account's request is still held.
        let (mut fresh_ids, request) = identity_routes::accept_request_including_friends(&listener);
        assert!(request.starts_with("GET /v2.0//me/friends?"), "{request}");
        raw_reply(
            &mut old_ids,
            200,
            r#"{"data":[{"id":"must-not-sync-old-id"}]}"#,
        );
        drop(old_ids);
        raw_reply(&mut fresh_ids, 200, r#"{"data":[]}"#);
        drop(fresh_ids);
        for (expected, body) in [
            ("POST /identity/2.0/external/connect ", String::new()),
            ("GET /identity/3.0/profile/own ", replacement.to_string()),
            (
                "GET /identity/2.0/friends ",
                r#"{"socialFriends":[]}"#.to_owned(),
            ),
            (
                "GET /v2.0//me/friends?",
                r#"{"data":[{"id":"new-platform-only","name":"New"}]}"#.to_owned(),
            ),
        ] {
            let (mut stream, request) =
                identity_routes::accept_request_including_friends(&listener);
            assert!(request.starts_with(expected), "{request}");
            if !expected.contains("v2.0") {
                assert!(
                    request
                        .to_lowercase()
                        .contains("x-access-token: replacement-access")
                );
            }
            if expected.starts_with("POST") {
                storage_session::assert_auth(&request, "replacement-access", "replacement-segment");
            }
            assert!(!request.contains("must-not-sync-old-id"));
            raw_reply(&mut stream, 200, &body);
        }
        listener
    });
    let runtime = platform_runtime(&sandbox, &origin);
    login(&runtime, 1);
    wait_signal(&runtime, &phase);
    runtime
        .execute_source("_G.SkynestAccount.native_login(true,true,false)")
        .unwrap();
    let ui = runtime.account_ui().unwrap();
    assert_eq!(ui.view, AccountView::SignIn);
    assert!(
        runtime
            .submit_account_login(ui.id, "replacement@example.invalid", "synthetic-password")
            .unwrap()
    );
    let mut completed = false;
    for _ in 0..1500 {
        dispatch_registered_application_events(runtime.lua()).unwrap();
        let saved = runtime
            .skynest_account
            .read_friends_cache_for_test("replacement");
        if saved.contains("new-platform-only") {
            completed = true;
            break;
        }
        thread::sleep(Duration::from_millis(2));
    }
    assert!(
        completed,
        "new SDK event inherited the old pending connection"
    );
    let listener = server.join().unwrap();
    assert!(matches!(listener.accept(),Err(e) if e.kind()==std::io::ErrorKind::WouldBlock));
    assert_eq!(
        game_environment(runtime.lua())
            .unwrap()
            .get::<i32>("friend_login_count")
            .unwrap(),
        2
    );
    assert_eq!(runtime.social.platform_state_for_test(), (0, false, true));
    assert_eq!(
        runtime.skynest_account.friends_profile_for_test().unwrap()["publicAccountId"],
        "replacement"
    );
    assert_eq!(
        runtime.skynest_account.read_friends_cache_for_test("own"),
        r#"{"friends":[{"accountId":"cached","nickName":"Cached"}]}"#
    );
}

#[test]
fn native_platform_retirement_prevents_late_personal_asset_bytes_and_cache_version_publication() {
    let sandbox = ShippedDataSandbox::new("native-platform-avatar-retirement");
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let origin = format!("http://{}", listener.local_addr().unwrap());
    let worker_origin = origin.clone();
    let (arrived, phase) = std::sync::mpsc::channel();
    let (release, resume) = std::sync::mpsc::channel();
    let server = thread::spawn(move || {
        let (mut stream, _) = identity_routes::accept_request_including_friends(&listener);
        raw_reply(&mut stream, 200, &linked_session());
        drop(stream);
        reply_initial_profiles(&listener, r#"{"id":"platform-own","name":"Own"}"#);
        for (expected, body) in [
            ("GET /v2.0//me/friends?", r#"{"data":[]}"#),
            ("POST /identity/2.0/external/connect ", ""),
        ] {
            let (mut stream, request) =
                identity_routes::accept_request_including_friends(&listener);
            assert!(request.starts_with(expected));
            raw_reply(&mut stream, 200, body);
        }
        let (mut stream, request) = identity_routes::accept_request_including_friends(&listener);
        assert!(request.starts_with("GET /identity/3.0/profile/own "));
        let mut profile = linked_profile();
        profile["personal"]["imageAssets"] = json!([{"url":format!("{worker_origin}/late.bin"),"hash":"must-not-publish-version","size":4,"dimension":64}]);
        raw_reply(&mut stream, 200, &profile.to_string());
        drop(stream);
        let (mut stream, request) = identity_routes::accept_request_including_friends(&listener);
        assert!(request.starts_with("GET /late.bin "));
        assert!(!request.to_lowercase().contains("x-access-token"));
        arrived.send(()).unwrap();
        resume.recv_timeout(Duration::from_secs(8)).unwrap();
        raw_reply(&mut stream, 200, "LATE");
        listener
    });
    let runtime = platform_runtime(&sandbox, &origin);
    login(&runtime, 1);
    wait_signal(&runtime, &phase);
    let registry = runtime.skynest_account.registry_path_for_test();
    let file = registry.parent().unwrap().join("avatarAssets/late.bin");
    assert_eq!(
        std::fs::read(&file).unwrap(),
        b"",
        "native destination opens before HTTP"
    );
    let registry_before = std::fs::read(&registry).unwrap();
    let queue = runtime.social.online_completion_count_probe();
    dispatch_registered_application_events(runtime.lua()).unwrap();
    assert_eq!(queue(), 0);
    runtime.set_facebook_session(None).unwrap();
    release.send(()).unwrap();
    let listener = server.join().unwrap();
    for _ in 0..500 {
        if queue() > 0 {
            break;
        }
        thread::sleep(Duration::from_millis(2));
    }
    assert!(queue() > 0);
    dispatch_registered_application_events(runtime.lua()).unwrap();
    assert_eq!(std::fs::read(file).unwrap(), b"");
    assert_eq!(
        std::fs::read(registry).unwrap(),
        registry_before,
        "late asset must not store its version"
    );
    assert_eq!(
        runtime.skynest_account.read_friends_cache_for_test("own"),
        r#"{"friends":[{"accountId":"cached","nickName":"Cached"}]}"#
    );
    assert!(matches!(listener.accept(),Err(e) if e.kind()==std::io::ErrorKind::WouldBlock));
}

#[test]
fn native_platform_provider_removal_retires_blocked_connect_before_profile_or_friends_writes() {
    for block_profile in [false, true] {
        retirement_at_phase(block_profile);
    }
}

fn retirement_at_phase(block_profile: bool) {
    let sandbox = ShippedDataSandbox::new("native-platform-retirement");
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let origin = format!("http://{}", listener.local_addr().unwrap());
    let (arrived, phase) = std::sync::mpsc::channel();
    let (release, resume) = std::sync::mpsc::channel();
    let server = thread::spawn(move || {
        let (mut stream, _) = identity_routes::accept_request_including_friends(&listener);
        raw_reply(&mut stream, 200, &linked_session());
        drop(stream);
        reply_initial_profiles(&listener, r#"{"id":"platform-own","name":"Own"}"#);
        let (mut stream, request) = identity_routes::accept_request_including_friends(&listener);
        assert!(request.starts_with("GET /v2.0//me/friends?"));
        raw_reply(&mut stream, 200, r#"{"data":[]}"#);
        drop(stream);
        let (mut stream, request) = identity_routes::accept_request_including_friends(&listener);
        assert!(request.starts_with("POST /identity/2.0/external/connect "));
        if block_profile {
            raw_reply(&mut stream, 200, "");
            drop(stream);
            let (profile_stream, request) =
                identity_routes::accept_request_including_friends(&listener);
            assert!(request.starts_with("GET /identity/3.0/profile/own "));
            stream = profile_stream;
        }
        arrived.send(()).unwrap();
        resume.recv_timeout(Duration::from_secs(8)).unwrap();
        raw_reply(
            &mut stream,
            200,
            if block_profile {
                r#"{"publicAccountId":"late-other-account","personal":{"nickName":"Must not publish"}}"#
            } else {
                ""
            },
        );
        listener
    });
    let runtime = platform_runtime(&sandbox, &origin);
    login(&runtime, 1);
    wait_signal(&runtime, &phase);
    let queue = runtime.social.online_completion_count_probe();
    dispatch_registered_application_events(runtime.lua()).unwrap();
    assert_eq!(queue(), 0);
    let old_profile = runtime.skynest_account.friends_profile_for_test().unwrap();
    runtime.set_facebook_session(None).unwrap();
    release.send(()).unwrap();
    let listener = server.join().unwrap();
    for _ in 0..500 {
        if queue() > 0 {
            break;
        }
        thread::sleep(Duration::from_millis(2));
    }
    assert!(queue() > 0, "retired worker actually finished");
    dispatch_registered_application_events(runtime.lua()).unwrap();
    assert_eq!(
        runtime.skynest_account.friends_profile_for_test().unwrap(),
        old_profile
    );
    assert_eq!(
        runtime.skynest_account.read_friends_cache_for_test("own"),
        r#"{"friends":[{"accountId":"cached","nickName":"Cached"}]}"#
    );
    assert_eq!(runtime.social.platform_state_for_test(), (0, false, false));
    assert_eq!(
        game_environment(runtime.lua())
            .unwrap()
            .get::<i32>("native_progress_count")
            .unwrap(),
        0
    );
    assert!(matches!(listener.accept(), Err(e) if e.kind()==std::io::ErrorKind::WouldBlock));
}
