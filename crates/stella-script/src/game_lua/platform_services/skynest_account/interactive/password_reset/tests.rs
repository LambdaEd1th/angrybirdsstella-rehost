//! Loopback-only transport and scheduler proofs, with isolated device storage.

use super::*;
use std::{
    io::Write,
    net::{TcpListener, TcpStream},
    sync::atomic::{AtomicU64, Ordering},
    thread,
    time::Instant,
};

struct Fixture {
    runtime: StellaLua,
    root: PathBuf,
}

impl Fixture {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(1);
        let root = std::env::temp_dir().join(format!(
            "stella-reset-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        let data = root.join("data");
        std::fs::create_dir_all(&data).unwrap();
        let runtime = StellaLua::new(data).unwrap();
        runtime
            .execute_source(
                r#"
                reset_successes, reset_failures = 0, 0
                _G.SkynestAccount.onLoginSuccess = function()
                    reset_successes = reset_successes + 1
                end
                _G.SkynestAccount.onLoginFailure = function(code)
                    reset_failures = reset_failures + 1
                    reset_failure_code = code
                end
                "#,
            )
            .unwrap();
        Self { runtime, root }
    }

    fn open(&self, register: bool) -> u64 {
        self.runtime
            .execute_source(&format!(
                "_G.SkynestAccount.native_login(true, false, {register})"
            ))
            .unwrap();
        let id = self.runtime.account_ui().unwrap().id;
        self.runtime
            .account_ui_action(id, AccountUiAction::ForgotPassword)
            .unwrap();
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
            .set_identity_client(Some("reset-client"), Some("reset-signature"), None)
            .unwrap();
        self.runtime.skynest_account.seed_test_tokens(
            true,
            "reset-access",
            "reset-refresh",
            "reset-segment",
        );
        listener
    }

    fn drain(&self) {
        dispatch_registered_application_events(self.runtime.lua()).unwrap();
    }

    fn wait_worker(&self) {
        // Wait for the actual worker payload, not an assumed socket timing.
        // The worker posts its scheduler event before releasing this lock.
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            if !self
                .runtime
                .skynest_account
                .online_completions
                .lock()
                .unwrap()
                .is_empty()
            {
                break;
            }
            assert!(Instant::now() < deadline, "reset worker timed out");
            thread::sleep(Duration::from_millis(1));
        }
    }

    fn callback_counts(&self) -> (i64, i64) {
        let env = game_environment(self.runtime.lua()).unwrap();
        (
            env.get("reset_successes").unwrap(),
            env.get("reset_failures").unwrap(),
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
                assert!(Instant::now() < deadline, "missing reset request");
                thread::sleep(Duration::from_millis(1));
            }
            Err(error) => panic!("reset accept failed: {error}"),
        }
    };
    stream.set_nonblocking(false).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(5)))
        .unwrap();
    let mut request = Vec::new();
    let mut chunk = [0; 2048];
    loop {
        let count = stream.read(&mut chunk).unwrap();
        assert_ne!(count, 0, "incomplete reset request");
        request.extend_from_slice(&chunk[..count]);
        if let Some(end) = request.windows(4).position(|part| part == b"\r\n\r\n") {
            let headers = std::str::from_utf8(&request[..end]).unwrap();
            let length = headers
                .lines()
                .find_map(|line| {
                    let (key, value) = line.split_once(':')?;
                    key.eq_ignore_ascii_case("content-length")
                        .then(|| value.trim().parse::<usize>().unwrap())
                })
                .unwrap_or(0);
            if request.len() >= end + 4 + length {
                return (stream, String::from_utf8(request).unwrap());
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

#[test]
fn identity_password_reset_locale_has_only_native_exact_substitutions() {
    for (input, expected) in [
        ("pt", "pt_BR"),
        ("zh-Hans", "zh_CN"),
        ("zh-Hant", "zh_TW"),
        ("zh-Hans-CN", "zh-Hans-CN"),
        ("pt-PT", "pt-PT"),
        ("en_US", "en_US"),
        ("fr", "fr"),
    ] {
        assert_eq!(password_reset_locale(input), expected);
    }
}

#[test]
fn identity_password_reset_requires_current_form_and_never_invents_offline_success() {
    let fixture = Fixture::new();
    let runtime = &fixture.runtime;
    runtime.enable_local_services().unwrap();
    assert!(!runtime.submit_account_password_reset(1, "x@y.z").unwrap());
    let id = fixture.open(true);
    for email in ["", " ", "\r\n\t", "\u{2003}"] {
        assert!(runtime.submit_account_password_reset(id, email).unwrap());
        let ui = runtime.account_ui().unwrap();
        assert_eq!(ui.view, AccountView::ForgotPassword);
        assert!(!ui.busy);
        assert_eq!(
            ui.field_error,
            Some(AccountFieldError {
                field: 15,
                message: 1
            })
        );
        assert!(
            runtime
                .skynest_account
                .online_completions
                .lock()
                .unwrap()
                .is_empty()
        );
    }
    assert!(
        !runtime
            .submit_account_password_reset(id + 1, "x@y.z")
            .unwrap()
    );
    assert!(runtime.submit_account_password_reset(id, "x@y.z").unwrap());
    assert!(runtime.account_ui().unwrap().busy);
    assert!(
        !runtime
            .submit_account_password_reset(id, "duplicate@y.z")
            .unwrap()
    );
    fixture.drain();
    let ui = runtime.account_ui().unwrap();
    assert_eq!(ui.view, AccountView::NoNetworkConnectivity);
    assert!(!ui.busy);
    assert_eq!(ui.field_error, None);
    assert_eq!(fixture.callback_counts(), (0, 0));
    runtime
        .account_ui_action(id, AccountUiAction::Continue)
        .unwrap();
    assert_eq!(runtime.account_ui().unwrap().view, AccountView::Register1);
    assert!(!runtime.submit_account_password_reset(id, "x@y.z").unwrap());
}

#[test]
fn identity_password_reset_native_form_headers_empty_2xx_and_posted_confirmation() {
    for (status, body, register) in [(201, "not JSON", false), (204, "", true)] {
        let fixture = Fixture::new();
        let runtime = &fixture.runtime;
        let listener = fixture.configure();
        runtime.skynest_account.seed_test_tokens(
            false,
            "existing-token",
            "existing-refresh",
            "existing-segment",
        );
        let id = fixture.open(register);
        let email = "  stella+reset@example.invalid  ";
        assert!(runtime.submit_account_password_reset(id, email).unwrap());
        assert!(runtime.account_ui().unwrap().busy);
        let (stream, request) = accept(&listener);
        assert_eq!(
            request.lines().next(),
            Some("POST /proxy/identity/2.0/abid/reset/password HTTP/1.1")
        );
        let headers = request
            .split_once("\r\n\r\n")
            .unwrap()
            .0
            .to_ascii_lowercase();
        assert!(headers.contains("\r\ncontent-type: application/x-www-form-urlencoded"));
        assert!(headers.contains("\r\nx-access-token: reset-access"));
        assert!(headers.contains("\r\nrovio-sgs: reset-segment"));
        let language = crate::preferred_languages::host_preferred_languages()
            .into_iter()
            .next()
            .unwrap_or_else(|| "en".to_owned());
        assert_eq!(
            request.split_once("\r\n\r\n").unwrap().1,
            form_body(&[
                ("email", email.to_owned()),
                ("locale", password_reset_locale(&language).to_owned())
            ])
        );
        assert!(!request.contains("clientId="));
        assert!(!request.contains("password="));
        reply(stream, status, body);
        fixture.wait_worker();
        assert!(runtime.account_ui().unwrap().busy);
        fixture.drain();
        // The worker completion posts a retained UI closure for the next drain.
        assert!(runtime.account_ui().unwrap().busy);
        assert_eq!(
            runtime.account_ui().unwrap().view,
            AccountView::ForgotPassword
        );
        fixture.drain();
        let ui = runtime.account_ui().unwrap();
        assert_eq!(ui.view, AccountView::PasswordResetEmailSent);
        assert!(!ui.busy);
        assert_eq!(ui.field_error, None);
        assert_eq!(fixture.callback_counts(), (0, 0));
        assert!(!runtime.skynest_account.state.lock().unwrap().logged_in);
        assert_eq!(
            runtime
                .skynest_account
                .state
                .lock()
                .unwrap()
                .identity_headers()
                .0
                .as_deref(),
            Some("existing-token")
        );
        assert!(
            matches!(listener.accept(), Err(error) if error.kind() == std::io::ErrorKind::WouldBlock)
        );
        runtime
            .account_ui_action(id, AccountUiAction::Continue)
            .unwrap();
        assert_eq!(
            runtime.account_ui().unwrap().view,
            if register {
                AccountView::Register1
            } else {
                AccountView::SignIn
            }
        );
    }
}

#[test]
fn identity_password_reset_http_errors_and_truncated_response_do_not_send_success() {
    for status in [400, 401, 404, 412, 500, 302] {
        let fixture = Fixture::new();
        let runtime = &fixture.runtime;
        let listener = fixture.configure();
        let id = fixture.open(false);
        runtime.submit_account_password_reset(id, "x@y.z").unwrap();
        let (mut stream, _) = accept(&listener);
        if status == 302 {
            let other = TcpListener::bind("127.0.0.1:0").unwrap();
            other.set_nonblocking(true).unwrap();
            write!(stream, "HTTP/1.1 302 Found\r\nLocation: http://{}/credential-sink\r\nContent-Length: 0\r\nConnection: close\r\n\r\n", other.local_addr().unwrap()).unwrap();
            drop(stream);
            fixture.wait_worker();
            assert!(
                matches!(other.accept(), Err(error) if error.kind() == std::io::ErrorKind::WouldBlock)
            );
        } else {
            reply(stream, status, "do not expose this response");
            if status == 401 {
                // The common request layer renews this route's parent L1
                // once, then preserves a second 401 as the form error.
                let (stream, renewal) = accept(&listener);
                assert!(renewal.starts_with("POST /proxy/identity/2.0/access HTTP/1.1\r\n"));
                reply(
                    stream,
                    200,
                    r#"{"accessToken":"renewed-reset","refreshToken":"renewed-refresh","expiresIn":3600,"segment":"renewed-segment"}"#,
                );
                let (stream, replay) = accept(&listener);
                assert!(
                    replay.starts_with("POST /proxy/identity/2.0/abid/reset/password HTTP/1.1\r\n")
                );
                assert!(
                    replay
                        .to_ascii_lowercase()
                        .contains("x-access-token: renewed-reset\r\n")
                );
                assert!(
                    replay
                        .to_ascii_lowercase()
                        .contains("rovio-sgs: renewed-segment\r\n")
                );
                reply(stream, 401, "still denied");
            }
            fixture.wait_worker();
        }
        fixture.drain();
        fixture.drain();
        let ui = runtime.account_ui().unwrap();
        assert_eq!(ui.view, AccountView::ForgotPassword, "HTTP {status}");
        assert!(!ui.busy);
        assert_eq!(
            ui.field_error,
            Some(AccountFieldError {
                field: 15,
                message: 1
            })
        );
        assert_eq!(fixture.callback_counts(), (0, 0));
        assert!(
            runtime
                .submit_account_password_reset(id, "retry@y.z")
                .unwrap()
        );
        let (stream, _) = accept(&listener);
        reply(stream, 204, "");
        fixture.wait_worker();
        fixture.drain();
        fixture.drain();
        assert_eq!(
            runtime.account_ui().unwrap().view,
            AccountView::PasswordResetEmailSent
        );
    }

    let fixture = Fixture::new();
    let listener = fixture.configure();
    let id = fixture.open(false);
    fixture
        .runtime
        .submit_account_password_reset(id, "x@y.z")
        .unwrap();
    let (mut stream, _) = accept(&listener);
    stream
        .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 20\r\nConnection: close\r\n\r\nx")
        .unwrap();
    drop(stream);
    fixture.wait_worker();
    fixture.drain();
    assert!(fixture.runtime.account_ui().unwrap().busy);
    fixture.drain();
    assert_eq!(
        fixture.runtime.account_ui().unwrap().view,
        AccountView::NoNetworkConnectivity
    );
    assert_eq!(fixture.callback_counts(), (0, 0));
}

#[test]
fn identity_password_reset_cancel_and_replaced_owner_discard_late_worker_results() {
    for replace in [false, true] {
        let fixture = Fixture::new();
        let runtime = &fixture.runtime;
        let listener = fixture.configure();
        let old_id = fixture.open(false);
        runtime
            .submit_account_password_reset(old_id, "x@y.z")
            .unwrap();
        let (stream, _) = accept(&listener);
        let new_id = if replace {
            Some(fixture.open(true))
        } else {
            runtime
                .account_ui_action(old_id, AccountUiAction::Cancel)
                .unwrap();
            assert!(runtime.account_ui().is_none());
            fixture.drain();
            assert_eq!(fixture.callback_counts(), (0, 0));
            fixture.drain();
            assert_eq!(fixture.callback_counts(), (0, 1));
            None
        };
        reply(stream, 204, "");
        fixture.wait_worker();
        fixture.drain();
        fixture.drain();
        if let Some(new_id) = new_id {
            assert_ne!(new_id, old_id);
            let current = runtime.account_ui().unwrap();
            assert_eq!(current.id, new_id);
            assert_eq!(current.view, AccountView::ForgotPassword);
            assert!(!current.busy);
            assert_eq!(fixture.callback_counts(), (0, 0));
        } else {
            assert!(runtime.account_ui().is_none());
            assert_eq!(fixture.callback_counts(), (0, 1));
            let env = game_environment(runtime.lua()).unwrap();
            assert_eq!(
                env.get::<String>("reset_failure_code").unwrap(),
                "ERROR_USER_CANCELLED_LOGIN"
            );
        }
        assert!(
            !runtime
                .submit_account_password_reset(old_id + 100, "x@y.z")
                .unwrap()
        );
    }
}

#[test]
fn identity_password_reset_same_owner_new_request_rejects_both_stale_stages() {
    for queue_old_ui_closure in [false, true] {
        let fixture = Fixture::new();
        let runtime = &fixture.runtime;
        let listener = fixture.configure();
        let id = fixture.open(false);
        runtime
            .submit_account_password_reset(id, "old@y.z")
            .unwrap();
        let (old_stream, _) = accept(&listener);
        // Test both a late worker and an already posted success UI closure.
        let old_stream = if queue_old_ui_closure {
            reply(old_stream, 204, "");
            fixture.wait_worker();
            fixture.drain();
            assert!(runtime.account_ui().unwrap().busy);
            None
        } else {
            Some(old_stream)
        };
        runtime
            .account_ui_action(id, AccountUiAction::Back)
            .unwrap();
        runtime
            .account_ui_action(id, AccountUiAction::ForgotPassword)
            .unwrap();
        runtime
            .submit_account_password_reset(id, "new@y.z")
            .unwrap();
        let (new_stream, request) = accept(&listener);
        assert!(request.contains("email=new%40y.z"));
        if let Some(stream) = old_stream {
            reply(stream, 204, "");
            fixture.wait_worker();
        }
        fixture.drain();
        fixture.drain();
        let current = runtime.account_ui().unwrap();
        assert_eq!(current.id, id);
        assert_eq!(current.view, AccountView::ForgotPassword);
        assert!(
            current.busy,
            "old completion must not finish the new request"
        );
        reply(new_stream, 404, "new request fails");
        fixture.wait_worker();
        fixture.drain();
        fixture.drain();
        let current = runtime.account_ui().unwrap();
        assert_eq!(current.view, AccountView::ForgotPassword);
        assert!(!current.busy);
        assert_eq!(
            current.field_error,
            Some(AccountFieldError {
                field: 15,
                message: 1
            })
        );
        assert_eq!(fixture.callback_counts(), (0, 0));
    }
}
