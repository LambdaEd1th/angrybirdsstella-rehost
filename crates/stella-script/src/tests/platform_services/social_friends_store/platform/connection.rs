use super::*;
mod inline;
use std::sync::{Mutex, atomic::AtomicUsize, mpsc};

#[test]
fn native_platform_facebook_oauth_cached_install_primes_profile_and_retires_late_delivery() {
    for retired in [false, true] {
        let sandbox = ShippedDataSandbox::new("native-oauth-cached-install");
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let origin = format!("http://{}", listener.local_addr().unwrap());
        let (arrived, phase) = mpsc::channel();
        let (release, resume) = mpsc::channel();
        let server = thread::spawn(move || {
            let (mut stream, request) =
                identity_routes::accept_request_including_friends(&listener);
            let (headers, body) = request.split_once("\r\n\r\n").unwrap();
            crate::facebook_graph::test_wire::assert_batch(
                &format!("{headers}\r\n\r\n"),
                body,
                "12345",
                &["me", "me/permissions"],
                "synthetic-restored-token",
            );
            arrived.send(()).unwrap();
            resume.recv_timeout(Duration::from_secs(8)).unwrap();
            raw_reply(
                &mut stream,
                200,
                &crate::facebook_graph::test_wire::batch_response(&[
                    (200, json!({"id":"restored-user","name":"Restored User"})),
                    (
                        200,
                        json!({"data":[{"permission":"email","status":"granted"}]}),
                    ),
                ]),
            );
            listener
        });
        let config = FacebookOAuthConfig {
            rest_root: None,
            graph_root: format!("{origin}/v2.0"),
            authorization_url: format!("{origin}/oauth"),
            app_id: "12345".into(),
            url_scheme_suffix: String::new(),
            request_birthday: true,
        };
        let cache_path = sandbox
            .data_root
            .parent()
            .unwrap()
            .join("facebook-token.plist");
        let cache = crate::FacebookTokenCache::open(&cache_path).unwrap();
        let initial = Arc::new(
            FacebookOAuthSession::new_with_cache(config.clone(), cache.clone(), |_| Ok(true))
                .unwrap(),
        );
        assert!(matches!(
            initial.clone().prepare_login(),
            SocialLoginRequest::AwaitingCallback
        ));
        initial
            .handle_open_url(
                "fb12345://authorize#access_token=synthetic-restored-token&expires_in=3600",
            )
            .unwrap();
        assert_eq!(initial.take_login_completion(), Some(Ok(())));
        assert_eq!(cache.take_error(), None);
        drop(initial);
        drop(cache);
        let provider = Arc::new(
            FacebookOAuthSession::new_with_cache(
                config,
                crate::FacebookTokenCache::open(&cache_path).unwrap(),
                |_| panic!("cache restore opened browser"),
            )
            .unwrap(),
        );
        assert!(provider.is_logged_in());
        let runtime = platform_runtime(&sandbox, &origin);
        let queued = runtime.social.online_completion_count_probe();
        runtime
            .set_facebook_session(Some(provider.clone()))
            .unwrap();
        wait_signal(&runtime, &phase);
        if retired {
            runtime.set_facebook_session(None).unwrap();
        }
        release.send(()).unwrap();
        let listener = server.join().unwrap();
        for _ in 0..1500 {
            if queued() >= 2 {
                break;
            }
            thread::sleep(Duration::from_millis(2));
        }
        assert!(
            queued() >= 2,
            "actual startup SDK and profile workers did not finish"
        );
        dispatch_registered_application_events(runtime.lua()).unwrap();
        match provider.clone().prepare_user_profile() {
            SocialProfileRequest::Ready(Ok(profile)) if !retired => {
                assert_eq!(profile.user.id, "restored-user");
                assert_eq!(profile.access_token, "synthetic-restored-token");
            }
            SocialProfileRequest::Pending(_) if retired => {}
            _ => panic!("startup profile cache did not follow provider lifetime"),
        }
        assert_eq!(
            provider.granted_permissions(),
            if retired {
                vec!["public_profile", "email", "user_friends", "user_birthday"]
            } else {
                vec!["email"]
            }
        );
        assert!(provider.clone().take_login_profile_request().is_none());
        assert!(matches!(listener.accept(), Err(e) if e.kind()==std::io::ErrorKind::WouldBlock));
    }
}

/// Models the embedding host's authorization completion for a supplied
/// synthetic session. This tests the SDK consumer, not an OAuth implementation.
/// Every profile/ID/friend response still uses production Graph HTTP and cache.
struct HostLoginGraph {
    graph: Arc<FacebookGraphSession>,
    open: AtomicBool,
    attempts: AtomicUsize,
    login: Mutex<Option<mpsc::Receiver<Result<(), SocialPlatformError>>>>,
    dispatcher: Mutex<Option<crate::SocialPlatformDispatcher>>,
}
impl SocialPlatformProvider for HostLoginGraph {
    fn set_application_dispatcher(&self, dispatcher: crate::SocialPlatformDispatcher) {
        *self.dispatcher.lock().unwrap() = Some(dispatcher);
    }
    fn is_logged_in(&self) -> bool {
        self.open.load(Ordering::Acquire) && self.graph.is_logged_in()
    }
    fn prepare_login(self: Arc<Self>) -> SocialLoginRequest {
        self.attempts.fetch_add(1, Ordering::AcqRel);
        let Some(login) = self.login.lock().unwrap().take() else {
            return SocialLoginRequest::Ready(Err(SocialPlatformError::NotLoggedIn));
        };
        SocialLoginRequest::Pending(Box::new(move || {
            login.recv_timeout(Duration::from_secs(8)).unwrap()?;
            self.open.store(true, Ordering::Release);
            Ok(())
        }))
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
        if !self.is_logged_in() {
            return SocialProfileRequest::Ready(Err(SocialPlatformError::NotLoggedIn));
        }
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

fn host_provider(
    origin: &str,
    open: bool,
) -> (
    Arc<HostLoginGraph>,
    mpsc::Sender<Result<(), SocialPlatformError>>,
) {
    let (send, receive) = mpsc::channel();
    (
        Arc::new(HostLoginGraph {
            graph: Arc::new(
                FacebookGraphSession::new(&format!("{origin}/v2.0"), "synthetic-host-session")
                    .unwrap(),
            ),
            open: AtomicBool::new(open),
            attempts: AtomicUsize::new(0),
            login: Mutex::new(Some(receive)),
            dispatcher: Mutex::new(None),
        }),
        send,
    )
}

#[test]
fn native_platform_sdk_completion_uses_main_thread_and_provider_lifetime() {
    let sandbox = ShippedDataSandbox::new("native-sdk-main-completion");
    let runtime = platform_runtime(&sandbox, "http://127.0.0.1:9");
    let (provider, _login) = host_provider("http://127.0.0.1:9", false);
    runtime
        .set_facebook_session(Some(provider.clone()))
        .unwrap();
    let dispatcher = provider.dispatcher.lock().unwrap().clone().unwrap();
    let observed = Arc::new(Mutex::new(Vec::new()));
    let capture = observed.clone();
    let task = crate::SocialPlatformTask::new(move || {
        capture.lock().unwrap().push(thread::current().id());
    });
    let send = dispatcher.clone();
    thread::spawn(move || {
        send(task.clone());
        send(task);
    })
    .join()
    .unwrap();
    assert!(observed.lock().unwrap().is_empty());
    // Skynest consumer retirement must not discard an active FBSession task.
    runtime
        .social
        .set_compatible_url("http://127.0.0.1:9/social")
        .unwrap();
    dispatch_registered_application_events(runtime.lua()).unwrap();
    assert_eq!(*observed.lock().unwrap(), [thread::current().id()]);

    let capture = observed.clone();
    dispatcher(crate::SocialPlatformTask::new(move || {
        capture.lock().unwrap().push(thread::current().id());
    }));
    runtime.set_facebook_session(None).unwrap();
    dispatch_registered_application_events(runtime.lua()).unwrap();
    assert_eq!(observed.lock().unwrap().len(), 1);
    // A worker finishing after replacement is retired as well.
    dispatcher(crate::SocialPlatformTask::new(|| {
        panic!("retired SDK task ran")
    }));
    dispatch_registered_application_events(runtime.lua()).unwrap();
}

fn explicit_session(linked: bool) -> String {
    let mut value: serde_json::Value = serde_json::from_str(&linked_session()).unwrap();
    if !linked {
        value["profile"]["socialNetworks"] = json!([]);
        value["profile"]["externalNetworks"] = json!([]);
    }
    value.to_string()
}

#[test]
fn native_platform_explicit_unlinked_connect_and_closed_host_login_use_real_chain() {
    for closed in [false, true] {
        explicit_connection_flow(closed, false);
    }
}

#[test]
fn native_platform_oauth_url_and_resume_drive_real_profile_and_connection_chain() {
    explicit_connection_flow(true, true);
}

fn explicit_connection_flow(closed: bool, oauth: bool) {
    let sandbox = ShippedDataSandbox::new("native-explicit-platform");
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let origin = format!("http://{}", listener.local_addr().unwrap());
    let connected = Arc::new(AtomicBool::new(false));
    let callback_seen = connected.clone();
    let (arrived, phase) = mpsc::channel();
    let (release, resume) = mpsc::channel();
    let server = thread::spawn(move || {
        let (mut stream, _) = identity_routes::accept_request_including_friends(&listener);
        raw_reply(&mut stream, 200, &explicit_session(closed));
        drop(stream);
        let (mut stream, request) = identity_routes::accept_request_including_friends(&listener);
        assert!(request.starts_with("GET /identity/2.0/friends "));
        raw_reply(&mut stream, 503, "");
        drop(stream);
        arrived.send(()).unwrap();
        let mut profiles = Vec::new();
        let mut batches = 0;
        // Hold all three misses until every request reaches the server. Exactly
        // one OAuth request reserves the permission piggyback; connection order
        // can differ from admission order because these are real workers.
        for _ in 0..if oauth { 3 } else { 1 } {
            let (stream, request) = identity_routes::accept_request_including_friends(&listener);
            let batch = request.starts_with("POST /v2.0 HTTP/1.1");
            if batch {
                assert!(oauth);
                batches += 1;
                let (headers, body) = request.split_once("\r\n\r\n").unwrap();
                crate::facebook_graph::test_wire::assert_batch(
                    &format!("{headers}\r\n\r\n"),
                    body,
                    "12345",
                    &["me", "me/permissions"],
                    "synthetic-host-session",
                );
            } else {
                assert!(request.starts_with("GET /v2.0/me?"), "{request}");
            }
            assert!(request.contains("access_token=synthetic-host-session"));
            assert!(!request.to_lowercase().contains("x-access-token"));
            profiles.push((stream, batch));
        }
        assert_eq!(batches, usize::from(oauth));
        for (mut stream, batch) in profiles {
            let profile = json!({"id":"new-platform-id","name":"New Platform"});
            let body = if batch {
                crate::facebook_graph::test_wire::batch_response(&[
                    (200, profile),
                    (
                        200,
                        json!({"data":[{"public_profile":true,"email":true,"user_friends":true,"user_birthday":true}]}),
                    ),
                ])
            } else {
                profile.to_string()
            };
            raw_reply(&mut stream, 200, &body);
        }
        let (mut stream, request) = identity_routes::accept_request_including_friends(&listener);
        assert!(request.starts_with("GET /v2.0//me/friends?"), "{request}");
        raw_reply(
            &mut stream,
            200,
            r#"{"data":[{"id":"repeated"},{"id":"repeated"}]}"#,
        );
        drop(stream);
        let (mut stream, request) = identity_routes::accept_request_including_friends(&listener);
        assert!(request.starts_with("POST /identity/2.0/external/connect "));
        storage_session::assert_auth(&request, "synthetic-friend-access", "8, 2");
        let body: serde_json::Value =
            serde_json::from_str(storage_session::body(&request)).unwrap();
        assert_eq!(body["externalAttributes"]["userId"], "new-platform-id");
        assert_eq!(
            body["externalAttributes"]["accessToken"],
            "synthetic-host-session"
        );
        arrived.send(()).unwrap();
        resume.recv_timeout(Duration::from_secs(8)).unwrap();
        assert!(!callback_seen.load(Ordering::Acquire));
        raw_reply(&mut stream, 201, "ignored");
        drop(stream);
        let (mut stream, request) = identity_routes::accept_request_including_friends(&listener);
        assert!(request.starts_with("POST /identity/2.0/friends "));
        assert_eq!(
            storage_session::body(&request),
            "networkId=repeated&networkId=repeated&networkProvider=facebook"
        );
        raw_reply(&mut stream, 204, "");
        drop(stream);
        let (mut stream, request) = identity_routes::accept_request_including_friends(&listener);
        assert!(request.starts_with("GET /identity/3.0/profile/own "));
        assert!(!request.to_lowercase().contains("rovio-sgs"));
        let mut own = linked_profile();
        own["publicAccountId"] = json!("connected-current");
        own["socialNetworks"][0]["id"] = json!("new-platform-id");
        own["socialNetworks"][0]["socialAttributes"]["name"] = json!("Current Own");
        raw_reply(&mut stream, 200, &own.to_string());
        drop(stream);
        let (mut stream, request) = identity_routes::accept_request_including_friends(&listener);
        assert!(request.starts_with("GET /identity/2.0/friends "));
        assert!(
            callback_seen.load(Ordering::Acquire),
            "refresh preceded actual Lua connection callback"
        );
        raw_reply(&mut stream, 200, r#"{"socialFriends":[]}"#);
        drop(stream);
        let (mut stream, request) = identity_routes::accept_request_including_friends(&listener);
        assert!(request.starts_with("GET /v2.0//me/friends?"));
        raw_reply(
            &mut stream,
            200,
            r#"{"data":[{"id":"merged-platform-only","name":"Friend"}]}"#,
        );
        listener
    });
    let runtime = platform_runtime(&sandbox, &origin);
    let (provider, complete_login) = host_provider(&origin, !closed);
    let browser_urls = Arc::new(Mutex::new(Vec::new()));
    let launched = browser_urls.clone();
    let oauth_provider = Arc::new(
        FacebookOAuthSession::new(
            FacebookOAuthConfig {
                rest_root: None,
                graph_root: format!("{origin}/v2.0"),
                authorization_url: format!("{origin}/oauth"),
                app_id: "12345".into(),
                url_scheme_suffix: String::new(),
                request_birthday: true,
            },
            move |url| {
                launched.lock().unwrap().push(url.to_owned());
                Ok(true)
            },
        )
        .unwrap(),
    );
    runtime
        .set_facebook_session(Some(if oauth {
            oauth_provider.clone()
        } else {
            provider.clone()
        }))
        .unwrap();
    let attempts = || {
        if oauth {
            browser_urls.lock().unwrap().len()
        } else {
            provider.attempts.load(Ordering::Acquire)
        }
    };
    runtime
        .lua()
        .globals()
        .set(
            "record_native_connection",
            runtime
                .lua()
                .create_function(move |_, ()| {
                    connected.store(true, Ordering::Release);
                    Ok(())
                })
                .unwrap(),
        )
        .unwrap();
    runtime.execute_source(r#"
            _G.SocialManager.onSocialNetworkConnected=function(network)
                if network~='facebook' or not _G.SocialManager.native_isConnectedToSocialNetwork() then error('state not installed before callback') end
                if _G.SocialManager.native_getLocalUserAccountId()~='connected-current' then error('callback used old profile') end
                platform_connected_count=platform_connected_count+1
                _G.record_native_connection()
            end
        "#).unwrap();
    login(&runtime, 1);
    wait_signal(&runtime, &phase);
    if closed {
        assert_eq!(runtime.social.platform_state_for_test(), (0, true, false));
        assert_eq!(attempts(), 1);
        runtime
            .execute_source("_G.SocialManager.native_connectToSocialNetwork()")
            .unwrap();
        assert_eq!(attempts(), 1);
        if oauth {
            let url = browser_urls.lock().unwrap()[0].clone();
            assert!(url.contains("response_type=token"));
            assert!(url.contains("redirect_uri=fb12345%3A%2F%2Fauthorize"));
            assert!(url.contains("scope=public_profile%2Cemail%2Cuser_friends%2Cuser_birthday"));
            assert!(runtime.handle_platform_open_url("fb12345://authorize#access_token=synthetic-host-session&granted_scopes=public_profile,email,user_friends").unwrap());
            assert_eq!(oauth_provider.session_state(), FacebookSessionState::Open);
            assert_eq!(oauth_provider.declined_permissions(), ["user_birthday"]);
            runtime.post_application_resumed();
            dispatch_registered_application_events(runtime.lua()).unwrap();
            assert!(oauth_provider.is_logged_in());
        } else {
            complete_login.send(Ok(())).unwrap();
        }
    } else {
        runtime
            .execute_source("_G.SocialManager.native_connectToSocialNetwork()")
            .unwrap();
        assert_eq!(attempts(), 0);
    }
    wait_signal(&runtime, &phase);
    runtime
        .execute_source("_G.SocialManager.native_connectToSocialNetwork()")
        .unwrap();
    assert_eq!(
        game_environment(runtime.lua())
            .unwrap()
            .get::<i32>("platform_connected_count")
            .unwrap(),
        0
    );
    release.send(()).unwrap();
    let mut done = false;
    for _ in 0..1500 {
        dispatch_registered_application_events(runtime.lua()).unwrap();
        if runtime
            .skynest_account
            .read_friends_cache_for_test("connected-current")
            .contains("merged-platform-only")
        {
            done = true;
            break;
        }
        thread::sleep(Duration::from_millis(2));
    }
    assert!(done, "real platform merge did not complete");
    let listener = server.join().unwrap();
    assert!(matches!(listener.accept(),Err(e) if e.kind()==std::io::ErrorKind::WouldBlock));
    assert_eq!(
        game_environment(runtime.lua())
            .unwrap()
            .get::<i32>("platform_connected_count")
            .unwrap(),
        1
    );
    assert_eq!(runtime.social.platform_state_for_test(), (0, false, true));
    assert_eq!(
        runtime.skynest_account.read_friends_cache_for_test("own"),
        r#"{"friends":[{"accountId":"cached","nickName":"Cached"}]}"#
    );
}

#[test]
fn native_platform_login_error_and_retired_host_completion_never_start_graph_or_publish() {
    for retired in [false, true] {
        let sandbox = ShippedDataSandbox::new("native-platform-login-failure");
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let origin = format!("http://{}", listener.local_addr().unwrap());
        let (arrived, phase) = mpsc::channel();
        let server = thread::spawn(move || {
            let (mut stream, _) = identity_routes::accept_request_including_friends(&listener);
            raw_reply(&mut stream, 200, &linked_session());
            drop(stream);
            let (mut stream, request) =
                identity_routes::accept_request_including_friends(&listener);
            assert!(request.starts_with("GET /identity/2.0/friends "));
            raw_reply(&mut stream, 503, "");
            drop(stream);
            arrived.send(()).unwrap();
            listener
        });
        let runtime = platform_runtime(&sandbox, &origin);
        let (provider, complete) = host_provider(&origin, false);
        runtime
            .set_facebook_session(Some(provider.clone()))
            .unwrap();
        login(&runtime, 1);
        wait_signal(&runtime, &phase);
        assert_eq!(provider.attempts.load(Ordering::Acquire), 1);
        let listener = server.join().unwrap();
        let queue = runtime.social.online_completion_count_probe();
        dispatch_registered_application_events(runtime.lua()).unwrap();
        assert_eq!(queue(), 0);
        if retired {
            runtime.set_facebook_session(None).unwrap();
        }
        complete
            .send(if retired {
                Ok(())
            } else {
                Err(SocialPlatformError::NotLoggedIn)
            })
            .unwrap();
        for _ in 0..500 {
            if queue() > 0 {
                break;
            }
            thread::sleep(Duration::from_millis(2));
        }
        assert!(queue() > 0, "actual login worker did not finish");
        dispatch_registered_application_events(runtime.lua()).unwrap();
        assert_eq!(runtime.social.platform_state_for_test(), (0, false, false));
        if !retired {
            runtime
                .execute_source("_G.SocialManager.native_connectToSocialNetwork()")
                .unwrap();
            assert_eq!(provider.attempts.load(Ordering::Acquire), 2);
            assert_eq!(runtime.social.platform_state_for_test(), (0, false, false));
        }
        assert_eq!(
            game_environment(runtime.lua())
                .unwrap()
                .get::<i32>("platform_connected_count")
                .unwrap(),
            0
        );
        assert_eq!(
            runtime.skynest_account.read_friends_cache_for_test("own"),
            r#"{"friends":[{"accountId":"cached","nickName":"Cached"}]}"#
        );
        assert!(matches!(listener.accept(),Err(e) if e.kind()==std::io::ErrorKind::WouldBlock));
    }
}

#[test]
fn native_platform_oauth_resume_cancels_on_delivery_without_account_gate_or_worker() {
    for initialized in [false, true] {
        let sandbox = ShippedDataSandbox::new("native-oauth-resume-cancel");
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let origin = format!("http://{}", listener.local_addr().unwrap());
        let (arrived, phase) = mpsc::channel();
        let server = thread::spawn(move || {
            if initialized {
                let (mut stream, _) = identity_routes::accept_request_including_friends(&listener);
                raw_reply(&mut stream, 200, &linked_session());
                drop(stream);
                let (mut stream, request) =
                    identity_routes::accept_request_including_friends(&listener);
                assert!(request.starts_with("GET /identity/2.0/friends "));
                raw_reply(&mut stream, 503, "");
            }
            arrived.send(()).unwrap();
            listener
        });
        let runtime = platform_runtime(&sandbox, &origin);
        let provider = Arc::new(
            FacebookOAuthSession::new(
                FacebookOAuthConfig {
                    rest_root: None,
                    graph_root: format!("{origin}/v2.0"),
                    authorization_url: format!("{origin}/oauth"),
                    app_id: "12345".into(),
                    url_scheme_suffix: String::new(),
                    request_birthday: true,
                },
                |_| Ok(true),
            )
            .unwrap(),
        );
        runtime
            .set_facebook_session(Some(provider.clone()))
            .unwrap();
        if initialized {
            login(&runtime, 1);
            wait_signal(&runtime, &phase);
            assert_eq!(runtime.social.platform_state_for_test(), (0, true, false));
        } else {
            assert!(matches!(
                provider.clone().prepare_login(),
                SocialLoginRequest::AwaitingCallback
            ));
        }
        let listener = server.join().unwrap();
        runtime.post_application_resumed();
        assert_eq!(provider.session_state(), FacebookSessionState::Opening);
        dispatch_registered_application_events(runtime.lua()).unwrap();
        assert_eq!(
            provider.session_state(),
            FacebookSessionState::ClosedLoginFailed
        );
        assert_eq!(runtime.social.platform_state_for_test(), (0, false, false));
        if initialized {
            // The nil-error login callback and subsequent closed-profile
            // failure both ran synchronously in this one activation delivery.
            assert_eq!(provider.take_login_completion(), None);
            assert_eq!(
                game_environment(runtime.lua())
                    .unwrap()
                    .get::<i32>("platform_connected_count")
                    .unwrap(),
                0
            );
        } else {
            assert_eq!(provider.take_login_completion(), Some(Ok(())));
        }
        assert!(
            !runtime
                .handle_platform_open_url("fb12345://authorize#access_token=synthetic-late")
                .unwrap()
        );
        assert!(matches!(listener.accept(), Err(e) if e.kind()==std::io::ErrorKind::WouldBlock));
        // Removing a provider retires delivery. It does not invent a logout
        // or activate that removed provider on a subsequent resume.
        assert!(matches!(
            provider.clone().prepare_login(),
            SocialLoginRequest::AwaitingCallback
        ));
        runtime.set_facebook_session(None).unwrap();
        runtime.post_application_resumed();
        dispatch_registered_application_events(runtime.lua()).unwrap();
        assert_eq!(provider.session_state(), FacebookSessionState::Opening);
        assert!(
            !runtime
                .handle_platform_open_url("fb12345://authorize#access_token=synthetic-removed")
                .unwrap()
        );
    }
}

#[test]
fn native_platform_explicit_connection_errors_never_emit_lua_success_or_refresh() {
    for failure_phase in 0..3 {
        let sandbox = ShippedDataSandbox::new("native-platform-explicit-errors");
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let origin = format!("http://{}", listener.local_addr().unwrap());
        let (arrived, phase) = mpsc::channel();
        let server = thread::spawn(move || {
            let (mut stream, _) = identity_routes::accept_request_including_friends(&listener);
            raw_reply(&mut stream, 200, &explicit_session(false));
            drop(stream);
            let (mut stream, request) =
                identity_routes::accept_request_including_friends(&listener);
            assert!(request.starts_with("GET /identity/2.0/friends "));
            raw_reply(&mut stream, 503, "");
            drop(stream);
            arrived.send(()).unwrap();
            for (index, (expected, response)) in [
                ("GET /v2.0/me?", r#"{"id":"new-platform-id","name":"New"}"#),
                ("GET /v2.0//me/friends?", r#"{"data":[]}"#),
                ("POST /identity/2.0/external/connect ", ""),
                ("GET /identity/3.0/profile/own ", ""),
            ]
            .into_iter()
            .enumerate()
            {
                let (mut stream, request) =
                    identity_routes::accept_request_including_friends(&listener);
                assert!(request.starts_with(expected), "{request}");
                let failing = index == [0, 2, 3][failure_phase];
                if failing {
                    if failure_phase == 0 {
                        raw_reply(&mut stream, 400, r#"{"error":null}"#);
                    } else {
                        raw_reply(&mut stream, if failure_phase == 1 { 503 } else { 201 }, "");
                    }
                    break;
                }
                raw_reply(&mut stream, 200, response);
            }
            listener
        });
        let runtime = platform_runtime(&sandbox, &origin);
        login(&runtime, 1);
        wait_signal(&runtime, &phase);
        runtime
            .execute_source("_G.SocialManager.native_connectToSocialNetwork()")
            .unwrap();
        assert_eq!(runtime.social.platform_state_for_test(), (0, true, false));
        for _ in 0..1500 {
            dispatch_registered_application_events(runtime.lua()).unwrap();
            if !runtime.social.platform_state_for_test().1 {
                break;
            }
            thread::sleep(Duration::from_millis(2));
        }
        assert_eq!(runtime.social.platform_state_for_test(), (0, false, false));
        let listener = server.join().unwrap();
        assert!(matches!(listener.accept(),Err(e) if e.kind()==std::io::ErrorKind::WouldBlock));
        assert_eq!(
            game_environment(runtime.lua())
                .unwrap()
                .get::<i32>("platform_connected_count")
                .unwrap(),
            0
        );
        assert_eq!(
            runtime.skynest_account.read_friends_cache_for_test("own"),
            r#"{"friends":[{"accountId":"cached","nickName":"Cached"}]}"#
        );
    }
}
