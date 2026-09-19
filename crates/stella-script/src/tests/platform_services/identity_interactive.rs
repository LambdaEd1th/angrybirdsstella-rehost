//! Native controller checks: no guest substitution or synthetic form success.

use super::identity_routes::accept_request;
use super::*;
use mlua::Table;
use std::time::Instant;

fn callbacks(runtime: &StellaLua) {
    runtime
        .execute_source(
            r#"
        account_successes = 0
        account_failures = 0
        local account = _G.SkynestAccount
        account.onLoginSuccess = function(guest, details)
            account_successes = account_successes + 1
            account_guest, account_details = guest, details
        end
        account.onLoginFailure = function(...)
            account_failures = account_failures + 1
            account_failure_args = { ... }
            account_failure_count = select('#', ...)
            account_failure_progress = account.native_isLoginInProgress()
        end
    "#,
        )
        .unwrap();
}

fn drain(runtime: &StellaLua) {
    dispatch_registered_application_events(runtime.lua()).unwrap();
}

fn wait_until(runtime: &StellaLua, mut condition: impl FnMut() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while !condition() {
        assert!(
            Instant::now() < deadline,
            "interactive identity operation timed out"
        );
        drain(runtime);
        thread::sleep(Duration::from_millis(1));
    }
}

#[test]
fn identity_interactive_flags_do_not_login_as_guest_and_cancel_posts_twice() {
    let sandbox = ShippedDataSandbox::new("account-interactive-cancel");
    let runtime = StellaLua::new(&sandbox.data_root).unwrap();
    runtime.enable_local_services().unwrap();
    callbacks(&runtime);
    let env = game_environment(runtime.lua()).unwrap();
    for register in [false, true] {
        runtime
            .execute_source(&format!(
                "_G.SkynestAccount.native_login(true, true, {register})"
            ))
            .unwrap();
        let ui = runtime.account_ui().unwrap();
        assert_eq!(
            ui.view,
            if register {
                AccountView::Register1
            } else {
                AccountView::SignIn
            }
        );
        assert!(!ui.busy);
        drain(&runtime);
        assert_eq!(env.get::<i64>("account_successes").unwrap(), 0);
        assert!(
            runtime
                .account_ui_action(ui.id, AccountUiAction::Cancel)
                .unwrap()
        );
        assert!(runtime.account_ui().is_none());
        let before = env.get::<i64>("account_failures").unwrap();
        drain(&runtime);
        assert_eq!(env.get::<i64>("account_failures").unwrap(), before);
        drain(&runtime);
        assert_eq!(env.get::<i64>("account_failures").unwrap(), before + 1);
        assert_eq!(env.get::<i64>("account_failure_count").unwrap(), 2);
        assert!(!env.get::<bool>("account_failure_progress").unwrap());
        let args = env.get::<Table>("account_failure_args").unwrap();
        assert_eq!(args.get::<String>(1).unwrap(), "ERROR_USER_CANCELLED_LOGIN");
        assert_eq!(args.get::<String>(2).unwrap(), "User cancelled login");
        assert!(
            !runtime
                .account_ui_action(ui.id, AccountUiAction::Cancel)
                .unwrap()
        );
    }
}

#[test]
fn identity_interactive_navigation_preserves_native_return_view_and_owner() {
    let sandbox = ShippedDataSandbox::new("account-navigation");
    let runtime = StellaLua::new(&sandbox.data_root).unwrap();
    callbacks(&runtime);
    runtime
        .execute_source("_G.SkynestAccount.native_login(true, false, false)")
        .unwrap();
    let old = runtime.account_ui().unwrap();
    for (action, expected) in [
        (AccountUiAction::Back, AccountView::Help1),
        (AccountUiAction::Continue, AccountView::Help2),
        (AccountUiAction::Continue, AccountView::Help3),
        (AccountUiAction::Continue, AccountView::SignIn),
        (AccountUiAction::Register, AccountView::Register1),
        (AccountUiAction::ForgotPassword, AccountView::ForgotPassword),
        (AccountUiAction::Continue, AccountView::Register1),
    ] {
        assert!(runtime.account_ui_action(old.id, action).unwrap());
        assert_eq!(runtime.account_ui().unwrap().view, expected);
    }
    assert!(
        !runtime
            .submit_account_login(old.id, "test@example.invalid", "unused")
            .unwrap()
    );
    runtime
        .execute_source("_G.SkynestAccount.native_login(true, false, false)")
        .unwrap();
    let current = runtime.account_ui().unwrap();
    assert_ne!(current.id, old.id);
    assert!(
        !runtime
            .account_ui_action(old.id, AccountUiAction::Cancel)
            .unwrap()
    );
    assert!(
        runtime
            .submit_account_login(current.id, "test@example.invalid", "unused")
            .unwrap()
    );
    assert_eq!(
        runtime.account_ui().unwrap().view,
        AccountView::NoNetworkConnectivity
    );
    drain(&runtime);
    assert_eq!(
        game_environment(runtime.lua())
            .unwrap()
            .get::<i64>("account_failures")
            .unwrap(),
        0
    );
}

#[test]
fn identity_interactive_signin_uses_credential_route_and_non_social_account_is_not_guest() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let address = listener.local_addr().unwrap();
    let server = thread::spawn(move || {
        let (mut stream, request) = accept_request(&listener);
        assert_eq!(
            request.lines().next(),
            Some("POST /proxy/identity/3.0/abid/login HTTP/1.1")
        );
        assert_eq!(
            request.split_once("\r\n\r\n").unwrap().1,
            "email=stella%2Btest%40example.invalid&password=one+%26+two%2Bthree"
        );
        assert!(
            request
                .to_ascii_lowercase()
                .contains("\r\nrovio-sgs: initial-segment\r\n")
        );
        let body = r#"{"accessToken":"email-token","refreshToken":"email-refresh","expiresIn":3600,"segment":"email-segment"}"#;
        write!(
            stream,
            "HTTP/1.1 201 Created\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        )
        .unwrap();
        let (mut stream, request) = accept_request(&listener);
        assert_eq!(
            request.lines().next(),
            Some("GET /proxy/identity/3.0/profile/own HTTP/1.1")
        );
        assert!(
            request
                .to_ascii_lowercase()
                .contains("\r\nx-access-token: email-token\r\n")
        );
        // Own-profile uses a separate direct GET, with only the token header.
        assert!(!request.to_ascii_lowercase().contains("\r\nrovio-sgs:"));
        let body = r#"{"publicAccountId":"email-player","personal":{"email":"stella+test@example.invalid","nickName":"Nickname is not email"},"socialNetworks":[]}"#;
        write!(
            stream,
            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        )
        .unwrap();
    });
    let sandbox = ShippedDataSandbox::new("account-signin");
    let runtime = StellaLua::new(&sandbox.data_root).unwrap();
    runtime
        .set_identity_url(&format!("http://{address}/proxy/identity/2.0"))
        .unwrap();
    runtime
        .set_identity_client(None, Some("fixture-signature"), None)
        .unwrap();
    callbacks(&runtime);
    runtime.skynest_account.seed_test_tokens(
        false,
        "initial-token",
        "initial-refresh",
        "initial-segment",
    );
    runtime
        .execute_source("_G.SkynestAccount.native_login(true, false, false)")
        .unwrap();
    let ui = runtime.account_ui().unwrap();
    // Showing the view must not start automatic access or submit credentials.
    drain(&runtime);
    assert!(
        runtime
            .submit_account_login(ui.id, "stella+test@example.invalid", "one & two+three")
            .unwrap()
    );
    assert!(runtime.account_ui().unwrap().busy);
    assert!(
        !runtime
            .submit_account_login(ui.id, "duplicate", "duplicate")
            .unwrap()
    );
    let env = game_environment(runtime.lua()).unwrap();
    wait_until(&runtime, || {
        env.get::<i64>("account_successes").unwrap() == 1
    });
    server.join().unwrap();
    assert!(runtime.account_ui().is_none());
    assert_eq!(env.get::<i64>("account_failures").unwrap(), 0);
    assert!(!env.get::<bool>("account_guest").unwrap());
    let details = env.get::<Table>("account_details").unwrap();
    assert!(!details.get::<bool>("isGuest").unwrap());
    assert!(!details.get::<bool>("isConnectedToSocialNetwork").unwrap());
    assert_eq!(details.get::<String>("id").unwrap(), "email-player");
    assert_eq!(
        details.get::<String>("name").unwrap(),
        "stella+test@example.invalid"
    );
    // A later cancellation retains native optional profile details and does
    // not sign the previously authenticated account out.
    runtime
        .execute_source("_G.SkynestAccount.native_login(true, false, true)")
        .unwrap();
    runtime
        .account_ui_action(runtime.account_ui().unwrap().id, AccountUiAction::Cancel)
        .unwrap();
    drain(&runtime);
    drain(&runtime);
    assert_eq!(env.get::<i64>("account_failure_count").unwrap(), 3);
    let args = env.get::<Table>("account_failure_args").unwrap();
    let details = args.get::<Table>(3).unwrap();
    assert_eq!(details.get::<String>("id").unwrap(), "email-player");
    assert_eq!(
        details.get::<String>("email").unwrap(),
        "stella+test@example.invalid"
    );
    assert!(
        runtime
            .lua()
            .globals()
            .get::<Table>("SkynestAccount")
            .unwrap()
            .get::<Function>("native_isLoggedIn")
            .unwrap()
            .call::<bool>(())
            .unwrap()
    );
}

#[test]
fn identity_interactive_login_errors_stay_inside_native_ui_for_retry_or_cancel() {
    for (status, expected_view, expected_field) in [
        (
            404,
            AccountView::SignIn,
            Some(AccountFieldError {
                field: 18,
                message: 3,
            }),
        ),
        (412, AccountView::AccountNotVerified, None),
        (
            500,
            AccountView::SignIn,
            Some(AccountFieldError {
                field: 19,
                message: 6,
            }),
        ),
        (200, AccountView::NoNetworkConnectivity, None),
    ] {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let address = listener.local_addr().unwrap();
        let server = thread::spawn(move || {
            let (mut stream, request) = accept_request(&listener);
            assert!(request.starts_with("POST /identity/3.0/abid/login HTTP/1.1\r\n"));
            let body = "This potentially untrusted echoed request is not shown in the UI";
            write!(
                stream,
                "HTTP/1.1 {status} Result\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            )
            .unwrap();
        });
        let sandbox = ShippedDataSandbox::new("account-login-errors");
        let runtime = StellaLua::new(&sandbox.data_root).unwrap();
        runtime
            .set_identity_url(&format!("http://{address}/identity/2.0"))
            .unwrap();
        callbacks(&runtime);
        runtime.skynest_account.seed_test_tokens(
            false,
            "error-token",
            "error-refresh",
            "error-segment",
        );
        runtime
            .execute_source("_G.SkynestAccount.native_login(true, false, false)")
            .unwrap();
        let id = runtime.account_ui().unwrap().id;
        runtime
            .submit_account_login(id, "test@example.invalid", "unused-test-password")
            .unwrap();
        wait_until(&runtime, || !runtime.account_ui().unwrap().busy);
        server.join().unwrap();
        let ui = runtime.account_ui().unwrap();
        assert_eq!(ui.view, expected_view);
        assert_eq!(ui.field_error, expected_field);
        let env = game_environment(runtime.lua()).unwrap();
        assert_eq!(env.get::<i64>("account_failures").unwrap(), 0);
        assert_eq!(env.get::<i64>("account_successes").unwrap(), 0);
        runtime
            .account_ui_action(id, AccountUiAction::Cancel)
            .unwrap();
        drain(&runtime);
        drain(&runtime);
        assert_eq!(env.get::<i64>("account_failures").unwrap(), 1);
    }
}

#[test]
fn identity_interactive_empty_204_profile_is_not_a_successful_account() {
    rejected_profile_response(204, "");
}

#[test]
fn identity_interactive_well_formed_201_profile_is_not_a_successful_account() {
    // 100672170 requires exactly 200 after the shared helper accepts 2xx.
    rejected_profile_response(
        201,
        r#"{"publicAccountId":"must-not-publish","personal":{"nickName":"Not Published","email":"fixture@example.invalid"}}"#,
    );
}

fn rejected_profile_response(status: u16, profile_body: &'static str) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let address = listener.local_addr().unwrap();
    let server = thread::spawn(move || {
        let (mut stream, request) = accept_request(&listener);
        assert_eq!(
            request.lines().next(),
            Some("POST /identity/3.0/abid/login HTTP/1.1")
        );
        assert_eq!(
            request.split_once("\r\n\r\n").unwrap().1,
            "email=fixture%40example.invalid&password=fixture-password"
        );
        let body = r#"{"accessToken":"empty-profile-token","refreshToken":"empty-profile-refresh","expiresIn":3600}"#;
        write!(
            stream,
            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        )
        .unwrap();
        let (mut stream, request) = accept_request(&listener);
        assert_eq!(
            request.lines().next(),
            Some("GET /identity/3.0/profile/own HTTP/1.1")
        );
        assert!(
            request
                .to_ascii_lowercase()
                .contains("\r\nx-access-token: empty-profile-token\r\n")
        );
        write!(stream, "HTTP/1.1 {status} Fixture\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{profile_body}", profile_body.len()).unwrap();
    });
    let sandbox = ShippedDataSandbox::new("account-empty-profile");
    let runtime = StellaLua::new(&sandbox.data_root).unwrap();
    runtime
        .set_identity_url(&format!("http://{address}/identity/2.0"))
        .unwrap();
    callbacks(&runtime);
    runtime.skynest_account.seed_test_tokens(
        false,
        "initial-token",
        "initial-refresh",
        "initial-segment",
    );
    runtime
        .execute_source("_G.SkynestAccount.native_login(true, false, false)")
        .unwrap();
    runtime
        .submit_account_login(
            runtime.account_ui().unwrap().id,
            "fixture@example.invalid",
            "fixture-password",
        )
        .unwrap();
    let env = game_environment(runtime.lua()).unwrap();
    wait_until(&runtime, || {
        env.get::<i64>("account_failures").unwrap() == 1
    });
    server.join().unwrap();
    assert!(runtime.account_ui().is_none());
    assert_eq!(env.get::<i64>("account_successes").unwrap(), 0);
    assert_eq!(env.get::<i64>("account_failure_count").unwrap(), 2);
    assert!(!env.get::<bool>("account_failure_progress").unwrap());
    let args = env.get::<Table>("account_failure_args").unwrap();
    assert_eq!(args.get::<String>(1).unwrap(), "ERROR_OTHER");
    assert_eq!(
        args.get::<String>(2).unwrap(),
        "Unable to retrieve account profile"
    );
    assert!(
        !runtime
            .lua()
            .globals()
            .get::<Table>("SkynestAccount")
            .unwrap()
            .get::<Function>("native_isLoggedIn")
            .unwrap()
            .call::<bool>(())
            .unwrap()
    );
}

#[test]
fn identity_interactive_social_login_never_substitutes_local_guest_success() {
    let sandbox = ShippedDataSandbox::new("social-not-guest");
    let runtime = StellaLua::new(&sandbox.data_root).unwrap();
    runtime.enable_local_services().unwrap();
    callbacks(&runtime);
    runtime
        .execute_source("_G.SkynestAccount.native_login(false, true, false)")
        .unwrap();
    drain(&runtime);
    let env = game_environment(runtime.lua()).unwrap();
    assert_eq!(env.get::<i64>("account_successes").unwrap(), 0);
    assert_eq!(env.get::<i64>("account_failures").unwrap(), 1);
    assert!(runtime.account_ui().is_none());
}
