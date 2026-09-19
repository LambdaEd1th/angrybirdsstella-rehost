//! All transport proofs bind 127.0.0.1 and use synthetic credentials only.

mod events;
mod regeneration;
mod signing;

use super::super::{IdentityEndpoint, PersonalProfile};
use super::*;
use serde_json::{Value, json};
use std::{
    io::{Read, Write},
    net::{TcpListener, TcpStream},
    sync::{
        Barrier,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    thread,
    time::{Duration, Instant},
};

struct Request {
    start: String,
    headers: BTreeMap<String, String>,
    body: String,
}

fn bind() -> (IdentityConfig, TcpListener) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    (
        IdentityConfig {
            identifiers: crate::game_lua::platform_services::skynest_account::identifiers::Identifiers::synthetic().into(),
            endpoint: IdentityEndpoint::parse(&format!(
                "http://{}/proxy/identity/3.0",
                listener.local_addr().unwrap()
            ))
            .unwrap(),
            client_id: "synthetic-client".to_owned(),
            signing: super::super::ClientSigning::literal(
                "fixture-client-signature-never-sgs".to_owned(),
                "fixture-client-salt".to_owned(),
            ),
        },
        listener,
    )
}

fn accept(listener: &TcpListener) -> (TcpStream, Request) {
    let deadline = Instant::now() + Duration::from_secs(5);
    let mut stream = loop {
        match listener.accept() {
            Ok((stream, _)) => break stream,
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                assert!(Instant::now() < deadline, "loopback request timed out");
                thread::sleep(Duration::from_millis(1));
            }
            Err(error) => panic!("loopback accept failed: {error}"),
        }
    };
    stream.set_nonblocking(false).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(5)))
        .unwrap();
    let mut bytes = Vec::new();
    let mut chunk = [0; 2048];
    loop {
        let read = stream.read(&mut chunk).unwrap();
        assert_ne!(read, 0, "incomplete request");
        bytes.extend_from_slice(&chunk[..read]);
        assert!(bytes.len() < 65_536, "unexpectedly large request");
        if let Some(end) = bytes.windows(4).position(|part| part == b"\r\n\r\n") {
            let text = std::str::from_utf8(&bytes[..end]).unwrap();
            let mut lines = text.lines();
            let start = lines.next().unwrap().to_owned();
            let headers: BTreeMap<_, _> = lines
                .map(|line| {
                    let (key, value) = line.split_once(':').unwrap();
                    (key.to_ascii_lowercase(), value.trim().to_owned())
                })
                .collect();
            let len = headers
                .get("content-length")
                .map(|value| value.parse::<usize>().unwrap())
                .unwrap_or(0);
            if bytes.len() >= end + 4 + len {
                return (
                    stream,
                    Request {
                        start,
                        headers,
                        body: String::from_utf8(bytes[end + 4..end + 4 + len].to_vec()).unwrap(),
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

fn server(responses: Vec<(u16, String)>) -> (IdentityConfig, thread::JoinHandle<Vec<Request>>) {
    let (config, listener) = bind();
    let task = thread::spawn(move || {
        responses
            .into_iter()
            .map(|(status, body)| {
                let (stream, request) = accept(&listener);
                reply(stream, status, &body);
                request
            })
            .collect()
    });
    (config, task)
}

fn profile() -> ProfileResponse {
    ProfileResponse {
        avatar_assets: Vec::new(),
        avatar_paths: BTreeMap::new(),
        public_account_id: "fixture-account".to_owned(),
        personal: PersonalProfile {
            nickname: "Synthetic".to_owned(),
            email: "fixture@example.invalid".to_owned(),
        },
        social_networks: Vec::new(),
        active_external_id: String::new(),
        active_social_network: None,
        active_social_name: String::new(),
        connected_to_social_network: false,
        raw: json!({"publicAccountId":"fixture-account","personal":{"nickName":"Synthetic","email":"fixture@example.invalid"}}),
    }
}

#[test]
fn identity_storage_lifetime_replacement_during_401_preserves_new_account_tokens() {
    let (config, listener) = bind();
    let session = IdentitySession::default();
    session.install_flat(&flat("old-access", "old-refresh", "old-segment"));
    let (epoch, generation) = session.storage_lifetime();
    let pending = session.clone();
    let url = format!(
        "http://{}/storage/1.0/state",
        listener.local_addr().unwrap()
    );
    let worker = thread::spawn(move || {
        pending.execute_storage(
            &config,
            epoch,
            generation,
            &PreparedRequest {
                url: &url,
                body: None,
                timeout: Duration::from_secs(5),
                still_current: None,
            },
        )
    });
    let (stream, request) = accept(&listener);
    assert_eq!(request.headers["x-access-token"], "old-access");
    session.install_flat(&flat("new-access", "new-refresh", "new-segment"));
    assert_eq!(session.epoch(), epoch, "UI completion epoch remains usable");
    assert!(!session.storage_lifetime_is_current(epoch, generation));
    reply(stream, 401, "{}");
    assert!(worker.join().unwrap().err().unwrap().is_stale());
    assert_eq!(session.level2_tokens().access_token, "new-access");
    assert!(
        matches!(listener.accept(), Err(error) if error.kind() == std::io::ErrorKind::WouldBlock)
    );
}

#[test]
fn identity_storage_lifetime_replacement_during_acquire_cannot_publish_old_tokens() {
    let (config, listener) = bind();
    let session = IdentitySession::default();
    let (epoch, generation) = session.storage_lifetime();
    let pending = session.clone();
    let url = format!(
        "http://{}/storage/1.0/state",
        listener.local_addr().unwrap()
    );
    let worker = thread::spawn(move || {
        pending.execute_storage(
            &config,
            epoch,
            generation,
            &PreparedRequest {
                url: &url,
                body: None,
                timeout: Duration::from_secs(5),
                still_current: None,
            },
        )
    });
    let (stream, request) = accept(&listener);
    assert!(request.start.contains("/session/1/apps/"));
    session.install_flat(&flat("new-access", "new-refresh", "new-segment"));
    reply(
        stream,
        200,
        &session_json("obsolete-access", "obsolete-refresh", json!([1])),
    );
    assert!(worker.join().unwrap().err().unwrap().is_stale());
    assert_eq!(session.level2_tokens().access_token, "new-access");
    assert_eq!(session.store().load().unwrap(), "new-refresh");
    assert!(session.profile().is_none());
    assert!(
        matches!(listener.accept(), Err(error) if error.kind() == std::io::ErrorKind::WouldBlock)
    );
}

#[test]
fn identity_storage_lifetime_different_account_after_renewal_cancels_replay() {
    let mut renewed: Value =
        serde_json::from_str(&session_json("renewed", "renewed-r", json!([2]))).unwrap();
    renewed["profile"]["publicAccountId"] = json!("different-account");
    let (config, server) = server(vec![(401, "{}".into()), (200, renewed.to_string())]);
    let session = IdentitySession::default();
    session.install_flat(&flat("old-access", "old-refresh", "old-segment"));
    session
        .install_profile_if_epoch(session.epoch(), &profile())
        .unwrap();
    let (epoch, generation) = session.storage_lifetime();
    let result = session.execute_storage(
        &config,
        epoch,
        generation,
        &PreparedRequest {
            url: &config.endpoint.request_url("synthetic-storage"),
            body: Some(("application/json", br#"{"oldAccountWrite":true}"#)),
            timeout: Duration::from_secs(5),
            still_current: None,
        },
    );
    assert!(result.err().unwrap().is_stale());
    assert_eq!(server.join().unwrap().len(), 2);
    assert_eq!(
        session.profile().unwrap().public_account_id,
        "different-account"
    );
    assert_eq!(session.level2_tokens().access_token, "renewed");
}

#[test]
fn identity_storage_lifetime_same_account_renewal_keeps_frozen_request_and_owner() {
    let (config, server) = server(vec![
        (401, "{}".into()),
        (200, session_json("renewed", "renewed-r", json!([2, 3]))),
        (200, "[]".into()),
    ]);
    let session = IdentitySession::default();
    session.install_flat(&flat("old-access", "old-refresh", "old-segment"));
    session
        .install_profile_if_epoch(session.epoch(), &profile())
        .unwrap();
    let (epoch, generation) = session.storage_lifetime();
    let response = session
        .execute_storage(
            &config,
            epoch,
            generation,
            &PreparedRequest {
                url: &config.endpoint.request_url("synthetic-storage"),
                body: Some(("application/json", br#"{"retained":[1,2,3]}"#)),
                timeout: Duration::from_secs(5),
                still_current: None,
            },
        )
        .unwrap();
    assert_eq!(response.status(), 200);
    assert!(session.storage_lifetime_is_current(epoch, generation));
    let requests = server.join().unwrap();
    assert_eq!(requests.len(), 3);
    assert_eq!(requests[0].start, requests[2].start);
    assert_eq!(requests[0].body, requests[2].body);
    assert_eq!(requests[2].headers["content-type"], "application/json");
    assert_eq!(requests[2].headers["x-access-token"], "renewed");
    assert_eq!(requests[2].headers["rovio-sgs"], "2, 3");
}

#[test]
fn identity_storage_lifetime_service_change_cancels_401_and_renewal_replay() {
    for cancel_during_renewal in [false, true] {
        let (config, listener) = bind();
        let session = IdentitySession::default();
        session.install_flat(&flat("old-access", "old-refresh", "old-segment"));
        let (epoch, generation) = session.storage_lifetime();
        let current = Arc::new(AtomicBool::new(true));
        let pending = session.clone();
        let pending_current = current.clone();
        let url = format!(
            "http://{}/storage/1.0/state",
            listener.local_addr().unwrap()
        );
        let worker = thread::spawn(move || {
            pending.execute_storage(
                &config,
                epoch,
                generation,
                &PreparedRequest {
                    url: &url,
                    body: Some(("application/json", br#"{"oldServiceWrite":true}"#)),
                    timeout: Duration::from_secs(5),
                    still_current: Some(&|| pending_current.load(Ordering::SeqCst)),
                },
            )
        });
        let (stream, request) = accept(&listener);
        assert_eq!(request.headers["x-access-token"], "old-access");
        if cancel_during_renewal {
            reply(stream, 401, "{}");
            let (renewal, request) = accept(&listener);
            assert!(request.start.contains("/session/1/apps/"));
            current.store(false, Ordering::SeqCst);
            reply(
                renewal,
                200,
                &session_json("renewed", "renewed-r", json!([2])),
            );
        } else {
            current.store(false, Ordering::SeqCst);
            reply(stream, 401, "{}");
        }
        assert!(worker.join().unwrap().err().unwrap().is_stale());
        assert!(session.storage_lifetime_is_current(epoch, generation));
        assert_eq!(
            session.level2_tokens().access_token,
            if cancel_during_renewal {
                "renewed"
            } else {
                "old-access"
            },
            "a storage URL change must not erase the identity session"
        );
        assert!(
            matches!(listener.accept(), Err(error) if error.kind() == std::io::ErrorKind::WouldBlock)
        );
    }
}

#[test]
fn identity_session_memory_store_bind_restores_original_profile_json_without_live_tokens() {
    let store = Arc::new(MemoryRefreshStore::default());
    let first = IdentitySession::with_refresh_store(store.clone());
    let mut saved = profile();
    saved.raw["uninterpreted"] = json!({"nested":[null, 7, {"opaque":"retained"}]});
    assert!(
        first
            .install_flat_if_epoch(first.epoch(), &flat("live-a", "saved-r", "live-s"))
            .unwrap()
    );
    assert!(
        first
            .install_profile_if_epoch(first.epoch(), &saved)
            .unwrap()
    );
    assert_eq!(store.load_profile().unwrap(), Some(saved.raw.clone()));

    let reconstructed = IdentitySession::default();
    let before = reconstructed.epoch();
    reconstructed.bind_store(store.clone()).unwrap();
    assert_eq!(reconstructed.epoch(), before + 1);
    let loaded = reconstructed.profile().unwrap();
    assert_eq!(loaded.raw, saved.raw);
    assert_eq!(loaded.public_account_id, "fixture-account");
    assert_eq!(loaded.personal.email, "fixture@example.invalid");
    assert!(reconstructed.level2_tokens().access_token.is_empty());
    assert!(reconstructed.level2_tokens().refresh_token.is_empty());
    assert!(reconstructed.level2_tokens().segment.is_empty());
    assert_eq!(store.load().unwrap(), "saved-r");
}

#[test]
fn identity_session_detach_preserves_previous_store_but_explicit_logout_clears_both_values() {
    let store = Arc::new(MemoryRefreshStore::default());
    let session = IdentitySession::with_refresh_store(store.clone());
    let saved = profile();
    assert!(
        session
            .install_flat_if_epoch(session.epoch(), &flat("a", "r", "s"))
            .unwrap()
    );
    assert!(
        session
            .install_profile_if_epoch(session.epoch(), &saved)
            .unwrap()
    );
    let before = session.epoch();
    session.detach_store();
    assert_eq!(session.epoch(), before + 1);
    assert!(session.profile().is_none());
    assert!(session.level2_tokens().access_token.is_empty());
    assert_eq!(store.load().unwrap(), "r");
    assert_eq!(store.load_profile().unwrap(), Some(saved.raw.clone()));
    // Logout of the detached host store must not erase the old endpoint's
    // credentials. Only an explicit logout after rebinding clears that store.
    session.logout().unwrap();
    assert_eq!(store.load().unwrap(), "r");
    assert_eq!(store.load_profile().unwrap(), Some(saved.raw));
    session.bind_store(store.clone()).unwrap();
    assert!(session.profile().is_some());
    session.logout().unwrap();
    assert_eq!(store.load().unwrap(), "");
    assert_eq!(store.load_profile().unwrap(), None);
    assert!(session.profile().is_none());
}

#[derive(Default)]
struct FailingLogoutStore {
    inner: MemoryRefreshStore,
    fail_refresh: AtomicBool,
    fail_profile: AtomicBool,
    refresh_writes: Mutex<Vec<String>>,
    profile_writes: Mutex<Vec<Option<Value>>>,
    write_order: Mutex<Vec<&'static str>>,
}

impl RefreshStore for FailingLogoutStore {
    fn load(&self) -> Result<String, StoreError> {
        self.inner.load()
    }
    fn load_profile(&self) -> Result<Option<Value>, StoreError> {
        self.inner.load_profile()
    }
    fn store(&self, value: &str) -> Result<(), StoreError> {
        self.write_order.lock().unwrap().push("refresh");
        self.refresh_writes.lock().unwrap().push(value.to_owned());
        if self.fail_refresh.load(Ordering::Relaxed) {
            Err(StoreError::Io)
        } else {
            self.inner.store(value)
        }
    }
    fn store_profile(&self, value: Option<&Value>) -> Result<(), StoreError> {
        self.write_order.lock().unwrap().push("profile");
        self.profile_writes.lock().unwrap().push(value.cloned());
        if self.fail_profile.load(Ordering::Relaxed) {
            Err(StoreError::Io)
        } else {
            self.inner.store_profile(value)
        }
    }
}

#[test]
fn identity_session_logout_storage_failure_still_invalidates_epoch_and_attempts_both_clears() {
    for (fail_refresh, fail_profile) in [(true, false), (false, true), (true, true)] {
        let store = Arc::new(FailingLogoutStore::default());
        let session = IdentitySession::with_refresh_store(store.clone());
        let epoch = session.epoch();
        session.install_level1_flat(&flat("parent-a", "parent-r", "parent-s"));
        assert!(
            session
                .install_flat_if_epoch(epoch, &flat("a", "r", "s"))
                .unwrap()
        );
        assert!(session.install_profile_if_epoch(epoch, &profile()).unwrap());
        store.refresh_writes.lock().unwrap().clear();
        store.profile_writes.lock().unwrap().clear();
        store.fail_refresh.store(fail_refresh, Ordering::Relaxed);
        store.fail_profile.store(fail_profile, Ordering::Relaxed);

        assert_eq!(session.logout(), Err(StoreError::Io));
        assert_eq!(session.epoch(), epoch + 1);
        assert!(session.profile().is_none());
        let current = session.level2_tokens();
        assert!(current.access_token.is_empty());
        assert!(current.refresh_token.is_empty());
        assert!(current.segment.is_empty());
        assert_eq!(current.absolute_expiry, 0);
        assert!(session.state.lock().unwrap().level1.access_token.is_empty());
        assert_eq!(*store.refresh_writes.lock().unwrap(), [String::new()]);
        assert_eq!(*store.profile_writes.lock().unwrap(), [None]);
        // Failure cannot let a worker holding the retired epoch publish again,
        // nor even retry a write through its stale token/profile completion.
        assert!(
            !session
                .install_flat_if_epoch(epoch, &flat("late", "late", "late"))
                .unwrap()
        );
        assert!(!session.install_profile_if_epoch(epoch, &profile()).unwrap());
        assert_eq!(store.refresh_writes.lock().unwrap().len(), 1);
        assert_eq!(store.profile_writes.lock().unwrap().len(), 1);
    }
}

#[test]
fn identity_session_storage_failure_recovery_reacquires_instead_of_accepting_cached_profile() {
    for fail_refresh in [true, false] {
        for has_cached_profile in [true, false] {
            let store = Arc::new(FailingLogoutStore::default());
            store.store("prior-refresh").unwrap();
            let session = IdentitySession::with_refresh_store(store.clone());
            let epoch = session.epoch();
            let old_profile = protocol::parse_profile_value(&json!({
                "publicAccountId":"cached-account",
                "personal":{"email":"cached@example.invalid"}
            }));
            if has_cached_profile {
                assert!(
                    session
                        .install_profile_if_epoch(epoch, &old_profile)
                        .unwrap()
                );
            }
            store.fail_refresh.store(fail_refresh, Ordering::Relaxed);
            store.fail_profile.store(!fail_refresh, Ordering::Relaxed);
            let mut first: Value =
                serde_json::from_str(&session_json("first-a", "first-r", json!([1]))).unwrap();
            first["profile"]["publicAccountId"] = json!("uncommitted-account");
            let mut recovered: Value =
                serde_json::from_str(&session_json("retry-a", "retry-r", json!([2]))).unwrap();
            recovered["profile"]["publicAccountId"] = json!("recovered-account");
            let (config, server) =
                server(vec![(200, first.to_string()), (200, recovered.to_string())]);

            assert_eq!(session.acquire_session(&config).err().unwrap().status, -1);
            assert_eq!(session.epoch(), epoch);
            assert!(session.level2_tokens().access_token.is_empty());
            assert_eq!(
                session.profile().map(|profile| profile.public_account_id),
                has_cached_profile.then(|| "cached-account".to_owned())
            );
            // Either persisted field may have failed. A surviving cached
            // profile must not turn the next login into a false success, and
            // an absent profile must not leave access permanently short-circuited.
            store.fail_refresh.store(false, Ordering::Relaxed);
            store.fail_profile.store(false, Ordering::Relaxed);
            let actual = session.acquire_session(&config).unwrap();
            assert_eq!(actual.public_account_id, "recovered-account");
            assert_eq!(session.level2_tokens().access_token, "retry-a");
            assert_eq!(store.load().unwrap(), "retry-r");
            assert_eq!(
                store.load_profile().unwrap(),
                Some(recovered["profile"].clone())
            );

            let requests = server.join().unwrap();
            assert_eq!(
                requests.len(),
                2,
                "recovery must perform a second HTTP acquire"
            );
            assert_eq!(requests[0].start, requests[1].start);
            let initial_body: Value = serde_json::from_str(&requests[0].body).unwrap();
            let retry_body: Value = serde_json::from_str(&requests[1].body).unwrap();
            assert_eq!(initial_body["refresh"]["token"], "prior-refresh");
            assert_eq!(
                retry_body["refresh"]["token"],
                if fail_refresh {
                    "prior-refresh"
                } else {
                    "first-r"
                }
            );
        }
    }
}

#[test]
fn identity_session_storage_failure_in_flat_install_invalidates_access_until_retry() {
    let store = Arc::new(FailingLogoutStore::default());
    let session = IdentitySession::with_refresh_store(store.clone());
    let epoch = session.epoch();
    assert!(
        session
            .install_flat_if_epoch(epoch, &flat("old-a", "old-r", "old-s"))
            .unwrap()
    );
    assert!(session.install_profile_if_epoch(epoch, &profile()).unwrap());
    store.fail_refresh.store(true, Ordering::Relaxed);

    assert_eq!(
        session.install_flat_if_epoch(epoch, &flat("new-a", "new-r", "new-s")),
        Err(StoreError::Io)
    );
    assert_eq!(session.epoch(), epoch);
    assert!(session.level2_tokens().access_token.is_empty());
    assert_eq!(store.load().unwrap(), "old-r");
    assert_eq!(
        session.profile().unwrap().public_account_id,
        "fixture-account"
    );

    store.fail_refresh.store(false, Ordering::Relaxed);
    assert!(
        session
            .install_flat_if_epoch(epoch, &flat("retry-a", "retry-r", "retry-s"))
            .unwrap()
    );
    assert_eq!(session.level2_tokens().access_token, "retry-a");
    assert_eq!(store.load().unwrap(), "retry-r");
}

#[test]
fn identity_session_storage_failure_in_profile_install_invalidates_access_until_retry() {
    let store = Arc::new(FailingLogoutStore::default());
    let session = IdentitySession::with_refresh_store(store.clone());
    let epoch = session.epoch();
    assert!(
        session
            .install_flat_if_epoch(epoch, &flat("a", "r", "s"))
            .unwrap()
    );
    let old = profile();
    assert!(session.install_profile_if_epoch(epoch, &old).unwrap());
    let replacement = protocol::parse_profile_value(&json!({"publicAccountId":"replacement"}));
    store.fail_profile.store(true, Ordering::Relaxed);

    assert_eq!(
        session.install_profile_if_epoch(epoch, &replacement),
        Err(StoreError::Io)
    );
    assert_eq!(session.epoch(), epoch);
    assert!(session.level2_tokens().access_token.is_empty());
    assert_eq!(
        session.profile().unwrap().public_account_id,
        old.public_account_id
    );
    assert_eq!(store.load_profile().unwrap(), Some(old.raw));

    store.fail_profile.store(false, Ordering::Relaxed);
    assert!(
        session
            .install_flat_if_epoch(epoch, &flat("retry-a", "retry-r", "retry-s"))
            .unwrap()
    );
    assert!(
        session
            .install_profile_if_epoch(epoch, &replacement)
            .unwrap()
    );
    assert_eq!(session.level2_tokens().access_token, "retry-a");
    assert_eq!(session.profile().unwrap().public_account_id, "replacement");
    assert_eq!(store.load_profile().unwrap(), Some(replacement.raw));
}

fn flat(access: &str, refresh: &str, segment: &str) -> AccessResponse {
    AccessResponse {
        access_token: access.to_owned(),
        refresh_token: refresh.to_owned(),
        absolute_expiry: i64::MAX,
        segment: Some(segment.to_owned()),
    }
}

fn flat_json(access: &str, segment: &str) -> String {
    json!({"accessToken":access,"refreshToken":"level1-refresh","segment":segment,"expiresIn":3600})
        .to_string()
}

fn session_json(access: &str, refresh: &str, segments: Value) -> String {
    json!({
        "userAuth":{"accessToken":access,"refreshToken":refresh,"expiresIn":3600},
        "segments":segments,
        "profile":{"publicAccountId":"fixture-account","personal":{"nickName":"Synthetic","email":"fixture@example.invalid"}},
        "config":{"enabled":true,"zero":0,"negative":-1,"fraction":1.9,"name":"fixture"}
    }).to_string()
}

#[test]
fn identity_session_endpoint_keeps_prefix_and_app_id_one_component() {
    let endpoint = IdentityEndpoint::parse("https://fixture.invalid/proxy/identity/2.0").unwrap();
    assert_eq!(
        endpoint.session_url("Purple"),
        "https://fixture.invalid/proxy/session/1/apps/Purple/sessions"
    );
    assert_eq!(
        endpoint.session_url("../A/B ?#"),
        "https://fixture.invalid/proxy/session/1/apps/%2E%2E%2FA%2FB%20%3F%23/sessions"
    );
}

#[test]
fn identity_session_nested_and_flat_numbers_preserve_distinct_expiry_contracts() {
    let mut nested: Value =
        serde_json::from_str(&session_json("a", "r", json!([1, -2, 3.9, 1e100, -1e100]))).unwrap();
    let mut flat: Value = serde_json::from_str(&flat_json("a", "s")).unwrap();
    for (seconds, expected_nested, expected_flat) in [
        (json!(0), 1000, 0),
        (json!(-2), 998, 0),
        (json!(3.9), 1003, 1003),
        (json!(4294967298i64), 1002, 1002),
        (json!(1e100), 999, 0),
        (json!(-1e100), 1000, 0),
    ] {
        nested["userAuth"]["expiresIn"] = seconds.clone();
        flat["expiresIn"] = seconds;
        let parsed = protocol::session(&nested, 1000).unwrap();
        assert_eq!(parsed.tokens.absolute_expiry, expected_nested);
        assert_eq!(
            protocol::flat_tokens(&flat, 1000).unwrap().absolute_expiry,
            expected_flat
        );
        assert_eq!(
            parsed.tokens.segment,
            format!("1, -2, 3, {}, {}", i64::MAX, i64::MIN)
        );
        assert_eq!(parsed.config["negative"], u64::MAX.to_string());
        assert_eq!(parsed.config["fraction"], "1");
        assert_eq!(parsed.config["enabled"], "true");
    }
    for invalid in [
        json!("1"),
        json!(true),
        Value::Null,
        json!(9223372036854775808u64),
    ] {
        nested["segments"] = json!([invalid]);
        assert!(protocol::session(&nested, 1000).is_err());
    }
}

#[test]
fn identity_session_profile_defaults_and_personal_overrides_abid_by_native_type_rules() {
    for value in [
        Value::Null,
        json!(false),
        json!(123),
        json!("profile"),
        json!([]),
        json!({}),
    ] {
        let profile = protocol::parse_profile_value(&value);
        assert!(profile.public_account_id.is_empty());
        assert!(profile.personal.email.is_empty());
        assert!(profile.personal.nickname.is_empty());
        assert!(profile.social_networks.is_empty());
    }
    let profile = protocol::parse_profile_value(&json!({
        "publicAccountId":false,
        "abid":{"nickName":"from-abid","email":"old@example.invalid"},
        "personal":{"nickName":false,"email":null},
        "socialNetworks":[null,{}, {"provider":"only-provider"}, {"provider":1,"id":"x"}, {"provider":"","id":""}, {"provider":"fixture","id":"two"}]
    }));
    assert!(profile.public_account_id.is_empty());
    assert_eq!(profile.personal.nickname, "false");
    assert!(profile.personal.email.is_empty());
    assert_eq!(profile.social_networks.len(), 2);
    assert_eq!(profile.social_networks[0]["id"], "");
    assert_eq!(profile.social_networks[1]["id"], "two");
    let mut body: Value = serde_json::from_str(&session_json("a", "r", json!([]))).unwrap();
    body.as_object_mut().unwrap().remove("profile");
    assert!(
        protocol::session(&body, 0)
            .unwrap()
            .profile
            .public_account_id
            .is_empty()
    );
}

#[test]
fn identity_session_flat_requires_both_tokens_but_ignores_wrong_typed_optional_segment() {
    let mut flat: Value = serde_json::from_str(&flat_json("a", "s")).unwrap();
    for value in [Value::Null, json!(true), json!(123), json!([]), json!({})] {
        flat["segment"] = value;
        assert!(protocol::flat_tokens(&flat, 0).unwrap().segment.is_empty());
    }
    for key in ["accessToken", "refreshToken"] {
        let mut missing = flat.clone();
        missing[key] = json!("");
        assert!(protocol::flat_tokens(&missing, 0).is_err());
    }
    let nested: Value = serde_json::from_str(&session_json("", "", json!([]))).unwrap();
    assert!(
        protocol::session(&nested, 0).is_ok(),
        "nested constructor has no flat nonempty test"
    );
}

#[test]
fn identity_session_own_profile_requires_200_but_accepts_default_empty_profile() {
    let (config, server) = server(vec![(201, "{}".to_owned()), (200, "null".to_owned())]);
    let first = agent()
        .get(config.endpoint.request_url("profile/own"))
        .call()
        .unwrap();
    assert_eq!(parse_profile_response(first).err().unwrap().status, 201);
    let second = agent()
        .get(config.endpoint.request_url("profile/own"))
        .call()
        .unwrap();
    let profile = parse_profile_response(second).unwrap();
    assert!(profile.public_account_id.is_empty());
    assert!(profile.personal.nickname.is_empty());
    server.join().unwrap();
}

#[test]
fn identity_session_acquire_uses_nested_json_without_level1_preflight() {
    let (config, server) = server(vec![(
        200,
        session_json("account-access", "account-refresh", json!([4, 9])),
    )]);
    let session = IdentitySession::default();
    assert_eq!(
        session.acquire_session(&config).unwrap().public_account_id,
        "fixture-account"
    );
    let requests = server.join().unwrap();
    assert_eq!(requests.len(), 1);
    assert_eq!(
        requests[0].start,
        "POST /proxy/session/1/apps/synthetic-client/sessions HTTP/1.1"
    );
    let body: Value = serde_json::from_str(&requests[0].body).unwrap();
    assert!(body["refresh"].is_null());
    assert_eq!(body["access"]["clientId"], "synthetic-client");
    assert_eq!(
        body["access"]["clientSignature"],
        "fixture-client-signature-never-sgs"
    );
    assert_eq!(body["access"]["sdkVersion"], "1130200");
    assert_eq!(body["access"]["fusionVersion"], "66595");
    assert!(
        body["access"]
            .as_object()
            .unwrap()
            .values()
            .all(Value::is_string)
    );
    assert!(!requests[0].headers.contains_key("x-access-token"));
    assert!(!requests[0].headers.contains_key("rovio-sgs"));
    let tokens = session.level2_tokens();
    assert_eq!(tokens.access_token, "account-access");
    assert_eq!(tokens.segment, "4, 9");
    assert!(session.state.lock().unwrap().level1.access_token.is_empty());
    assert_eq!(session.config().unwrap()["negative"], u64::MAX.to_string());
}

#[test]
fn identity_session_stored_refresh_401_retries_null_once_and_rotates_store() {
    let store = Arc::new(MemoryRefreshStore::default());
    store.store("old-refresh").unwrap();
    let session = IdentitySession::with_refresh_store(store.clone());
    let (config, server) = server(vec![
        (401, "rejected".to_owned()),
        (200, session_json("new-a", "new-r", json!([]))),
    ]);
    session.acquire_session(&config).unwrap();
    let requests = server.join().unwrap();
    let first: Value = serde_json::from_str(&requests[0].body).unwrap();
    let second: Value = serde_json::from_str(&requests[1].body).unwrap();
    assert_eq!(first["refresh"], json!({"token":"old-refresh"}));
    assert!(second["refresh"].is_null());
    assert_eq!(first["access"], second["access"]);
    assert_eq!(store.load().unwrap(), "new-r");
}

#[test]
fn identity_session_memory_refresh_injection_survives_new_instance_without_claiming_disk() {
    let store = Arc::new(MemoryRefreshStore::default());
    let first = IdentitySession::with_refresh_store(store.clone());
    first.install_flat(&flat("previous-a", "retained-r", "previous-s"));
    drop(first);
    let second = IdentitySession::with_refresh_store(store);
    let (config, server) = server(vec![(200, session_json("new-a", "new-r", json!([1])))]);
    second.acquire_session(&config).unwrap();
    let requests = server.join().unwrap();
    let body: Value = serde_json::from_str(&requests[0].body).unwrap();
    assert_eq!(body["refresh"], json!({"token":"retained-r"}));
}

#[test]
fn identity_session_success_replaces_config_instead_of_merging() {
    let mut second: Value =
        serde_json::from_str(&session_json("second-a", "second-r", json!([]))).unwrap();
    second["config"] = json!({});
    let (config, server) = server(vec![
        (200, session_json("first-a", "first-r", json!([]))),
        (200, second.to_string()),
    ]);
    let session = IdentitySession::default();
    session.acquire_session(&config).unwrap();
    assert!(!session.config().unwrap().is_empty());
    session.state.lock().unwrap().level2.access_token.clear();
    session.acquire_session(&config).unwrap();
    assert!(session.config().unwrap().is_empty());
    assert_eq!(session.level2_tokens().access_token, "second-a");
    server.join().unwrap();
}

#[test]
fn identity_session_logout_preserves_native_config_until_explicit_host_provider_reset() {
    let (config, server) = server(vec![(200, session_json("a", "r", json!([])))]);
    let session = IdentitySession::default();
    session.acquire_session(&config).unwrap();
    let config_before = session.config().unwrap();
    session.logout().unwrap();
    assert_eq!(session.config().unwrap(), config_before);
    assert!(session.profile().is_none());
    assert!(session.level2_tokens().access_token.is_empty());
    session.reset_config();
    assert!(session.config().is_none());
    server.join().unwrap();
}

#[test]
fn identity_session_social_projection_matches_first_selected_external_pair_not_vector_nonempty() {
    let cases = [
        (
            json!({"socialNetworks":[{"provider":"facebook","id":"x"}]}),
            false,
            "",
            "",
        ),
        (
            json!({"externalNetworks":[{"provider":"facebook","id":"x"}]}),
            true,
            "x",
            "",
        ),
        (
            json!({"externalNetworks":[{"provider":"unknown","id":"x"}]}),
            false,
            "x",
            "",
        ),
        (
            json!({"socialNetworks":[{"provider":"unknown","id":"","socialAttributes":{"name":"default pair"}}]}),
            true,
            "",
            "default pair",
        ),
        (
            json!({"externalNetworks":[null,{"provider":"facebook","id":"x"}],"socialNetworks":[{"provider":"facebook","id":"x"}]}),
            false,
            "",
            "",
        ),
        (
            json!({"externalNetworks":[{"provider":"facebook","id":"x"}],"socialNetworks":[{"provider":"facebook","id":"x","socialAttributes":{"name":"first"}},{"provider":"facebook","id":"x","socialAttributes":{"name":"second"}}]}),
            true,
            "x",
            "first",
        ),
        (
            json!({"externalNetworks":[{"provider":"odd1","id":"x"}],"socialNetworks":[{"provider":"odd2","id":"x","socialAttributes":{"name":"enum zero"}}]}),
            true,
            "x",
            "enum zero",
        ),
        (
            json!({"personal":{"nickName":"not a fallback"},"externalNetworks":[{"provider":"facebook","id":"x"}],"socialNetworks":[{"provider":"facebook","id":"x","socialAttributes":{"name":false}}]}),
            true,
            "x",
            "",
        ),
    ];
    for (value, connected, id, name) in cases {
        let profile = protocol::parse_profile_value(&value);
        assert_eq!(profile.connected_to_social_network, connected, "{value}");
        assert_eq!(profile.active_external_id, id, "{value}");
        assert_eq!(profile.active_social_name, name, "{value}");
    }
    let profile = protocol::parse_profile_value(
        &json!({"externalNetworks":[{"provider":"facebook","id":"x"}]}),
    );
    assert_eq!(
        profile.social_networks.len(),
        1,
        "constructor appends missing known pair"
    );
}

#[test]
fn identity_session_common_access_parser_accepts_2xx_and_preserves_large_integer_low_word() {
    let (config,server) = server(vec![
        (201,r#"{"accessToken":"a","refreshToken":"r","expiresIn":9007199254740993,"segment":false}"#.to_owned()),
        (200,r#"{"accessToken":"a","refreshToken":"r","expiresIn":1e100}"#.to_owned()),
        (204,String::new()),
        (200,r#"{"accessToken":"a","refreshToken":"","expiresIn":1}"#.to_owned()),
    ]);
    let response = agent()
        .get(config.endpoint.request_url("fixture"))
        .call()
        .unwrap();
    let before = unix_seconds();
    let access = parse_access_response(response).unwrap();
    assert!((before + 1..=unix_seconds() + 1).contains(&access.absolute_expiry));
    assert!(access.segment.is_none());
    let response = agent()
        .get(config.endpoint.request_url("fixture"))
        .call()
        .unwrap();
    assert_eq!(parse_access_response(response).unwrap().absolute_expiry, 0);
    for _ in 0..2 {
        let response = agent()
            .get(config.endpoint.request_url("fixture"))
            .call()
            .unwrap();
        assert_eq!(parse_access_response(response).err().unwrap().status, -1);
    }
    server.join().unwrap();
}

#[test]
fn identity_session_401_rejection_is_bounded_for_refresh_and_null() {
    for stored in ["", "rejected-refresh"] {
        let store = Arc::new(MemoryRefreshStore::default());
        store.store(stored).unwrap();
        let session = IdentitySession::with_refresh_store(store.clone());
        let attempts = if stored.is_empty() { 1 } else { 2 };
        let (config, server) = server(vec![
            (401, "secret-response-not-in-error".to_owned());
            attempts
        ]);
        let error = session.acquire_session(&config).err().unwrap();
        assert_eq!(error.status, 401);
        assert_eq!(error.to_string(), "identity status 401");
        assert_eq!(server.join().unwrap().len(), attempts);
        assert!(store.load().unwrap().is_empty());
        assert!(session.level2_tokens().access_token.is_empty());
        assert!(session.profile().is_none());
    }
}

#[test]
fn identity_session_negotiation_requires_exact_200_and_does_not_retry_other_errors() {
    for status in [201, 204, 302, 400, 403, 412, 500] {
        let (config, server) = server(vec![(status, session_json("a", "r", json!([])))]);
        let session = IdentitySession::default();
        assert_eq!(
            session.acquire_session(&config).err().unwrap().status,
            i32::from(status)
        );
        assert_eq!(server.join().unwrap().len(), 1);
        assert!(session.level2_tokens().access_token.is_empty());
        assert!(session.profile().is_none());
    }
}

#[test]
fn identity_session_config_failure_cannot_publish_tokens_profile_or_refresh() {
    for invalid in [
        Value::Null,
        json!([]),
        json!({"bad":[]}),
        json!({"bad":null}),
        json!({"bad":{}}),
    ] {
        let mut body: Value =
            serde_json::from_str(&session_json("new-a", "new-r", json!([3]))).unwrap();
        body["config"] = invalid;
        let (config, server) = server(vec![(200, body.to_string())]);
        let store = Arc::new(MemoryRefreshStore::default());
        store.store("old-r").unwrap();
        let session = IdentitySession::with_refresh_store(store.clone());
        let mut old_profile = profile();
        old_profile.public_account_id = "previous-account".to_owned();
        assert!(
            session
                .install_profile_if_epoch(session.epoch(), &old_profile)
                .unwrap()
        );
        assert!(session.acquire_session(&config).is_err());
        server.join().unwrap();
        assert!(session.level2_tokens().access_token.is_empty());
        assert_eq!(store.load().unwrap(), "old-r");
        assert_eq!(
            session.profile().unwrap().public_account_id,
            "previous-account"
        );
        assert!(session.config().is_none());
    }
}

#[test]
fn identity_session_post_replays_frozen_body_once_with_new_access_and_segments() {
    let (config, server) = server(vec![
        (401, "denied".to_owned()),
        (
            200,
            session_json("renewed-access", "renewed-refresh", json!([7, 8])),
        ),
        (202, "{}".to_owned()),
    ]);
    let session = IdentitySession::default();
    session.install_flat(&flat("old-access", "old-refresh", "old-segments"));
    let body = "email=a%2Bb%40example.invalid&password=synthetic%26%3D";
    assert_eq!(
        session
            .execute_form(&config, ProviderLevel::Level2, "abid/login", body)
            .unwrap()
            .status(),
        202
    );
    let requests = server.join().unwrap();
    assert_eq!(requests[0].body, body);
    assert_eq!(requests[2].body, body);
    assert_eq!(requests[0].start, requests[2].start);
    assert_eq!(requests[0].headers["x-access-token"], "old-access");
    assert_eq!(requests[0].headers["rovio-sgs"], "old-segments");
    assert_eq!(requests[2].headers["x-access-token"], "renewed-access");
    assert_eq!(requests[2].headers["rovio-sgs"], "7, 8");
    assert!(
        !requests[2]
            .headers
            .values()
            .any(|value| value == "fixture-client-signature-never-sgs")
    );
    assert_eq!(session.level2_tokens().refresh_token, "renewed-refresh");
}

#[test]
fn identity_session_second_protected_401_does_not_replay_again_or_fallback_provider() {
    let (config, server) = server(vec![
        (401, String::new()),
        (200, session_json("a2", "r2", json!([2]))),
        (401, "password-private".to_owned()),
    ]);
    let session = IdentitySession::default();
    session.install_flat(&flat("a1", "r1", "s1"));
    session.install_level1_flat(&flat("parent-a", "parent-r", "parent-s"));
    let error = session
        .execute_form(
            &config,
            ProviderLevel::Level2,
            "abid/login",
            "password=fixture",
        )
        .err()
        .unwrap();
    assert_eq!(error.status, 401);
    assert!(!format!("{error:?}").contains("password"));
    let requests = server.join().unwrap();
    assert_eq!(requests.len(), 3);
    assert_eq!(requests[2].headers["x-access-token"], "a2");
    assert_eq!(
        session.state.lock().unwrap().level1.access_token,
        "parent-a"
    );
}

#[test]
fn identity_session_get_replays_same_url_and_non401_never_renews() {
    let (config, server) = server(vec![
        (401, String::new()),
        (200, session_json("new", "r", json!([12]))),
        (204, String::new()),
        (403, String::new()),
    ]);
    let session = IdentitySession::default();
    session.install_flat(&flat("old", "r", "s"));
    assert_eq!(
        session
            .execute_get(&config, ProviderLevel::Level2, "fixture/get")
            .unwrap()
            .status(),
        204
    );
    assert_eq!(
        session
            .execute_get(&config, ProviderLevel::Level2, "fixture/forbidden")
            .err()
            .unwrap()
            .status,
        403
    );
    let requests = server.join().unwrap();
    assert_eq!(requests[0].start, requests[2].start);
    assert!(requests[0].body.is_empty() && requests[2].body.is_empty());
    assert_eq!(requests[2].headers["rovio-sgs"], "12");
    assert_eq!(requests.len(), 4);
}

#[test]
fn identity_session_parent_requests_use_access_not_app_session_and_keep_layers_separate() {
    let (config, server) = server(vec![
        (200, flat_json("parent-a", "parent-s")),
        (401, String::new()),
        (200, flat_json("parent-new", "parent-new-s")),
        (200, "{}".to_owned()),
    ]);
    let session = IdentitySession::default();
    session.install_flat(&flat("account-a", "account-r", "account-s"));
    session
        .execute_form(
            &config,
            ProviderLevel::Level1,
            "abid/validate/email",
            "email=fixture",
        )
        .unwrap();
    let requests = server.join().unwrap();
    assert_eq!(
        requests[0].start,
        "POST /proxy/identity/2.0/access HTTP/1.1"
    );
    assert_eq!(requests[2].start, requests[0].start);
    for index in [0, 2] {
        let fields: Vec<_> = requests[index].body.split('&').collect();
        assert_eq!(fields.len(), 14);
        assert!(fields.contains(&"locale=en_EN"));
        assert!(fields.contains(&"distributionChannel=apple"));
        for (key, value) in [
            ("deviceType", crate::native_device_info_model()),
            (
                "osVersion",
                super::super::os_version::current_version().unwrap(),
            ),
        ] {
            let encoded = super::super::form_body(&[(key, value)]);
            assert!(fields.contains(&encoded.as_str()));
        }
        assert!(
            !fields
                .iter()
                .any(|field| field.starts_with("definition=") || field.starts_with("buildId="))
        );
    }
    assert!(!requests[0].headers.contains_key("x-access-token"));
    assert_eq!(requests[1].headers["x-access-token"], "parent-a");
    assert_eq!(requests[3].headers["x-access-token"], "parent-new");
    assert_eq!(requests[3].headers["rovio-sgs"], "parent-new-s");
    assert_eq!(requests[1].body, requests[3].body);
    assert_eq!(session.level2_tokens().access_token, "account-a");
    assert_eq!(session.level2_tokens().refresh_token, "account-r");
}

#[test]
fn identity_session_level1_200_boundary_and_level2_nonempty_expiry_short_circuit() {
    let (config, server) = server(vec![
        (201, flat_json("parent", "s")),
        (200, "{}".to_owned()),
    ]);
    let session = IdentitySession::default();
    assert_eq!(
        session
            .execute_get(&config, ProviderLevel::Level1, "fixture")
            .err()
            .unwrap()
            .status,
        201
    );
    session.install_flat(&flat("expired-account", "r", "s"));
    session.state.lock().unwrap().level2.absolute_expiry = 1;
    session
        .execute_get(&config, ProviderLevel::Level2, "fixture")
        .unwrap();
    let requests = server.join().unwrap();
    assert_eq!(requests.len(), 2);
    assert_eq!(requests[1].headers["x-access-token"], "expired-account");
    let mut tokens = Tokens {
        access_token: "a".to_owned(),
        absolute_expiry: unix_seconds() + 599,
        ..Tokens::default()
    };
    assert!(tokens.needs_acquire(ProviderLevel::Level1));
    assert!(!tokens.needs_acquire(ProviderLevel::Level2));
    tokens.absolute_expiry = 0;
    assert!(!tokens.needs_acquire(ProviderLevel::Level1));
}

#[test]
fn identity_session_logout_invalidates_delayed_acquire_without_waiting_for_http() {
    for level in [ProviderLevel::Level1, ProviderLevel::Level2] {
        let (config, listener) = bind();
        let store = Arc::new(MemoryRefreshStore::default());
        store.store("prior-refresh").unwrap();
        let session = IdentitySession::with_refresh_store(store.clone());
        let epoch = session.epoch();
        assert!(session.install_profile_if_epoch(epoch, &profile()).unwrap());
        let worker = session.clone();
        let task =
            thread::spawn(move || worker.execute_get_at_epoch(&config, epoch, level, "fixture"));
        let (stream, _) = accept(&listener);
        assert_eq!(session.logout().unwrap(), epoch + 1);
        let body = match level {
            ProviderLevel::Level1 => flat_json("late-a", "late-s"),
            ProviderLevel::Level2 => session_json("late-a", "late-r", json!([1])),
        };
        reply(stream, 200, &body);
        let error = task.join().unwrap().err().unwrap();
        assert!(error.is_stale());
        assert!(session.level2_tokens().access_token.is_empty());
        assert!(session.state.lock().unwrap().level1.access_token.is_empty());
        assert!(session.profile().is_none());
        assert!(session.config().is_none());
        assert!(store.load().unwrap().is_empty());
        assert!(
            !session
                .install_flat_if_epoch(epoch, &flat("stale", "stale", "stale"))
                .unwrap()
        );
        assert!(!session.install_profile_if_epoch(epoch, &profile()).unwrap());
    }
}

#[test]
fn identity_session_stale_epoch_refuses_before_http_and_clones_share_single_renewal() {
    let (config, listener) = bind();
    let session = IdentitySession::default();
    let old = session.epoch();
    session.logout().unwrap();
    assert!(
        session
            .acquire_session_at_epoch(&config, old)
            .err()
            .unwrap()
            .is_stale()
    );
    assert!(matches!(listener.accept(),Err(error) if error.kind()==std::io::ErrorKind::WouldBlock));
    let barrier = Arc::new(Barrier::new(3));
    let (tx, rx) = mpsc::channel();
    let mut tasks = Vec::new();
    for _ in 0..2 {
        let session = session.clone();
        let config = config.clone();
        let barrier = barrier.clone();
        let tx = tx.clone();
        tasks.push(thread::spawn(move || {
            barrier.wait();
            tx.send(
                session
                    .acquire_session(&config)
                    .map(|profile| profile.public_account_id),
            )
            .unwrap();
        }));
    }
    barrier.wait();
    let (stream, _) = accept(&listener);
    reply(stream, 200, &session_json("a", "r", json!([])));
    for _ in 0..2 {
        assert_eq!(
            rx.recv_timeout(Duration::from_secs(5)).unwrap().unwrap(),
            "fixture-account"
        );
    }
    for task in tasks {
        task.join().unwrap();
    }
    assert!(matches!(listener.accept(),Err(error) if error.kind()==std::io::ErrorKind::WouldBlock));
}

#[test]
fn identity_session_parent_and_account_renewal_locks_do_not_block_each_other() {
    let (config, listener) = bind();
    let session = IdentitySession::default();
    let epoch = session.epoch();
    let parent_session = session.clone();
    let parent_config = config.clone();
    let parent = thread::spawn(move || {
        parent_session.execute_get_at_epoch(&parent_config, epoch, ProviderLevel::Level1, "fixture")
    });
    let (parent_stream, parent_request) = accept(&listener);
    assert!(parent_request.start.contains("/identity/2.0/access"));
    let account_session = session.clone();
    let account = thread::spawn(move || account_session.acquire_session_at_epoch(&config, epoch));
    // Parent access response is deliberately withheld; an independent Level2
    // acquire must still reach the loopback listener and finish successfully.
    let (account_stream, account_request) = accept(&listener);
    assert!(account_request.start.contains("/session/1/apps/"));
    reply(
        account_stream,
        200,
        &session_json("account", "refresh", json!([1])),
    );
    assert_eq!(
        account.join().unwrap().unwrap().public_account_id,
        "fixture-account"
    );
    session.logout().unwrap();
    reply(
        parent_stream,
        200,
        &flat_json("late-parent", "parent-segment"),
    );
    assert!(parent.join().unwrap().err().unwrap().is_stale());
}

#[test]
fn own_profile_publication_storage_order_and_partial_failure_keep_their_owner() {
    for failure in [None, Some("profile"), Some("refresh")] {
        let store = Arc::new(FailingLogoutStore::default());
        let session = IdentitySession::with_refresh_store(store.clone());
        session.bind_success_events(crate::ApplicationEventScheduler::default());
        session.install_flat(&flat("old-a", "old-r", "old-s"));
        let old = profile();
        session
            .install_profile_if_epoch(session.epoch(), &old)
            .unwrap();
        store.write_order.lock().unwrap().clear();
        store
            .fail_profile
            .store(failure == Some("profile"), Ordering::Relaxed);
        store
            .fail_refresh
            .store(failure == Some("refresh"), Ordering::Relaxed);
        let mut owner = session
            .own_profile_owner_for_request(session.request_owner(ProviderLevel::Level2))
            .unwrap();
        let original_owner = owner;
        let mut next = old.clone();
        next.public_account_id = "replacement".into();
        next.raw = json!({"publicAccountId":"replacement"});
        let before = session.login_profile_identity(owner).unwrap();
        let result = session.publish_login_profile(
            &mut owner,
            &flat("new-a", "new-r", "new-s"),
            &next,
            &super::super::identifiers::Identifiers::synthetic(),
            before,
        );
        assert!(
            session.own_profile_owner_is_current(owner),
            "failure must retain its own callback lifetime"
        );
        if failure == Some("profile") {
            assert_eq!(result, Err(StoreError::Io));
            assert_eq!(*store.write_order.lock().unwrap(), ["profile"]);
            assert_eq!(owner, original_owner);
            assert_eq!(session.profile().unwrap().raw, old.raw);
            assert_eq!(session.level2_tokens().access_token, "old-a");
            assert_eq!(store.load().unwrap(), "old-r");
        } else {
            assert_eq!(*store.write_order.lock().unwrap(), ["profile", "refresh"]);
            assert_ne!(owner, original_owner);
            assert_eq!(session.profile().unwrap().raw, next.raw);
            assert_eq!(store.load_profile().unwrap(), Some(next.raw));
            assert_eq!(session.level2_tokens().refresh_token, "new-r");
            if failure.is_some() {
                assert_eq!(result, Err(StoreError::Io));
                assert!(session.level2_tokens().access_token.is_empty());
                assert_eq!(store.load().unwrap(), "old-r");
            } else {
                assert_eq!(result, Ok(true));
                assert_eq!(session.level2_tokens().access_token, "new-a");
                assert_eq!(store.load().unwrap(), "new-r");
            }
        }
        assert_eq!(session.pop_success_owner().is_some(), failure.is_none());
    }
}

#[test]
fn own_profile_admission_requires_current_level2_permission_without_mutating_identity() {
    let session = IdentitySession::default();
    let level1 = session.request_owner(ProviderLevel::Level1);
    let level2 = session.request_owner(ProviderLevel::Level2);
    assert!(session.own_profile_owner_for_request(level1).is_none());
    let lifetime = session.storage_lifetime();
    assert!(session.own_profile_owner_for_request(level2).is_some());
    assert_eq!(session.storage_lifetime(), lifetime);
    assert!(session.level2_tokens().access_token.is_empty());
    session.install_flat(&flat("replacement", "refresh", "segments"));
    assert!(session.own_profile_owner_for_request(level2).is_none());
}
