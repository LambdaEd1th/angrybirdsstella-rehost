//! Disk/restart tests use only fresh isolated saves and synthetic loopback accounts.

use super::{identity_routes::accept_request, *};
use std::time::Instant;

fn configure(runtime: &StellaLua, url: &str) {
    // Same order as desktop startup. Configuring must not log out a cached
    // account before the first network request can consume its refresh.
    runtime.set_identity_url(url).unwrap();
    runtime
        .set_identity_client(Some("fixture-app"), Some("fixture-signature"), None)
        .unwrap();
    runtime
        .execute_source(
            r#"
        persistent_successes, persistent_failures = 0, 0
        _G.SkynestAccount.onLoginSuccess = function(guest, details)
            persistent_successes = persistent_successes + 1
            persistent_details = details
        end
        _G.SkynestAccount.onLoginFailure = function(code, message, details)
            persistent_failures = persistent_failures + 1
            persistent_details = details
        end
    "#,
        )
        .unwrap();
}

fn login(runtime: &StellaLua, success: bool) {
    runtime
        .execute_source("_G.SkynestAccount.native_login(false,false,false)")
        .unwrap();
    let env = game_environment(runtime.lua()).unwrap();
    let deadline = Instant::now() + Duration::from_secs(8);
    loop {
        dispatch_registered_application_events(runtime.lua()).unwrap();
        let count = env
            .get::<i64>(if success {
                "persistent_successes"
            } else {
                "persistent_failures"
            })
            .unwrap();
        if count != 0 {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "account disk continuation timed out"
        );
        thread::sleep(Duration::from_millis(1));
    }
}

fn response(refresh: &str) -> String {
    serde_json::json!({
        "userAuth":{"accessToken":"synthetic-access-never-persist","refreshToken":refresh,"expiresIn":3600},
        "segments":[8,2],"config":{},
        "profile":{"publicAccountId":"saved-account","abid":{"email":"saved@example.invalid"},
            "personal":{"nickName":"Saved"},"unknown":{"preserve":[1,true,"opaque"]}}
    }).to_string()
}

fn serve(listener: &TcpListener, expected_refresh: Option<&str>, status: u16, body: &str) {
    let (mut stream, request) = accept_request(listener);
    assert_eq!(
        request.lines().next(),
        Some("POST /proxy/session/1/apps/fixture-app/sessions HTTP/1.1")
    );
    let body_json: serde_json::Value =
        serde_json::from_str(request.split_once("\r\n\r\n").unwrap().1).unwrap();
    assert_eq!(
        body_json["refresh"],
        expected_refresh
            .map(|value| serde_json::json!({"token":value}))
            .unwrap_or(serde_json::Value::Null)
    );
    write!(
        stream,
        "HTTP/1.1 {status} Fixture\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    )
    .unwrap();
}

#[test]
fn identity_registry_process_reopen_preserves_refresh_and_raw_profile_until_logout() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let url = format!(
        "http://{}/proxy/identity/3.0",
        listener.local_addr().unwrap()
    );
    let server = thread::spawn(move || {
        serve(&listener, None, 200, &response("refresh-one"));
        serve(&listener, Some("refresh-one"), 503, "{}");
        serve(
            &listener,
            Some("refresh-one"),
            200,
            &response("refresh-two"),
        );
        serve(&listener, None, 200, &response("refresh-three"));
    });
    let sandbox = ShippedDataSandbox::new("registry-reopen");
    let first = StellaLua::new(&sandbox.data_root).unwrap();
    configure(&first, &url);
    login(&first, true);
    first.set_identity_url(&url).unwrap();
    first
        .set_identity_client(Some("fixture-app"), Some("fixture-signature"), None)
        .unwrap();
    first
        .execute_source("assert(_G.SkynestAccount.native_isLoggedIn())")
        .unwrap();
    let path = first.skynest_account.registry_path_for_test();
    let encrypted = std::fs::read(&path).unwrap();
    assert!(encrypted.len().is_multiple_of(16));
    for secret in [
        "refresh-one",
        "saved@example.invalid",
        "synthetic-access-never-persist",
    ] {
        assert!(
            !encrypted
                .windows(secret.len())
                .any(|bytes| bytes == secret.as_bytes())
        );
    }
    drop(first);

    let second = StellaLua::new(&sandbox.data_root).unwrap();
    configure(&second, &url);
    assert_eq!(
        std::fs::read(&path).unwrap(),
        encrypted,
        "startup setters must not persist logout"
    );
    login(&second, false);
    let env = game_environment(second.lua()).unwrap();
    let details: mlua::Table = env.get("persistent_details").unwrap();
    assert_eq!(details.get::<String>("id").unwrap(), "saved-account");
    assert_eq!(
        details.get::<String>("email").unwrap(),
        "saved@example.invalid"
    );
    drop(second);

    let third = StellaLua::new(&sandbox.data_root).unwrap();
    configure(&third, &url);
    login(&third, true);
    third
        .execute_source("_G.SkynestAccount.native_logout()")
        .unwrap();
    drop(third);
    let fourth = StellaLua::new(&sandbox.data_root).unwrap();
    configure(&fourth, &url);
    login(&fourth, true);
    server.join().unwrap();
}

#[test]
fn identity_registry_logout_before_first_online_operation_clears_saved_refresh() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let url = format!(
        "http://{}/proxy/identity/3.0",
        listener.local_addr().unwrap()
    );
    let server = thread::spawn(move || {
        serve(&listener, None, 200, &response("refresh-before-restart"));
        // Logout itself performs no HTTP; the next process/runtime must no
        // longer submit the refresh saved before that explicit logout.
        serve(&listener, None, 200, &response("fresh-after-logout"));
    });
    let sandbox = ShippedDataSandbox::new("registry-lazy-logout");
    let first = StellaLua::new(&sandbox.data_root).unwrap();
    configure(&first, &url);
    login(&first, true);
    drop(first);
    let second = StellaLua::new(&sandbox.data_root).unwrap();
    configure(&second, &url);
    second
        .execute_source(
            "_G.SkynestAccount.native_logout(); assert(not _G.SkynestAccount.native_isLoggedIn())",
        )
        .unwrap();
    drop(second);
    let third = StellaLua::new(&sandbox.data_root).unwrap();
    configure(&third, &url);
    login(&third, true);
    server.join().unwrap();
}

#[test]
fn identity_registry_provider_switch_detaches_without_erasing_or_forwarding_credentials() {
    let first_listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let second_listener = TcpListener::bind("127.0.0.1:0").unwrap();
    first_listener.set_nonblocking(true).unwrap();
    second_listener.set_nonblocking(true).unwrap();
    let first_url = format!(
        "http://{}/proxy/identity/2.0",
        first_listener.local_addr().unwrap()
    );
    let second_url = format!(
        "http://{}/proxy/identity/3.0",
        second_listener.local_addr().unwrap()
    );
    let server = thread::spawn(move || {
        serve(&first_listener, None, 200, &response("first-only-refresh"));
        serve(
            &second_listener,
            None,
            200,
            &response("second-only-refresh"),
        );
        serve(
            &first_listener,
            Some("first-only-refresh"),
            200,
            &response("first-rotated"),
        );
    });
    let sandbox = ShippedDataSandbox::new("registry-provider-isolation");
    let runtime = StellaLua::new(&sandbox.data_root).unwrap();
    configure(&runtime, &first_url);
    login(&runtime, true);
    let first_path = runtime.skynest_account.registry_path_for_test();
    let first_bytes = std::fs::read(&first_path).unwrap();
    configure(&runtime, &second_url);
    assert_eq!(std::fs::read(&first_path).unwrap(), first_bytes);
    assert_ne!(runtime.skynest_account.registry_path_for_test(), first_path);
    login(&runtime, true);
    configure(&runtime, &first_url);
    login(&runtime, true);
    server.join().unwrap();
}

#[test]
fn identity_registry_corruption_is_reported_without_replacing_file_or_sending_http() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let url = format!(
        "http://{}/proxy/identity/3.0",
        listener.local_addr().unwrap()
    );
    let sandbox = ShippedDataSandbox::new("registry-corrupt-preserve");
    let runtime = StellaLua::new(&sandbox.data_root).unwrap();
    configure(&runtime, &url);
    let path = runtime.skynest_account.registry_path_for_test();
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    let corrupted = b"synthetic damaged ciphertext";
    std::fs::write(&path, corrupted).unwrap();
    let error = runtime
        .execute_source("_G.SkynestAccount.native_login(false,false,false)")
        .unwrap_err();
    assert!(!error.to_string().contains("synthetic damaged"));
    assert_eq!(std::fs::read(path).unwrap(), corrupted);
    assert_eq!(
        listener.accept().unwrap_err().kind(),
        std::io::ErrorKind::WouldBlock
    );
}

#[test]
fn identity_registry_subprocess_probe() {
    let Some(data) = std::env::var_os("STELLA_REGISTRY_CHILD_DATA") else {
        return;
    };
    let url = std::env::var("STELLA_REGISTRY_CHILD_URL").unwrap();
    let runtime = StellaLua::new(std::path::PathBuf::from(data)).unwrap();
    configure(&runtime, &url);
    login(&runtime, true);
    let deadline = Instant::now() + Duration::from_secs(8);
    while runtime.social.native_friends_completions_for_test() == 0 {
        assert!(
            Instant::now() < deadline,
            "child native friends request did not complete"
        );
        dispatch_registered_application_events(runtime.lua()).unwrap();
        thread::sleep(Duration::from_millis(1));
    }
}

#[test]
fn identity_registry_refresh_survives_two_independent_os_processes() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let url = format!(
        "http://{}/proxy/identity/3.0",
        listener.local_addr().unwrap()
    );
    let server = thread::spawn(move || {
        serve(&listener, None, 200, &response("cross-process-refresh"));
        let (mut stream, request) =
            super::identity_routes::accept_request_including_friends(&listener);
        assert_eq!(
            request.lines().next(),
            Some("GET /proxy/identity/2.0/friends HTTP/1.1")
        );
        assert!(super::identity_routes::unavailable_friends_fixture(
            &mut stream,
            request.as_bytes()
        ));
        drop(stream);
        serve(
            &listener,
            Some("cross-process-refresh"),
            200,
            &response("cross-process-rotated"),
        );
        let (mut stream, request) =
            super::identity_routes::accept_request_including_friends(&listener);
        assert_eq!(
            request.lines().next(),
            Some("GET /proxy/identity/2.0/friends HTTP/1.1")
        );
        assert!(super::identity_routes::unavailable_friends_fixture(
            &mut stream,
            request.as_bytes()
        ));
    });
    let sandbox = ShippedDataSandbox::new("registry-real-process-restart");
    for _ in 0..2 {
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact","tests::platform_services::identity_persistence::identity_registry_subprocess_probe","--nocapture"])
            .env("STELLA_REGISTRY_CHILD_DATA",&sandbox.data_root)
            .env("STELLA_REGISTRY_CHILD_URL",&url)
            .output().unwrap();
        assert!(
            output.status.success(),
            "isolated registry child failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    server.join().unwrap();
}
