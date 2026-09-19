use super::*;

#[test]
fn native_platform_resume_defers_recheck_and_runs_real_sync_without_new_lua_login() {
    let sandbox = ShippedDataSandbox::new("native-platform-resume");
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let origin = format!("http://{}", listener.local_addr().unwrap());
    let (arrived, phase) = mpsc::channel();
    let (release, resume) = mpsc::channel();
    let server = thread::spawn(move || {
        let (mut stream, _) = identity_routes::accept_request_including_friends(&listener);
        raw_reply(&mut stream, 200, &linked_session());
        drop(stream);
        let (mut stream, request) = identity_routes::accept_request_including_friends(&listener);
        assert!(request.starts_with("GET /identity/2.0/friends "));
        raw_reply(&mut stream, 503, "");
        drop(stream);
        arrived.send(()).unwrap();
        // This phase is released only after the caller verifies that posting
        // the native activation event performed no Graph HTTP synchronously.
        resume.recv_timeout(Duration::from_secs(8)).unwrap();
        assert!(matches!(listener.accept(), Err(e) if e.kind()==std::io::ErrorKind::WouldBlock));
        arrived.send(()).unwrap();
        let (mut stream, request) = identity_routes::accept_request_including_friends(&listener);
        assert!(request.starts_with("GET /v2.0/me?"));
        raw_reply(
            &mut stream,
            200,
            r#"{"id":"platform-own","name":"Own on resume"}"#,
        );
        drop(stream);
        let (mut stream, request) = identity_routes::accept_request_including_friends(&listener);
        assert!(request.starts_with("GET /v2.0//me/friends?"));
        arrived.send(()).unwrap();
        resume.recv_timeout(Duration::from_secs(8)).unwrap();
        // Repeated resume while connecting consults the completed profile
        // cache and rejects the overlapping connect, preserving the first job.
        assert!(matches!(listener.accept(), Err(e) if e.kind()==std::io::ErrorKind::WouldBlock));
        raw_reply(&mut stream, 200, r#"{"data":[{"id":"platform-only"}]}"#);
        drop(stream);
        for (prefix, body) in [
            ("POST /identity/2.0/external/connect ", String::new()),
            ("POST /identity/2.0/friends ", String::new()),
            (
                "GET /identity/3.0/profile/own ",
                linked_profile().to_string(),
            ),
            (
                "GET /identity/2.0/friends ",
                r#"{"socialFriends":[]}"#.to_owned(),
            ),
            (
                "GET /v2.0//me/friends?",
                r#"{"data":[{"id":"platform-only","name":"Resumed Platform Friend"}]}"#.to_owned(),
            ),
        ] {
            let (mut stream, request) =
                identity_routes::accept_request_including_friends(&listener);
            assert!(request.starts_with(prefix), "{request}");
            if prefix.contains("external/connect") {
                assert!(request.contains("synthetic-resume"));
                assert!(!request.contains("synthetic-friend-refresh"));
            }
            if prefix == "POST /identity/2.0/friends " {
                assert!(request.ends_with("networkId=platform-only&networkProvider=facebook"));
            }
            raw_reply(&mut stream, 200, &body);
        }
        listener
    });
    let runtime = StellaLua::new(&sandbox.data_root).unwrap();
    configure(&runtime, &format!("{origin}/identity/3.0"));
    let graph = Arc::new(SwitchedGraph {
        open: AtomicBool::new(false),
        graph: Arc::new(
            FacebookGraphSession::new(&format!("{origin}/v2.0"), "synthetic-resume").unwrap(),
        ),
    });
    runtime.set_facebook_session(Some(graph.clone())).unwrap();
    runtime.execute_source("platform_connected_count=0; _G.SocialManager.onSocialNetworkConnected=function() platform_connected_count=platform_connected_count+1 end").unwrap();
    login(&runtime, 1);
    wait_signal(&runtime, &phase);
    graph.open.store(true, Ordering::Release);
    runtime.post_application_resumed();
    release.send(()).unwrap();
    // No scheduler drain until the server confirms there was no eager request.
    phase.recv_timeout(Duration::from_secs(8)).unwrap();
    wait_signal(&runtime, &phase);
    assert_eq!(runtime.social.platform_state_for_test(), (0, true, false));
    runtime.post_application_resumed();
    dispatch_registered_application_events(runtime.lua()).unwrap();
    assert_eq!(runtime.social.platform_state_for_test(), (0, true, false));
    release.send(()).unwrap();
    let mut completed = false;
    for _ in 0..1500 {
        dispatch_registered_application_events(runtime.lua()).unwrap();
        if runtime
            .skynest_account
            .read_friends_cache_for_test("own")
            .contains("Resumed Platform Friend")
        {
            completed = true;
            break;
        }
        thread::sleep(Duration::from_millis(2));
    }
    assert!(completed, "resume never completed real platform sync/merge");
    let listener = server.join().unwrap();
    assert!(matches!(listener.accept(), Err(e) if e.kind()==std::io::ErrorKind::WouldBlock));
    assert_eq!(runtime.social.platform_state_for_test(), (0, false, true));
    let env = game_environment(runtime.lua()).unwrap();
    assert_eq!(env.get::<i32>("friend_login_count").unwrap(), 1);
    assert_eq!(
        env.get::<i32>("platform_connected_count").unwrap(),
        0,
        "SDK resume is not a new Lua connection callback"
    );
}

#[test]
fn native_platform_resume_delivery_skips_logged_out_and_uninitialized_identity() {
    for logout_before_delivery in [false, true] {
        let sandbox = ShippedDataSandbox::new("native-platform-resume-retired");
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let origin = format!("http://{}", listener.local_addr().unwrap());
        let runtime = StellaLua::new(&sandbox.data_root).unwrap();
        configure(&runtime, &format!("{origin}/identity/3.0"));
        let graph = Arc::new(SwitchedGraph {
            open: AtomicBool::new(false),
            graph: Arc::new(
                FacebookGraphSession::new(&format!("{origin}/v2.0"), "synthetic-resume-retired")
                    .unwrap(),
            ),
        });
        runtime.set_facebook_session(Some(graph.clone())).unwrap();
        let listener = if logout_before_delivery {
            let server = thread::spawn(move || {
                let (mut stream, _) = identity_routes::accept_request_including_friends(&listener);
                raw_reply(&mut stream, 200, &linked_session());
                drop(stream);
                let (mut stream, request) =
                    identity_routes::accept_request_including_friends(&listener);
                assert!(request.starts_with("GET /identity/2.0/friends "));
                raw_reply(&mut stream, 503, "");
                listener
            });
            login(&runtime, 1);
            server.join().unwrap()
        } else {
            listener
        };
        graph.open.store(true, Ordering::Release);
        runtime.post_application_resumed();
        if logout_before_delivery {
            runtime
                .execute_source("_G.SkynestAccount.native_logout()")
                .unwrap();
            assert!(!graph.is_logged_in());
        }
        for _ in 0..4 {
            dispatch_registered_application_events(runtime.lua()).unwrap();
        }
        assert_eq!(runtime.social.platform_state_for_test(), (0, false, false));
        assert!(matches!(listener.accept(), Err(e) if e.kind()==std::io::ErrorKind::WouldBlock));
        assert!(runtime.skynest_account.friends_profile_for_test().is_none());
    }
}
