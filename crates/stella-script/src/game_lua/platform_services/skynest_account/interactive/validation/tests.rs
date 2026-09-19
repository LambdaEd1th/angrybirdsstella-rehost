//! Isolated native editing checks and loopback-only request completions.

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
            "stella-validation-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        let data = root.join("data");
        fs::create_dir_all(&data).unwrap();
        let runtime = StellaLua::new(data).unwrap();
        runtime.execute_source(r#"
            validation_successes, validation_failures = 0, 0
            _G.SkynestAccount.onLoginSuccess = function() validation_successes = validation_successes + 1 end
            _G.SkynestAccount.onLoginFailure = function() validation_failures = validation_failures + 1 end
        "#).unwrap();
        Self { runtime, root }
    }
    fn open(&self) -> u64 {
        self.runtime
            .execute_source("_G.SkynestAccount.native_login(true,false,false)")
            .unwrap();
        self.runtime.account_ui().unwrap().id
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
            .set_identity_client(Some("validate-client"), Some("validate-signature"), None)
            .unwrap();
        self.runtime.skynest_account.seed_test_tokens(
            true,
            "validate-access",
            "validate-refresh",
            "validate-segment",
        );
        self.runtime.skynest_account.seed_test_tokens(
            false,
            "validation-l2-access",
            "validation-l2-refresh",
            "validation-l2-segment",
        );
        listener
    }
    fn validate(&self, id: u64, generation: u64, email: &str) {
        assert!(
            self.runtime
                .validate_account_field(id, AccountValidationField::Email, generation, email)
                .unwrap()
        );
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
            assert!(Instant::now() < deadline, "validation worker timed out");
            thread::sleep(Duration::from_millis(1));
        }
    }
    fn results(&self) -> Vec<AccountValidationResult> {
        self.runtime.take_account_validation_results()
    }
    fn assert_no_login(&self) {
        let env = game_environment(self.runtime.lua()).unwrap();
        assert_eq!(env.get::<i64>("validation_successes").unwrap(), 0);
        assert_eq!(env.get::<i64>("validation_failures").unwrap(), 0);
        assert!(!self.runtime.skynest_account.state.lock().unwrap().logged_in);
        assert!(self.runtime.skynest_account.session.profile().is_none());
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

fn accept(listener: &TcpListener) -> (TcpStream, String) {
    let deadline = Instant::now() + Duration::from_secs(5);
    let mut stream = loop {
        match listener.accept() {
            Ok((stream, _)) => break stream,
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                assert!(Instant::now() < deadline, "validation request timed out");
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
        assert!(bytes.len() < 65536);
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
fn error(field: u32, message: u32) -> Option<AccountFieldError> {
    Some(AccountFieldError { field, message })
}

#[test]
fn identity_validation_email_exact_ascii_last_separator_and_length_contract() {
    for email in [
        "a@b.c",
        "a@.",
        "a@@.",
        "a..@..",
        "a@b.c.",
        "!#$%&'*+-/=?^_`{|}~@.",
    ] {
        assert!(native_email_syntax(email), "{email}");
    }
    for email in [
        "", "@.", "a.b@c", "a@b", "a@b.c@", " a@b.c", "a@b.c ", "é@b.c", "a@b.c\n", "a\0@b.c",
    ] {
        assert!(!native_email_syntax(email), "{email:?}");
    }
    assert!(native_email_syntax(&format!("{}@.", "a".repeat(254))));
    assert!(!native_email_syntax(&format!("{}@.", "a".repeat(255))));
    let permitted =
        b"abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789!#$%&'*+-/=?^_`{|}~.@";
    for byte in 0..=127u8 {
        let value = format!("x{}@.", char::from(byte));
        assert_eq!(
            native_email_syntax(&value),
            permitted.contains(&byte),
            "ASCII {byte}"
        );
    }
}

#[test]
fn identity_validation_json_code_has_native_number_type_and_low_word_semantics() {
    for (body, expected) in [
        (r#"{"code":0}"#, Some(0)),
        (r#"{"code":10}"#, Some(10)),
        (r#"{"code":3.9}"#, Some(3)),
        (r#"{"code":4294967296}"#, Some(0)),
        (r#"{"code":4294967306}"#, Some(10)),
        (r#"{"code":-1}"#, Some(u32::MAX)),
        (r#"{"code":1e100}"#, Some(u32::MAX)),
        (r#"{"code":-1e100}"#, Some(0)),
        (r#"{"code":9223372036854775808}"#, None),
        (r#"{"code":"0"}"#, None),
        (r#"{"code":null}"#, None),
        (r#"{"code":true}"#, None),
        (r#"{}"#, None),
        (r#"[]"#, None),
    ] {
        assert_eq!(
            native_validation_code(&serde_json::from_str(body).unwrap()),
            expected,
            "{body}"
        );
    }
}

#[test]
fn identity_validation_feedback_preserves_error_then_noop_validity_callbacks() {
    for (view, code, expected, valid) in [
        (AccountView::SignIn, 0, error(18, 3), true),
        (AccountView::SignIn, 1, error(18, 1), false),
        (AccountView::SignIn, 2, None, true),
        (AccountView::SignIn, 3, error(18, 3), false),
        (AccountView::SignIn, 4, error(18, 3), false),
        (AccountView::Register2, 0, None, true),
        (AccountView::Register2, 1, error(16, 1), false),
        (AccountView::Register2, 2, error(16, 2), false),
        (AccountView::Register2, 3, error(16, 1), false),
        (AccountView::Register2, 4, error(16, 1), false),
        (AccountView::ForgotPassword, 0, error(15, 1), true),
        (AccountView::ForgotPassword, 1, error(15, 1), false),
        (AccountView::ForgotPassword, 2, error(15, 2), true),
        (AccountView::ForgotPassword, 3, error(15, 1), false),
        (AccountView::ForgotPassword, 4, error(15, 1), false),
        (AccountView::Help1, 0, None, true),
        (AccountView::Help1, 2, None, false),
    ] {
        assert_eq!(
            email_feedback(view, code),
            (expected, valid),
            "{view:?} {code}"
        );
    }
    assert_eq!(
        password_feedback(AccountView::SignIn, "abc"),
        (error(19, 6), false)
    );
    assert_eq!(
        password_feedback(AccountView::Register2, "abc"),
        (error(17, 4), false)
    );
    assert_eq!(
        password_feedback(AccountView::Register2, ""),
        (error(17, 5), false)
    );
    assert_eq!(
        password_feedback(AccountView::Help1, "abc"),
        (error(23, 4), false)
    );
    assert_eq!(password_feedback(AccountView::SignIn, "éééé"), (None, true));
}

#[test]
fn identity_validation_empty_timers_skip_and_password_feedback_is_local_not_submit_popup() {
    let fixture = Fixture::new();
    let listener = fixture.configure();
    let id = fixture.open();
    for field in [
        AccountValidationField::Email,
        AccountValidationField::Password,
    ] {
        assert!(
            !fixture
                .runtime
                .validate_account_field(id, field, 1, "")
                .unwrap()
        );
        assert!(
            !fixture
                .runtime
                .validate_account_field(id + 1, field, 1, "abc")
                .unwrap()
        );
    }
    assert!(
        !fixture
            .runtime
            .invalidate_account_validation(id + 1, AccountValidationField::Email, 1)
            .unwrap()
    );
    assert!(
        fixture
            .runtime
            .validate_account_field(id, AccountValidationField::Password, 2, "abc")
            .unwrap()
    );
    assert_eq!(
        fixture.results(),
        vec![AccountValidationResult {
            id,
            view: AccountView::SignIn,
            field: AccountValidationField::Password,
            generation: 2,
            error: error(19, 6),
            valid: false
        }]
    );
    assert!(!fixture.runtime.account_ui().unwrap().busy);
    assert_eq!(fixture.runtime.account_ui().unwrap().field_error, None);
    assert!(matches!(listener.accept(),Err(error) if error.kind()==std::io::ErrorKind::WouldBlock));
    fixture.assert_no_login();
}

#[test]
fn identity_validation_public_text_uses_native_first_nul_cache_boundary() {
    let fixture = Fixture::new();
    let listener = fixture.configure();
    let id = fixture.open();
    for field in [
        AccountValidationField::Email,
        AccountValidationField::Password,
    ] {
        assert!(
            !fixture
                .runtime
                .validate_account_field(id, field, 1, "\0ignored text")
                .unwrap()
        );
    }
    assert!(
        fixture
            .runtime
            .validate_account_field(id, AccountValidationField::Password, 2, "abc\0long suffix")
            .unwrap()
    );
    assert_eq!(fixture.results()[0].error, error(19, 6));
    fixture.validate(id, 3, "raw+tag@example.test.\0not-an-email");
    let (stream, request) = accept(&listener);
    assert_eq!(
        request.split_once("\r\n\r\n").unwrap().1,
        "email=raw%2Btag%40example.test."
    );
    reply(stream, 200, r#"{"code":10}"#);
    fixture.wait_worker();
    fixture.drain();
    fixture.drain();
    let results = fixture.results();
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].generation, 3);
    assert_eq!(results[0].error, None);
    assert!(results[0].valid);
    fixture.assert_no_login();
}

#[test]
fn identity_validation_local_email_failure_is_async_and_absent_provider_never_guesses_valid() {
    let fixture = Fixture::new();
    let id = fixture.open();
    fixture.runtime.enable_local_services().unwrap();
    fixture.validate(id, 1, "no-space @example.test");
    assert!(fixture.results().is_empty());
    fixture.drain();
    let result = fixture.results();
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].error, error(18, 1));
    assert_eq!(
        fixture.runtime.account_ui().unwrap().view,
        AccountView::SignIn
    );
    fixture.validate(id, 2, "x@example.test");
    assert_eq!(
        fixture.runtime.account_ui().unwrap().view,
        AccountView::SignIn
    );
    fixture.drain();
    assert_eq!(
        fixture.runtime.account_ui().unwrap().view,
        AccountView::NoNetworkConnectivity
    );
    assert!(fixture.results().is_empty());
    fixture.assert_no_login();
}

#[test]
fn identity_validation_email_form_2xx_and_server_codes_reach_original_mappings() {
    for (code, expected) in [
        (0, error(18, 3)),
        (1, error(18, 1)),
        (2, error(18, 1)),
        (3, error(18, 3)),
        (4, error(18, 3)),
        (10, None),
        (99, error(18, 1)),
    ] {
        let fixture = Fixture::new();
        let listener = fixture.configure();
        let id = fixture.open();
        fixture.validate(id, code as u64, "raw+tag@example.test.");
        let (stream, request) = accept(&listener);
        assert!(request.starts_with("POST /proxy/identity/2.0/abid/validate/email HTTP/1.1\r\n"));
        assert!(
            request
                .to_ascii_lowercase()
                .contains("rovio-sgs: validate-segment\r\n")
        );
        assert!(
            request
                .to_ascii_lowercase()
                .contains("x-access-token: validate-access\r\n")
        );
        assert_eq!(
            request.split_once("\r\n\r\n").unwrap().1,
            "email=raw%2Btag%40example.test."
        );
        reply(stream, 201, &format!("{{\"code\":{code}}}"));
        fixture.wait_worker();
        fixture.drain();
        assert!(fixture.results().is_empty());
        fixture.drain();
        let results = fixture.results();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].error, expected, "server {code}");
        assert_eq!(results[0].generation, code as u64);
        assert_eq!(
            fixture.runtime.account_ui().unwrap().view,
            AccountView::SignIn
        );
        assert_eq!(fixture.runtime.account_ui().unwrap().field_error, None);
        assert!(!fixture.runtime.account_ui().unwrap().busy);
        fixture.assert_no_login();
    }
}

#[test]
fn identity_validation_all_http_parse_and_transport_exceptions_show_network_page() {
    for (status, body) in [
        (0, ""),
        (400, "{}"),
        (412, "{}"),
        (451, "{}"),
        (500, "{}"),
        (204, ""),
        (200, "{}"),
        (200, r#"{"code":"0"}"#),
        (200, "not JSON"),
    ] {
        let fixture = Fixture::new();
        let listener = fixture.configure();
        let id = fixture.open();
        fixture.validate(id, 1, "x@example.test");
        let (stream, _) = accept(&listener);
        if status == 0 {
            drop(stream);
        } else {
            reply(stream, status, body);
        }
        fixture.wait_worker();
        fixture.drain();
        assert_eq!(
            fixture.runtime.account_ui().unwrap().view,
            AccountView::SignIn
        );
        fixture.drain();
        assert_eq!(
            fixture.runtime.account_ui().unwrap().view,
            AccountView::NoNetworkConnectivity,
            "{status}"
        );
        assert!(fixture.results().is_empty());
        fixture.assert_no_login();
    }
}

#[test]
fn identity_validation_does_not_follow_server_redirects() {
    let fixture = Fixture::new();
    let listener = fixture.configure();
    let id = fixture.open();
    let target = TcpListener::bind("127.0.0.1:0").unwrap();
    target.set_nonblocking(true).unwrap();
    fixture.validate(id, 1, "x@example.test");
    let (mut stream, _) = accept(&listener);
    write!(stream,"HTTP/1.1 302 Found\r\nLocation: http://{}/stolen\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",target.local_addr().unwrap()).unwrap();
    drop(stream);
    fixture.wait_worker();
    fixture.drain();
    fixture.drain();
    assert_eq!(
        fixture.runtime.account_ui().unwrap().view,
        AccountView::NoNetworkConnectivity
    );
    assert!(matches!(target.accept(),Err(error) if error.kind()==std::io::ErrorKind::WouldBlock));
    fixture.assert_no_login();
}

#[test]
fn identity_validation_same_owner_old_workers_survive_edits_and_map_delivery_view() {
    let fixture = Fixture::new();
    let listener = fixture.configure();
    let id = fixture.open();
    fixture.validate(id, 1, "old@example.test");
    let (old, _) = accept(&listener);
    fixture
        .runtime
        .invalidate_account_validation(id, AccountValidationField::Email, 2)
        .unwrap();
    fixture.validate(id, 2, "new@example.test");
    let (new, _) = accept(&listener);
    reply(new, 200, r#"{"code":0}"#);
    fixture.wait_worker();
    fixture.drain();
    fixture.drain();
    let results = fixture.results();
    assert_eq!(results[0].generation, 2);
    assert_eq!(results[0].error, error(18, 3));
    fixture
        .runtime
        .account_ui_action(id, AccountUiAction::ForgotPassword)
        .unwrap();
    reply(old, 200, r#"{"code":10}"#);
    fixture.wait_worker();
    fixture.drain();
    fixture.drain();
    let results = fixture.results();
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].generation, 1);
    assert_eq!(results[0].view, AccountView::ForgotPassword);
    assert_eq!(results[0].error, error(15, 2));
    assert!(results[0].valid);
    fixture.assert_no_login();
}

#[test]
fn identity_validation_busy_and_help_do_not_cancel_timer_or_already_posted_feedback() {
    let fixture = Fixture::new();
    let listener = fixture.configure();
    let id = fixture.open();
    fixture.validate(id, 1, "x@example.test");
    let (stream, _) = accept(&listener);
    reply(stream, 200, r#"{"code":0}"#);
    fixture.wait_worker();
    fixture.drain();
    fixture
        .runtime
        .account_ui_action(id, AccountUiAction::Back)
        .unwrap();
    fixture
        .runtime
        .skynest_account
        .interactive
        .borrow_mut()
        .ui
        .as_mut()
        .unwrap()
        .busy = true;
    fixture.drain();
    let results = fixture.results();
    assert_eq!(results[0].view, AccountView::Help1);
    assert_eq!(results[0].error, None);
    assert!(fixture.runtime.account_ui().unwrap().busy);
    assert!(
        fixture
            .runtime
            .validate_account_field(id, AccountValidationField::Password, 2, "abc")
            .unwrap()
    );
    assert_eq!(fixture.results()[0].error, error(23, 4));
    fixture.validate(id, 3, "broken syntax");
    fixture.drain();
    assert_eq!(fixture.results()[0].view, AccountView::Help1);
    fixture.assert_no_login();
}

#[test]
fn identity_validation_real_submission_progress_retains_controller_mapping_until_network_error() {
    // SkynestLoginUI vptr 100AAD0E0 +64 -> 1007584CC. Its W1==12 path
    // 1007584E4..4F4 skips STR W1,[X19,#0x5C] at 100758504. In contrast,
    // LoginUIProvider displays a progress UIView which ignores these errors.
    for (view, password_error, local_email_error, remote_email_error) in [
        (AccountView::SignIn, error(19, 6), error(18, 1), None),
        (
            AccountView::Register2,
            error(17, 4),
            error(16, 1),
            error(16, 2),
        ),
        (
            AccountView::ForgotPassword,
            error(23, 4),
            error(15, 1),
            error(15, 2),
        ),
    ] {
        let fixture = Fixture::new();
        let listener = fixture.configure();
        let id = fixture.open();
        match view {
            AccountView::Register2 => {
                assert!(
                    fixture
                        .runtime
                        .account_ui_action(id, AccountUiAction::Register)
                        .unwrap()
                );
                assert!(
                    fixture
                        .runtime
                        .submit_account_birthday(
                            id,
                            RegistrationBirthday {
                                year: 2000,
                                month: 1,
                                day: 2,
                            }
                        )
                        .unwrap()
                );
            }
            AccountView::ForgotPassword => {
                assert!(
                    fixture
                        .runtime
                        .account_ui_action(id, AccountUiAction::ForgotPassword)
                        .unwrap()
                );
            }
            _ => {}
        }
        let submitted = match view {
            AccountView::SignIn => {
                fixture
                    .runtime
                    .submit_account_login(id, "submit@example.test", "secret123")
            }
            AccountView::Register2 => fixture.runtime.submit_account_registration(
                id,
                "submit@example.test",
                "secret123",
                AccountGender::Male,
            ),
            AccountView::ForgotPassword => fixture
                .runtime
                .submit_account_password_reset(id, "submit@example.test"),
            _ => unreachable!(),
        };
        assert!(submitted.unwrap());
        let (pending_submit, _) = accept(&listener);
        let snapshot = fixture.runtime.account_ui().unwrap();
        assert_eq!(snapshot.view, view);
        assert!(snapshot.busy);
        assert_eq!(snapshot.field_error, None);

        assert!(
            fixture
                .runtime
                .validate_account_field(id, AccountValidationField::Password, 1, "abc")
                .unwrap()
        );
        let results = fixture.results();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].view, view);
        assert_eq!(results[0].error, password_error);
        assert_eq!(fixture.runtime.account_ui(), Some(snapshot.clone()));

        fixture.validate(id, 2, "broken syntax");
        fixture.drain();
        let results = fixture.results();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].view, view);
        assert_eq!(results[0].error, local_email_error);
        assert_eq!(fixture.runtime.account_ui(), Some(snapshot.clone()));

        fixture.validate(id, 3, "x@example.test");
        let (stream, _) = accept(&listener);
        reply(stream, 200, r#"{"code":10}"#);
        fixture.wait_worker();
        fixture.drain();
        fixture.drain();
        let results = fixture.results();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].view, view);
        assert_eq!(results[0].error, remote_email_error);
        assert_eq!(fixture.runtime.account_ui(), Some(snapshot.clone()));

        fixture.validate(id, 4, "error@example.test");
        let (stream, _) = accept(&listener);
        reply(stream, 500, "{}");
        fixture.wait_worker();
        fixture.drain();
        assert_eq!(fixture.runtime.account_ui(), Some(snapshot));
        fixture.drain();
        assert_eq!(
            fixture.runtime.account_ui().unwrap().view,
            AccountView::NoNetworkConnectivity
        );
        assert!(!fixture.runtime.account_ui().unwrap().busy);
        assert!(fixture.results().is_empty());
        fixture.assert_no_login();

        // Retire the held submission in a different owner and reap its worker.
        // No fixture leaves a real request/thread waiting after this test.
        assert_ne!(fixture.open(), id);
        reply(pending_submit, 500, "{}");
        fixture.wait_worker();
        fixture.drain();
        fixture.drain();
    }
}

#[test]
fn identity_validation_cancel_or_owner_replacement_drops_both_lifetime_stages() {
    for after_worker in [false, true] {
        let fixture = Fixture::new();
        let listener = fixture.configure();
        let id = fixture.open();
        fixture.validate(id, 1, "old@example.test");
        let (stream, _) = accept(&listener);
        if after_worker {
            reply(stream, 500, "old failure");
            fixture.wait_worker();
            fixture.drain();
        } else {
            fixture
                .runtime
                .account_ui_action(id, AccountUiAction::Cancel)
                .unwrap();
            reply(stream, 500, "old failure");
            fixture.wait_worker();
        }
        let replacement = fixture.open();
        assert_ne!(replacement, id);
        fixture.drain();
        fixture.drain();
        fixture.drain();
        assert_eq!(
            fixture.runtime.account_ui().unwrap().view,
            AccountView::SignIn
        );
        assert_eq!(fixture.runtime.account_ui().unwrap().id, replacement);
        assert!(fixture.results().is_empty());
    }
}
