use super::*;

#[test]
fn native_platform_logout_selects_first_external_provider_even_before_friends_initialization() {
    let cases = [
        (json!([{"provider":"facebook","id":"platform-own"}]), true),
        (json!([{"provider":"facebook","id":""}]), true),
        (json!([{"provider":"facebook","id":false}]), true),
        (
            json!([{"provider":"sinaweibo","id":"sina"},{"provider":"facebook","id":"platform-own"}]),
            false,
        ),
        (
            json!([{"provider":"unknown","id":"odd"},{"provider":"facebook","id":"platform-own"}]),
            false,
        ),
        (
            json!([null,{"provider":"facebook","id":"platform-own"}]),
            false,
        ),
        (json!([]), false),
    ];
    for (external, logout_facebook) in cases {
        let sandbox = ShippedDataSandbox::new("native-selected-platform-logout");
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let origin = format!("http://{}", listener.local_addr().unwrap());
        let server = thread::spawn(move || {
            let (mut stream, _) = identity_routes::accept_request_including_friends(&listener);
            let mut session: serde_json::Value = serde_json::from_str(&linked_session()).unwrap();
            session["profile"]["externalNetworks"] = external;
            raw_reply(&mut stream, 200, &session.to_string());
            drop(stream);
            let (mut stream, request) =
                identity_routes::accept_request_including_friends(&listener);
            assert!(request.starts_with("GET /identity/2.0/friends "));
            raw_reply(&mut stream, 503, "");
            drop(stream);
            let (mut stream, request) =
                identity_routes::accept_request_including_friends(&listener);
            assert!(request.starts_with("GET /v2.0/me?"), "{request}");
            raw_reply(&mut stream, 200, r#"{"id":"platform-own","name":"Own"}"#);
            listener
        });
        let first = StellaLua::new(&sandbox.data_root).unwrap();
        configure(&first, &format!("{origin}/identity/3.0"));
        login(&first, 1);
        drop(first);
        // Real saved identity is selected lazily during native_logout. No
        // FriendsStore or login callback has been constructed in this runtime.
        let runtime = StellaLua::new(&sandbox.data_root).unwrap();
        configure(&runtime, &format!("{origin}/identity/3.0"));
        let graph = Arc::new(
            FacebookGraphSession::new(&format!("{origin}/v2.0"), "synthetic-logout").unwrap(),
        );
        graph.user_profile().unwrap();
        runtime.set_facebook_session(Some(graph.clone())).unwrap();
        let original: mlua::Function = runtime
            .lua()
            .named_registry_value("stella.social.account-logout")
            .unwrap();
        let account = runtime.skynest_account.clone();
        let observed_graph = graph.clone();
        let called = Arc::new(AtomicBool::new(false));
        let observed = called.clone();
        runtime
            .lua()
            .set_named_registry_value(
                "stella.social.account-logout",
                runtime
                    .lua()
                    .create_function(move |_, network: i32| {
                        assert_eq!(
                            account.friends_profile_for_test().unwrap()["publicAccountId"],
                            "own"
                        );
                        assert!(observed_graph.is_logged_in());
                        original.call::<()>(network)?;
                        assert_eq!(observed_graph.is_logged_in(), !logout_facebook);
                        assert_eq!(
                            account.friends_profile_for_test().unwrap()["publicAccountId"],
                            "own",
                            "base identity cleared before platform logout finished"
                        );
                        observed.store(true, Ordering::Release);
                        Ok(())
                    })
                    .unwrap(),
            )
            .unwrap();
        runtime
            .execute_source("_G.SkynestAccount.native_logout()")
            .unwrap();
        assert!(called.load(Ordering::Acquire));
        assert!(runtime.skynest_account.friends_profile_for_test().is_none());
        assert_eq!(graph.is_logged_in(), !logout_facebook);
        assert_eq!(
            game_environment(runtime.lua())
                .unwrap()
                .get::<i32>("friend_login_count")
                .unwrap(),
            0
        );
        if !logout_facebook {
            assert_eq!(graph.user_profile().unwrap().user.id, "platform-own");
        }
        let listener = server.join().unwrap();
        assert!(
            matches!(listener.accept(), Err(e) if e.kind()==std::io::ErrorKind::WouldBlock),
            "ordinary logout must not send external/disconnect or another Graph request"
        );
    }
}

#[test]
fn native_platform_logout_retires_real_inflight_friend_ids_before_identity_clear() {
    let sandbox = ShippedDataSandbox::new("native-platform-logout-inflight");
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let origin = format!("http://{}", listener.local_addr().unwrap());
    let (arrived, phase) = mpsc::channel();
    let (release, resume) = mpsc::channel();
    let server = thread::spawn(move || {
        let (mut stream, _) = identity_routes::accept_request_including_friends(&listener);
        raw_reply(&mut stream, 200, &linked_session());
        drop(stream);
        reply_initial_profiles(&listener, r#"{"id":"platform-own","name":"Own"}"#);
        let (mut stream, request) = identity_routes::accept_request_including_friends(&listener);
        assert!(request.starts_with("GET /v2.0//me/friends?"));
        arrived.send(()).unwrap();
        resume.recv_timeout(Duration::from_secs(8)).unwrap();
        raw_reply(
            &mut stream,
            200,
            r#"{"data":[{"id":"must-not-sync","name":"Late"}]}"#,
        );
        listener
    });
    let runtime = platform_runtime(&sandbox, &origin);
    let graph = Arc::new(
        FacebookGraphSession::new(&format!("{origin}/v2.0"), "synthetic-logout-pending").unwrap(),
    );
    runtime.set_facebook_session(Some(graph.clone())).unwrap();
    login(&runtime, 1);
    wait_signal(&runtime, &phase);
    assert_eq!(runtime.social.platform_state_for_test(), (1, true, false));
    let env = game_environment(runtime.lua()).unwrap();
    // Initial availability already reported the linked profile independently
    // of the held SDK connect request. Logout must permit no further success.
    assert_eq!(env.get::<i32>("platform_connected_count").unwrap(), 1);
    assert_eq!(env.get::<i32>("native_progress_count").unwrap(), 0);
    runtime
        .execute_source("_G.SkynestAccount.native_logout()")
        .unwrap();
    assert!(!graph.is_logged_in());
    assert!(runtime.skynest_account.friends_profile_for_test().is_none());
    release.send(()).unwrap();
    let listener = server.join().unwrap();
    let mut finished = false;
    for _ in 0..1500 {
        if (runtime.social.online_completion_count_probe())() > 0 {
            finished = true;
            break;
        }
        thread::sleep(Duration::from_millis(2));
    }
    assert!(finished, "late worker did not actually finish");
    for _ in 0..4 {
        dispatch_registered_application_events(runtime.lua()).unwrap();
    }
    assert_eq!(runtime.social.platform_state_for_test(), (0, false, false));
    assert_eq!(env.get::<i32>("platform_connected_count").unwrap(), 1);
    assert_eq!(env.get::<i32>("native_progress_count").unwrap(), 0);
    assert_eq!(
        runtime.skynest_account.read_friends_cache_for_test("own"),
        r#"{"friends":[{"accountId":"cached","nickName":"Cached"}]}"#
    );
    assert!(matches!(
        graph.user_profile(),
        Err(SocialPlatformError::NotLoggedIn)
    ));
    assert!(matches!(listener.accept(), Err(e) if e.kind()==std::io::ErrorKind::WouldBlock));
}
