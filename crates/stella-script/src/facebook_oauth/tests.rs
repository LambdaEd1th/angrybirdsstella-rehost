use super::*;
mod application;
mod dialog;

fn config() -> FacebookOAuthConfig {
    FacebookOAuthConfig {
        rest_root: None,
        graph_root: "http://127.0.0.1:9/v2.0".into(),
        authorization_url: "http://127.0.0.1:9/oauth".into(),
        app_id: "12345".into(),
        url_scheme_suffix: "test".into(),
        request_birthday: true,
    }
}

#[test]
fn facebook_oauth_callback_does_not_report_implicit_permissions_as_declined() {
    let session = Arc::new(FacebookOAuthSession::new(config(), |_| Ok(true)).unwrap());
    assert!(matches!(
        session.clone().prepare_login(),
        SocialLoginRequest::AwaitingCallback
    ));
    assert!(session.handle_open_url("fb12345test://authorize#access_token=synthetic-implicit-permissions&granted_scopes=email").unwrap());
    assert_eq!(session.take_login_completion(), Some(Ok(())));
    assert!(session.is_logged_in());
    assert_eq!(session.granted_permissions(), ["email"]);
    assert_eq!(
        session.declined_permissions(),
        ["user_friends", "user_birthday"]
    );
}

#[test]
fn facebook_oauth_declined_updates_preserve_unmentioned_denials_without_duplicates() {
    let mut declined = vec!["email".into(), "previous_denial".into(), "email".into()];
    protocol::update_declined(
        &mut declined,
        &[
            "email".into(),
            "public_profile".into(),
            "basic_info".into(),
            "user_birthday".into(),
            "user_birthday".into(),
        ],
        &["email".into()],
    );
    assert_eq!(declined, ["previous_denial", "user_birthday"]);
    protocol::update_declined(&mut declined, &[], &["user_birthday".into()]);
    assert_eq!(declined, ["previous_denial"]);
}

#[test]
fn facebook_oauth_launch_contains_native_browser_protocol() {
    let urls = Arc::new(Mutex::new(Vec::new()));
    let captured = urls.clone();
    let session = Arc::new(
        FacebookOAuthSession::new(config(), move |url| {
            captured.lock().unwrap().push(url.to_owned());
            Ok(true)
        })
        .unwrap(),
    );
    assert!(matches!(
        session.clone().prepare_login(),
        SocialLoginRequest::AwaitingCallback
    ));
    let urls = urls.lock().unwrap();
    let url = &urls[0];
    for part in [
        "response_type=token",
        "redirect_uri=fb12345test%3A%2F%2Fauthorize",
        "sdk_version=3.14.1",
        "legacy_override=v2.0",
        "local_client_id=test",
        "scope=public_profile%2Cemail%2Cuser_friends%2Cuser_birthday",
        "browser_auth",
    ] {
        assert!(url.contains(part), "missing {part}");
    }
    assert_eq!(session.session_state(), FacebookSessionState::Opening);
    assert!(!session.is_logged_in());
    assert!(session.take_login_completion().is_none());
}

#[test]
fn facebook_oauth_implicit_cancel_reports_nil_error_then_closed_profile_once() {
    let session = Arc::new(FacebookOAuthSession::new(config(), |_| Ok(true)).unwrap());
    assert!(matches!(
        session.clone().prepare_login(),
        SocialLoginRequest::AwaitingCallback
    ));
    session.logout().unwrap(); // FacebookService logout is a no-op while opening.
    assert_eq!(session.session_state(), FacebookSessionState::Opening);
    session.application_resumed();
    assert_eq!(
        session.session_state(),
        FacebookSessionState::ClosedLoginFailed
    );
    assert_eq!(session.take_login_completion(), Some(Ok(())));
    assert!(matches!(
        session.clone().prepare_user_profile(),
        SocialProfileRequest::Ready(Err(SocialPlatformError::NotLoggedIn))
    ));
    session.application_resumed();
    assert!(session.take_login_completion().is_none());
    assert!(
        !session
            .handle_open_url("fb12345test://authorize#access_token=synthetic-late")
            .unwrap()
    );
    assert!(!session.is_logged_in());
}

#[test]
fn facebook_oauth_callback_precedes_resume_and_preserves_fragment_precedence() {
    let session = Arc::new(FacebookOAuthSession::new(config(), |_| Ok(true)).unwrap());
    assert!(matches!(
        session.clone().prepare_login(),
        SocialLoginRequest::AwaitingCallback
    ));
    assert!(
        !session
            .handle_open_url("fb999://authorize#access_token=wrong")
            .unwrap()
    );
    assert!(session.handle_open_url("fb12345test://authorize?access_token=query#access_token=synthetic%2Btoken&granted_scopes=public_profile,email").unwrap());
    assert!(session.is_logged_in());
    assert_eq!(session.granted_permissions(), ["public_profile", "email"]);
    assert_eq!(
        session.declined_permissions(),
        ["user_friends", "user_birthday"]
    );
    assert!(session.graph().unwrap().has_access_token("synthetic+token"));
    assert!(matches!(
        session.clone().take_login_profile_request(),
        Some(SocialProfileRequest::Pending(_))
    ));
    assert!(session.clone().take_login_profile_request().is_none());
    session.application_resumed();
    assert_eq!(session.session_state(), FacebookSessionState::Open);
    assert_eq!(session.take_login_completion(), Some(Ok(())));
    assert!(session.take_login_completion().is_none());
    session.logout().unwrap();
    assert!(!session.is_logged_in());
    assert!(session.state.lock().unwrap().graph.is_none());
    assert!(!format!("{session:?}").contains("synthetic"));
}

#[test]
fn facebook_oauth_launch_failure_explicit_denial_and_retry_are_real_failures() {
    let unavailable = Arc::new(FacebookOAuthSession::new(config(), |_| Ok(false)).unwrap());
    assert!(matches!(
        unavailable.prepare_login(),
        SocialLoginRequest::Ready(Err(SocialPlatformError::Unavailable))
    ));
    let session = Arc::new(FacebookOAuthSession::new(config(), |_| Ok(true)).unwrap());
    for _ in 0..2 {
        assert!(matches!(
            session.clone().prepare_login(),
            SocialLoginRequest::AwaitingCallback
        ));
        assert!(
            session
                .handle_open_url("fb12345test://authorize#error=access_denied")
                .unwrap()
        );
        assert_eq!(
            session.take_login_completion(),
            Some(Err(SocialPlatformError::Cancelled))
        );
        assert_eq!(
            session.session_state(),
            FacebookSessionState::ClosedLoginFailed
        );
        assert!(session.clone().take_login_profile_request().is_none());
    }
}

#[test]
fn facebook_oauth_parser_handles_empty_values_duplicates_and_invalid_encoding() {
    let params = protocol::callback_params(
        &config(),
        "fb12345test://authorize?k=a+b&k=x%3Dy&empty#k=last%20one&flag",
    )
    .unwrap()
    .unwrap();
    assert_eq!(params["k"], "last one");
    assert_eq!(params["empty"], "");
    assert_eq!(params["flag"], "");
    for suffix in ["x=%", "x=%GG", "x=%FF"] {
        assert_eq!(
            protocol::callback_params(&config(), &format!("fb12345test://authorize?{suffix}")),
            Err(SocialPlatformError::InvalidResponse)
        );
    }
    let mut invalid = config();
    invalid.authorization_url = "https://user:secret@example.com/oauth".into();
    assert!(matches!(
        FacebookOAuthSession::new(invalid, |_| Ok(true)),
        Err(SocialPlatformError::InvalidConfiguration)
    ));
}

#[test]
fn facebook_oauth_browser_fallback_reuses_logger_and_waits_for_actual_result() {
    for accept_retry in [true, false] {
        let urls = Arc::new(Mutex::new(Vec::new()));
        let captured = urls.clone();
        let session = Arc::new(
            FacebookOAuthSession::new(config(), move |url| {
                let mut urls = captured.lock().unwrap();
                urls.push(url.to_owned());
                Ok(urls.len() == 1 || accept_retry)
            })
            .unwrap(),
        );
        assert!(matches!(
            session.clone().prepare_login(),
            SocialLoginRequest::AwaitingCallback
        ));
        assert!(
            session
                .handle_open_url("fb12345test://authorize#error=service_disabled_use_browser")
                .unwrap()
        );
        let urls = urls.lock().unwrap();
        assert_eq!(urls.len(), 2);
        let state_param = |url: &String| {
            url.split('&')
                .find(|p| p.starts_with("state="))
                .unwrap()
                .to_owned()
        };
        assert_eq!(state_param(&urls[0]), state_param(&urls[1]));
        assert!(session.clone().take_login_profile_request().is_none());
        if accept_retry {
            assert_eq!(session.session_state(), FacebookSessionState::Opening);
            assert_eq!(session.take_login_completion(), None);
            assert!(
                session
                    .handle_open_url("fb12345test://authorize#access_token=synthetic-retry")
                    .unwrap()
            );
            assert_eq!(session.take_login_completion(), Some(Ok(())));
            assert!(session.is_logged_in());
        } else {
            assert_eq!(
                session.session_state(),
                FacebookSessionState::ClosedLoginFailed
            );
            assert_eq!(
                session.take_login_completion(),
                Some(Err(SocialPlatformError::Unavailable))
            );
            assert!(!session.is_logged_in());
        }
    }
}

#[test]
fn facebook_oauth_reopen_retains_service_user_but_uses_current_token_and_logout_clears() {
    use std::{io::Write, net::TcpListener, thread, time::Duration};
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let mut cfg = config();
    cfg.graph_root = format!("http://{}/v2.0", listener.local_addr().unwrap());
    let server = thread::spawn(move || {
        for (token, name) in [("one", "First User"), ("three", "After Logout")] {
            let (mut stream, _) = listener.accept().unwrap();
            stream
                .set_read_timeout(Some(Duration::from_secs(5)))
                .unwrap();
            let (headers, body) = crate::facebook_graph::test_wire::read_request(&mut stream);
            crate::facebook_graph::test_wire::assert_batch(
                &headers,
                &body,
                "12345",
                &["me", "me/permissions"],
                token,
            );
            let body = crate::facebook_graph::test_wire::batch_response(&[
                (200, serde_json::json!({"id":token,"name":name})),
                (
                    200,
                    serde_json::json!({"data":[{"permission":"email","status":"granted"}]}),
                ),
            ]);
            write!(
                stream,
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            )
            .unwrap();
        }
        listener.set_nonblocking(true).unwrap();
        listener
    });
    let session = Arc::new(FacebookOAuthSession::new(cfg, |_| Ok(true)).unwrap());
    let authorize = |token: &str| {
        assert!(matches!(
            session.clone().prepare_login(),
            SocialLoginRequest::AwaitingCallback
        ));
        assert!(
            session
                .handle_open_url(&format!(
                    "fb12345test://authorize#access_token={token}&expires=-1"
                ))
                .unwrap()
        );
        assert_eq!(session.take_login_completion(), Some(Ok(())));
    };
    authorize("one");
    let first = session.user_profile().unwrap();
    assert_eq!(first.user.name, "First User");
    session.close();
    session.logout().unwrap(); // Already closed: keep service cache.
    authorize("two");
    let Some(SocialProfileRequest::Ready(Ok(cached))) =
        session.clone().take_login_profile_request()
    else {
        panic!("reopen discarded the completed service cache");
    };
    assert_eq!(cached.user.id, "one");
    assert_eq!(cached.user.name, "First User");
    assert_eq!(cached.access_token, "two");
    assert_eq!(session.user_profile().unwrap().access_token, "two");
    assert_eq!(
        session.publish_user_profile(&first),
        Err(SocialPlatformError::Cancelled)
    );
    session.logout().unwrap(); // Open logout: clear service and token caches.
    authorize("three");
    let Some(request @ SocialProfileRequest::Pending(_)) =
        session.clone().take_login_profile_request()
    else {
        panic!("open logout retained the old service cache");
    };
    let third = request.execute().unwrap();
    session.publish_user_profile(&third).unwrap();
    assert_eq!(third.user.name, "After Logout");
    assert_eq!(third.access_token, "three");
    let listener = server.join().unwrap();
    assert!(matches!(listener.accept(), Err(e) if e.kind() == std::io::ErrorKind::WouldBlock));
}

#[test]
fn facebook_oauth_valid_token_reopens_without_browser_and_open_logout_clears_it() {
    let launches = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let captured = launches.clone();
    let session = Arc::new(
        FacebookOAuthSession::new(config(), move |_| {
            captured.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            Ok(true)
        })
        .unwrap(),
    );
    assert!(matches!(
        session.clone().prepare_login(),
        SocialLoginRequest::AwaitingCallback
    ));
    session
        .handle_open_url("fb12345test://authorize#access_token=synthetic-cached&expires_in=3600")
        .unwrap();
    assert_eq!(session.take_login_completion(), Some(Ok(())));
    session.close();
    session.logout().unwrap(); // Closed logout preserves the token cache.
    assert!(matches!(
        session.clone().prepare_login(),
        SocialLoginRequest::Ready(Ok(()))
    ));
    assert!(session.is_logged_in());
    assert_eq!(launches.load(std::sync::atomic::Ordering::SeqCst), 1);
    assert!(matches!(
        session.clone().take_login_profile_request(),
        Some(SocialProfileRequest::Pending(_))
    ));
    assert!(session.clone().take_login_profile_request().is_none());
    session.logout().unwrap();
    assert!(matches!(
        session.clone().prepare_login(),
        SocialLoginRequest::AwaitingCallback
    ));
    assert_eq!(launches.load(std::sync::atomic::Ordering::SeqCst), 2);
}

#[test]
fn facebook_oauth_expired_or_insufficient_tokens_open_but_next_session_reauthorizes() {
    for suffix in ["expires=-1", "granted_scopes=email"] {
        let session = Arc::new(FacebookOAuthSession::new(config(), |_| Ok(true)).unwrap());
        assert!(matches!(
            session.clone().prepare_login(),
            SocialLoginRequest::AwaitingCallback
        ));
        session
            .handle_open_url(&format!(
                "fb12345test://authorize#access_token=synthetic-invalid-cache&{suffix}"
            ))
            .unwrap();
        assert_eq!(session.take_login_completion(), Some(Ok(())));
        assert!(
            session.is_logged_in(),
            "newly returned token is not expiry-gated"
        );
        session.close();
        assert!(matches!(
            session.clone().prepare_login(),
            SocialLoginRequest::AwaitingCallback
        ));
        assert!(
            session
                .token_cache
                .admitted(&protocol::permissions(&config()))
                .unwrap()
                .is_none()
        );
    }
}
