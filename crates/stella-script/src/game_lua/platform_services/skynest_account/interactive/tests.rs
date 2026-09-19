//! Synthetic loopback own-profile ownership regressions; no provider files.

mod avatar_assets;
mod request_ownership;

use super::*;
use session::{MemoryRefreshStore, RefreshStore};
use std::{
    io::{Read, Write},
    net::{TcpListener, TcpStream},
    thread,
    time::{Duration, Instant},
};

fn fixture() -> (Lua, SkynestAccountRuntime, Arc<MemoryRefreshStore>) {
    let store = Arc::new(MemoryRefreshStore::default());
    let (lua, runtime) = fixture_with_store(store.clone());
    (lua, runtime, store)
}

fn fixture_with_store(store: Arc<dyn RefreshStore>) -> (Lua, SkynestAccountRuntime) {
    let mut state = OfflineState::new(PathBuf::from(
        "synthetic-own-profile/not-read-or-written.json",
    ));
    state.identity_session = IdentitySession::with_refresh_store(store.clone());
    let runtime = SkynestAccountRuntime::new(
        Arc::new(Mutex::new(state)),
        ApplicationEventScheduler::default(),
        crate::game_lua::platform_services::skynest_account::identifiers::Identifiers::synthetic()
            .into(),
    );
    let lua = Lua::new();
    lua.load(
        r#"
        login_successes = 0
        login_failures = 0
        SkynestAccount = {
            onLoginSuccess = function(_, details)
                login_successes = login_successes + 1
                delivered_id = details.id
            end,
            onLoginFailure = function()
                login_failures = login_failures + 1
            end,
        }
        "#,
    )
    .exec()
    .unwrap();
    replace_identity(&runtime.session, "account-a");
    (lua, runtime)
}

fn access(id: &str) -> AccessResponse {
    AccessResponse {
        access_token: format!("{id}-synthetic-access"),
        refresh_token: format!("{id}-synthetic-refresh"),
        absolute_expiry: i64::MAX,
        segment: Some("synthetic-segments".to_owned()),
    }
}

fn raw_profile(id: &str) -> serde_json::Value {
    serde_json::json!({
        "publicAccountId": id,
        "personal": {"nickName": id, "email": format!("{id}@example.invalid")},
    })
}

fn replace_identity(session: &IdentitySession, id: &str) {
    session.install_flat(&access(id));
    let raw = raw_profile(id);
    let mut profile: ProfileResponse = serde_json::from_value(raw.clone()).unwrap();
    profile.raw = raw;
    assert!(
        session
            .install_profile_if_epoch(session.epoch(), &profile)
            .unwrap()
    );
}

fn bind() -> (IdentityConfig, TcpListener) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let config = IdentityConfig {
        identifiers:
            crate::game_lua::platform_services::skynest_account::identifiers::Identifiers::synthetic().into(),
        endpoint: IdentityEndpoint::parse(&format!(
            "http://{}/identity/3.0",
            listener.local_addr().unwrap()
        ))
        .unwrap(),
        client_id: "synthetic-client".to_owned(),
        signing: super::super::ClientSigning::literal(String::new(), String::new()),
    };
    (config, listener)
}

fn accept_profile(listener: &TcpListener) -> TcpStream {
    let deadline = Instant::now() + Duration::from_secs(5);
    let mut stream = loop {
        match listener.accept() {
            Ok((stream, _)) => break stream,
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                assert!(Instant::now() < deadline, "own-profile request timed out");
                thread::sleep(Duration::from_millis(1));
            }
            Err(error) => panic!("loopback accept failed: {error}"),
        }
    };
    stream.set_nonblocking(false).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(5)))
        .unwrap();
    let mut request = Vec::new();
    while !request.windows(4).any(|bytes| bytes == b"\r\n\r\n") {
        let mut bytes = [0; 1024];
        let count = stream.read(&mut bytes).unwrap();
        assert_ne!(count, 0);
        request.extend_from_slice(&bytes[..count]);
        assert!(request.len() < 8192);
    }
    let request = String::from_utf8(request).unwrap().to_ascii_lowercase();
    assert!(request.starts_with("get /identity/3.0/profile/own "));
    assert!(request.contains("x-access-token: account-b-synthetic-access\r\n"));
    stream
}

fn reply(mut stream: TcpStream, status: u16) {
    let body = raw_profile("account-b").to_string();
    write!(
        stream,
        "HTTP/1.1 {status} Fixture\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    )
    .unwrap();
}

fn await_profile_result(runtime: &SkynestAccountRuntime) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while runtime.online_completions.lock().unwrap().is_empty() {
        assert!(
            Instant::now() < deadline,
            "own-profile completion timed out"
        );
        thread::sleep(Duration::from_millis(1));
    }
}

#[test]
fn own_profile_publication_keeps_its_login_alive_after_profile_generation_changes() {
    let (lua, runtime, store) = fixture();
    let epoch = runtime.session.epoch();
    let (config, listener) = bind();
    // An already constructed response retains its timestamp across arbitrary
    // application-queue/HTTP delays, even when it is now in the past.
    let mut returned_access = access("account-b");
    returned_access.absolute_expiry = 1234;
    runtime
        .dispatch_interactive(
            &lua,
            runtime.session.request_owner(ProviderLevel::Level2),
            InteractiveCompletion::Tokens {
                login_job: runtime.begin_login_job().unwrap(),
                config,
                access: returned_access,
            },
        )
        .unwrap();
    let request_owner = runtime.session.storage_lifetime();
    let stream = accept_profile(&listener);
    // Receiving login tokens and sending the direct GET cannot publish them.
    assert_eq!(
        runtime.session.level2_tokens().access_token,
        access("account-a").access_token
    );
    assert_eq!(
        runtime.session.profile().unwrap().raw,
        raw_profile("account-a")
    );
    assert_eq!(store.load().unwrap(), access("account-a").refresh_token);
    assert_eq!(
        store.load_profile().unwrap(),
        Some(raw_profile("account-a"))
    );
    assert!(runtime.pop_session_success().is_none());
    reply(stream, 200);
    await_profile_result(&runtime);
    assert_eq!(runtime.session.epoch(), epoch);
    assert_ne!(runtime.session.storage_lifetime(), request_owner);
    assert_eq!(runtime.session.level2_tokens().absolute_expiry, 1234);
    assert_eq!(
        runtime.session.level2_tokens().access_token,
        access("account-b").access_token
    );
    assert_eq!(store.load().unwrap(), access("account-b").refresh_token);
    assert!(runtime.pop_session_success().is_some());
    assert_eq!(
        runtime.session.profile().unwrap().public_account_id,
        "account-b"
    );
    assert_eq!(
        store.load_profile().unwrap(),
        Some(raw_profile("account-b"))
    );
    super::super::dispatch_online_completion(&lua, &runtime).unwrap();
    assert_eq!(lua.globals().get::<u32>("login_successes").unwrap(), 0);
    assert!(runtime.state.lock().unwrap().login_in_progress);
    super::super::dispatch_local_completion(&lua, &runtime).unwrap();
    assert_eq!(lua.globals().get::<u32>("login_successes").unwrap(), 1);
    assert_eq!(
        lua.globals().get::<String>("delivered_id").unwrap(),
        "account-b"
    );
    assert_eq!(lua.globals().get::<u32>("login_failures").unwrap(), 0);
    assert!(!runtime.state.lock().unwrap().login_in_progress);
}

#[test]
fn own_profile_http_or_parse_failure_preserves_previous_identity_and_reports_failure() {
    for (status, body) in [(401, "{}"), (201, "{}"), (503, "{}"), (200, "malformed")] {
        let (lua, runtime, store) = fixture();
        let lifetime = runtime.session.storage_lifetime();
        let (config, listener) = bind();
        runtime
            .dispatch_interactive(
                &lua,
                runtime.session.request_owner(ProviderLevel::Level2),
                InteractiveCompletion::Tokens {
                    login_job: runtime.begin_login_job().unwrap(),
                    config,
                    access: access("account-b"),
                },
            )
            .unwrap();
        let mut stream = accept_profile(&listener);
        write!(
            stream,
            "HTTP/1.1 {status} Fixture\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        )
        .unwrap();
        drop(stream);
        await_profile_result(&runtime);
        assert_eq!(runtime.session.storage_lifetime(), lifetime);
        assert_eq!(
            runtime.session.level2_tokens().access_token,
            access("account-a").access_token
        );
        assert_eq!(
            runtime.session.profile().unwrap().raw,
            raw_profile("account-a")
        );
        assert_eq!(store.load().unwrap(), access("account-a").refresh_token);
        assert_eq!(
            store.load_profile().unwrap(),
            Some(raw_profile("account-a"))
        );
        assert!(runtime.pop_session_success().is_none());
        super::super::dispatch_online_completion(&lua, &runtime).unwrap();
        assert_eq!(lua.globals().get::<u32>("login_failures").unwrap(), 0);
        super::super::dispatch_local_completion(&lua, &runtime).unwrap();
        assert_eq!(lua.globals().get::<u32>("login_failures").unwrap(), 1);
        assert_eq!(lua.globals().get::<u32>("login_successes").unwrap(), 0);
        assert!(!runtime.state.lock().unwrap().login_in_progress);
        assert!(
            matches!(listener.accept(), Err(error) if error.kind() == std::io::ErrorKind::WouldBlock)
        );
    }
}

#[test]
fn own_profile_null_response_publishes_native_empty_profile_with_returned_tokens() {
    let (lua, runtime, store) = fixture();
    let (config, listener) = bind();
    runtime
        .dispatch_interactive(
            &lua,
            runtime.session.request_owner(ProviderLevel::Level2),
            InteractiveCompletion::Tokens {
                login_job: runtime.begin_login_job().unwrap(),
                config,
                access: access("account-b"),
            },
        )
        .unwrap();
    let mut stream = accept_profile(&listener);
    write!(
        stream,
        "HTTP/1.1 200 Fixture\r\nContent-Length: 4\r\nConnection: close\r\n\r\nnull"
    )
    .unwrap();
    drop(stream);
    await_profile_result(&runtime);
    assert!(
        runtime
            .session
            .profile()
            .unwrap()
            .public_account_id
            .is_empty()
    );
    assert_eq!(
        runtime.session.level2_tokens().access_token,
        access("account-b").access_token
    );
    assert_eq!(store.load().unwrap(), access("account-b").refresh_token);
    assert_eq!(store.load_profile().unwrap(), Some(serde_json::Value::Null));
    assert!(runtime.pop_session_success().is_some());
    super::super::dispatch_online_completion(&lua, &runtime).unwrap();
    super::super::dispatch_local_completion(&lua, &runtime).unwrap();
    assert_eq!(lua.globals().get::<u32>("login_successes").unwrap(), 1);
    assert_eq!(lua.globals().get::<u32>("login_failures").unwrap(), 0);
}

#[test]
fn logout_during_own_profile_does_not_install_returned_credentials() {
    let (lua, runtime, store) = fixture();
    let (config, listener) = bind();
    runtime
        .dispatch_interactive(
            &lua,
            runtime.session.request_owner(ProviderLevel::Level2),
            InteractiveCompletion::Tokens {
                login_job: runtime.begin_login_job().unwrap(),
                config,
                access: access("account-b"),
            },
        )
        .unwrap();
    let stream = accept_profile(&listener);
    runtime.session.logout().unwrap();
    reply(stream, 200);
    await_profile_result(&runtime);
    super::super::dispatch_online_completion(&lua, &runtime).unwrap();
    super::super::dispatch_local_completion(&lua, &runtime).unwrap();
    assert!(runtime.session.level2_tokens().access_token.is_empty());
    assert!(runtime.session.profile().is_none());
    assert!(store.load().unwrap().is_empty());
    assert!(store.load_profile().unwrap().is_none());
    assert!(runtime.pop_session_success().is_none());
    assert_eq!(lua.globals().get::<u32>("login_successes").unwrap(), 0);
    assert_eq!(lua.globals().get::<u32>("login_failures").unwrap(), 0);
}

#[test]
fn replaced_own_profile_cannot_overwrite_cache_or_deliver_late_success_or_failure() {
    // Cover all three races: response still in flight, queued online result,
    // and queued final Lua callback. Include failures so old errors cannot
    // complete or disturb C's new login either.
    for status in [200, 503] {
        for replacement_stage in 0..3 {
            let (lua, runtime, store) = fixture();
            let epoch = runtime.session.epoch();
            let (config, listener) = bind();
            runtime
                .dispatch_interactive(
                    &lua,
                    runtime.session.request_owner(ProviderLevel::Level2),
                    InteractiveCompletion::Tokens {
                        login_job: runtime.begin_login_job().unwrap(),
                        config,
                        access: access("account-b"),
                    },
                )
                .unwrap();
            let stream = accept_profile(&listener);
            if replacement_stage == 0 {
                replace_identity(&runtime.session, "account-c");
                runtime.begin_login_job().unwrap();
            }
            reply(stream, status);
            await_profile_result(&runtime);
            if replacement_stage == 1 {
                replace_identity(&runtime.session, "account-c");
                runtime.begin_login_job().unwrap();
            }
            super::super::dispatch_online_completion(&lua, &runtime).unwrap();
            if replacement_stage == 2 {
                assert_eq!(runtime.completions.borrow().len(), 1);
                replace_identity(&runtime.session, "account-c");
                runtime.begin_login_job().unwrap();
            }
            super::super::dispatch_local_completion(&lua, &runtime).unwrap();
            assert_eq!(runtime.session.epoch(), epoch, "not an epoch cancellation");
            assert_eq!(
                runtime.session.profile().unwrap().public_account_id,
                "account-c"
            );
            assert_eq!(
                store.load_profile().unwrap(),
                Some(raw_profile("account-c"))
            );
            assert_eq!(store.load().unwrap(), "account-c-synthetic-refresh");
            assert_eq!(
                runtime.session.level2_tokens().access_token,
                "account-c-synthetic-access"
            );
            assert_eq!(lua.globals().get::<u32>("login_successes").unwrap(), 0);
            assert_eq!(lua.globals().get::<u32>("login_failures").unwrap(), 0);
            assert!(runtime.state.lock().unwrap().login_in_progress);
            assert!(runtime.completions.borrow().is_empty());
            assert!(
                matches!(listener.accept(), Err(error) if error.kind() == std::io::ErrorKind::WouldBlock),
                "direct own-profile does not gain an HTTP retry"
            );
        }
    }
}
