//! Registration tests use only isolated saves and loopback HTTP endpoints.

use super::*;
use std::{
    io::Write,
    net::{TcpListener, TcpStream},
    sync::atomic::{AtomicU64, Ordering},
    thread,
    time::Instant,
};

fn date(year: i32, month: u32, day: u32) -> RegistrationBirthday {
    RegistrationBirthday { year, month, day }
}

struct Fixture {
    runtime: StellaLua,
    root: PathBuf,
}

impl Fixture {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(1);
        let root = std::env::temp_dir().join(format!(
            "stella-registration-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        let data = root.join("data");
        std::fs::create_dir_all(&data).unwrap();
        let runtime = StellaLua::new(data).unwrap();
        runtime
            .execute_source(
                r#"
            registration_successes, registration_failures = 0, 0
            _G.SkynestAccount.onLoginSuccess = function()
                registration_successes = registration_successes + 1
            end
            _G.SkynestAccount.onLoginFailure = function(code)
                registration_failures = registration_failures + 1
                registration_failure_code = code
            end
        "#,
            )
            .unwrap();
        Self { runtime, root }
    }

    fn open(&self) -> u64 {
        self.runtime
            .execute_source("_G.SkynestAccount.native_login(true,false,true)")
            .unwrap();
        self.runtime.account_ui().unwrap().id
    }

    fn ready(&self) -> u64 {
        let id = self.open();
        assert!(
            self.runtime
                .submit_account_birthday(id, date(2000, 1, 2))
                .unwrap()
        );
        assert_eq!(
            self.runtime.account_ui().unwrap().view,
            AccountView::Register2
        );
        id
    }

    fn configure(&self) -> TcpListener {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        self.runtime
            .set_identity_url(&format!(
                "http://{}/proxy/identity/3.0",
                listener.local_addr().unwrap()
            ))
            .unwrap();
        self.runtime
            .set_identity_client(Some("register-client"), Some("register-signature"), None)
            .unwrap();
        self.runtime.skynest_account.seed_test_tokens(
            true,
            "register-access",
            "register-refresh",
            "register-segment",
        );
        listener
    }

    fn submit(&self, id: u64) {
        assert!(
            self.runtime
                .submit_account_registration(
                    id,
                    "person@example.test",
                    "secret123",
                    AccountGender::Female
                )
                .unwrap()
        );
        assert!(self.runtime.account_ui().unwrap().busy);
    }

    fn drain(&self) {
        dispatch_registered_application_events(self.runtime.lua()).unwrap();
    }

    fn wait_worker(&self) {
        let deadline = Instant::now() + Duration::from_secs(5);
        while self
            .runtime
            .skynest_account
            .online_completions
            .lock()
            .unwrap()
            .is_empty()
        {
            assert!(Instant::now() < deadline, "registration worker timed out");
            thread::sleep(Duration::from_millis(1));
        }
    }

    fn callbacks(&self) -> (i64, i64) {
        let env = game_environment(self.runtime.lua()).unwrap();
        (
            env.get("registration_successes").unwrap(),
            env.get("registration_failures").unwrap(),
        )
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

fn accept(listener: &TcpListener) -> (TcpStream, String) {
    let deadline = Instant::now() + Duration::from_secs(5);
    let mut stream = loop {
        match listener.accept() {
            Ok((stream, _)) => break stream,
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                assert!(Instant::now() < deadline, "registration request timed out");
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
        assert!(bytes.len() < 65536, "unexpected oversized fixture request");
        if let Some(end) = bytes.windows(4).position(|part| part == b"\r\n\r\n") {
            let headers = std::str::from_utf8(&bytes[..end]).unwrap();
            let length = headers
                .lines()
                .find_map(|line| {
                    let (key, value) = line.split_once(':')?;
                    key.eq_ignore_ascii_case("content-length")
                        .then(|| value.trim().parse::<usize>().unwrap())
                })
                .unwrap_or(0);
            if bytes.len() >= end + 4 + length {
                return (stream, String::from_utf8(bytes).unwrap());
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

const TOKENS: &str = r#"{"accessToken":"registered-access","refreshToken":"registered-refresh","expiresIn":3600,"segment":"adult"}"#;

#[test]
fn identity_registration_gregorian_and_exact_thirteenth_birthday_boundaries() {
    for value in [
        date(2000, 2, 29),
        date(2004, 2, 29),
        date(2026, 4, 30),
        date(1, 1, 1),
    ] {
        assert_eq!(birthday_error(value), None);
    }
    for value in [
        date(1900, 2, 29),
        date(2100, 2, 29),
        date(2026, 4, 31),
        date(0, 1, 1),
        date(-1, 1, 1),
        date(2026, 0, 1),
        date(2026, 13, 1),
        date(2026, 1, 0),
        date(2026, 1, u32::MAX),
    ] {
        assert!(birthday_error(value).is_some(), "{value:?}");
    }
    let born = date(2013, 9, 5);
    assert!(!at_least_thirteen(born, date(2026, 9, 4)));
    assert!(at_least_thirteen(born, date(2026, 9, 5)));
    assert!(at_least_thirteen(born, date(2026, 9, 6)));
    assert!(!at_least_thirteen(born, date(2026, 8, 31)));
    assert!(at_least_thirteen(born, date(2027, 1, 1)));
    assert!(!at_least_thirteen(date(2012, 2, 29), date(2025, 2, 28)));
    assert!(at_least_thirteen(date(2012, 2, 29), date(2025, 3, 1)));
    assert!(!at_least_thirteen(date(i32::MAX, 1, 1), date(2026, 1, 1)));
}

#[test]
fn identity_registration_date_errors_do_not_close_gate_but_underage_is_sticky() {
    let fixture = Fixture::new();
    let id = fixture.open();
    let backend = &fixture.runtime.skynest_account;
    assert!(!backend.submit_birthday_on(id + 1, date(2000, 1, 1), date(2026, 9, 5)));
    assert!(backend.submit_birthday_on(id, date(2012, 2, 30), date(2026, 9, 5)));
    assert_eq!(
        fixture.runtime.account_ui().unwrap().field_error,
        Some(AccountFieldError {
            field: 13,
            message: 7
        })
    );
    assert!(!backend.interactive.borrow().registration.blocked);
    assert!(backend.submit_birthday_on(id, date(2013, 9, 6), date(2026, 9, 5)));
    assert_eq!(
        fixture.runtime.account_ui().unwrap().view,
        AccountView::RegistrationFailure
    );
    fixture
        .runtime
        .account_ui_action(id, AccountUiAction::Back)
        .unwrap();
    assert_eq!(
        fixture.runtime.account_ui().unwrap().view,
        AccountView::RegistrationFailure
    );
    assert!(!backend.submit_birthday_on(id, date(2000, 1, 1), date(2026, 9, 5)));
    let replacement = fixture.open();
    assert_ne!(id, replacement);
    assert_eq!(
        fixture.runtime.account_ui().unwrap().view,
        AccountView::RegistrationFailure
    );
    fixture
        .runtime
        .account_ui_action(replacement, AccountUiAction::Continue)
        .unwrap();
    assert!(fixture.runtime.account_ui().is_none());
    assert_eq!(fixture.callbacks(), (0, 0));
    fixture.drain();
    fixture.drain();
    assert_eq!(fixture.callbacks(), (0, 1));
}

fn assert_underage_gate_survives_identity_reset(reset: impl FnOnce(&StellaLua)) {
    let fixture = Fixture::new();
    let old = fixture.open();
    assert!(fixture.runtime.skynest_account.submit_birthday_on(
        old,
        date(2013, 9, 6),
        date(2026, 9, 5)
    ));
    assert_eq!(
        fixture.runtime.account_ui().unwrap().view,
        AccountView::RegistrationFailure
    );
    reset(&fixture.runtime);
    assert!(fixture.runtime.account_ui().is_none());
    let current = fixture.open();
    assert!(
        current > old,
        "identity reset must not reuse an old native UI owner"
    );
    assert_eq!(
        fixture.runtime.account_ui().unwrap().view,
        AccountView::RegistrationFailure
    );
    assert!(!fixture.runtime.skynest_account.submit_birthday_on(
        current,
        date(2000, 1, 1),
        date(2026, 9, 5)
    ));
    assert!(
        fixture
            .runtime
            .skynest_account
            .interactive
            .borrow()
            .registration
            .blocked
    );
    fixture.drain();
    fixture.drain();
    assert_eq!(fixture.callbacks(), (0, 0));
    assert!(
        fixture
            .runtime
            .skynest_account
            .online_completions
            .lock()
            .unwrap()
            .is_empty()
    );
}

#[test]
fn identity_registration_underage_gate_survives_native_logout() {
    assert_underage_gate_survives_identity_reset(|runtime| {
        runtime
            .execute_source("_G.SkynestAccount.native_logout()")
            .unwrap();
    });
}

#[test]
fn identity_registration_underage_gate_survives_endpoint_and_client_reconfiguration() {
    assert_underage_gate_survives_identity_reset(|runtime| {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        runtime
            .set_identity_url(&format!(
                "http://{}/identity/3.0",
                listener.local_addr().unwrap()
            ))
            .unwrap();
    });
    assert_underage_gate_survives_identity_reset(|runtime| {
        runtime
            .set_identity_client(Some("changed-client"), Some("changed-signature"), None)
            .unwrap();
    });
}

#[test]
fn identity_registration_is_owner_scoped_and_offline_is_async_not_guest_success() {
    let fixture = Fixture::new();
    let runtime = &fixture.runtime;
    runtime.enable_local_services().unwrap();
    runtime
        .execute_source("os.date = function() error('mutable game os.date used') end")
        .unwrap();
    let today = runtime.account_calendar_today().unwrap();
    assert!(today.year >= 2026 && birthday_error(today).is_none());
    let id = fixture.open();
    assert!(
        !runtime
            .submit_account_registration(id, "a@b.c", "abcdefgh", AccountGender::Male)
            .unwrap()
    );
    runtime
        .submit_account_birthday(id, date(2000, 1, 2))
        .unwrap();
    for (email, password, field) in [("", "", 16), ("x@y.z", "", 17), ("x@y.z", "abcdefg", 17)] {
        assert!(
            runtime
                .submit_account_registration(id, email, password, AccountGender::Male)
                .unwrap()
        );
        assert_eq!(
            runtime.account_ui().unwrap().field_error.unwrap().field,
            field
        );
        assert!(!runtime.account_ui().unwrap().busy);
    }
    // Native strlen checks UTF-8 bytes, not glyphs; no email regex or trim.
    assert!(
        runtime
            .submit_account_registration(id, " ", "éééé", AccountGender::Male)
            .unwrap()
    );
    assert!(runtime.account_ui().unwrap().busy);
    assert!(
        !runtime
            .submit_account_registration(id, "x@y.z", "abcdefgh", AccountGender::Male)
            .unwrap()
    );
    assert_eq!(fixture.callbacks(), (0, 0));
    fixture.drain();
    assert_eq!(
        runtime.account_ui().unwrap().view,
        AccountView::NoNetworkConnectivity
    );
    assert_eq!(fixture.callbacks(), (0, 0));
    assert!(
        runtime
            .skynest_account
            .online_completions
            .lock()
            .unwrap()
            .is_empty()
    );
}

#[test]
fn identity_registration_2xx_tokens_wait_for_confirmation_before_profile_and_callback() {
    let fixture = Fixture::new();
    let listener = fixture.configure();
    let id = fixture.ready();
    fixture.submit(id);
    let (stream, request) = accept(&listener);
    assert!(request.starts_with("POST /proxy/identity/2.0/abid/register HTTP/1.1\r\n"));
    let lowercase = request.to_ascii_lowercase();
    assert!(lowercase.contains("x-access-token: register-access\r\n"));
    assert!(lowercase.contains("rovio-sgs: register-segment\r\n"));
    let body = request.split_once("\r\n\r\n").unwrap().1;
    assert!(body.contains("email=person%40example.test"));
    assert!(body.contains("password=secret123"));
    assert!(body.contains("birthday=2000-1-2"));
    assert!(body.contains("gender=female"));
    assert!(body.contains("locale="));
    assert!(!body.contains("persistentGuid"));
    reply(stream, 201, TOKENS);
    fixture.wait_worker();
    fixture.drain();
    assert!(fixture.runtime.account_ui().unwrap().busy);
    fixture.drain();
    assert_eq!(
        fixture.runtime.account_ui().unwrap().view,
        AccountView::ThanksForRegistering
    );
    assert_eq!(fixture.callbacks(), (0, 0));
    assert!(
        fixture
            .runtime
            .skynest_account
            .state
            .lock()
            .unwrap()
            .identity_headers()
            .0
            .is_none()
    );
    assert!(matches!(listener.accept(),Err(error) if error.kind()==std::io::ErrorKind::WouldBlock));
    assert!(
        !fixture
            .runtime
            .account_ui_action(id + 1, AccountUiAction::Continue)
            .unwrap()
    );
    fixture
        .runtime
        .account_ui_action(id, AccountUiAction::Continue)
        .unwrap();
    assert!(fixture.runtime.account_ui().is_none());
    assert_eq!(fixture.callbacks(), (0, 0));
    fixture.drain();
    let (stream, profile_request) = accept(&listener);
    assert!(profile_request.starts_with("GET /proxy/identity/3.0/profile/own HTTP/1.1"));
    assert!(
        profile_request
            .to_ascii_lowercase()
            .contains("x-access-token: registered-access\r\n")
    );
    reply(
        stream,
        200,
        r#"{"publicAccountId":"registered-person","personal":{"nickName":"New Player","email":"person@example.test"}}"#,
    );
    fixture.wait_worker();
    fixture.drain();
    assert_eq!(fixture.callbacks(), (0, 0));
    fixture.drain();
    assert_eq!(fixture.callbacks(), (1, 0));
    fixture.drain();
    assert_eq!(fixture.callbacks(), (1, 0));
}

#[test]
fn identity_registration_existing_online_guest_uses_upgrade_payload_without_auto_guest_login() {
    let fixture = Fixture::new();
    let listener = fixture.configure();
    let session = &fixture.runtime.skynest_account.session;
    assert!(
        session
            .install_profile_if_epoch(
                session.epoch(),
                &ProfileResponse {
                    avatar_assets: Vec::new(),
                    avatar_paths: Default::default(),
                    public_account_id: "online-guest-id".to_owned(),
                    personal: PersonalProfile::default(),
                    social_networks: vec![],
                    active_external_id: String::new(),
                    active_social_network: None,
                    active_social_name: String::new(),
                    connected_to_social_network: false,
                    raw: serde_json::json!({"publicAccountId":"online-guest-id"}),
                }
            )
            .unwrap()
    );
    fixture.runtime.skynest_account.seed_test_tokens(
        false,
        "online-guest-token",
        "guest-refresh",
        "guest-segment",
    );
    let id = fixture.ready();
    assert!(
        fixture
            .runtime
            .submit_account_registration(
                id,
                "raw+tag@example.test",
                "pass &+=",
                AccountGender::Male
            )
            .unwrap()
    );
    let (stream, request) = accept(&listener);
    assert!(request.starts_with("POST /proxy/identity/3.0/guest/upgrade HTTP/1.1\r\n"));
    assert!(
        request
            .to_ascii_lowercase()
            .contains("x-access-token: online-guest-token\r\n")
    );
    let body = request.split_once("\r\n\r\n").unwrap().1;
    assert!(body.contains("email=raw%2Btag%40example.test"));
    assert!(body.contains("password=pass+%26%2B%3D"));
    assert!(body.contains("gender=male"));
    let identifiers = &fixture.runtime.skynest_account.identifiers;
    let installation = identifiers.installation_id().unwrap();
    assert!(body.contains(&format!("persistentGuid={}", form_component(&installation))));
    assert!(!body.contains(&format!("persistentGuid={}", identifiers.persistent_guid)));
    reply(stream, 200, TOKENS);
    fixture.wait_worker();
    fixture.drain();
    fixture.drain();
    assert_eq!(
        fixture.runtime.account_ui().unwrap().view,
        AccountView::ThanksForRegistering
    );
    assert_eq!(fixture.callbacks(), (0, 0));
    fixture
        .runtime
        .account_ui_action(id, AccountUiAction::Cancel)
        .unwrap();
    assert!(fixture.runtime.account_ui().is_none());
    assert!(
        fixture
            .runtime
            .skynest_account
            .interactive
            .borrow()
            .registration
            .accepted
            .is_none()
    );
    fixture.drain();
    fixture.drain();
    assert_eq!(fixture.callbacks(), (0, 1));
}

#[test]
fn identity_registration_guest_confirmation_regenerates_before_callback_or_reports_failure() {
    for break_registry in [false, true] {
        let fixture = Fixture::new();
        let listener = fixture.configure();
        let session = &fixture.runtime.skynest_account.session;
        let mut guest: ProfileResponse = serde_json::from_value(serde_json::json!({
            "publicAccountId":"same-guest", "personal":{}
        }))
        .unwrap();
        guest.raw = serde_json::json!({"publicAccountId":"same-guest","personal":{}});
        session
            .install_profile_if_epoch(session.epoch(), &guest)
            .unwrap();
        fixture.runtime.skynest_account.seed_test_tokens(
            false,
            "guest-access",
            "guest-refresh",
            "guest-segment",
        );
        let identifiers = &fixture.runtime.skynest_account.identifiers;
        let old_id = identifiers.installation_id().unwrap();
        let device_guid = identifiers.persistent_guid.clone();
        let app_data = fixture
            .runtime
            .skynest_account
            .state
            .lock()
            .unwrap()
            .persistence_path
            .parent()
            .unwrap()
            .to_owned();
        let registry_path = app_data.join("stella-installation.registry");
        let id = fixture.ready();
        fixture.submit(id);
        let (stream, request) = accept(&listener);
        assert!(request.starts_with("POST /proxy/identity/3.0/guest/upgrade HTTP/1.1"));
        assert!(request.contains(&format!("persistentGuid={}", form_component(&old_id))));
        reply(stream, 200, TOKENS);
        fixture.wait_worker();
        fixture.drain();
        fixture.drain();
        assert_eq!(
            fixture.runtime.account_ui().unwrap().view,
            AccountView::ThanksForRegistering
        );
        assert_eq!(fixture.callbacks(), (0, 0));
        assert_eq!(identifiers.installation_id().unwrap(), old_id);
        fixture
            .runtime
            .account_ui_action(id, AccountUiAction::Continue)
            .unwrap();
        fixture.drain();
        let (stream, request) = accept(&listener);
        assert!(request.starts_with("GET /proxy/identity/3.0/profile/own HTTP/1.1"));
        assert_eq!(identifiers.installation_id().unwrap(), old_id);
        assert_eq!(session.level2_tokens().access_token, "guest-access");
        if break_registry {
            std::fs::write(&registry_path, b"synthetic damaged registry").unwrap();
        }
        reply(
            stream,
            200,
            r#"{"publicAccountId":"same-guest","personal":{"email":"person@example.test"}}"#,
        );
        fixture.wait_worker();
        // Worker publication/rotation precedes deferred Lua success or failure.
        assert_eq!(fixture.callbacks(), (0, 0));
        assert_eq!(session.level2_tokens().access_token, "registered-access");
        assert_eq!(
            session.profile().unwrap().personal.email,
            "person@example.test"
        );
        if break_registry {
            assert!(identifiers.installation_id().is_err());
            assert_eq!(
                std::fs::read(&registry_path).unwrap(),
                b"synthetic damaged registry"
            );
            assert!(
                fixture
                    .runtime
                    .skynest_account
                    .pop_session_success()
                    .is_none()
            );
        } else {
            let current = identifiers.installation_id().unwrap();
            assert_ne!(current, old_id);
            let reopened = super::super::super::identifiers::Identifiers::for_app_data(
                &device_guid,
                &app_data,
            );
            assert_eq!(reopened.installation_id().unwrap(), current);
            assert_eq!(reopened.persistent_guid, device_guid);
        }
        fixture.drain();
        assert_eq!(fixture.callbacks(), (0, 0));
        fixture.drain();
        assert_eq!(
            fixture.callbacks(),
            if break_registry { (0, 1) } else { (1, 0) }
        );
        fixture.drain();
        assert_eq!(
            fixture.callbacks(),
            if break_registry { (0, 1) } else { (1, 0) }
        );
    }
}

#[test]
fn identity_registration_http_errors_and_empty_tokens_follow_native_states() {
    for (status, body, view, blocked) in [
        (0, "", AccountView::NoNetworkConnectivity, false),
        (400, "bad", AccountView::Register2, false),
        (412, "bad", AccountView::Register2, false),
        (451, "blocked", AccountView::RegistrationFailure, true),
        (500, "error", AccountView::RegistrationFailure, false),
        (302, "redirect", AccountView::RegistrationFailure, false),
        (204, "", AccountView::NoNetworkConnectivity, false),
        (200, "not JSON", AccountView::NoNetworkConnectivity, false),
        (
            200,
            r#"{"accessToken":"","refreshToken":"r","expiresIn":1}"#,
            AccountView::NoNetworkConnectivity,
            false,
        ),
        (
            200,
            r#"{"accessToken":"a","refreshToken":"","expiresIn":1}"#,
            AccountView::NoNetworkConnectivity,
            false,
        ),
    ] {
        let fixture = Fixture::new();
        let listener = fixture.configure();
        let id = fixture.ready();
        fixture.submit(id);
        let (stream, _) = accept(&listener);
        if status == 0 {
            // Actual loopback transport failure, not a synthetic -1 result.
            drop(stream);
        } else {
            reply(stream, status, body);
        }
        fixture.wait_worker();
        fixture.drain();
        assert!(fixture.runtime.account_ui().unwrap().busy, "{status}");
        fixture.drain();
        let ui = fixture.runtime.account_ui().unwrap();
        assert_eq!(ui.view, view, "{status} {body}");
        assert!(!ui.busy);
        assert_eq!(
            ui.field_error,
            if status == 400 || status == 412 {
                Some(AccountFieldError {
                    field: 16,
                    message: 1,
                })
            } else {
                None
            }
        );
        assert_eq!(
            fixture
                .runtime
                .skynest_account
                .interactive
                .borrow()
                .registration
                .blocked,
            blocked
        );
        assert_eq!(fixture.callbacks(), (0, 0));
        assert!(
            fixture
                .runtime
                .skynest_account
                .state
                .lock()
                .unwrap()
                .identity_headers()
                .0
                .is_none()
        );
    }
}

#[test]
fn identity_registration_same_owner_new_request_ignores_old_http_451() {
    let fixture = Fixture::new();
    let listener = fixture.configure();
    let id = fixture.ready();
    fixture.submit(id);
    let (old_stream, _) = accept(&listener);
    let old_request = fixture
        .runtime
        .skynest_account
        .interactive
        .borrow()
        .registration
        .request;
    fixture
        .runtime
        .account_ui_action(id, AccountUiAction::Back)
        .unwrap();
    fixture
        .runtime
        .submit_account_birthday(id, date(2000, 1, 2))
        .unwrap();
    fixture.submit(id);
    let (new_stream, _) = accept(&listener);
    assert_ne!(
        fixture
            .runtime
            .skynest_account
            .interactive
            .borrow()
            .registration
            .request,
        old_request
    );
    reply(old_stream, 451, "old request denied");
    fixture.wait_worker();
    fixture.drain();
    assert!(fixture.runtime.account_ui().unwrap().busy);
    assert!(
        !fixture
            .runtime
            .skynest_account
            .interactive
            .borrow()
            .registration
            .blocked
    );
    reply(new_stream, 200, TOKENS);
    fixture.wait_worker();
    fixture.drain();
    fixture.drain();
    assert_eq!(
        fixture.runtime.account_ui().unwrap().view,
        AccountView::ThanksForRegistering
    );
    assert!(
        !fixture
            .runtime
            .skynest_account
            .interactive
            .borrow()
            .registration
            .blocked
    );
    assert_eq!(fixture.callbacks(), (0, 0));
}

#[test]
fn identity_registration_cancel_and_replaced_request_discard_stale_worker_and_posted_results() {
    for after_worker in [false, true] {
        let fixture = Fixture::new();
        let listener = fixture.configure();
        let old = fixture.ready();
        fixture.submit(old);
        let (stream, _) = accept(&listener);
        if after_worker {
            reply(stream, 200, TOKENS);
            fixture.wait_worker();
            fixture.drain();
        } else {
            fixture
                .runtime
                .account_ui_action(old, AccountUiAction::Cancel)
                .unwrap();
            reply(stream, 200, TOKENS);
            fixture.wait_worker();
        }
        let replacement = fixture.ready();
        assert_ne!(replacement, old);
        fixture.drain();
        fixture.drain();
        fixture.drain();
        let ui = fixture.runtime.account_ui().unwrap();
        assert_eq!(ui.id, replacement);
        assert_eq!(ui.view, AccountView::Register2);
        assert!(!ui.busy);
        assert_eq!(
            fixture.callbacks(),
            if after_worker { (0, 0) } else { (0, 1) }
        );
        assert!(
            fixture
                .runtime
                .skynest_account
                .state
                .lock()
                .unwrap()
                .identity_headers()
                .0
                .is_none()
        );
        assert!(
            fixture
                .runtime
                .skynest_account
                .interactive
                .borrow()
                .registration
                .accepted
                .is_none()
        );
    }
}

#[test]
fn identity_native_logout_prevents_late_session_worker_and_queued_completion_revival() {
    for after_worker in [false, true] {
        let fixture = Fixture::new();
        let listener = fixture.configure();
        fixture
            .runtime
            .execute_source("_G.SkynestAccount.native_login(false,false,false)")
            .unwrap();
        let (stream, request) = accept(&listener);
        assert!(
            request.starts_with("POST /proxy/session/1/apps/register-client/sessions HTTP/1.1\r\n")
        );
        let body = r#"{"userAuth":{"accessToken":"late-access","refreshToken":"late-refresh","expiresIn":3600},"segments":[5],"profile":{"publicAccountId":"late-account","personal":{"nickName":"Late Player","email":"late@example.invalid"}},"config":{}}"#;
        if after_worker {
            reply(stream, 200, body);
            fixture.wait_worker();
            fixture
                .runtime
                .execute_source("_G.SkynestAccount.native_logout()")
                .unwrap();
        } else {
            fixture
                .runtime
                .execute_source("_G.SkynestAccount.native_logout()")
                .unwrap();
            reply(stream, 200, body);
            fixture.wait_worker();
        }
        fixture.drain();
        fixture.drain();
        assert_eq!(
            fixture.callbacks(),
            (0, 0),
            "stage after_worker={after_worker}"
        );
        let state = fixture.runtime.skynest_account.state.lock().unwrap();
        assert!(!state.logged_in);
        assert!(!state.login_in_progress);
        assert_eq!(state.identity_headers(), (None, None));
        drop(state);
        assert!(fixture.runtime.skynest_account.session.profile().is_none());
        assert!(fixture.runtime.account_ui().is_none());
    }
}
