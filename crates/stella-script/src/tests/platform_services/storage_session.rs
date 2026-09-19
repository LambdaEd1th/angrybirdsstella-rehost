//! Storage transport tests use synthetic loopback accounts and isolated saves.

use super::{identity_routes::accept_request, *};
use base64::Engine as _;
use std::{io::Cursor, net::TcpStream, time::Instant};

pub(super) fn listener() -> TcpListener {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    listener
}

pub(super) fn configure(runtime: &StellaLua, listener: &TcpListener) {
    let origin = format!("http://{}", listener.local_addr().unwrap());
    runtime
        .set_identity_url(&format!("{origin}/proxy/identity/3.0"))
        .unwrap();
    runtime
        .set_identity_client(Some("storage-fixture"), Some("never-a-segment"), None)
        .unwrap();
    runtime
        .set_storage_url(&format!("{origin}/storage/1.0"))
        .unwrap();
}

pub(super) fn respond(stream: &mut TcpStream, status: u16, body: &serde_json::Value) {
    let body = if status == 204 {
        String::new()
    } else {
        body.to_string()
    };
    write!(stream,"HTTP/1.1 {status} Fixture\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",body.len()).unwrap();
}

pub(super) fn body(request: &str) -> &str {
    request.split_once("\r\n\r\n").unwrap().1
}

fn header<'a>(request: &'a str, name: &str) -> Option<&'a str> {
    request
        .split_once("\r\n\r\n")
        .unwrap()
        .0
        .lines()
        .find_map(|line| {
            let (key, value) = line.split_once(':')?;
            key.eq_ignore_ascii_case(name).then(|| value.trim())
        })
}

pub(super) fn assert_auth(request: &str, token: &str, segments: &str) {
    assert_eq!(header(request, "x-access-token"), Some(token));
    assert_eq!(header(request, "rovio-sgs"), Some(segments));
    assert!(!request.contains("never-a-segment"));
}

pub(super) fn acquire(listener: &TcpListener, refresh: Option<&str>, token: &str) {
    acquire_at(listener, "/proxy", refresh, token);
}

fn acquire_at(listener: &TcpListener, prefix: &str, refresh: Option<&str>, token: &str) {
    let (mut stream, request) = accept_request(listener);
    assert_eq!(
        request.lines().next(),
        Some(format!("POST {prefix}/session/1/apps/storage-fixture/sessions HTTP/1.1").as_str())
    );
    let value: serde_json::Value = serde_json::from_str(body(&request)).unwrap();
    assert_eq!(
        value["refresh"],
        refresh
            .map(|token| serde_json::json!({"token":token}))
            .unwrap_or(serde_json::Value::Null)
    );
    respond(
        &mut stream,
        200,
        &serde_json::json!({
            "userAuth":{"accessToken":token,"refreshToken":"rotated-refresh","expiresIn":3600},
            "segments":[8,9],"config":{},"profile":{"publicAccountId":"storage-account"}
        }),
    );
}

pub(super) fn wait_for(runtime: &StellaLua, name: &str) {
    let environment = game_environment(runtime.lua()).unwrap();
    let deadline = Instant::now() + Duration::from_secs(8);
    loop {
        dispatch_registered_application_events(runtime.lua()).unwrap();
        if environment.get::<bool>(name).unwrap_or(false) {
            return;
        }
        assert!(
            Instant::now() < deadline,
            "missing storage completion {name}"
        );
        thread::sleep(Duration::from_millis(1));
    }
}

fn percent_decode(input: &str) -> String {
    let mut bytes = Vec::new();
    let mut input = input.bytes();
    while let Some(byte) = input.next() {
        bytes.push(match byte {
            b'+' => b' ',
            b'%' => {
                let high = char::from(input.next().unwrap()).to_digit(16).unwrap();
                let low = char::from(input.next().unwrap()).to_digit(16).unwrap();
                ((high << 4) | low) as u8
            }
            byte => byte,
        });
    }
    String::from_utf8(bytes).unwrap()
}

pub(super) fn form_fields(body: &str) -> BTreeMap<String, String> {
    body.split('&')
        .map(|field| {
            let (key, value) = field.split_once('=').unwrap();
            (percent_decode(key), percent_decode(value))
        })
        .collect()
}

/// Independent decoding oracle: do not round-trip through production codec.
pub(super) fn decode_sdkv2(value: &str) -> String {
    let compressed = base64::engine::general_purpose::URL_SAFE
        .decode(value)
        .unwrap();
    assert!(compressed.len() >= 13, "SDKv2 requires the LZMA envelope");
    let mut reader =
        lzma_rust2::LzmaReader::new_mem_limit(Cursor::new(compressed), 65536, None).unwrap();
    let mut decoded = Vec::new();
    reader.read_to_end(&mut decoded).unwrap();
    String::from_utf8(decoded).unwrap()
}

#[derive(Clone, Copy, Debug)]
enum Operation {
    Get,
    Set,
    Batch,
}

impl Operation {
    fn start(self, runtime: &StellaLua) {
        runtime.execute_source(match self {
            Self::Get=>r##"storage_done=false; _G.SkynestStorage.native_getKey("nick/name",function(...) storage_args=select("#",...); storage_value=...; storage_done=true end)"##,
            Self::Set=>r##"storage_done=false; _G.SkynestStorage.native_setKey("nick/name","Stella + 中文 / value",function(...) storage_args=select("#",...); storage_done=true end)"##,
            Self::Batch=>r##"storage_done=false; _G.SkynestStorage.native_getKeyForAccountIds("nick/name",{"friend-a","friend-b"},function(...) storage_args=select("#",...); storage_value=...; storage_done=true end)"##,
        }).unwrap();
    }

    fn assert_request(self, request: &str) {
        let expected = match self {
            Self::Get => {
                "GET /storage/1.0/state?key=%5Bmy%5D%2F%5Bclient%5D%2Fnick_2Fname HTTP/1.1"
            }
            Self::Set => "POST /storage/1.0/state HTTP/1.1",
            Self::Batch => "POST /storage/1.0/states/query HTTP/1.1",
        };
        assert_eq!(request.lines().next(), Some(expected));
        match self {
            Self::Get => assert!(body(request).is_empty()),
            Self::Set => {
                assert_eq!(
                    header(request, "content-type"),
                    Some("application/x-www-form-urlencoded")
                );
                let fields = form_fields(body(request));
                assert_eq!(fields["key"], "[my]/[client]/nick_2Fname");
                assert_eq!(fields["encoding"], "SDKv2");
                assert_eq!(fields["hash"], "");
                assert_eq!(fields["force"], "false");
                assert_eq!(decode_sdkv2(&fields["value"]), "Stella + 中文 / value");
            }
            Self::Batch => {
                assert_eq!(header(request, "content-type"), Some("application/json"));
                assert_eq!(
                    serde_json::from_str::<serde_json::Value>(body(request)).unwrap(),
                    serde_json::json!({"keys":["[my]/[client]/nick_2Fname"],"accountIds":["friend-a","friend-b"]})
                );
            }
        }
    }

    fn response(self) -> serde_json::Value {
        match self {
            Self::Get => {
                serde_json::json!([{"hash":"read-hash","encoding":"SDKv1","value":"server-value"}])
            }
            Self::Set => serde_json::json!([{"hash":"write-hash"}]),
            Self::Batch => {
                serde_json::json!({"result":[{"accountId":"friend-a","states":[{"encoding":"SDKv1","value":"friend-value"}]}]})
            }
        }
    }
}

#[test]
fn storage_session_acquires_level2_without_prior_login_or_explicit_credentials() {
    let listener = listener();
    let sandbox = ShippedDataSandbox::new("storage-auto-acquire");
    let runtime = StellaLua::new(&sandbox.data_root).unwrap();
    configure(&runtime, &listener);
    let server = thread::spawn(move || {
        acquire(&listener, None, "acquired-access");
        let (mut stream, request) = accept_request(&listener);
        Operation::Get.assert_request(&request);
        assert_auth(&request, "acquired-access", "8, 9");
        respond(&mut stream, 200, &Operation::Get.response());
        listener
    });
    Operation::Get.start(&runtime);
    wait_for(&runtime, "storage_done");
    let env = game_environment(runtime.lua()).unwrap();
    assert_eq!(env.get::<String>("storage_value").unwrap(), "server-value");
    assert_eq!(env.get::<i64>("storage_args").unwrap(), 1);
    assert!(
        matches!(server.join().unwrap().accept(),Err(error) if error.kind()==std::io::ErrorKind::WouldBlock)
    );
}

#[test]
fn storage_session_401_replays_frozen_get_form_and_json_with_renewed_headers() {
    for operation in [Operation::Get, Operation::Set, Operation::Batch] {
        let listener = listener();
        let sandbox = ShippedDataSandbox::new("storage-renewal-replay");
        let runtime = StellaLua::new(&sandbox.data_root).unwrap();
        configure(&runtime, &listener);
        runtime
            .skynest_account
            .seed_test_tokens(false, "old-access", "old-refresh", "old-segment");
        let server = thread::spawn(move || {
            let (mut stream, first) = accept_request(&listener);
            operation.assert_request(&first);
            assert_auth(&first, "old-access", "old-segment");
            respond(&mut stream, 401, &serde_json::json!({}));
            acquire(&listener, Some("old-refresh"), "new-access");
            let (mut stream, replay) = accept_request(&listener);
            operation.assert_request(&replay);
            assert_auth(&replay, "new-access", "8, 9");
            assert_eq!(first.lines().next(), replay.lines().next());
            assert_eq!(body(&first), body(&replay));
            assert_eq!(
                header(&first, "content-type"),
                header(&replay, "content-type")
            );
            respond(&mut stream, 200, &operation.response());
            listener
        });
        operation.start(&runtime);
        wait_for(&runtime, "storage_done");
        let env = game_environment(runtime.lua()).unwrap();
        match operation {
            Operation::Get => {
                assert_eq!(env.get::<String>("storage_value").unwrap(), "server-value")
            }
            Operation::Set => assert_eq!(env.get::<i64>("storage_args").unwrap(), 0),
            Operation::Batch => assert_eq!(
                env.get::<mlua::Table>("storage_value")
                    .unwrap()
                    .get::<String>("friend-a")
                    .unwrap(),
                "friend-value"
            ),
        }
        assert!(
            matches!(server.join().unwrap().accept(),Err(error) if error.kind()==std::io::ErrorKind::WouldBlock)
        );
    }
}

#[test]
fn storage_session_second_401_terminates_without_parent_fallback_or_third_request() {
    for operation in [Operation::Get, Operation::Set, Operation::Batch] {
        let listener = listener();
        let sandbox = ShippedDataSandbox::new("storage-second-401");
        let runtime = StellaLua::new(&sandbox.data_root).unwrap();
        configure(&runtime, &listener);
        runtime
            .skynest_account
            .seed_test_tokens(false, "old-access", "old-refresh", "old-segment");
        let server = thread::spawn(move || {
            let (mut stream, first) = accept_request(&listener);
            operation.assert_request(&first);
            respond(&mut stream, 401, &serde_json::json!({}));
            acquire(&listener, Some("old-refresh"), "new-access");
            let (mut stream, replay) = accept_request(&listener);
            assert_auth(&replay, "new-access", "8, 9");
            assert_eq!(body(&first), body(&replay));
            respond(&mut stream, 401, &serde_json::json!({}));
            listener
        });
        operation.start(&runtime);
        wait_for(&runtime, "storage_done");
        assert_eq!(
            game_environment(runtime.lua())
                .unwrap()
                .get::<i64>("storage_args")
                .unwrap(),
            0
        );
        assert!(
            matches!(server.join().unwrap().accept(),Err(error) if error.kind()==std::io::ErrorKind::WouldBlock)
        );
    }
}

#[test]
fn storage_session_non200_success_codes_fail_and_do_not_publish_response_hashes() {
    for status in [201, 204] {
        for operation in [Operation::Get, Operation::Set, Operation::Batch] {
            let listener = listener();
            let sandbox = ShippedDataSandbox::new("storage-exact-200");
            let runtime = StellaLua::new(&sandbox.data_root).unwrap();
            configure(&runtime, &listener);
            runtime.skynest_account.seed_test_tokens(
                false,
                "old-access",
                "old-refresh",
                "old-segment",
            );
            let server = thread::spawn(move || {
                let (mut stream, request) = accept_request(&listener);
                operation.assert_request(&request);
                respond(&mut stream, status, &operation.response());
                let (mut stream, probe) = accept_request(&listener);
                Operation::Set.assert_request(&probe); // hash must still be empty
                assert_auth(&probe, "old-access", "old-segment");
                respond(&mut stream, 200, &Operation::Set.response());
                listener
            });
            operation.start(&runtime);
            wait_for(&runtime, "storage_done");
            assert_eq!(
                game_environment(runtime.lua())
                    .unwrap()
                    .get::<i64>("storage_args")
                    .unwrap(),
                0
            );
            Operation::Set.start(&runtime);
            wait_for(&runtime, "storage_done");
            assert!(
                matches!(server.join().unwrap().accept(),Err(error) if error.kind()==std::io::ErrorKind::WouldBlock)
            );
        }
    }
}

#[test]
fn storage_session_explicit_credentials_never_acquire_or_renew_identity() {
    let listener = listener();
    let sandbox = ShippedDataSandbox::new("storage-explicit-credentials");
    let runtime = StellaLua::new(&sandbox.data_root).unwrap();
    configure(&runtime, &listener);
    runtime
        .set_storage_credentials(Some("explicit-access"), Some("explicit-segment"))
        .unwrap();
    let server = thread::spawn(move || {
        let (mut stream, request) = accept_request(&listener);
        Operation::Get.assert_request(&request);
        assert_auth(&request, "explicit-access", "explicit-segment");
        respond(&mut stream, 401, &serde_json::json!({}));
        listener
    });
    Operation::Get.start(&runtime);
    wait_for(&runtime, "storage_done");
    assert_eq!(
        game_environment(runtime.lua())
            .unwrap()
            .get::<i64>("storage_args")
            .unwrap(),
        0
    );
    assert!(
        matches!(server.join().unwrap().accept(),Err(error) if error.kind()==std::io::ErrorKind::WouldBlock)
    );
}

fn replace_account(runtime: &StellaLua, origin: &str, change_provider: bool) {
    if change_provider {
        runtime
            .set_identity_url(&format!("{origin}/replacement/identity/3.0"))
            .unwrap();
    } else {
        runtime
            .execute_source("_G.SkynestAccount.native_logout()")
            .unwrap();
    }
}

pub(super) fn wait_pending(runtime: &StellaLua) {
    let deadline = Instant::now() + Duration::from_secs(8);
    while runtime.skynest_storage.pending_online_count_for_test() == 0 {
        assert!(
            Instant::now() < deadline,
            "storage worker did not enqueue completion"
        );
        thread::sleep(Duration::from_millis(1));
    }
}

#[test]
fn storage_session_account_replacement_discards_old_hash_and_late_get_completion() {
    // Direct identity replacement changes the account generation even though
    // the session epoch itself need not change. Also cover logout and provider.
    for boundary in [None, Some(false), Some(true)] {
        let listener = listener();
        let origin = format!("http://{}", listener.local_addr().unwrap());
        let sandbox = ShippedDataSandbox::new("storage-late-get");
        let runtime = StellaLua::new(&sandbox.data_root).unwrap();
        configure(&runtime, &listener);
        runtime
            .skynest_account
            .seed_test_tokens(false, "old-access", "old-refresh", "old-segment");
        let (old_seen_tx, old_seen_rx) = mpsc::channel();
        let (release_old_tx, release_old_rx) = mpsc::channel();
        let server = thread::spawn(move || {
            // Establish a real old-account cache before leaving another read in flight.
            let (mut first, request) = accept_request(&listener);
            Operation::Get.assert_request(&request);
            respond(&mut first, 200, &Operation::Get.response());
            let (mut old, request) = accept_request(&listener);
            assert_auth(&request, "old-access", "old-segment");
            old_seen_tx.send(()).unwrap();
            let (mut new, request) = accept_request(&listener);
            Operation::Set.assert_request(&request); // old read-hash is not inherited
            assert_auth(&request, "replacement-access", "replacement-segment");
            respond(
                &mut new,
                200,
                &serde_json::json!([{"hash":"new-account-hash"}]),
            );
            release_old_rx.recv_timeout(Duration::from_secs(8)).unwrap();
            respond(
                &mut old,
                200,
                &serde_json::json!([{
                    "hash":"late-old-hash", "value":"late-old-value", "encoding":"SDKv1"
                }]),
            );
            let (mut probe, request) = accept_request(&listener);
            assert_eq!(form_fields(body(&request))["hash"], "new-account-hash");
            respond(&mut probe, 200, &Operation::Set.response());
        });
        Operation::Get.start(&runtime);
        wait_for(&runtime, "storage_done");
        runtime
            .execute_source(
                r#"
            old_callback=false
            _G.SkynestStorage.native_getKey("nick/name",function() old_callback=true end)
        "#,
            )
            .unwrap();
        old_seen_rx.recv_timeout(Duration::from_secs(8)).unwrap();
        if let Some(change_provider) = boundary {
            replace_account(&runtime, &origin, change_provider);
        }
        runtime.skynest_account.seed_test_tokens(
            false,
            "replacement-access",
            "replacement-refresh",
            "replacement-segment",
        );
        Operation::Set.start(&runtime);
        wait_for(&runtime, "storage_done");
        release_old_tx.send(()).unwrap();
        wait_pending(&runtime);
        dispatch_registered_application_events(runtime.lua()).unwrap();
        assert!(
            !game_environment(runtime.lua())
                .unwrap()
                .get::<bool>("old_callback")
                .unwrap()
        );
        Operation::Set.start(&runtime);
        wait_for(&runtime, "storage_done");
        server.join().unwrap();
    }
}

#[test]
fn storage_session_late_cloud_load_cannot_complete_or_clear_replacement_transaction() {
    for change_provider in [false, true] {
        let listener = listener();
        let origin = format!("http://{}", listener.local_addr().unwrap());
        let sandbox = ShippedDataSandbox::new("storage-late-cloud-transaction");
        let runtime = StellaLua::new(&sandbox.data_root).unwrap();
        configure(&runtime, &listener);
        runtime
            .execute_source(
                r#"
            _G.SkynestAccount.onLoginSuccess=function() login_done=true end
            _G.SkynestAccount.onLoginFailure=function() error("fixture account login failed") end
            cloud_count=0
            _G.SkynestStorage.cloudDataSync=function(document)
                cloud_count=cloud_count+1; cloud_coins=document.coins; cloud_done=true
            end
            notifyEventManager=function(name)
                if name=="EID_SYNC_CLOUD_COMPLETED" then save_done=true end
            end
        "#,
            )
            .unwrap();
        let (old_seen_tx, old_seen_rx) = mpsc::channel();
        let (new_seen_tx, new_seen_rx) = mpsc::channel();
        let (release_old_tx, release_old_rx) = mpsc::channel();
        let (release_new_tx, release_new_rx) = mpsc::channel();
        let server = thread::spawn(move || {
            acquire(&listener, None, "old-account-access");
            let (mut old, request) = accept_request(&listener);
            assert!(request.starts_with(
                "GET /storage/1.0/state?key=%5Bmy%5D%2F%5Bclient%5D%2FPurpleState HTTP/1.1"
            ));
            assert_auth(&request, "old-account-access", "8, 9");
            old_seen_tx.send(()).unwrap();
            acquire_at(
                &listener,
                if change_provider {
                    "/replacement"
                } else {
                    "/proxy"
                },
                None,
                "new-account-access",
            );
            let (mut new, request) = accept_request(&listener);
            assert_auth(&request, "new-account-access", "8, 9");
            new_seen_tx.send(()).unwrap();
            release_old_rx.recv_timeout(Duration::from_secs(8)).unwrap();
            respond(
                &mut old,
                200,
                &serde_json::json!([{"hash":"old-cloud-hash","value":"coins = 111\n","encoding":"SDKv1"}]),
            );
            release_new_rx.recv_timeout(Duration::from_secs(8)).unwrap();
            respond(
                &mut new,
                200,
                &serde_json::json!([{"hash":"new-cloud-hash","value":"coins = 222\n","encoding":"SDKv1"}]),
            );
            let (mut save, request) = accept_request(&listener);
            let fields = form_fields(body(&request));
            assert_eq!(fields["key"], "[my]/[client]/PurpleState");
            assert_eq!(fields["hash"], "new-cloud-hash");
            assert_eq!(decode_sdkv2(&fields["value"]), "coins = 333\n");
            respond(
                &mut save,
                200,
                &serde_json::json!([{"hash":"saved-cloud-hash"}]),
            );
        });
        runtime
            .execute_source("login_done=false; _G.SkynestAccount.native_login(false,false,false)")
            .unwrap();
        wait_for(&runtime, "login_done");
        runtime
            .execute_source("assert(_G.SkynestStorage.native_loadCloudSettings())")
            .unwrap();
        old_seen_rx.recv_timeout(Duration::from_secs(8)).unwrap();
        replace_account(&runtime, &origin, change_provider);
        runtime
            .execute_source("login_done=false; _G.SkynestAccount.native_login(false,false,false)")
            .unwrap();
        wait_for(&runtime, "login_done");
        runtime
            .execute_source("assert(_G.SkynestStorage.native_loadCloudSettings())")
            .unwrap();
        new_seen_rx.recv_timeout(Duration::from_secs(8)).unwrap();
        release_old_tx.send(()).unwrap();
        wait_pending(&runtime);
        dispatch_registered_application_events(runtime.lua()).unwrap();
        runtime
            .execute_source(
                r#"
            assert(cloud_count==0)
            assert(_G.SkynestStorage.native_isTransactionInProcess())
            assert(not _G.SkynestStorage.native_saveCloudSettings({coins=999}))
        "#,
            )
            .unwrap();
        release_new_tx.send(()).unwrap();
        wait_for(&runtime, "cloud_done");
        runtime
            .execute_source(
                r#"
            assert(cloud_count==1 and cloud_coins==222)
            assert(not _G.SkynestStorage.native_isTransactionInProcess())
            assert(_G.SkynestStorage.native_saveCloudSettings({coins=333}))
        "#,
            )
            .unwrap();
        wait_for(&runtime, "save_done");
        server.join().unwrap();
    }
}

#[test]
fn storage_session_url_switch_during_401_or_renewal_never_replays_old_service() {
    for during_renewal in [false, true] {
        let listener = listener();
        let origin = format!("http://{}", listener.local_addr().unwrap());
        let sandbox = ShippedDataSandbox::new("storage-url-switch-401");
        let runtime = StellaLua::new(&sandbox.data_root).unwrap();
        configure(&runtime, &listener);
        runtime
            .execute_source(
                r#"
            _G.SkynestAccount.onLoginSuccess=function() login_done=true end
            _G.SkynestAccount.onLoginFailure=function() error("fixture login failed") end
            cloud_count=0
            _G.SkynestStorage.cloudDataSync=function(document)
                cloud_count=cloud_count+1; cloud_coins=document.coins; cloud_done=true
            end
            notifyEventManager=function(name)
                if name=="EID_SYNC_CLOUD_COMPLETED" then save_done=true end
            end
        "#,
            )
            .unwrap();
        let (old_seen_tx, old_seen_rx) = mpsc::channel();
        let (new_seen_tx, new_seen_rx) = mpsc::channel();
        let (release_old_tx, release_old_rx) = mpsc::channel();
        let (release_new_tx, release_new_rx) = mpsc::channel();
        let server = thread::spawn(move || {
            acquire(&listener, None, "old-access");
            let (mut old, request) = accept_request(&listener);
            assert!(request.starts_with("GET /storage/1.0/state?key="));
            assert_auth(&request, "old-access", "8, 9");
            let mut renewal = if during_renewal {
                respond(&mut old, 401, &serde_json::json!({}));
                let (stream, request) = accept_request(&listener);
                assert_eq!(
                    request.lines().next(),
                    Some("POST /proxy/session/1/apps/storage-fixture/sessions HTTP/1.1")
                );
                assert_eq!(
                    serde_json::from_str::<serde_json::Value>(body(&request)).unwrap()["refresh"],
                    serde_json::json!({"token":"rotated-refresh"})
                );
                Some(stream)
            } else {
                None
            };
            old_seen_tx.send(()).unwrap();
            release_old_rx.recv_timeout(Duration::from_secs(8)).unwrap();
            if let Some(renewal) = renewal.as_mut() {
                respond(
                    renewal,
                    200,
                    &serde_json::json!({
                        "userAuth":{"accessToken":"renewed-access","refreshToken":"second-refresh","expiresIn":3600},
                        "segments":[4,5],"config":{},"profile":{"publicAccountId":"storage-account"}
                    }),
                );
            } else {
                respond(&mut old, 401, &serde_json::json!({}));
            }
            // The next HTTP request must be the new transaction, never an old
            // URL replay (nor an unnecessary renewal after the cancelled 401).
            let (mut new, request) = accept_request(&listener);
            assert_eq!(
                request.lines().next(),
                Some(
                    "GET /replacement-storage/1.0/state?key=%5Bmy%5D%2F%5Bclient%5D%2FPurpleState HTTP/1.1"
                )
            );
            assert_auth(
                &request,
                if during_renewal {
                    "renewed-access"
                } else {
                    "old-access"
                },
                if during_renewal { "4, 5" } else { "8, 9" },
            );
            new_seen_tx.send(()).unwrap();
            release_new_rx.recv_timeout(Duration::from_secs(8)).unwrap();
            respond(
                &mut new,
                200,
                &serde_json::json!([{"hash":"replacement-cloud-hash","encoding":"SDKv1","value":"coins = 444\n"}]),
            );
            let (mut save, request) = accept_request(&listener);
            assert_eq!(
                request.lines().next(),
                Some("POST /replacement-storage/1.0/state HTTP/1.1")
            );
            assert_eq!(
                form_fields(body(&request))["hash"],
                "replacement-cloud-hash"
            );
            respond(
                &mut save,
                200,
                &serde_json::json!([{"hash":"replacement-saved-hash"}]),
            );
            listener
        });
        runtime
            .execute_source("_G.SkynestAccount.native_login(false,false,false)")
            .unwrap();
        wait_for(&runtime, "login_done");
        runtime
            .execute_source("assert(_G.SkynestStorage.native_loadCloudSettings())")
            .unwrap();
        old_seen_rx.recv_timeout(Duration::from_secs(8)).unwrap();
        runtime
            .set_storage_url(&format!("{origin}/replacement-storage/1.0"))
            .unwrap();
        runtime
            .execute_source(
                r#"
            assert(_G.SkynestStorage.native_loadCloudSettings())
            assert(_G.SkynestStorage.native_isTransactionInProcess())
        "#,
            )
            .unwrap();
        release_old_tx.send(()).unwrap();
        new_seen_rx.recv_timeout(Duration::from_secs(8)).unwrap();
        wait_pending(&runtime);
        dispatch_registered_application_events(runtime.lua()).unwrap();
        runtime
            .execute_source(
                r#"
            assert(cloud_count==0)
            assert(_G.SkynestStorage.native_isTransactionInProcess())
        "#,
            )
            .unwrap();
        release_new_tx.send(()).unwrap();
        wait_for(&runtime, "cloud_done");
        runtime
            .execute_source(
                r#"
            assert(cloud_count==1 and cloud_coins==444)
            assert(not _G.SkynestStorage.native_isTransactionInProcess())
            assert(_G.SkynestStorage.native_saveCloudSettings({coins=555}))
        "#,
            )
            .unwrap();
        wait_for(&runtime, "save_done");
        assert!(
            matches!(server.join().unwrap().accept(),Err(error) if error.kind()==std::io::ErrorKind::WouldBlock)
        );
    }
}
