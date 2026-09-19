use super::*;

#[test]
fn native_friends_damaged_inner_cache_stops_login_callback_without_erasing_input() {
    let sandbox = ShippedDataSandbox::new("native-friends-invalid-cache");
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let origin = format!("http://{}", listener.local_addr().unwrap());
    let server = thread::spawn(move || {
        let (mut stream, _) = identity_routes::accept_request_including_friends(&listener);
        raw_reply(
            &mut stream,
            200,
            &String::from_utf8(session_response("own")).unwrap(),
        );
        listener
    });
    let runtime = StellaLua::new(&sandbox.data_root).unwrap();
    configure(&runtime, &format!("{origin}/identity/2.0"));
    runtime
        .skynest_account
        .seed_friends_cache_for_test("own", "{");
    runtime
        .execute_source("_G.SkynestAccount.native_login(false,false,false)")
        .unwrap();
    let deadline = std::time::Instant::now() + Duration::from_secs(8);
    let error = loop {
        if let Err(error) = dispatch_registered_application_events(runtime.lua()) {
            break error;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "damaged friends cache was silently accepted"
        );
        thread::sleep(Duration::from_millis(1));
    };
    let listener = server.join().unwrap();
    assert!(
        error
            .to_string()
            .contains("Parsing friends cache JSON failed")
    );
    assert_eq!(
        game_environment(runtime.lua())
            .unwrap()
            .get::<i32>("friend_login_count")
            .unwrap(),
        0
    );
    assert_eq!(
        runtime.skynest_account.read_friends_cache_for_test("own"),
        "{"
    );
    assert!(matches!(listener.accept(),Err(e) if e.kind()==std::io::ErrorKind::WouldBlock));
}

#[test]
fn native_friends_empty_refresh_clears_relations_without_search_or_progress_request() {
    let sandbox = ShippedDataSandbox::new("native-friends-empty-refresh");
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let origin = format!("http://{}", listener.local_addr().unwrap());
    let server = thread::spawn(move || {
        let (mut stream, _) = identity_routes::accept_request_including_friends(&listener);
        raw_reply(
            &mut stream,
            200,
            &String::from_utf8(session_response("own")).unwrap(),
        );
        drop(stream);
        let (mut stream, request) = identity_routes::accept_request_including_friends(&listener);
        assert!(request.starts_with("GET /identity/2.0/friends HTTP/1.1"));
        raw_reply(&mut stream, 200, r#"{"socialFriends":[]}"#);
        listener
    });
    let runtime = StellaLua::new(&sandbox.data_root).unwrap();
    configure(&runtime, &format!("{origin}/identity/2.0"));
    runtime
        .set_storage_url(&format!("{origin}/storage/1.0"))
        .unwrap();
    runtime.skynest_account.seed_friends_cache_for_test(
        "own",
        r#"{"friends":[{"accountId":"old","nickName":"Old"}]}"#,
    );
    runtime.execute_source("native_progress_count=0; _G.SocialManager.onFriendsProgressUpdated=function() native_progress_count=native_progress_count+1 end").unwrap();
    login(&runtime, 1);
    wait_refresh(&runtime);
    let listener = server.join().unwrap();
    assert_eq!(
        game_environment(runtime.lua())
            .unwrap()
            .get::<i32>("native_progress_count")
            .unwrap(),
        0
    );
    let saved: serde_json::Value =
        serde_json::from_str(&runtime.skynest_account.read_friends_cache_for_test("own")).unwrap();
    assert_eq!(saved, json!({"friends":[],"socialNetworkFriends":[]}));
    assert!(matches!(listener.accept(),Err(e) if e.kind()==std::io::ErrorKind::WouldBlock));
}

#[test]
fn native_friends_progress_takes_first_fifty_ids_before_filtering_unnamed_records() {
    let sandbox = ShippedDataSandbox::new("native-friends-fifty-progress");
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let origin = format!("http://{}", listener.local_addr().unwrap());
    let server = thread::spawn(move || {
        let (mut stream, _) = identity_routes::accept_request_including_friends(&listener);
        raw_reply(
            &mut stream,
            200,
            &String::from_utf8(session_response("own")).unwrap(),
        );
        drop(stream);
        let (mut stream, request) = identity_routes::accept_request_including_friends(&listener);
        assert!(request.starts_with("GET /identity/2.0/friends HTTP/1.1"));
        raw_reply(&mut stream, 503, "");
        drop(stream);
        let (mut stream, request) = identity_routes::accept_request_including_friends(&listener);
        assert!(request.starts_with("POST /storage/1.0/states/query HTTP/1.1"));
        let body: serde_json::Value =
            serde_json::from_str(storage_session::body(&request)).unwrap();
        assert_eq!(
            body["accountIds"],
            json!((0..50).map(|i| format!("{i:02}")).collect::<Vec<_>>())
        );
        let item = |id: &str| json!({"accountId":id,"states":[{"value":"","encoding":"SDKv1"}]});
        raw_reply(
            &mut stream,
            200,
            &json!({"result":[item("00"),item("49"),item("50"),item("unknown")]}).to_string(),
        );
    });
    let runtime = StellaLua::new(&sandbox.data_root).unwrap();
    configure(&runtime, &format!("{origin}/identity/2.0"));
    runtime
        .set_storage_url(&format!("{origin}/storage/1.0"))
        .unwrap();
    let friends=(0..60).rev().map(|i|json!({"accountId":format!("{i:02}"),"nickName":if i==0 {String::new()} else {format!("Name {i}")}})).collect::<Vec<_>>();
    runtime
        .skynest_account
        .seed_friends_cache_for_test("own", &json!({"friends":friends}).to_string());
    runtime.execute_source("native_progress_count=0; _G.SocialManager.onFriendsProgressUpdated=function(ok,values) native_progress_count=native_progress_count+1; native_progress_friends=values; native_progress_success=ok end").unwrap();
    login(&runtime, 1);
    wait_refresh(&runtime);
    runtime
        .execute_source("_G.SocialManager.native_getFriendsProgress()")
        .unwrap();
    wait_progress(&runtime, 1);
    server.join().unwrap();
    let env = game_environment(runtime.lua()).unwrap();
    assert!(env.get::<bool>("native_progress_success").unwrap());
    let friends = env.get::<mlua::Table>("native_progress_friends").unwrap();
    assert_eq!(friends.raw_len(), 2);
    // Native looks up the returned ID in the whole current store; it does not
    // intersect the response with the request's first-fifty vector.
    assert_eq!(
        friends
            .raw_get::<mlua::Table>(1)
            .unwrap()
            .get::<String>("accountId")
            .unwrap(),
        "49"
    );
    assert_eq!(
        friends
            .raw_get::<mlua::Table>(2)
            .unwrap()
            .get::<String>("accountId")
            .unwrap(),
        "50"
    );
}

fn wait_refresh(runtime: &StellaLua) {
    let deadline = std::time::Instant::now() + Duration::from_secs(8);
    while runtime.social.native_friends_completions_for_test() == 0 {
        assert!(
            std::time::Instant::now() < deadline,
            "native friends failure callback missing"
        );
        dispatch_registered_application_events(runtime.lua()).unwrap();
        thread::sleep(Duration::from_millis(1));
    }
}

#[test]
fn native_friends_errors_preserve_cache_and_do_not_fake_progress_completion() {
    for (get_status, get_body, search) in [
        (201, "", None),
        (200, "{", None),
        (200, r#"{"socialFriends":[{"id":9}]}"#, None),
        (200, r#"{"socialFriends":[{"id":"new"}]}"#, Some((201, ""))),
        (
            200,
            r#"{"socialFriends":[{"id":"new"}]}"#,
            Some((200, " \n")),
        ),
    ] {
        let sandbox = ShippedDataSandbox::new("native-friends-error");
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let origin = format!("http://{}", listener.local_addr().unwrap());
        let server = thread::spawn(move || {
            let (mut stream, request) =
                identity_routes::accept_request_including_friends(&listener);
            assert!(request.starts_with("POST /session/1/apps/friends-fixture/sessions HTTP/1.1"));
            raw_reply(
                &mut stream,
                200,
                &String::from_utf8(session_response("own")).unwrap(),
            );
            drop(stream);
            let (mut stream, request) =
                identity_routes::accept_request_including_friends(&listener);
            assert!(request.starts_with("GET /identity/2.0/friends HTTP/1.1"));
            raw_reply(&mut stream, get_status, get_body);
            drop(stream);
            if let Some((status, body)) = search {
                let (mut stream, request) =
                    identity_routes::accept_request_including_friends(&listener);
                assert!(request.starts_with("POST /identity/2.0/profile/search HTTP/1.1"));
                assert_eq!(storage_session::body(&request), "publicAccountId=new");
                raw_reply(&mut stream, status, body);
            }
        });
        let runtime = StellaLua::new(&sandbox.data_root).unwrap();
        configure(&runtime, &format!("{origin}/identity/2.0"));
        let cache = r#"{"friends":[{"accountId":"old","nickName":"Old"}]}"#;
        runtime
            .skynest_account
            .seed_friends_cache_for_test("own", cache);
        runtime.execute_source("native_progress_count=0; _G.SocialManager.onFriendsProgressUpdated=function() native_progress_count=native_progress_count+1 end").unwrap();
        login(&runtime, 1);
        wait_refresh(&runtime);
        server.join().unwrap();
        runtime
            .execute_source("remaining_friends=_G.SocialManager.native_getFriends()")
            .unwrap();
        let env = game_environment(runtime.lua()).unwrap();
        assert_eq!(env.get::<i32>("native_progress_count").unwrap(), 0);
        assert_eq!(
            env.get::<mlua::Table>("remaining_friends")
                .unwrap()
                .raw_get::<mlua::Table>(1)
                .unwrap()
                .get::<String>("accountId")
                .unwrap(),
            "old"
        );
        assert_eq!(
            runtime.skynest_account.read_friends_cache_for_test("own"),
            cache
        );
    }
}

#[test]
fn native_friends_pending_search_cannot_write_or_publish_after_provider_replacement() {
    let sandbox = ShippedDataSandbox::new("native-friends-retired-search");
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let origin = format!("http://{}", listener.local_addr().unwrap());
    let (seen_tx, seen_rx) = std::sync::mpsc::channel();
    let (release_tx, release_rx) = std::sync::mpsc::channel();
    let server = thread::spawn(move || {
        let (mut stream, _) = identity_routes::accept_request_including_friends(&listener);
        raw_reply(
            &mut stream,
            200,
            &String::from_utf8(session_response("own")).unwrap(),
        );
        drop(stream);
        let (mut stream, request) = identity_routes::accept_request_including_friends(&listener);
        assert!(request.starts_with("GET /identity/2.0/friends HTTP/1.1"));
        raw_reply(
            &mut stream,
            200,
            r#"{"socialFriends":[{"id":"late","socialNetworks":[{"provider":"gamecenter","networkId":"late-id","socialAttributes":{"name":"Late"}}]}]}"#,
        );
        drop(stream);
        let (mut stream, request) = identity_routes::accept_request_including_friends(&listener);
        assert!(request.starts_with("POST /identity/2.0/profile/search HTTP/1.1"));
        storage_session::assert_auth(&request, "synthetic-friend-access", "8, 2");
        seen_tx.send(()).unwrap();
        release_rx.recv_timeout(Duration::from_secs(8)).unwrap();
        raw_reply(&mut stream, 200, r#"{"publicAccountId":"late"}"#);
    });
    let runtime = StellaLua::new(&sandbox.data_root).unwrap();
    configure(&runtime, &format!("{origin}/identity/2.0"));
    runtime.skynest_account.seed_friends_cache_for_test(
        "own",
        r#"{"friends":[{"accountId":"old","nickName":"Old"}]}"#,
    );
    login(&runtime, 1);
    seen_rx.recv_timeout(Duration::from_secs(8)).unwrap();
    let old_path = runtime.skynest_account.registry_path_for_test();
    let bytes = std::fs::read(&old_path).unwrap();
    let old_friends_path = old_path.parent().unwrap().join("skynest_friends_store_own");
    let old_friends_bytes = std::fs::read(&old_friends_path).unwrap();
    let completion_probe = runtime.social.online_completion_count_probe();
    let replacement = TcpListener::bind("127.0.0.1:0").unwrap();
    replacement.set_nonblocking(true).unwrap();
    runtime
        .set_identity_url(&format!(
            "http://{}/identity/2.0",
            replacement.local_addr().unwrap()
        ))
        .unwrap();
    runtime
        .execute_source("remaining_friends=_G.SocialManager.native_getFriends()")
        .unwrap();
    assert_eq!(
        game_environment(runtime.lua())
            .unwrap()
            .get::<mlua::Table>("remaining_friends")
            .unwrap()
            .raw_len(),
        0
    );
    release_tx.send(()).unwrap();
    server.join().unwrap();
    let deadline = std::time::Instant::now() + Duration::from_secs(8);
    while completion_probe() == 0 {
        assert!(std::time::Instant::now() < deadline);
        thread::sleep(Duration::from_millis(1));
    }
    dispatch_registered_application_events(runtime.lua()).unwrap();
    runtime
        .execute_source("remaining_friends=_G.SocialManager.native_getFriends()")
        .unwrap();
    assert_eq!(
        game_environment(runtime.lua())
            .unwrap()
            .get::<mlua::Table>("remaining_friends")
            .unwrap()
            .raw_len(),
        0
    );
    assert_eq!(std::fs::read(old_path).unwrap(), bytes);
    assert_eq!(std::fs::read(old_friends_path).unwrap(), old_friends_bytes);
    assert!(matches!(replacement.accept(),Err(e) if e.kind()==std::io::ErrorKind::WouldBlock));
}
