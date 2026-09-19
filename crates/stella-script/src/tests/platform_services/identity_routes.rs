//! Transport regressions: the server accepts only the native per-route version.

use super::*;
use std::{net::TcpStream, time::Instant};

pub(super) fn accept_request(listener: &TcpListener) -> (TcpStream, String) {
    loop {
        let (mut stream, request) = accept_request_including_friends(listener);
        if unavailable_friends_fixture(&mut stream, request.as_bytes()) {
            continue;
        }
        return (stream, request);
    }
}

// Identity/storage fixtures do not offer the newly recovered friends service.
// Reject that independent background route explicitly, without consuming an
// identity response or changing the assertions for the operation under test.
// Native social tests use accept_request_including_friends and assert its wire.
pub(super) fn unavailable_friends_fixture(stream: &mut TcpStream, request: &[u8]) -> bool {
    let first = request.split(|b| *b == b'\n').next().unwrap_or_default();
    let mut parts = first.split(|b| *b == b' ');
    if parts.next() != Some(b"GET".as_slice())
        || !parts
            .next()
            .is_some_and(|p| p.ends_with(b"/identity/2.0/friends"))
    {
        return false;
    }
    write!(
        stream,
        "HTTP/1.1 503 Friends fixture unavailable\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
    )
    .unwrap();
    eprintln!("non-social fixture rejected native friends GET with 503");
    true
}

pub(super) fn accept_request_including_friends(listener: &TcpListener) -> (TcpStream, String) {
    let deadline = Instant::now() + Duration::from_secs(5);
    let mut stream = loop {
        match listener.accept() {
            Ok((stream, _)) => break stream,
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                assert!(Instant::now() < deadline, "missing identity request");
                thread::sleep(Duration::from_millis(1));
            }
            Err(error) => panic!("identity accept failed: {error}"),
        }
    };
    // macOS inherits the listening socket's nonblocking flag on accept.
    stream.set_nonblocking(false).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(5)))
        .unwrap();
    let mut request = Vec::new();
    let mut chunk = [0; 4096];
    loop {
        let read = stream.read(&mut chunk).unwrap();
        assert_ne!(read, 0, "incomplete identity request");
        request.extend_from_slice(&chunk[..read]);
        if let Some(end) = request.windows(4).position(|part| part == b"\r\n\r\n") {
            let headers = std::str::from_utf8(&request[..end]).unwrap();
            let body_size = headers
                .lines()
                .find_map(|line| {
                    let (key, value) = line.split_once(':')?;
                    key.eq_ignore_ascii_case("content-length")
                        .then(|| value.trim().parse::<usize>().unwrap())
                })
                .unwrap_or(0);
            if request.len() >= end + 4 + body_size {
                return (stream, String::from_utf8(request).unwrap());
            }
        }
    }
}

fn wait_for_identity(runtime: &StellaLua) {
    let environment = game_environment(runtime.lua()).unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    while environment.get::<i64>("route_finished").unwrap() == 0 {
        assert!(
            Instant::now() < deadline,
            "identity completion was not delivered"
        );
        runtime.update(1.0 / 60.0).unwrap();
        thread::sleep(Duration::from_millis(1));
    }
}

#[test]
fn compatible_identity_mixed_versions_reach_only_explicit_origin_and_prefix() {
    for configured_version in ["2.0", "3.0"] {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let address = listener.local_addr().unwrap();
        let server = thread::spawn(move || {
            let routes = [
                (
                    "POST /proxy/purple/session/1/apps/Purple/sessions HTTP/1.1",
                    r#"{"userAuth":{"accessToken":"routed-token","refreshToken":"routed-refresh","expiresIn":3600},"segments":[3,7],"profile":{"publicAccountId":"routed-player","personal":{"nickName":"Routed Stella"},"externalNetworks":[{"provider":"facebook","id":"routed-social-id"}],"socialNetworks":[{"provider":"facebook","id":"routed-social-id","socialAttributes":{"name":"Routed Stella"}}]},"config":{}}"#,
                ),
                (
                    "POST /proxy/purple/identity/2.0/profile/nickname/validate HTTP/1.1",
                    r#"{"isValid":true,"validationMsg":""}"#,
                ),
            ];
            for (index, (route, body)) in routes.iter().enumerate() {
                let (mut stream, request) = accept_request(&listener);
                // Reject mismatches before returning a successful provider
                // response: a blanket success mock would conceal bad routes.
                if request.lines().next() != Some(route) {
                    stream.write_all(b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n").unwrap();
                    panic!("unexpected identity route: {request}");
                }
                let lower = request.to_ascii_lowercase();
                assert!(
                    lower.contains(&format!("\r\nhost: {address}\r\n")),
                    "{request}"
                );
                if index != 0 {
                    assert!(
                        lower.contains("\r\nx-access-token: routed-token\r\n"),
                        "{request}"
                    );
                    assert!(lower.contains("\r\nrovio-sgs: 3, 7\r\n"));
                }
                if index == 1 {
                    assert_eq!(
                        request.split_once("\r\n\r\n").unwrap().1,
                        "nickname=Routed+Stella&checkUnique=true"
                    );
                }
                write!(stream, "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()).unwrap();
            }
        });
        let sandbox = ShippedDataSandbox::new("account-session-routes");
        let runtime = StellaLua::new(&sandbox.data_root).unwrap();
        runtime
            .set_identity_url(&format!(
                "http://{address}/proxy/purple/identity/{configured_version}/"
            ))
            .unwrap();
        // Failed configuration is atomic: it cannot discard the explicitly
        // selected service or silently fall back to an unrelated host/root.
        for invalid in [
            "https://different.invalid/arbitrary-root",
            "https://different.invalid/identity/2.0?redirect=true",
            "https://different.invalid/%2e%2e/identity/2.0",
        ] {
            assert!(runtime.set_identity_url(invalid).is_err());
        }
        runtime
            .execute_source(
                r#"
            route_finished = 0
            route_logins = 0
            route_failures = 0
            local account = _G.SkynestAccount
            account.onLoginFailure = function(...)
                route_failures = route_failures + 1
                route_finished = 1
            end
            account.onLoginSuccess = function(guest, profile)
                assert(guest == false and profile.id == 'routed-player')
                assert(profile.name == 'Routed Stella')
                route_logins = route_logins + 1
                account.native_validateNickname('Routed Stella', function(ok, valid)
                    assert(ok == true and valid == true)
                    route_finished = route_finished + 1
                end)
            end
            account.native_login(false, false, false)
        "#,
            )
            .unwrap();
        wait_for_identity(&runtime);
        server.join().unwrap();
        let environment = game_environment(runtime.lua()).unwrap();
        assert_eq!(environment.get::<i64>("route_failures").unwrap(), 0);
        assert_eq!(environment.get::<i64>("route_logins").unwrap(), 1);
        assert_eq!(environment.get::<i64>("route_finished").unwrap(), 1);
    }
}

#[test]
fn compatible_identity_does_not_follow_provider_redirects() {
    let forbidden = TcpListener::bind("127.0.0.1:0").unwrap();
    forbidden.set_nonblocking(true).unwrap();
    let redirect_target = format!("http://{}/credentials", forbidden.local_addr().unwrap());
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let url = format!("http://{}/identity/2.0", listener.local_addr().unwrap());
    let server = thread::spawn(move || {
        let (mut stream, request) = accept_request(&listener);
        assert!(request.starts_with("POST /session/1/apps/Purple/sessions HTTP/1.1\r\n"));
        write!(stream, "HTTP/1.1 307 Temporary Redirect\r\nLocation: {redirect_target}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n").unwrap();
    });
    let sandbox = ShippedDataSandbox::new("account-session-redirect");
    let runtime = StellaLua::new(&sandbox.data_root).unwrap();
    runtime.set_identity_url(&url).unwrap();
    runtime
        .execute_source(
            r#"
        route_finished = 0
        route_failures = 0
        route_logins = 0
        local account = _G.SkynestAccount
        account.onLoginSuccess = function()
            route_logins = route_logins + 1
            route_finished = 1
        end
        account.onLoginFailure = function(code, message)
            assert(code == 'ERROR_OTHER' and type(message) == 'string')
            route_failures = route_failures + 1
            route_finished = 1
        end
        account.native_login(false, false, false)
    "#,
        )
        .unwrap();
    wait_for_identity(&runtime);
    server.join().unwrap();
    let environment = game_environment(runtime.lua()).unwrap();
    assert_eq!(environment.get::<i64>("route_failures").unwrap(), 1);
    assert_eq!(environment.get::<i64>("route_logins").unwrap(), 0);
    assert!(
        matches!(forbidden.accept(), Err(error) if error.kind() == std::io::ErrorKind::WouldBlock)
    );
}

#[test]
fn compatible_identity_nickname_rejects_otherwise_valid_201_response() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let address = listener.local_addr().unwrap();
    let server = thread::spawn(move || {
        let (mut stream, request) = accept_request(&listener);
        assert!(request.starts_with("POST /identity/2.0/profile/nickname/validate HTTP/1.1\r\n"));
        let headers = request.to_ascii_lowercase();
        assert!(headers.contains("x-access-token: nickname-access\r\n"));
        assert!(headers.contains("rovio-sgs: nickname-segment\r\n"));
        let body = r#"{"isValid":true,"validationMsg":""}"#;
        write!(
            stream,
            "HTTP/1.1 201 Created\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        )
        .unwrap();
    });
    let sandbox = ShippedDataSandbox::new("account-nickname-201");
    let runtime = StellaLua::new(&sandbox.data_root).unwrap();
    runtime
        .set_identity_url(&format!("http://{address}/identity/3.0"))
        .unwrap();
    runtime.skynest_account.seed_test_tokens(
        false,
        "nickname-access",
        "nickname-refresh",
        "nickname-segment",
    );
    runtime
        .execute_source(
            r#"
        route_finished = 0
        _G.SkynestAccount.native_validateNickname('nickname', function(...)
            nickname_count = select('#', ...)
            nickname_result = ...
            route_finished = route_finished + 1
        end)
    "#,
        )
        .unwrap();
    wait_for_identity(&runtime);
    server.join().unwrap();
    let env = game_environment(runtime.lua()).unwrap();
    assert_eq!(env.get::<i64>("nickname_count").unwrap(), 1);
    assert!(!env.get::<bool>("nickname_result").unwrap());
    assert_eq!(env.get::<i64>("route_finished").unwrap(), 1);
}

#[test]
fn compatible_identity_session_projects_empty_and_active_social_profiles_into_native_callback() {
    for (profile, expected_id, expected_name, expected_guest, expected_connected) in [
        (r#"{}"#, "", "", false, false),
        (
            r#"{"publicAccountId":"social-only","socialNetworks":[{"provider":"facebook","id":"linked-id","socialAttributes":{"name":"Not Active"}}]}"#,
            "social-only",
            "",
            true,
            false,
        ),
        (
            r#"{"publicAccountId":"active-social","externalNetworks":[{"provider":"facebook","id":"linked-id"}],"socialNetworks":[{"provider":"facebook","id":"linked-id","socialAttributes":{"name":"Active Name"}}]}"#,
            "active-social",
            "Active Name",
            false,
            true,
        ),
    ] {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let address = listener.local_addr().unwrap();
        let sandbox = ShippedDataSandbox::new("account-session-profile-projection");
        let runtime = StellaLua::new(&sandbox.data_root).unwrap();
        runtime
            .set_identity_url(&format!("http://{address}/identity/3.0"))
            .unwrap();
        // Start the unchanged five-second transport deadline only after
        // creating the VM/asset fixture, so setup load is not a request timeout.
        let server = thread::spawn(move || {
            let (mut stream, request) = accept_request(&listener);
            assert_eq!(
                request.lines().next(),
                Some("POST /session/1/apps/Purple/sessions HTTP/1.1")
            );
            let profile: serde_json::Value = serde_json::from_str(profile).unwrap();
            let body = serde_json::json!({
                "userAuth": {"accessToken": "projection-access", "refreshToken": "projection-refresh", "expiresIn": 3600},
                "segments": [], "profile": profile, "config": {}
            }).to_string();
            write!(
                stream,
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            )
            .unwrap();
            listener
        });
        runtime
            .execute_source(
                r#"
            route_finished, projection_successes, projection_failures = 0, 0, 0
            _G.SkynestAccount.onLoginSuccess = function(guest, details)
                projection_successes = projection_successes + 1
                projection_guest, projection_details = guest, details
                route_finished = route_finished + 1
            end
            _G.SkynestAccount.onLoginFailure = function()
                projection_failures = projection_failures + 1
                route_finished = route_finished + 1
            end
            _G.SkynestAccount.native_login(false, false, false)
        "#,
            )
            .unwrap();
        wait_for_identity(&runtime);
        let listener = server.join().unwrap();
        let env = game_environment(runtime.lua()).unwrap();
        assert_eq!(
            env.get::<i64>("projection_failures").unwrap(),
            0,
            "{profile}"
        );
        assert_eq!(
            env.get::<i64>("projection_successes").unwrap(),
            1,
            "{profile}"
        );
        assert_eq!(env.get::<bool>("projection_guest").unwrap(), expected_guest);
        let details = env.get::<mlua::Table>("projection_details").unwrap();
        assert_eq!(details.get::<bool>("isGuest").unwrap(), expected_guest);
        assert_eq!(
            details.get::<bool>("isConnectedToSocialNetwork").unwrap(),
            expected_connected
        );
        assert_eq!(details.get::<String>("id").unwrap(), expected_id);
        assert_eq!(details.get::<String>("name").unwrap(), expected_name);
        // Store initialization performs its native friends GET even without
        // a platform login; it must not trigger own-profile or avatar fetches.
        let (mut stream, request) = accept_request_including_friends(&listener);
        assert_eq!(
            request.lines().next(),
            Some("GET /identity/2.0/friends HTTP/1.1")
        );
        assert!(unavailable_friends_fixture(&mut stream, request.as_bytes()));
        drop(stream);
        let deadline = Instant::now() + Duration::from_secs(5);
        while runtime.social.native_friends_completions_for_test() == 0 {
            assert!(Instant::now() < deadline);
            dispatch_registered_application_events(runtime.lua()).unwrap();
            thread::sleep(Duration::from_millis(1));
        }
        assert!(
            matches!(listener.accept(), Err(error) if error.kind() == std::io::ErrorKind::WouldBlock)
        );
    }
}
