//! Non-storage identity ownership races: synthetic memory state and loopback HTTP.

use super::super::IdentityEndpoint;
use super::*;
use serde_json::json;
use std::{
    io::{Read, Write},
    net::{TcpListener, TcpStream},
    sync::mpsc,
    thread,
    time::{Duration, Instant},
};

struct Request {
    start: String,
    headers: BTreeMap<String, String>,
    body: String,
}

fn fixture() -> (IdentitySession, IdentityConfig, TcpListener) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let config = IdentityConfig {
        identifiers:
            crate::game_lua::platform_services::skynest_account::identifiers::Identifiers::synthetic().into(),
        endpoint: IdentityEndpoint::parse(&format!(
            "http://{}/ownership/identity/3.0",
            listener.local_addr().unwrap()
        ))
        .unwrap(),
        client_id: "ownership-fixture".to_owned(),
        signing: super::super::ClientSigning::literal(
            "synthetic-client-signature-not-sgs".to_owned(),
            "synthetic-salt".to_owned(),
        ),
    };
    (IdentitySession::default(), config, listener)
}

fn accept(listener: &TcpListener) -> (TcpStream, Request) {
    let deadline = Instant::now() + Duration::from_secs(5);
    let mut stream = loop {
        match listener.accept() {
            Ok((stream, _)) => break stream,
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                assert!(
                    Instant::now() < deadline,
                    "ownership loopback request timed out"
                );
                thread::sleep(Duration::from_millis(1));
            }
            Err(error) => panic!("ownership loopback accept failed: {error}"),
        }
    };
    stream.set_nonblocking(false).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(5)))
        .unwrap();
    let mut bytes = Vec::new();
    let mut chunk = [0; 2048];
    loop {
        let count = stream.read(&mut chunk).unwrap();
        assert_ne!(count, 0, "incomplete ownership HTTP request");
        bytes.extend_from_slice(&chunk[..count]);
        assert!(bytes.len() < 65_536);
        if let Some(end) = bytes.windows(4).position(|part| part == b"\r\n\r\n") {
            let mut lines = std::str::from_utf8(&bytes[..end]).unwrap().lines();
            let start = lines.next().unwrap().to_owned();
            let headers: BTreeMap<_, _> = lines
                .map(|line| {
                    let (key, value) = line.split_once(':').unwrap();
                    (key.to_ascii_lowercase(), value.trim().to_owned())
                })
                .collect();
            let length = headers
                .get("content-length")
                .map(|value| value.parse::<usize>().unwrap())
                .unwrap_or(0);
            if bytes.len() >= end + 4 + length {
                return (
                    stream,
                    Request {
                        start,
                        headers,
                        body: String::from_utf8(bytes[end + 4..end + 4 + length].to_vec()).unwrap(),
                    },
                );
            }
        }
    }
}

fn reply(mut stream: TcpStream, status: u16, body: &str) {
    write!(
        stream,
        "HTTP/1.1 {status} Fixture\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    )
    .unwrap();
}

fn flat(access: &str, refresh: &str, segment: &str) -> AccessResponse {
    AccessResponse {
        access_token: access.to_owned(),
        refresh_token: refresh.to_owned(),
        segment: Some(segment.to_owned()),
        absolute_expiry: i64::MAX,
    }
}

fn profile(account: &str) -> ProfileResponse {
    protocol::parse_profile_value(&json!({
        "publicAccountId":account,
        "personal":{"nickName":format!("Synthetic {account}")},
        "opaque":{"kept":true}
    }))
}

fn session_json(account: &str, access: &str, refresh: &str) -> String {
    json!({
        "userAuth":{"accessToken":access,"refreshToken":refresh,"expiresIn":3600},
        "segments":[6,7],"config":{"ownership":"fixture"},
        "profile":profile(account).raw,
    })
    .to_string()
}

fn assert_no_http(listener: &TcpListener) {
    assert!(
        matches!(listener.accept(), Err(error) if error.kind()==std::io::ErrorKind::WouldBlock)
    );
}

fn install_replacement(session: &IdentitySession) {
    let epoch = session.epoch();
    assert!(
        session
            .install_flat_if_epoch(
                epoch,
                &flat(
                    "replacement-access",
                    "replacement-refresh",
                    "replacement-segment"
                )
            )
            .unwrap()
    );
    assert!(
        session
            .install_profile_if_epoch(epoch, &profile("replacement-account"))
            .unwrap()
    );
    assert_eq!(
        session.epoch(),
        epoch,
        "replacement must not rely on logout epoch"
    );
}

fn assert_replacement_intact(session: &IdentitySession) {
    let tokens = session.level2_tokens();
    assert_eq!(tokens.access_token, "replacement-access");
    assert_eq!(tokens.refresh_token, "replacement-refresh");
    assert_eq!(tokens.segment, "replacement-segment");
    assert_eq!(
        session.profile().unwrap().public_account_id,
        "replacement-account"
    );
    assert_eq!(session.store().load().unwrap(), "replacement-refresh");
    assert_eq!(
        session.store().load_profile().unwrap().unwrap(),
        profile("replacement-account").raw
    );
}

#[test]
fn identity_ownership_retained_tokens_require_the_original_level2_install_permission() {
    let (session, _, _) = fixture();
    let old = session.request_owner(ProviderLevel::Level2);
    let parent = session.request_owner(ProviderLevel::Level1);
    install_replacement(&session);
    for stale_or_parent in [old, parent] {
        assert!(
            session
                .install_flat_for_request_owner(
                    stale_or_parent,
                    &flat("obsolete-access", "obsolete-refresh", "obsolete-segment"),
                )
                .unwrap()
                .is_none()
        );
        assert_replacement_intact(&session);
    }
    let current = session.request_owner(ProviderLevel::Level2);
    let installed = session
        .install_flat_for_request_owner(
            current,
            &flat("accepted-access", "accepted-refresh", "accepted-segment"),
        )
        .unwrap()
        .expect("current Level2 authorization may publish its tokens");
    assert!(!session.request_owner_is_current(current));
    assert!(session.own_profile_owner_is_current(installed));
    assert_eq!(session.level2_tokens().access_token, "accepted-access");
    assert_eq!(session.store().load().unwrap(), "accepted-refresh");
}

#[test]
fn identity_ownership_nonstorage_401_cannot_clear_replacement_or_replay_old_request() {
    for form in [false, true] {
        let (session, config, listener) = fixture();
        session.install_flat(&flat("old-access", "old-refresh", "old-segment"));
        assert!(
            session
                .install_profile_if_epoch(session.epoch(), &profile("old-account"))
                .unwrap()
        );
        let epoch = session.epoch();
        let pending = session.clone();
        let worker = thread::spawn(move || {
            if form {
                pending.execute_form_at_epoch(
                    &config,
                    epoch,
                    ProviderLevel::Level2,
                    "abid/login",
                    "email=old%40example.invalid&password=synthetic",
                )
            } else {
                pending.execute_get_at_epoch(
                    &config,
                    epoch,
                    ProviderLevel::Level2,
                    "fixture/profile",
                )
            }
        });
        let (stream, request) = accept(&listener);
        assert_eq!(request.headers["x-access-token"], "old-access");
        assert_eq!(request.headers["rovio-sgs"], "old-segment");
        if form {
            assert_eq!(
                request.start,
                "POST /ownership/identity/3.0/abid/login HTTP/1.1"
            );
            assert_eq!(
                request.body,
                "email=old%40example.invalid&password=synthetic"
            );
        } else {
            assert!(
                request
                    .start
                    .starts_with("GET /ownership/identity/2.0/fixture/profile ")
            );
            assert!(request.body.is_empty());
        }
        install_replacement(&session);
        assert_eq!(session.epoch(), epoch);
        reply(stream, 401, "{}");
        assert!(worker.join().unwrap().err().unwrap().is_stale());
        assert_replacement_intact(&session);
        assert_no_http(&listener);
    }
}

#[test]
fn identity_ownership_old_initial_or_renewal_acquire_cannot_publish_or_clear_new_identity() {
    for renewal in [false, true] {
        for response_status in [200, 401] {
            let (session, config, listener) = fixture();
            session.store().store("old-refresh").unwrap();
            assert!(
                session
                    .install_profile_if_epoch(session.epoch(), &profile("cached-old-account"))
                    .unwrap()
            );
            if renewal {
                session.install_flat(&flat("old-access", "old-refresh", "old-segment"));
            }
            let epoch = session.epoch();
            let pending = session.clone();
            let worker = thread::spawn(move || {
                if renewal {
                    pending
                        .execute_get_at_epoch(
                            &config,
                            epoch,
                            ProviderLevel::Level2,
                            "fixture/profile",
                        )
                        .map(|_| String::new())
                } else {
                    pending
                        .acquire_session_at_epoch(&config, epoch)
                        .map(|profile| profile.public_account_id)
                }
            });
            if renewal {
                let (stream, request) = accept(&listener);
                assert_eq!(request.headers["x-access-token"], "old-access");
                reply(stream, 401, "{}");
            }
            let (stream, request) = accept(&listener);
            assert_eq!(
                request.start,
                "POST /ownership/session/1/apps/ownership-fixture/sessions HTTP/1.1"
            );
            assert_eq!(
                serde_json::from_str::<serde_json::Value>(&request.body).unwrap()["refresh"],
                json!({"token":"old-refresh"})
            );
            install_replacement(&session);
            assert_eq!(session.epoch(), epoch);
            reply(
                stream,
                response_status,
                &session_json("obsolete-account", "obsolete-access", "obsolete-refresh"),
            );
            assert!(worker.join().unwrap().err().unwrap().is_stale());
            assert_replacement_intact(&session);
            assert!(
                session.config().is_none(),
                "obsolete response cannot install config"
            );
            assert_no_http(&listener);
        }
    }
}

#[test]
fn identity_ownership_initial_auto_acquire_may_replace_cached_public_id_and_succeed() {
    let (session, config, listener) = fixture();
    let store = Arc::new(MemoryRefreshStore::default());
    store.store("cached-refresh").unwrap();
    store
        .store_profile(Some(&profile("cached-account").raw))
        .unwrap();
    session.bind_store(store.clone()).unwrap();
    let epoch = session.epoch();
    let pending = session.clone();
    let worker = thread::spawn(move || pending.acquire_session_at_epoch(&config, epoch));
    let (stream, request) = accept(&listener);
    assert_eq!(
        request.start,
        "POST /ownership/session/1/apps/ownership-fixture/sessions HTTP/1.1"
    );
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&request.body).unwrap()["refresh"],
        json!({"token":"cached-refresh"})
    );
    reply(
        stream,
        200,
        &session_json("resolved-account", "resolved-access", "resolved-refresh"),
    );
    assert_eq!(
        worker.join().unwrap().unwrap().public_account_id,
        "resolved-account"
    );
    assert_eq!(session.epoch(), epoch);
    assert_eq!(
        session.profile().unwrap().public_account_id,
        "resolved-account"
    );
    assert_eq!(session.level2_tokens().access_token, "resolved-access");
    assert_eq!(store.load().unwrap(), "resolved-refresh");
    assert_eq!(
        store.load_profile().unwrap().unwrap(),
        profile("resolved-account").raw
    );
    assert_no_http(&listener);
}

#[test]
fn identity_ownership_level1_renewal_remains_independent_of_level2_replacement() {
    let (session, config, listener) = fixture();
    session.install_flat(&flat(
        "old-account-access",
        "old-account-refresh",
        "old-account-segment",
    ));
    session.install_level1_flat(&flat("parent-access", "parent-refresh", "parent-segment"));
    let epoch = session.epoch();
    let pending = session.clone();
    let worker = thread::spawn(move || {
        pending.execute_form_at_epoch(
            &config,
            epoch,
            ProviderLevel::Level1,
            "abid/validate/email",
            "email=fixture%40example.invalid",
        )
    });
    let (stream, first) = accept(&listener);
    assert_eq!(first.headers["x-access-token"], "parent-access");
    install_replacement(&session);
    reply(stream, 401, "{}");
    let (stream, access) = accept(&listener);
    assert_eq!(access.start, "POST /ownership/identity/2.0/access HTTP/1.1");
    assert!(!access.headers.contains_key("x-access-token"));
    reply(stream, 200, &json!({"accessToken":"renewed-parent-access",
        "refreshToken":"renewed-parent-refresh","segment":"renewed-parent-segment","expiresIn":3600}).to_string());
    let (stream, replay) = accept(&listener);
    assert_eq!(first.start, replay.start);
    assert_eq!(first.body, replay.body);
    assert_eq!(
        first.headers["content-type"],
        replay.headers["content-type"]
    );
    assert_eq!(replay.headers["x-access-token"], "renewed-parent-access");
    assert_eq!(replay.headers["rovio-sgs"], "renewed-parent-segment");
    reply(stream, 200, "{}");
    assert_eq!(worker.join().unwrap().unwrap().status(), 200);
    assert_replacement_intact(&session);
    assert_no_http(&listener);
}

#[test]
fn identity_ownership_precaptured_request_waiting_for_renewal_cannot_use_replacement_identity() {
    let (session, config, listener) = fixture();
    session.store().store("old-queued-refresh").unwrap();
    let mut owner = session.request_owner(ProviderLevel::Level2);
    let epoch = owner.epoch();
    let renewal = session.level2_renewal.lock().unwrap();
    let pending = session.clone();
    let (started_tx, started_rx) = mpsc::channel();
    let (finished_tx, finished_rx) = mpsc::channel();
    let worker = thread::spawn(move || {
        started_tx.send(()).unwrap();
        let result = pending.execute_get_for_owner(
            &config,
            &mut owner,
            ProviderLevel::Level2,
            "fixture/queued",
        );
        finished_tx
            .send(result.map(|response| response.status()))
            .unwrap();
    });
    started_rx.recv_timeout(Duration::from_secs(5)).unwrap();
    // Keep the renewal lock held across replacement. This short bounded wait
    // also checks that the old request cannot finish while acquisition is gated.
    assert!(matches!(
        finished_rx.recv_timeout(Duration::from_millis(50)),
        Err(mpsc::RecvTimeoutError::Timeout)
    ));
    assert_no_http(&listener);
    install_replacement(&session);
    assert_eq!(session.epoch(), epoch);
    drop(renewal);
    let error = finished_rx
        .recv_timeout(Duration::from_secs(5))
        .unwrap()
        .unwrap_err();
    assert!(error.is_stale());
    worker.join().unwrap();
    assert_replacement_intact(&session);
    assert_no_http(&listener);
}

#[test]
fn identity_ownership_initial_protected_get_adopts_only_its_own_resolved_account() {
    let (session, config, listener) = fixture();
    let store = Arc::new(MemoryRefreshStore::default());
    store.store("cached-refresh").unwrap();
    store
        .store_profile(Some(&profile("cached-account").raw))
        .unwrap();
    session.bind_store(store.clone()).unwrap();
    let original = session.request_owner(ProviderLevel::Level2);
    let mut owner = original;
    let pending = session.clone();
    let worker = thread::spawn(move || {
        let result = pending.execute_get_for_owner(
            &config,
            &mut owner,
            ProviderLevel::Level2,
            "fixture/resolved",
        );
        (result.map(|response| response.status()), owner)
    });
    let (stream, acquire) = accept(&listener);
    assert_eq!(
        acquire.start,
        "POST /ownership/session/1/apps/ownership-fixture/sessions HTTP/1.1"
    );
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&acquire.body).unwrap()["refresh"],
        json!({"token":"cached-refresh"})
    );
    reply(
        stream,
        200,
        &session_json("resolved-account", "resolved-access", "resolved-refresh"),
    );
    let (stream, protected) = accept(&listener);
    assert_eq!(
        protected.start,
        "GET /ownership/identity/2.0/fixture/resolved HTTP/1.1"
    );
    assert_eq!(protected.headers["x-access-token"], "resolved-access");
    assert_eq!(protected.headers["rovio-sgs"], "6, 7");
    reply(stream, 200, "{}");
    let (result, adopted) = worker.join().unwrap();
    assert_eq!(result.unwrap(), 200);
    assert_eq!(adopted.epoch(), original.epoch());
    assert!(session.request_owner_is_current(adopted));
    assert!(!session.request_owner_is_current(original));
    assert_eq!(
        session.profile().unwrap().public_account_id,
        "resolved-account"
    );
    assert_eq!(store.load().unwrap(), "resolved-refresh");
    assert_no_http(&listener);
}

#[test]
fn identity_ownership_401_renewal_may_publish_different_account_but_cannot_adopt_or_replay() {
    let (session, config, listener) = fixture();
    session.install_flat(&flat(
        "original-access",
        "original-refresh",
        "original-segment",
    ));
    assert!(
        session
            .install_profile_if_epoch(session.epoch(), &profile("original-account"))
            .unwrap()
    );
    let mut owner = session.request_owner(ProviderLevel::Level2);
    let original_epoch = owner.epoch();
    let pending = session.clone();
    let worker = thread::spawn(move || {
        let result = pending.execute_get_for_owner(
            &config,
            &mut owner,
            ProviderLevel::Level2,
            "fixture/original-account",
        );
        (result.map(|response| response.status()), owner)
    });
    let (stream, protected) = accept(&listener);
    assert_eq!(protected.headers["x-access-token"], "original-access");
    reply(stream, 401, "{}");
    let (stream, acquire) = accept(&listener);
    assert_eq!(
        acquire.start,
        "POST /ownership/session/1/apps/ownership-fixture/sessions HTTP/1.1"
    );
    reply(
        stream,
        200,
        &session_json("resolved-other-account", "other-access", "other-refresh"),
    );
    let (result, old_owner) = worker.join().unwrap();
    assert!(result.unwrap_err().is_stale());
    assert_eq!(old_owner.epoch(), original_epoch);
    assert!(!session.request_owner_is_current(old_owner));
    assert_eq!(
        session.profile().unwrap().public_account_id,
        "resolved-other-account"
    );
    assert_eq!(session.level2_tokens().access_token, "other-access");
    assert_eq!(session.store().load().unwrap(), "other-refresh");
    assert_eq!(
        session.store().load_profile().unwrap().unwrap(),
        profile("resolved-other-account").raw
    );
    assert_no_http(&listener);
}
