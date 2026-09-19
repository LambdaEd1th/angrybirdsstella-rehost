use super::*;
use serde_json::json;
use std::{
    io::Write,
    net::{TcpListener, TcpStream},
    thread,
    time::Instant,
};

fn accept(listener: &TcpListener) -> TcpStream {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        match listener.accept() {
            Ok((s, _)) => {
                s.set_nonblocking(false).unwrap();
                s.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
                return s;
            }
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                assert!(
                    Instant::now() < deadline,
                    "platform HTTP request did not arrive"
                );
                thread::sleep(Duration::from_millis(1));
            }
            Err(e) => panic!("{e}"),
        }
    }
}
fn read_request(stream: &mut TcpStream) -> String {
    let mut bytes = Vec::new();
    while !bytes.ends_with(b"\r\n\r\n") {
        let mut byte = [0];
        stream.read_exact(&mut byte).unwrap();
        bytes.push(byte[0]);
        assert!(bytes.len() < 8192);
    }
    String::from_utf8(bytes).unwrap()
}
fn reply(stream: &mut TcpStream, status: u16, body: &str) {
    write!(
        stream,
        "HTTP/1.1 {status} Fixture\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    )
    .unwrap();
}

#[test]
fn facebook_graph_profile_admission_keeps_concurrent_misses_and_defers_cache_publication() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let session = std::sync::Arc::new(
        FacebookGraphSession::new(
            &format!("http://{}/v2.0", listener.local_addr().unwrap()),
            "synthetic-concurrent",
        )
        .unwrap(),
    );
    let first = session.clone().prepare_user_profile();
    let second = session.clone().prepare_user_profile();
    let server = thread::spawn(move || {
        let mut a = accept(&listener);
        let ar = read_request(&mut a);
        let mut b = accept(&listener);
        let br = read_request(&mut b);
        assert_eq!(ar, br);
        reply(&mut b, 200, r#"{"id":"B","name":"B"}"#);
        reply(&mut a, 200, r#"{"id":"A","name":"A"}"#);
        listener
    });
    let a = thread::spawn(move || first.execute());
    let b = thread::spawn(move || second.execute());
    let mut profiles = [a.join().unwrap().unwrap(), b.join().unwrap().unwrap()];
    profiles.sort_by(|a, b| a.user.id.cmp(&b.user.id));
    assert_eq!(
        (&profiles[0].user.id, &profiles[1].user.id),
        (&"A".to_owned(), &"B".to_owned())
    );
    assert!(
        session.state.lock().unwrap().profile.is_none(),
        "worker completion must not publish the native cache"
    );
    session.publish_user_profile(&profiles[0]).unwrap();
    let captured = session.clone().prepare_user_profile();
    session.publish_user_profile(&profiles[1]).unwrap();
    assert_eq!(
        captured.execute().unwrap().user.id,
        "A",
        "cached admission captured its own result"
    );
    assert_eq!(
        session.user_profile().unwrap().user.id,
        "B",
        "last main-thread cache publication wins"
    );
    let listener = server.join().unwrap();
    assert!(matches!(listener.accept(), Err(e) if e.kind()==std::io::ErrorKind::WouldBlock));
}

#[test]
fn facebook_graph_uses_native_paths_query_encoding_and_cached_platform_profile() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let root = format!("http://{}/graph/v2.0", listener.local_addr().unwrap());
    let server = thread::spawn(move || {
        let mut own = accept(&listener);
        let request = read_request(&mut own);
        assert!(request.starts_with("GET /graph/v2.0/me?format=json&sdk=ios&access_token=synthetic%20%2B%2F%C3%A9 HTTP/1.1\r\n"));
        let headers = request.to_ascii_lowercase();
        assert!(
            !headers.contains("x-access-token:")
                && !headers.contains("rovio-sgs:")
                && !headers.contains("authorization:")
        );
        reply(
            &mut own,
            201,
            r#"{"id":"own","username":"own-user","name":"Own\u0000suffix"}"#,
        );
        drop(own);
        let mut friends = accept(&listener);
        let request = read_request(&mut friends);
        assert!(request.starts_with("GET /graph/v2.0//me/friends?format=json&sdk=ios&access_token=synthetic%20%2B%2F%C3%A9 HTTP/1.1\r\n"));
        reply(
            &mut friends,
            200,
            r#"{"data":[{"id":"friend","username":"fallback","name":"","picture":{"url":"must-not-use"}}],"paging":{"next":"must-not-fetch"}}"#,
        );
    });
    let session = FacebookGraphSession::new(&root, "synthetic +/é").unwrap();
    let own = session.user_profile().unwrap();
    assert_eq!(own.user.id, "own");
    assert_eq!(own.user.name, "Own");
    assert_eq!(
        own.user.avatar_url,
        "https://graph.facebook.com/own/picture?type=large"
    );
    assert_eq!(own.access_token, "synthetic +/é");
    assert!(!format!("{session:?} {own:?}").contains("synthetic +/é"));
    assert_eq!(session.user_profile().unwrap().user, own.user);
    let friends = session.friends(SocialFriendDetails::Profiles).unwrap();
    assert_eq!(friends.users.len(), 1);
    assert_eq!(friends.users[0].username, "fallback");
    assert_eq!(friends.users[0].name, "");
    assert_eq!(
        friends.users[0].avatar_url,
        "https://graph.facebook.com/friend/picture?type=normal"
    );
    assert!(friends.next_page.is_empty());
    server.join().unwrap();
}

#[test]
fn facebook_graph_nil_own_name_cannot_fabricate_a_profile_success() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let session = FacebookGraphSession::new(
        &format!("http://{}", listener.local_addr().unwrap()),
        "synthetic-invalid-profile",
    )
    .unwrap();
    let server = thread::spawn(move || {
        for body in [
            r#"{"id":"own"}"#,
            r#"{"id":"own","name":null}"#,
            r#"{"id":"own","name":""}"#,
        ] {
            let mut stream = accept(&listener);
            read_request(&mut stream);
            reply(&mut stream, 200, body);
        }
    });
    for _ in 0..2 {
        assert!(matches!(
            session.user_profile(),
            Err(SocialPlatformError::InvalidResponse)
        ));
        assert!(session.state.lock().unwrap().profile.is_none());
    }
    assert_eq!(
        session.user_profile().unwrap().user.id,
        "own",
        "empty NSString remains a valid name"
    );
    server.join().unwrap();
}

#[test]
fn facebook_graph_http_and_invalid_user_failures_do_not_become_empty_success() {
    for (status, body, error) in [
        (401, r#"{"data":[]}"#, SocialPlatformError::Http(401)),
        (
            400,
            r#"{"error":{"code":190}}"#,
            SocialPlatformError::Http(400),
        ),
        (
            200,
            r#"{"data":null}"#,
            SocialPlatformError::InvalidResponse,
        ),
        (
            200,
            r#"{"data":[{"id":17}]}"#,
            SocialPlatformError::InvalidResponse,
        ),
    ] {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let root = format!("http://{}", listener.local_addr().unwrap());
        let server = thread::spawn(move || {
            let mut s = accept(&listener);
            read_request(&mut s);
            reply(&mut s, status, body);
        });
        let session = FacebookGraphSession::new(&root, "synthetic-token").unwrap();
        let actual = session.friends(SocialFriendDetails::Profiles).unwrap_err();
        assert_eq!(actual, error);
        assert!(!format!("{actual:?} {actual}").contains("synthetic"));
        server.join().unwrap();
    }
}

#[test]
fn facebook_graph_native_non_json_wrapper_reaches_each_actual_consumer() {
    // 2DC5FC wraps the body before2DE0D0 inspects result-level errors/code.
    // Error-like body keys alone do not produce NSError with HTTP200.
    for (status, body) in [
        (200, "{"),
        (204, ""),
        (200, "true"),
        (200, r#"{"error":null,"data":[]}"#),
        (200, r#"{"error_code":0}"#),
    ] {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let root = format!("http://{}", listener.local_addr().unwrap());
        let server = thread::spawn(move || {
            for _ in 0..2 {
                let mut stream = accept(&listener);
                read_request(&mut stream);
                reply(&mut stream, status, body);
            }
        });
        let session = FacebookGraphSession::new(&root, "synthetic-wrapper").unwrap();
        assert!(
            session
                .friends(SocialFriendDetails::Profiles)
                .unwrap()
                .users
                .is_empty()
        );
        assert!(matches!(
            session.user_profile(),
            Err(SocialPlatformError::InvalidResponse)
        ));
        server.join().unwrap();
    }
}

#[test]
fn facebook_graph_friend_details_nil_ids_nul_strings_and_paging_follow_native_projection() {
    let value = json!({"data":[{}, {"id":"id\0suffix","name":"Name\0hidden","username":"User"}],"paging":{"next":"ignored"}});
    let full = parse_friends(&value, SocialFriendDetails::Profiles).unwrap();
    assert_eq!(full.users[0].id, "");
    assert_eq!(
        full.users[0].avatar_url,
        "https://graph.facebook.com/(null)/picture?type=normal"
    );
    assert_eq!(full.users[1].id, "id");
    assert_eq!(full.users[1].name, "Name");
    assert_eq!(full.users[1].avatar_url, "https://graph.facebook.com/id");
    let ids = parse_friends(&value, SocialFriendDetails::Identifiers).unwrap();
    assert!(
        ids.users
            .iter()
            .all(|u| u.name.is_empty() && u.username.is_empty() && u.avatar_url.is_empty())
    );
    let many = json!({"data":vec![json!({"id":"same"});5000],"paging":{"next":"page\0tail"}});
    let parsed = parse_friends(&many, SocialFriendDetails::Profiles).unwrap();
    assert_eq!(parsed.users.len(), 5000);
    assert_eq!(parsed.next_page, "page");
    for root in [
        Value::Null,
        json!({}),
        json!({"data":{}}),
        json!({"data":[]}),
    ] {
        assert!(
            parse_friends(&root, SocialFriendDetails::Profiles)
                .unwrap()
                .users
                .is_empty()
        );
    }
    for root in [
        json!([]),
        json!({"data":null}),
        json!({"data":{"id":"bad"}}),
        json!({"data":[false]}),
        json!({"data":[{"id":null}]}),
    ] {
        assert!(parse_friends(&root, SocialFriendDetails::Profiles).is_err());
    }
}

#[test]
fn facebook_graph_nil_batch_friends_preserve_sdk_errors() {
    for entry in [Value::Null, json!(false), json!({}), json!({"body":null})] {
        for status in [200, 400] {
            let response = wire::unpack(
                wire::Response {
                    status,
                    value: json!([entry]),
                    error: None,
                },
                1,
            )
            .unwrap()
            .pop()
            .unwrap()
            .into_result();
            if status == 400 {
                assert!(matches!(response, Err(SocialPlatformError::Http(400))));
                continue;
            }
            let value = response.unwrap();
            assert!(value.is_null());
            for details in [
                SocialFriendDetails::Identifiers,
                SocialFriendDetails::Profiles,
            ] {
                let friends = parse_friends(&value, details).unwrap();
                assert!(friends.users.is_empty());
                assert!(friends.next_page.is_empty());
            }
        }
    }
}

#[test]
fn facebook_graph_session_close_retires_inflight_result_and_prevents_new_http() {
    use std::sync::{Arc, mpsc};
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let root = format!("http://{}", listener.local_addr().unwrap());
    let (seen_tx, seen_rx) = mpsc::channel();
    let (release_tx, release_rx) = mpsc::channel();
    let server = thread::spawn(move || {
        let mut s = accept(&listener);
        read_request(&mut s);
        seen_tx.send(()).unwrap();
        release_rx.recv_timeout(Duration::from_secs(5)).unwrap();
        reply(&mut s, 200, r#"{"data":[{"id":"late","name":"Late"}]}"#);
    });
    let session = Arc::new(FacebookGraphSession::new(&root, "synthetic-token").unwrap());
    let worker_session = Arc::clone(&session);
    let worker = thread::spawn(move || worker_session.friends(SocialFriendDetails::Profiles));
    seen_rx.recv_timeout(Duration::from_secs(5)).unwrap();
    session.close();
    assert!(matches!(
        session.clone().prepare_login(),
        crate::SocialLoginRequest::Ready(Err(SocialPlatformError::NotLoggedIn))
    ));
    release_tx.send(()).unwrap();
    assert_eq!(worker.join().unwrap(), Err(SocialPlatformError::Cancelled));
    server.join().unwrap();
    assert!(!session.is_logged_in());
    assert_eq!(
        session.friends(SocialFriendDetails::Profiles),
        Err(SocialPlatformError::NotLoggedIn)
    );
    assert!(matches!(
        session.user_profile(),
        Err(SocialPlatformError::NotLoggedIn)
    ));
}

#[test]
fn facebook_graph_configuration_errors_do_not_disclose_credentials() {
    for root in [
        "file:///tmp/x",
        "https://user:secret@example.invalid",
        "http://127.0.0.1/root?access_token=secret",
        "http://127.0.0.1/#secret",
    ] {
        let error = FacebookGraphSession::new(root, "synthetic-secret").unwrap_err();
        assert_eq!(error, SocialPlatformError::InvalidConfiguration);
        assert!(!format!("{error:?} {error}").contains("secret"));
    }
    assert!(matches!(
        FacebookGraphSession::new("http://127.0.0.1", ""),
        Err(SocialPlatformError::NotLoggedIn)
    ));
}

#[test]
fn facebook_graph_logout_clears_only_open_session_and_rejects_delayed_cache_publication() {
    for close_first in [false, true] {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let root = format!("http://{}", listener.local_addr().unwrap());
        let server = thread::spawn(move || {
            let mut stream = accept(&listener);
            let request = read_request(&mut stream);
            assert!(request.starts_with("GET /me?"));
            reply(&mut stream, 200, r#"{"id":"own","name":"Own"}"#);
            listener
        });
        let session =
            std::sync::Arc::new(FacebookGraphSession::new(&root, "synthetic-logout").unwrap());
        let profile = session.user_profile().unwrap();
        assert_eq!(profile.access_token, "synthetic-logout");
        if close_first {
            session.close();
        }
        session.logout().unwrap();
        assert!(!session.is_logged_in());
        assert!(matches!(
            session.clone().prepare_user_profile(),
            crate::SocialProfileRequest::Ready(Err(SocialPlatformError::NotLoggedIn))
        ));
        assert!(matches!(
            session.clone().prepare_login(),
            crate::SocialLoginRequest::Ready(Err(SocialPlatformError::NotLoggedIn))
        ));
        assert_eq!(
            session.publish_user_profile(&profile),
            Err(SocialPlatformError::Cancelled)
        );
        session.logout().unwrap();
        let state = session.state.lock().unwrap();
        assert_eq!(state.profile.is_some(), close_first);
        assert_eq!(state.access_token.is_empty(), !close_first);
        let listener = server.join().unwrap();
        assert!(matches!(listener.accept(), Err(e) if e.kind()==std::io::ErrorKind::WouldBlock));
    }
}

#[test]
fn facebook_graph_logout_during_real_profile_http_cannot_restore_token_or_cache() {
    use std::sync::{Arc, mpsc};
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let root = format!("http://{}", listener.local_addr().unwrap());
    let (arrived, seen) = mpsc::channel();
    let (release, resume) = mpsc::channel();
    let server = thread::spawn(move || {
        let mut stream = accept(&listener);
        assert!(read_request(&mut stream).starts_with("GET /me?"));
        arrived.send(()).unwrap();
        resume.recv_timeout(Duration::from_secs(5)).unwrap();
        reply(&mut stream, 200, r#"{"id":"late","name":"Late"}"#);
        listener
    });
    let session = Arc::new(FacebookGraphSession::new(&root, "synthetic-late").unwrap());
    let request = session.clone().prepare_user_profile();
    let worker = thread::spawn(move || request.execute());
    seen.recv_timeout(Duration::from_secs(5)).unwrap();
    session.logout().unwrap();
    release.send(()).unwrap();
    assert!(matches!(
        worker.join().unwrap(),
        Err(SocialPlatformError::Cancelled)
    ));
    let state = session.state.lock().unwrap();
    assert!(state.access_token.is_empty());
    assert!(state.profile.is_none());
    let listener = server.join().unwrap();
    assert!(matches!(listener.accept(), Err(e) if e.kind()==std::io::ErrorKind::WouldBlock));
}
