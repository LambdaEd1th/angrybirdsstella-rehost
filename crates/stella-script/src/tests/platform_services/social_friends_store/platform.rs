use super::*;
mod connection;
mod lifecycle;
mod logout;
mod resume;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

struct SwitchedGraph {
    open: AtomicBool,
    graph: Arc<FacebookGraphSession>,
}
impl SocialPlatformProvider for SwitchedGraph {
    fn prepare_login(self: Arc<Self>) -> SocialLoginRequest {
        SocialLoginRequest::Ready(Err(SocialPlatformError::NotLoggedIn))
    }
    fn is_logged_in(&self) -> bool {
        self.open.load(Ordering::Acquire) && self.graph.is_logged_in()
    }
    fn logout(&self) -> Result<(), SocialPlatformError> {
        if self.is_logged_in() {
            self.open.store(false, Ordering::Release);
            self.graph.logout()?;
        }
        Ok(())
    }
    fn user_profile(&self) -> Result<SocialPlatformProfile, SocialPlatformError> {
        if !self.is_logged_in() {
            return Err(SocialPlatformError::NotLoggedIn);
        }
        self.graph.user_profile()
    }
    fn prepare_user_profile(self: Arc<Self>) -> SocialProfileRequest {
        // This fixture only switches availability; all profile data still
        // comes from actual Graph requests with the production cache lifecycle.
        self.graph.clone().prepare_user_profile()
    }
    fn publish_user_profile(
        &self,
        profile: &SocialPlatformProfile,
    ) -> Result<(), SocialPlatformError> {
        self.graph.publish_user_profile(profile)
    }
    fn friends(
        &self,
        details: SocialFriendDetails,
    ) -> Result<SocialPlatformFriends, SocialPlatformError> {
        if !self.is_logged_in() {
            return Err(SocialPlatformError::NotLoggedIn);
        }
        self.graph.friends(details)
    }
}

#[test]
fn native_platform_sdk_success_after_real_storage_401_runs_sync_without_another_lua_login() {
    let sandbox = ShippedDataSandbox::new("native-platform-session-event");
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let origin = format!("http://{}", listener.local_addr().unwrap());
    let (arrived, ready) = std::sync::mpsc::channel();
    let server = thread::spawn(move || {
        let (mut stream, _) = identity_routes::accept_request_including_friends(&listener);
        raw_reply(&mut stream, 200, &linked_session());
        drop(stream);
        let (mut stream, request) = identity_routes::accept_request_including_friends(&listener);
        assert!(request.starts_with("GET /identity/2.0/friends "));
        raw_reply(&mut stream, 503, "");
        drop(stream);
        arrived.send(()).unwrap();
        let (mut stream, request) = identity_routes::accept_request_including_friends(&listener);
        assert!(request.starts_with("POST /storage/2.0/states/query "));
        storage_session::assert_auth(&request, "synthetic-friend-access", "8, 2");
        raw_reply(&mut stream, 401, "");
        drop(stream);
        let (mut stream, request) = identity_routes::accept_request_including_friends(&listener);
        assert!(request.starts_with("POST /session/1/apps/friends-fixture/sessions "));
        let mut renewed: serde_json::Value = serde_json::from_str(&linked_session()).unwrap();
        renewed["userAuth"]["accessToken"] = json!("renewed-platform-identity-access");
        raw_reply(&mut stream, 200, &renewed.to_string());
        drop(stream);
        // SDK success and the protected storage replay are independent workers.
        // Hold Graph's first response until the storage replay has been served.
        let mut graph = None;
        let mut replay = false;
        while graph.is_none() || !replay {
            let (mut stream, request) =
                identity_routes::accept_request_including_friends(&listener);
            if request.starts_with("GET /v2.0/me?") {
                assert!(graph.is_none());
                graph = Some(stream);
            } else {
                assert!(
                    request.starts_with("POST /storage/2.0/states/query "),
                    "{request}"
                );
                assert!(!replay);
                replay = true;
                storage_session::assert_auth(&request, "renewed-platform-identity-access", "8, 2");
                raw_reply(&mut stream, 200, r#"{"result":[]}"#);
            }
        }
        let mut stream = graph.unwrap();
        raw_reply(
            &mut stream,
            200,
            r#"{"id":"platform-own","name":"Open after login"}"#,
        );
        drop(stream);
        for (expected, response) in [
            ("GET /v2.0//me/friends?",r#"{"data":[]}"#.to_owned()),
            ("POST /identity/2.0/external/connect ", String::new()),
            ("GET /identity/3.0/profile/own ",linked_profile().to_string()),
            ("GET /identity/2.0/friends ",r#"{"socialFriends":[{"id":"game","socialNetworks":[{"provider":"facebook","networkId":"platform-friend"}]}]}"#.to_owned()),
            ("POST /identity/2.0/profile/search ",String::new()),
            ("GET /v2.0//me/friends?",r#"{"data":[{"id":"platform-friend","name":"Session Friend"}]}"#.to_owned()),
            ("POST /storage/2.0/states/query ",r#"{"result":[{"accountId":"game","states":[{"value":"updated","encoding":"SDKv1"}]}]}"#.to_owned()),
        ] {
            let (mut stream, request) = identity_routes::accept_request_including_friends(&listener);
            assert!(request.starts_with(expected), "{request}");
            if expected.starts_with("POST /identity") || expected.starts_with("POST /storage") || expected=="GET /identity/2.0/friends " {
                storage_session::assert_auth(&request, "renewed-platform-identity-access", "8, 2");
            }
            raw_reply(&mut stream, 200, &response);
        }
        listener
    });
    let runtime = StellaLua::new(&sandbox.data_root).unwrap();
    configure(&runtime, &format!("{origin}/identity/3.0"));
    runtime
        .set_storage_url(&format!("{origin}/storage/2.0"))
        .unwrap();
    let graph = Arc::new(SwitchedGraph {
        open: AtomicBool::new(false),
        graph: Arc::new(
            FacebookGraphSession::new(&format!("{origin}/v2.0"), "synthetic-platform").unwrap(),
        ),
    });
    runtime.set_facebook_session(Some(graph.clone())).unwrap();
    runtime.skynest_account.seed_friends_cache_for_test(
        "own",
        r#"{"friends":[{"accountId":"cached","nickName":"Cached"}]}"#,
    );
    runtime.execute_source("native_progress_count=0; _G.SocialManager.onFriendsProgressUpdated=function(ok,f) native_progress_count=native_progress_count+1; session_friends=f end").unwrap();
    login(&runtime, 1);
    wait_signal(&runtime, &ready);
    graph.open.store(true, Ordering::Release);
    runtime
        .execute_source("_G.SocialManager.native_getFriendsProgress()")
        .unwrap();
    wait_progress(&runtime, 2);
    let listener = server.join().unwrap();
    assert!(matches!(listener.accept(), Err(e) if e.kind()==std::io::ErrorKind::WouldBlock));
    let env = game_environment(runtime.lua()).unwrap();
    assert_eq!(env.get::<i32>("friend_login_count").unwrap(), 1);
    let friend = env
        .get::<mlua::Table>("session_friends")
        .unwrap()
        .raw_get::<mlua::Table>(1)
        .unwrap();
    assert_eq!(friend.get::<String>("nickname").unwrap(), "Session Friend");
    assert_eq!(runtime.social.platform_state_for_test(), (0, false, true));
}

fn reply_initial_profiles(listener: &TcpListener, body: &str) {
    let (mut first, first_request) = identity_routes::accept_request_including_friends(listener);
    let (mut second, second_request) = identity_routes::accept_request_including_friends(listener);
    assert!(first_request.starts_with("GET /v2.0/me?"));
    assert!(second_request.starts_with("GET /v2.0/me?"));
    raw_reply(&mut first, 200, body);
    raw_reply(&mut second, 200, body);
}

fn linked_profile() -> serde_json::Value {
    json!({"publicAccountId":"own","personal":{"nickName":"Own"},
        "socialNetworks":[{"provider":"facebook","id":"platform-own","socialAttributes":{"name":"Own"}}],
        "externalNetworks":[{"provider":"facebook","id":"platform-own"}]})
}

#[test]
fn native_platform_graph_profile_failure_keeps_preclear_disk_cache_and_completes_real_progress() {
    let sandbox = ShippedDataSandbox::new("native-platform-profile-error");
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let origin = format!("http://{}", listener.local_addr().unwrap());
    let server = thread::spawn(move || {
        let responses = [
            ("POST /session/1/apps/friends-fixture/sessions ", linked_session()),
            ("GET /v2.0/me?", r#"{"id":"platform-own","name":"Own"}"#.to_owned()),
            ("GET /v2.0//me/friends?", r#"{"data":[]}"#.to_owned()),
            ("POST /identity/2.0/external/connect ", String::new()),
            ("GET /identity/3.0/profile/own ", linked_profile().to_string()),
            ("GET /identity/2.0/friends ", r#"{"socialFriends":[{"id":"game","socialNetworks":[{"provider":"facebook","networkId":"p"}]}]}"#.to_owned()),
            ("POST /identity/2.0/profile/search ", String::new()),
            ("GET /v2.0//me/friends?", r#"{"error":null}"#.to_owned()),
            ("POST /storage/2.0/states/query ", r#"{"result":[{"accountId":"game","states":[{"value":"unnamed","encoding":"SDKv1"}]}]}"#.to_owned()),
        ];
        for (expected, response) in responses {
            if expected == "GET /v2.0/me?" {
                reply_initial_profiles(&listener, &response);
                continue;
            }

            let (mut stream, request) =
                identity_routes::accept_request_including_friends(&listener);
            assert!(request.starts_with(expected), "{request}");
            if expected.starts_with("POST /storage") {
                let body: serde_json::Value =
                    serde_json::from_str(storage_session::body(&request)).unwrap();
                assert_eq!(body["accountIds"], json!(["game"]));
            }
            raw_reply(
                &mut stream,
                if response == r#"{"error":null}"# {
                    400
                } else {
                    200
                },
                &response,
            );
        }
    });
    let runtime = platform_runtime(&sandbox, &origin);
    runtime
        .set_storage_url(&format!("{origin}/storage/2.0"))
        .unwrap();
    runtime.skynest_account.seed_friends_cache_for_test(
        "own",
        r#"{"friends":[],"socialNetworkFriends":[{"socialNetwork":1,"uid":"old","name":"Old"}]}"#,
    );
    runtime.execute_source("_G.SocialManager.onFriendsProgressUpdated=function(ok,f) native_progress_count=native_progress_count+1; failed_platform_progress_ok=ok; failed_platform_progress=f end").unwrap();
    login(&runtime, 1);
    wait_progress(&runtime, 1);
    server.join().unwrap();
    let saved: serde_json::Value =
        serde_json::from_str(&runtime.skynest_account.read_friends_cache_for_test("own")).unwrap();
    assert_eq!(saved["friends"][0]["accountId"], "game");
    assert_eq!(saved["friends"][0]["socialNetworkProfiles"][0]["name"], "");
    assert_eq!(saved["socialNetworkFriends"][0]["uid"], "old");
    let env = game_environment(runtime.lua()).unwrap();
    assert!(env.get::<bool>("failed_platform_progress_ok").unwrap());
    assert_eq!(
        env.get::<mlua::Table>("failed_platform_progress")
            .unwrap()
            .raw_len(),
        0
    );
}

fn linked_session() -> String {
    let mut value: serde_json::Value = serde_json::from_slice(&session_response("own")).unwrap();
    value["profile"] = linked_profile();
    value.to_string()
}

#[test]
fn native_platform_cached_availability_callback_is_synchronous_before_lua_login_success() {
    let sandbox = ShippedDataSandbox::new("native-platform-ready-profile");
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let origin = format!("http://{}", listener.local_addr().unwrap());
    let server = thread::spawn(move || {
        let (mut stream, request) = identity_routes::accept_request_including_friends(&listener);
        assert!(request.starts_with("GET /v2.0/me?"));
        raw_reply(
            &mut stream,
            200,
            r#"{"id":"platform-own","name":"Preloaded"}"#,
        );
        drop(stream);
        let (mut stream, request) = identity_routes::accept_request_including_friends(&listener);
        assert!(request.starts_with("POST /session/1/apps/friends-fixture/sessions "));
        raw_reply(&mut stream, 200, &linked_session());
        drop(stream);
        let (mut stream, request) = identity_routes::accept_request_including_friends(&listener);
        assert!(
            request.starts_with("GET /v2.0//me/friends?"),
            "no extra me request after cache hit: {request}"
        );
        raw_reply(&mut stream, 503, "");
        listener
    });
    let graph = Arc::new(
        FacebookGraphSession::new(&format!("{origin}/v2.0"), "synthetic-preloaded").unwrap(),
    );
    graph.user_profile().unwrap();
    let runtime = StellaLua::new(&sandbox.data_root).unwrap();
    configure(&runtime, &format!("{origin}/identity/3.0"));
    runtime.set_facebook_session(Some(graph)).unwrap();
    runtime.execute_source("_G.SocialManager.onSocialNetworkConnected=function() availability_login_count=friend_login_count; availability_connected=_G.SocialManager.native_isConnectedToSocialNetwork() end").unwrap();
    login(&runtime, 1);
    let env = game_environment(runtime.lua()).unwrap();
    assert_eq!(env.get::<i32>("availability_login_count").unwrap(), 0);
    assert!(env.get::<bool>("availability_connected").unwrap());
    assert!(env.get::<bool>("connected_at_login").unwrap());
    let listener = server.join().unwrap();
    for _ in 0..500 {
        dispatch_registered_application_events(runtime.lua()).unwrap();
        if !runtime.social.platform_state_for_test().1 {
            break;
        }
        thread::sleep(Duration::from_millis(2));
    }
    assert_eq!(runtime.social.platform_state_for_test(), (0, false, false));
    assert!(matches!(listener.accept(), Err(e) if e.kind()==std::io::ErrorKind::WouldBlock));
}

fn wait_signal(runtime: &StellaLua, signal: &std::sync::mpsc::Receiver<()>) {
    for _ in 0..1500 {
        dispatch_registered_application_events(runtime.lua()).unwrap();
        if signal.try_recv().is_ok() {
            return;
        }
        thread::sleep(Duration::from_millis(2));
    }
    panic!("platform phase timed out");
}

fn platform_runtime(sandbox: &ShippedDataSandbox, origin: &str) -> StellaLua {
    let runtime = StellaLua::new(&sandbox.data_root).unwrap();
    configure(&runtime, &format!("{origin}/identity/3.0"));
    runtime
        .set_facebook_session(Some(std::sync::Arc::new(
            FacebookGraphSession::new(&format!("{origin}/v2.0"), "synthetic-platform").unwrap(),
        )))
        .unwrap();
    runtime.skynest_account.seed_friends_cache_for_test(
        "own",
        r#"{"friends":[{"accountId":"cached","nickName":"Cached"}]}"#,
    );
    runtime.execute_source("native_progress_count=0; platform_connected_count=0; _G.SocialManager.onSocialNetworkConnected=function() platform_connected_count=platform_connected_count+1 end; _G.SocialManager.onFriendsProgressUpdated=function() native_progress_count=native_progress_count+1 end").unwrap();
    runtime
}

#[test]
fn native_platform_async_identity_mismatch_and_graph_error_do_not_start_constructor_refresh() {
    for graph_error in [false, true] {
        let sandbox = ShippedDataSandbox::new("native-platform-readiness-error");
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let origin = format!("http://{}", listener.local_addr().unwrap());
        let server = thread::spawn(move || {
            let (mut stream, _) = identity_routes::accept_request_including_friends(&listener);
            raw_reply(&mut stream, 200, &linked_session());
            drop(stream);
            for _ in 0..2 {
                let (mut stream, request) =
                    identity_routes::accept_request_including_friends(&listener);
                assert!(request.starts_with("GET /v2.0/me?"));
                raw_reply(
                    &mut stream,
                    if graph_error { 400 } else { 200 },
                    if graph_error {
                        r#"{"error":null}"#
                    } else {
                        r#"{"id":"other-platform-user","name":"Other"}"#
                    },
                );
            }
            listener
        });
        let runtime = platform_runtime(&sandbox, &origin);
        login(&runtime, 1);
        let listener = server.join().unwrap();
        for _ in 0..500 {
            dispatch_registered_application_events(runtime.lua()).unwrap();
            if runtime.social.platform_state_for_test().0 == 0 {
                break;
            }
            thread::sleep(Duration::from_millis(2));
        }
        assert_eq!(runtime.social.platform_state_for_test(), (0, false, false));
        assert_eq!(
            runtime.skynest_account.read_friends_cache_for_test("own"),
            r#"{"friends":[{"accountId":"cached","nickName":"Cached"}]}"#
        );
        assert_eq!(
            game_environment(runtime.lua())
                .unwrap()
                .get::<i32>("platform_connected_count")
                .unwrap(),
            0
        );
        assert_eq!(
            game_environment(runtime.lua())
                .unwrap()
                .get::<i32>("native_progress_count")
                .unwrap(),
            0
        );
        assert!(matches!(listener.accept(), Err(e) if e.kind()==std::io::ErrorKind::WouldBlock));
    }
}

#[test]
fn native_platform_connection_failures_stop_at_the_failing_wire_phase_and_preserve_cache() {
    for fail_at in 0..4 {
        let sandbox = ShippedDataSandbox::new("native-platform-connect-error");
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let origin = format!("http://{}", listener.local_addr().unwrap());
        let server = thread::spawn(move || {
            let (mut stream, _) = identity_routes::accept_request_including_friends(&listener);
            raw_reply(&mut stream, 200, &linked_session());
            drop(stream);
            reply_initial_profiles(&listener, r#"{"id":"platform-own","name":"Own"}"#);
            for (step, expected, success) in [
                (0, "GET /v2.0//me/friends?", r#"{"data":[{"id":"friend"}]}"#),
                (1, "POST /identity/2.0/external/connect ", ""),
                (2, "POST /identity/2.0/friends ", ""),
                (3, "GET /identity/3.0/profile/own ", ""),
            ] {
                let (mut stream, request) =
                    identity_routes::accept_request_including_friends(&listener);
                assert!(request.starts_with(expected), "{request}");
                // profile/own's201 is failure despite common POSTs accepting it.
                raw_reply(
                    &mut stream,
                    if step == fail_at {
                        if step == 3 { 201 } else { 503 }
                    } else {
                        200
                    },
                    success,
                );
                if step == fail_at {
                    break;
                }
            }
            listener
        });
        let runtime = platform_runtime(&sandbox, &origin);
        login(&runtime, 1);
        // The server cannot finish until the app dispatches its profile check.
        for _ in 0..1500 {
            dispatch_registered_application_events(runtime.lua()).unwrap();
            if runtime.social.platform_state_for_test() == (0, false, false) {
                break;
            }
            thread::sleep(Duration::from_millis(2));
        }
        let listener = server.join().unwrap();
        assert_eq!(runtime.social.platform_state_for_test(), (0, false, false));
        assert_eq!(
            runtime.skynest_account.read_friends_cache_for_test("own"),
            r#"{"friends":[{"accountId":"cached","nickName":"Cached"}]}"#
        );
        let env = game_environment(runtime.lua()).unwrap();
        assert_eq!(env.get::<i32>("native_progress_count").unwrap(), 0);
        assert_eq!(
            env.get::<i32>("platform_connected_count").unwrap(),
            1,
            "manager availability is distinct from SDK connection sync"
        );
        assert!(matches!(listener.accept(), Err(e) if e.kind()==std::io::ErrorKind::WouldBlock));
    }
}

#[test]
fn native_platform_linked_session_real_graph_connect_sync_profile_and_merge_order() {
    let sandbox = ShippedDataSandbox::new("native-platform-success");
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let origin = format!("http://{}", listener.local_addr().unwrap());
    let (arrived, phase) = std::sync::mpsc::channel();
    let (release, resume) = std::sync::mpsc::channel();
    let server = thread::spawn(move || {
        let mut wire = Vec::new();
        let accept = |expected: &str, wire: &mut Vec<String>| {
            let (stream, request) = identity_routes::accept_request_including_friends(&listener);
            assert!(
                request.starts_with(expected),
                "expected {expected}, got {request}"
            );
            wire.push(request.clone());
            (stream, request)
        };
        let (mut stream, _) = accept("POST /session/1/apps/friends-fixture/sessions ", &mut wire);
        raw_reply(&mut stream, 200, &linked_session());
        drop(stream);
        let (mut stream, request) = accept("GET /v2.0/me?", &mut wire);
        assert!(request.contains("access_token=synthetic-platform%20%2B%2F"));
        assert!(!request.to_lowercase().contains("x-access-token"));
        // Readiness must remain nonzero while the platform profile is pending.
        arrived.send(()).unwrap();
        resume.recv_timeout(Duration::from_secs(8)).unwrap();
        let (mut available, _) = accept("GET /v2.0/me?", &mut wire);
        let profile = r#"{"id":"platform-own","name":"Platform Own","username":"own-user"}"#;
        raw_reply(&mut stream, 200, profile);
        drop(stream);
        raw_reply(&mut available, 200, profile);
        drop(available);
        // The second profile request in731EF0 reuses the FacebookService cache.
        let (mut stream, _) = accept("GET /v2.0//me/friends?", &mut wire);
        raw_reply(
            &mut stream,
            200,
            r#"{"data":[{"id":"p +/"},{"id":""},{"id":"p +/"}]}"#,
        );
        drop(stream);
        let (mut stream, request) = accept("POST /identity/2.0/external/connect ", &mut wire);
        storage_session::assert_auth(&request, "synthetic-friend-access", "8, 2");
        let body: serde_json::Value =
            serde_json::from_str(storage_session::body(&request)).unwrap();
        assert_eq!(
            body,
            json!({"provider":"facebook","externalAttributes":{
            "accessToken":"synthetic-platform +/","userId":"platform-own","name":"Platform Own",
            "avatarUrl":"https://graph.facebook.com/platform-own/picture?type=large"}})
        );
        raw_reply(&mut stream, 201, "ignored-connect-body");
        drop(stream);
        let (mut stream, request) = accept("POST /identity/2.0/friends ", &mut wire);
        storage_session::assert_auth(&request, "synthetic-friend-access", "8, 2");
        assert_eq!(
            storage_session::body(&request),
            "networkId=p+%2B%2F&networkId=&networkId=p+%2B%2F&networkProvider=facebook"
        );
        raw_reply(&mut stream, 204, "");
        drop(stream);
        let (mut stream, request) = accept("GET /identity/3.0/profile/own ", &mut wire);
        assert!(
            request
                .to_lowercase()
                .contains("x-access-token: synthetic-friend-access")
        );
        assert!(!request.to_lowercase().contains("rovio-sgs"));
        raw_reply(&mut stream, 200, &linked_profile().to_string());
        drop(stream);
        let (mut stream, _) = accept("GET /identity/2.0/friends ", &mut wire);
        raw_reply(
            &mut stream,
            200,
            r#"{"socialFriends":[{"id":"game","socialNetworks":[{"provider":"facebook","networkId":"p +/","socialAttributes":{"name":"","avatarUrl":""}}]},{"id":"fixed","socialNetworks":[{"provider":"facebook","networkId":"fixed-platform","socialAttributes":{"name":"Fixed","avatarUrl":"fixed-url"}}]}]}"#,
        );
        drop(stream);
        let (mut stream, request) = accept("POST /identity/2.0/profile/search ", &mut wire);
        assert_eq!(
            storage_session::body(&request),
            "publicAccountId=game&publicAccountId=fixed"
        );
        raw_reply(&mut stream, 200, "");
        drop(stream);
        let (mut stream, _) = accept("GET /v2.0//me/friends?", &mut wire);
        arrived.send(()).unwrap();
        resume.recv_timeout(Duration::from_secs(8)).unwrap();
        raw_reply(
            &mut stream,
            200,
            r#"{"data":[{"id":"p +/","name":"Earlier"},{"id":"platform-only","name":"No game account"},{"id":"p +/","username":"Fallback"},{"id":"fixed-platform","name":"Do not replace"}]}"#,
        );
        drop(stream);
        let (mut stream, request) = accept("POST /storage/2.0/states/query ", &mut wire);
        let body: serde_json::Value =
            serde_json::from_str(storage_session::body(&request)).unwrap();
        assert_eq!(body["accountIds"], json!(["fixed", "game"]));
        storage_session::assert_auth(&request, "synthetic-friend-access", "8, 2");
        raw_reply(
            &mut stream,
            200,
            r#"{"result":[{"accountId":"game","states":[{"value":"real-progress","encoding":"SDKv1"}]}]}"#,
        );
        (wire, listener)
    });
    let runtime = StellaLua::new(&sandbox.data_root).unwrap();
    configure(&runtime, &format!("{origin}/identity/3.0"));
    runtime
        .set_storage_url(&format!("{origin}/storage/2.0"))
        .unwrap();
    runtime
        .set_facebook_session(Some(std::sync::Arc::new(
            FacebookGraphSession::new(&format!("{origin}/v2.0"), "synthetic-platform +/").unwrap(),
        )))
        .unwrap();
    runtime.skynest_account.seed_friends_cache_for_test("own", &json!({"friends":[{"accountId":"cached","nickName":"Cached"}],
        "socialNetworkFriends":[{"socialNetwork":2,"uid":"sina-preserved","name":"Sina"},{"socialNetwork":1,"uid":"old-facebook","name":"Old"}]}).to_string());
    runtime.execute_source("native_progress_count=0; platform_connected_count=0; _G.SocialManager.onSocialNetworkConnected=function(n) platform_connected_count=platform_connected_count+1; platform_connected_network=n; platform_connected_inside=_G.SocialManager.native_isConnectedToSocialNetwork(); platform_local_id=_G.SocialManager.native_getLocalUserAccountId() end; _G.SocialManager.onFriendsProgressUpdated=function(ok,f) native_progress_count=native_progress_count+1; native_progress_success=ok; native_progress_friends=f end").unwrap();
    login(&runtime, 1);
    wait_signal(&runtime, &phase);
    let cache = || {
        serde_json::from_str::<serde_json::Value>(
            &runtime.skynest_account.read_friends_cache_for_test("own"),
        )
        .unwrap()
    };
    assert_eq!(cache()["friends"][0]["accountId"], "cached");
    assert_eq!(
        game_environment(runtime.lua())
            .unwrap()
            .get::<i32>("native_progress_count")
            .unwrap(),
        0
    );
    release.send(()).unwrap();
    wait_signal(&runtime, &phase);
    let before = cache();
    assert_eq!(before["friends"][1]["socialNetworkProfiles"][0]["name"], "");
    assert_eq!(
        before["socialNetworkFriends"].as_array().unwrap().len(),
        2,
        "platform clear must not persist before response"
    );
    release.send(()).unwrap();
    wait_progress(&runtime, 1);
    let (wire, listener) = server.join().unwrap();
    assert_eq!(wire.len(), 11);
    assert!(
        matches!(listener.accept(), Err(e) if e.kind()==std::io::ErrorKind::WouldBlock),
        "no recursive session-success from profile-only refresh"
    );
    let after = cache();
    assert_eq!(after["friends"].as_array().unwrap().len(), 2);
    assert_eq!(
        after["friends"][0]["socialNetworkProfiles"][0]["name"],
        "Fixed"
    );
    assert_eq!(
        after["friends"][0]["socialNetworkProfiles"][0]["avatarUrl"],
        "fixed-url"
    );
    assert_eq!(
        after["friends"][1]["socialNetworkProfiles"][0]["name"],
        "Fallback"
    );
    assert_eq!(
        after["friends"][1]["socialNetworkProfiles"][0]["avatarUrl"],
        "https://graph.facebook.com/p +//picture?type=normal"
    );
    assert_eq!(after["socialNetworkFriends"].as_array().unwrap().len(), 4);
    assert!(
        after["socialNetworkFriends"]
            .as_array()
            .unwrap()
            .iter()
            .any(|p| p["uid"] == "sina-preserved")
    );
    assert!(!after.to_string().contains("old-facebook"));
    let env = game_environment(runtime.lua()).unwrap();
    assert_eq!(env.get::<i32>("platform_connected_count").unwrap(), 1);
    assert_eq!(
        env.get::<String>("platform_connected_network").unwrap(),
        "facebook"
    );
    assert!(env.get::<bool>("platform_connected_inside").unwrap());
    assert_eq!(env.get::<String>("platform_local_id").unwrap(), "own");
    assert_eq!(env.get::<i32>("friend_login_count").unwrap(), 1);
    assert!(env.get::<bool>("native_progress_success").unwrap());
    let item = env
        .get::<mlua::Table>("native_progress_friends")
        .unwrap()
        .raw_get::<mlua::Table>(1)
        .unwrap();
    assert_eq!(item.get::<String>("nickname").unwrap(), "Fallback");
    assert_eq!(item.get::<String>("progress").unwrap(), "real-progress");
}
