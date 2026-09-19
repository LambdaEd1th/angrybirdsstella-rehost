use super::*;
use crate::facebook_graph::test_wire;
use serde_json::json;
use std::{io::Write, net::TcpListener, thread, time::Duration};

mod completion;
mod system_account;

fn metadata(now: f64) -> token_cache::CachedToken {
    token_cache::CachedToken::from_response(
        "synthetic-old".into(),
        vec![
            "public_profile".into(),
            "email".into(),
            "user_friends".into(),
        ],
        &BTreeMap::from([("expires_in".into(), "400000".into())]),
        3,
        now,
    )
}

#[test]
fn facebook_oauth_refresh_admission_has_independent_strict_clocks_and_native_login_type() {
    let now = 200000.0;
    let mut token = metadata(now - 86400.0);
    token.refresh_permissions(vec!["email".into()], now - 86400.0);
    let mut state = State::default();
    state.install(token);
    assert_eq!(state.admit(now).unwrap(), (false, false));
    assert_eq!(state.admit(now + 0.01).unwrap(), (true, true));
    assert_eq!(state.admit(now + 3600.01).unwrap(), (false, false));
    state.attempted_permissions -= 0.01;
    assert_eq!(state.admit(now + 3600.01).unwrap(), (false, true));
    state
        .token
        .as_mut()
        .unwrap()
        .extend(None, now + 500000.0, now);
    assert_eq!(state.admit(now + 200000.0).unwrap(), (false, true));
}

#[test]
fn facebook_oauth_refresh_batch_mutates_only_on_main_and_normalizes_same_session_profile() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let cfg = FacebookOAuthConfig {
        rest_root: None,
        graph_root: format!("http://{}/v2.0", listener.local_addr().unwrap()),
        authorization_url: "http://127.0.0.1:9/oauth".into(),
        app_id: "12345".into(),
        url_scheme_suffix: String::new(),
        request_birthday: false,
    };
    let cache = FacebookTokenCache::memory();
    let session =
        Arc::new(FacebookOAuthSession::new_with_cache(cfg, cache.clone(), |_| Ok(true)).unwrap());
    assert!(matches!(
        session.clone().prepare_login(),
        SocialLoginRequest::AwaitingCallback
    ));
    session
        .handle_open_url("fb12345://authorize#access_token=synthetic-old&expires_in=3600")
        .unwrap();
    assert_eq!(session.take_login_completion(), Some(Ok(())));
    let now = token_cache::now();
    session
        .state
        .lock()
        .unwrap()
        .refresh
        .install(metadata(now - 86401.0));
    let tasks = Arc::new(Mutex::new(VecDeque::new()));
    let queued = tasks.clone();
    session
        .set_application_dispatcher(Arc::new(move |task| queued.lock().unwrap().push_back(task)));
    let expires = now + 100000.0;
    let server = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(5)))
            .unwrap();
        let (headers, body) = test_wire::read_request(&mut stream);
        test_wire::assert_batch(
            &headers,
            &body,
            "12345",
            &["me", "method/auth.extendSSOAccessToken", "me/permissions"],
            "synthetic-old",
        );
        let body = test_wire::batch_response(&[
            (200, json!({"id":"user", "name":"Current User"})),
            (
                200,
                json!({"access_token":"synthetic-new", "expires_at":expires}),
            ),
            (
                200,
                json!({"data":[{"permission":"email","status":"granted"},
                {"permission":"user_friends","status":"declined"},
                {"permission":"public_profile","status":"declined"}]}),
            ),
        ]);
        write!(
            stream,
            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        )
        .unwrap();
    });
    let request = session.clone().prepare_user_profile();
    let profile = thread::spawn(move || request.execute())
        .join()
        .unwrap()
        .unwrap();
    server.join().unwrap();
    assert_eq!(profile.access_token, "synthetic-old");
    assert_eq!(session.session_state(), FacebookSessionState::Open);
    assert_eq!(
        session.granted_permissions(),
        ["public_profile", "email", "user_friends"]
    );
    assert!(
        !cache
            .admitted(&[])
            .unwrap()
            .unwrap()
            .admits(&[], now + 4000.0)
    );
    assert_eq!(tasks.lock().unwrap().len(), 1);
    tasks.lock().unwrap().pop_front().unwrap().run();
    assert_eq!(
        session.session_state(),
        FacebookSessionState::OpenTokenExtended
    );
    assert_eq!(session.granted_permissions(), ["email"]);
    assert_eq!(session.declined_permissions(), ["user_friends"]);
    assert_eq!(session.take_login_completion(), None);
    let published = session.publish_completed_profile(&profile).unwrap();
    assert_eq!(published.access_token, "synthetic-new");
    let persisted = cache.admitted(&["email".into()]).unwrap().unwrap();
    assert_eq!(persisted.token, "synthetic-new");
    assert!(persisted.admits(&["email".into()], expires - 1.0));
    assert!(!persisted.admits(&[], expires));
    assert!(!persisted.should_extend(now + 300000.0).unwrap());
    assert!(
        !persisted
            .should_refresh_permissions(token_cache::now())
            .unwrap()
    );
    // Insufficient newly granted scopes cause a new browser session. Even an
    // identical token string cannot make the previous profile belong to it.
    assert!(matches!(
        session.clone().prepare_login(),
        SocialLoginRequest::AwaitingCallback
    ));
    session
        .handle_open_url("fb12345://authorize#access_token=synthetic-new")
        .unwrap();
    assert!(matches!(
        session.publish_completed_profile(&published),
        Err(SocialPlatformError::Cancelled)
    ));
}

#[test]
fn facebook_oauth_refresh_permissions_legacy_values_and_empty_response_match_native() {
    assert_eq!(
        parse_permissions(&json!({"data":[{"email":0,"user_friends":false}]})),
        Some((
            vec!["email".into(), "user_friends".into()],
            vec!["email".into(), "user_friends".into()]
        ))
    );
    assert_eq!(parse_permissions(&json!({"data":[]})), None);
    assert_eq!(parse_permissions(&json!({"data":[{}]})), None);
    assert_eq!(
        parse_permissions(&json!({"data":[{"permission":"","status":"granted"}]})),
        Some((vec![String::new()], vec![String::new()]))
    );
}

#[test]
fn facebook_oauth_refresh_errors_nil_tokens_and_retired_tasks_follow_session_rules() {
    for case in 0..15 {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let cfg = FacebookOAuthConfig {
            rest_root: None,
            graph_root: format!("http://{}/v2.0", listener.local_addr().unwrap()),
            authorization_url: "http://127.0.0.1:9/oauth".into(),
            app_id: "12345".into(),
            url_scheme_suffix: String::new(),
            request_birthday: false,
        };
        let session = Arc::new(FacebookOAuthSession::new(cfg, |_| Ok(true)).unwrap());
        assert!(matches!(
            session.clone().prepare_login(),
            SocialLoginRequest::AwaitingCallback
        ));
        session
            .handle_open_url("fb12345://authorize#access_token=synthetic-old")
            .unwrap();
        session.take_login_completion();
        let now = token_cache::now();
        session
            .state
            .lock()
            .unwrap()
            .refresh
            .install(metadata(now - 90000.0));
        let extension = match case {
            0 => (200, json!({"expires_at":now+100000.0})), // nil token retains old.
            1 => (200, json!({"access_token":null,"expires_at":now+100000.0})),
            2 => (200, json!({"access_token":"ignored","expires_at":0})),
            3 => (400, json!({"error":{"code":190,"error_subcode":459}})),
            4 => (400, json!({"error":{"code":190}})),
            7 => (400, json!({"code":190})),
            8 => (400, json!({"error":{"error_code":190}})),
            9 => (400, json!({"error":{"code":"190"}})),
            10 => (400, json!({"error":{"code":4294967486u64}})),
            11 => (400, json!({"error":{"code":190.9,"error_subcode":459.9}})),
            12 => (400, json!({"error_code":190})),
            13 => (400, json!({"error":{"code":190.9}})),
            14 => (400, json!({"error":{"code":190,"error_subcode":"459"}})),
            _ => (
                200,
                json!({"access_token":"synthetic-new","expires_at":now+100000.0}),
            ),
        };
        let permissions = if case == 5 {
            (400, json!({"error":{"code":190}})) // canCloseSessionOnError=false.
        } else {
            (200, json!({"data":[{"email":false}]}))
        };
        let server = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            stream
                .set_read_timeout(Some(Duration::from_secs(5)))
                .unwrap();
            let (headers, body) = test_wire::read_request(&mut stream);
            test_wire::assert_batch(
                &headers,
                &body,
                "12345",
                &["me", "method/auth.extendSSOAccessToken", "me/permissions"],
                "synthetic-old",
            );
            let body = test_wire::batch_response(&[
                (200, json!({"id":"user","name":"Name"})),
                extension,
                permissions,
            ]);
            write!(
                stream,
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            )
            .unwrap();
        });
        let profile = session.clone().prepare_user_profile().execute().unwrap();
        server.join().unwrap();
        assert_eq!(session.session_state(), FacebookSessionState::Open);
        if case == 6 {
            session.logout().unwrap();
        }
        session.dispatch_pending_sdk_completions();
        match case {
            0 => {
                assert_eq!(
                    session.session_state(),
                    FacebookSessionState::OpenTokenExtended
                );
                assert_eq!(
                    session
                        .publish_completed_profile(&profile)
                        .unwrap()
                        .access_token,
                    "synthetic-old"
                );
            }
            1..=3 | 7..=9 | 11..=12 => {
                assert_eq!(session.session_state(), FacebookSessionState::Open);
                assert!(session.token_cache.admitted(&[]).unwrap().is_some());
            }
            4 | 6 | 10 | 13..=14 => {
                assert_eq!(session.session_state(), FacebookSessionState::Closed);
                assert!(session.token_cache.admitted(&[]).unwrap().is_none());
                assert!(session.publish_completed_profile(&profile).is_err());
            }
            5 => {
                assert_eq!(
                    session.session_state(),
                    FacebookSessionState::OpenTokenExtended
                );
                assert_eq!(
                    session.granted_permissions(),
                    ["public_profile", "email", "user_friends"]
                );
            }
            _ => unreachable!(),
        }
        if matches!(case, 0..=3 | 7..=9 | 11..=12) {
            assert_eq!(session.granted_permissions(), ["email"]);
        }
    }
}
