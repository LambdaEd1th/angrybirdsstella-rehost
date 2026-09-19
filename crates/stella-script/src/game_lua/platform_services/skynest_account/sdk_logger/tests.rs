use super::super::{AccessResponse, ClientSigning, IdentityEndpoint, identifiers::Identifiers};
use super::*;
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    io::{Read, Write},
    net::{TcpListener, TcpStream},
    thread,
    time::Duration,
};

struct Request {
    path: String,
    headers: BTreeMap<String, String>,
    body: Vec<u8>,
}

fn setup() -> (IdentitySession, IdentityConfig, TcpListener) {
    let server = TcpListener::bind("127.0.0.1:0").unwrap();
    server.set_nonblocking(true).unwrap();
    let session = IdentitySession::default();
    session.install_flat(&AccessResponse {
        access_token: "synthetic-access".into(),
        refresh_token: "synthetic-refresh".into(),
        absolute_expiry: 0,
        segment: Some("synthetic-segment".into()),
    });
    let config = IdentityConfig {
        endpoint: IdentityEndpoint::parse(&format!(
            "http://{}/proxy/identity/3.0",
            server.local_addr().unwrap()
        ))
        .unwrap(),
        client_id: "synthetic/client".into(),
        signing: ClientSigning::default(),
        identifiers: Identifiers::synthetic().into(),
    };
    (session, config, server)
}

fn accept(server: &TcpListener) -> (TcpStream, Request) {
    let deadline = Instant::now() + Duration::from_secs(5);
    let mut stream = loop {
        match server.accept() {
            Ok((stream, _)) => break stream,
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                assert!(Instant::now() < deadline, "log request missing");
                thread::sleep(Duration::from_millis(1));
            }
            Err(error) => panic!("{error}"),
        }
    };
    stream.set_nonblocking(false).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(5)))
        .unwrap();
    let mut bytes = Vec::new();
    let mut part = [0; 1024];
    let boundary = loop {
        let n = stream.read(&mut part).unwrap();
        assert_ne!(n, 0);
        bytes.extend_from_slice(&part[..n]);
        if let Some(n) = bytes.windows(4).position(|v| v == b"\r\n\r\n") {
            break n + 4;
        }
    };
    let head = std::str::from_utf8(&bytes[..boundary]).unwrap();
    let mut lines = head.lines();
    let path = lines.next().unwrap().to_owned();
    let headers: BTreeMap<_, _> = lines
        .filter_map(|line| line.split_once(':'))
        .map(|(k, v)| (k.to_ascii_lowercase(), v.trim().to_owned()))
        .collect();
    let len: usize = headers["content-length"].parse().unwrap();
    while bytes.len() < boundary + len {
        let n = stream.read(&mut part).unwrap();
        assert_ne!(n, 0);
        bytes.extend_from_slice(&part[..n]);
    }
    (
        stream,
        Request {
            path,
            headers,
            body: bytes[boundary..boundary + len].to_vec(),
        },
    )
}

fn respond(mut stream: TcpStream, status: u16, body: &str) {
    write!(stream,"HTTP/1.1 {status} Response\r\nContent-Length: {}\r\nContent-Type: application/json\r\nConnection: close\r\n\r\n{body}",body.len()).unwrap();
}

fn settled(session: &IdentitySession) -> SdkLogSnapshot {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let snapshot = session.sdk_logger.snapshot();
        if snapshot.in_flight_batches == 0 {
            return snapshot;
        }
        assert!(Instant::now() < deadline, "SDK log worker did not settle");
        thread::sleep(Duration::from_millis(1));
    }
}

#[test]
fn sdk_log_short_device_id_matches_independent_crc32_little_endian_base64() {
    for (source, expected) in [
        ("", "AAAAAA"),
        ("123456789", "Jjn0yw"),
        ("00112233445566778899AABBCCDDEEFF", "YG16BQ"),
        ("AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA", "MIzpKg"),
    ] {
        assert_eq!(short_device_id(source), expected);
    }
}

#[test]
fn sdk_log_config_empty_case_and_off_preserve_native_listener_lifetime() {
    let (session, config, _server) = setup();
    let logger = &session.sdk_logger;
    logger.configure(&config, "");
    assert!(logger.state.lock().unwrap().binding.is_none());
    logger.configure(&config, "debug");
    assert!(!logger.snapshot().listening);
    assert!(logger.state.lock().unwrap().binding.is_some());
    for (value, level) in [
        ("DEBUG", SdkLogLevel::Debug),
        ("INFO", SdkLogLevel::Info),
        ("WARN", SdkLogLevel::Warn),
        ("ERROR", SdkLogLevel::Error),
    ] {
        logger.configure(&config, value);
        assert_eq!(logger.snapshot().threshold, level);
        assert!(logger.snapshot().listening);
        logger.configure(&config, "");
        assert_eq!(logger.snapshot().threshold, level);
    }
    assert!(logger.submit_at(&session, SdkLogLevel::Error, "native", "retained", 11));
    logger.configure(&config, "OFF");
    assert!(logger.snapshot().listening);
    assert!(!logger.submit_at(&session, SdkLogLevel::Error, "native", "filtered", 12));
    assert!(logger.submit_at(&session, SdkLogLevel::Off, "native", "level zero", 13));
    assert_eq!(logger.snapshot().queued_records, 2);
    let first_url = logger
        .state
        .lock()
        .unwrap()
        .binding
        .as_ref()
        .unwrap()
        .url
        .clone();
    let mut other = config.clone();
    other.client_id = "other".into();
    logger.configure(&other, "DEBUG");
    assert_eq!(
        logger.state.lock().unwrap().binding.as_ref().unwrap().url,
        first_url
    );
}

#[test]
fn sdk_log_ten_records_send_real_json_with_provider_headers_and_no_lua_fields() {
    let (session, config, server) = setup();
    let logger = &session.sdk_logger;
    logger.configure(&config, "INFO");
    assert!(!logger.submit_at(&session, SdkLogLevel::Debug, "ignored", "filtered", 0));
    for index in 0..9 {
        assert!(logger.submit_at(
            &session,
            SdkLogLevel::Info,
            "tag\0é",
            &format!("record-{index}\n"),
            1700000000000 + index
        ));
    }
    assert!(matches!(server.accept(),Err(error) if error.kind()==std::io::ErrorKind::WouldBlock));
    assert!(logger.submit_at(&session, SdkLogLevel::Error, "last", "tenth", 1700000000009));
    let (stream, request) = accept(&server);
    let id = short_device_id(&config.identifiers.persistent_guid);
    assert_eq!(
        request.path,
        format!("POST /proxy/session/1/apps/synthetic%2Fclient/test_devices/{id}/logs HTTP/1.1")
    );
    assert_eq!(request.headers["x-access-token"], "synthetic-access");
    assert_eq!(request.headers["rovio-sgs"], "synthetic-segment");
    assert_eq!(request.headers["content-type"], "application/json");
    let body: Value = serde_json::from_slice(&request.body).unwrap();
    assert_eq!(body.as_object().unwrap().len(), 1);
    assert_eq!(body["logs"].as_array().unwrap().len(), 10);
    assert_eq!(
        body["logs"][0],
        json!({"message":"record-0\n","tag":"tag\0é","time":1700000000000u64,"level":"INFO"})
    );
    assert_eq!(
        body["logs"][9],
        json!({"message":"tenth","tag":"last","time":1700000000009u64,"level":"ERROR"})
    );
    respond(stream, 200, "{}");
    let state = settled(&session);
    assert_eq!(state.completed_batches, 1);
    assert_eq!(state.queued_records, 0);
    assert_eq!(state.failed_batches, 0);
}

#[test]
fn sdk_log_http_exception_is_fatal_without_false_deregistration_or_retry() {
    let (session, config, server) = setup();
    let logger = &session.sdk_logger;
    logger.configure(&config, "WARN");
    logger.submit_at(&session, SdkLogLevel::Error, "native", "first", 17);
    logger.flush_timer(&session, 0);
    let (stream, _) = accept(&server);
    respond(stream, 403, "rejected");
    let state = settled(&session);
    assert_eq!(state.failed_batches, 1);
    assert!(state.listening);
    assert_eq!(state.queued_records, 0);
    assert_eq!(state.last_error.as_deref(), Some("identity status 403"));
    assert!(state.fatal);
    assert!(!logger.submit_at(&session, SdkLogLevel::Warn, "native", "second", 18));
    logger.flush_timer(&session, 0);
    assert!(matches!(server.accept(),Err(error) if error.kind()==std::io::ErrorKind::WouldBlock));
}

#[test]
fn sdk_log_401_replays_same_device_batch_after_actual_session_renewal() {
    let (session, config, server) = setup();
    let previous: super::super::ProfileResponse =
        serde_json::from_value(json!({"publicAccountId":"previous-account"})).unwrap();
    session
        .install_profile_if_epoch(session.epoch(), &previous)
        .unwrap();
    let previous_lifetime = session.storage_lifetime();
    let logger = &session.sdk_logger;
    logger.configure(&config, "DEBUG");
    logger.submit_at(&session, SdkLogLevel::Info, "native", "frozen", 22);
    logger.flush_timer(&session, 0);
    let (stream, first) = accept(&server);
    respond(stream, 401, "unauthorized");
    let (stream, renewal) = accept(&server);
    assert_eq!(
        renewal.path,
        "POST /proxy/session/1/apps/synthetic%2Fclient/sessions HTTP/1.1"
    );
    let body: Value = serde_json::from_slice(&renewal.body).unwrap();
    assert_eq!(body["refresh"]["token"], "synthetic-refresh");
    respond(stream,200,&json!({"userAuth":{"accessToken":"renewed","refreshToken":"new-refresh","expiresIn":3600},"segments":[2,7],"profile":{"publicAccountId":"renewed-account"},"config":{"device.logLevel":"ERROR"}}).to_string());
    let (stream, second) = accept(&server);
    assert_eq!(second.path, first.path);
    assert_eq!(second.body, first.body);
    assert_eq!(second.headers["x-access-token"], "renewed");
    assert_eq!(second.headers["rovio-sgs"], "2, 7");
    respond(stream, 200, "{}");
    let state = settled(&session);
    assert_eq!(state.completed_batches, 1);
    assert_eq!(state.threshold, SdkLogLevel::Error);
    assert_ne!(
        session.storage_lifetime().1,
        previous_lifetime.1,
        "this device request must actually cross its own renewal's account generation"
    );
}

#[test]
fn sdk_log_provider_reset_cancels_old_timer_and_inflight_status() {
    let (session, config, server) = setup();
    let logger = &session.sdk_logger;
    logger.configure(&config, "DEBUG");
    logger.submit_at(&session, SdkLogLevel::Info, "native", "old", 31);
    logger.flush_timer(&session, 0);
    let (stream, _) = accept(&server);
    session.detach_store();
    respond(stream, 403, "old error");
    logger.configure(&config, "WARN");
    logger.submit_at(&session, SdkLogLevel::Warn, "native", "new", 32);
    logger.flush_timer(&session, 0);
    drop(logger.send_gate.lock().unwrap());
    let state = logger.snapshot();
    assert_eq!(state.queued_records, 1);
    assert_eq!(state.failed_batches, 0);
    assert_eq!(state.in_flight_batches, 0);
    assert!(!state.fatal);
    assert!(matches!(server.accept(),Err(error) if error.kind()==std::io::ErrorKind::WouldBlock));
}

fn host_fixture() -> crate::StellaLua {
    let root = std::env::temp_dir().join(format!(
        "stella-sdk-log-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir(&root).unwrap();
    crate::StellaLua::new(root).unwrap()
}

#[test]
fn sdk_log_real_dispatcher_flushes_at_five_seconds_and_keeps_one_off_timer() {
    let host = host_fixture();
    let (seed, config, server) = setup();
    let session = &host.skynest_account.session;
    session.install_flat(&AccessResponse {
        access_token: "synthetic-access".into(),
        refresh_token: "synthetic-refresh".into(),
        absolute_expiry: 0,
        segment: None,
    });
    drop(seed);
    let logger = &session.sdk_logger;
    logger.configure(&config, "INFO");
    assert!(host.submit_sdk_log(SdkLogLevel::Info, "native", "before OFF"));
    let dispatch = |delta| {
        host.application_event_dispatcher
            .dispatch(host.lua(), delta)
            .unwrap()
    };
    dispatch(4.0);
    logger.configure(&config, "DEBUG"); // active reconfiguration must not add a timer
    logger.configure(&config, "OFF");
    dispatch(0.5);
    assert_eq!(host.sdk_log_snapshot().queued_records, 1);
    assert!(matches!(server.accept(),Err(error) if error.kind()==std::io::ErrorKind::WouldBlock));
    dispatch(0.5);
    let (stream, request) = accept(&server);
    let body: Value = serde_json::from_slice(&request.body).unwrap();
    assert_eq!(body["logs"][0]["message"], "before OFF");
    assert!(body["logs"][0]["time"].as_u64().unwrap() > 1_000_000_000_000);
    respond(stream, 200, "{}");
    assert_eq!(settled(session).completed_batches, 1);
    dispatch(5.0); // empty queue still reschedules, no empty HTTP request
    assert!(matches!(server.accept(),Err(error) if error.kind()==std::io::ErrorKind::WouldBlock));
    assert!(!host.submit_sdk_log(SdkLogLevel::Error, "native", "filtered"));
    logger.configure(&config, "WARN");
    assert!(host.submit_sdk_log(SdkLogLevel::Warn, "native", "second interval"));
    dispatch(4.0); // duplicate reconfiguration timer would fire here
    assert_eq!(host.sdk_log_snapshot().queued_records, 1);
    dispatch(1.0);
    let (stream, request) = accept(&server);
    let body: Value = serde_json::from_slice(&request.body).unwrap();
    assert_eq!(body["logs"].as_array().unwrap().len(), 1);
    assert_eq!(body["logs"][0]["message"], "second interval");
    respond(stream, 200, "{}");
    assert_eq!(settled(session).completed_batches, 2);
}

#[test]
fn sdk_log_actual_session_config_publishes_before_dispatch_and_cached_session_does_not_reset() {
    let host = host_fixture();
    let (_, config, server) = setup();
    let session = &host.skynest_account.session;
    let worker = {
        let session = session.clone();
        let config = config.clone();
        thread::spawn(move || session.acquire_session(&config))
    };
    let (stream, request) = accept(&server);
    assert!(request.path.ends_with("/sessions HTTP/1.1"));
    respond(stream,200,&json!({"userAuth":{"accessToken":"synthetic-a","refreshToken":"synthetic-r","expiresIn":3600},"segments":[],"profile":{"publicAccountId":"synthetic-account"},"config":{"device.logLevel":"INFO"}}).to_string());
    worker.join().unwrap().unwrap();
    assert!(host.sdk_log_snapshot().listening);
    assert!(host.submit_sdk_log(SdkLogLevel::Info, "native", "queued"));
    // No Lua dispatch is needed to activate the listener. Cache hits do not
    // reinterpret config or rebuild its queue; ordinary logout retains it too.
    session.acquire_session(&config).unwrap();
    session.logout().unwrap();
    assert_eq!(host.sdk_log_snapshot().queued_records, 1);
    assert_eq!(host.sdk_log_snapshot().threshold, SdkLogLevel::Info);
    assert!(host.sdk_log_snapshot().listening);
    assert!(matches!(server.accept(),Err(error) if error.kind()==std::io::ErrorKind::WouldBlock));
}

#[test]
fn sdk_log_second_401_is_terminal_for_batch() {
    let (session, config, server) = setup();
    let logger = &session.sdk_logger;
    logger.configure(&config, "DEBUG");
    logger.submit_at(&session, SdkLogLevel::Info, "native", "once", 22);
    logger.flush_timer(&session, 0);
    let (stream, first) = accept(&server);
    respond(stream, 401, "unauthorized");
    let (stream, _) = accept(&server);
    respond(stream,200,&json!({"userAuth":{"accessToken":"renewed","refreshToken":"new-refresh","expiresIn":3600},"segments":[],"profile":{"publicAccountId":"renewed-account"},"config":{}}).to_string());
    let (stream, retry) = accept(&server);
    assert_eq!(retry.body, first.body);
    respond(stream, 401, "still unauthorized");
    let state = settled(&session);
    assert_eq!(state.failed_batches, 1);
    assert_eq!(state.last_error.as_deref(), Some("identity status 401"));
    assert!(state.listening);
    assert!(state.fatal);
    assert_eq!(state.queued_records, 0);
    assert!(matches!(server.accept(),Err(error) if error.kind()==std::io::ErrorKind::WouldBlock));
}

#[test]
fn sdk_log_unhandled_worker_error_reaches_host_frame_and_remains_terminal() {
    let host = host_fixture();
    let (_, config, server) = setup();
    let session = &host.skynest_account.session;
    session.install_flat(&AccessResponse {
        access_token: "synthetic-access".into(),
        refresh_token: "synthetic-refresh".into(),
        absolute_expiry: 0,
        segment: None,
    });
    session.sdk_logger.configure(&config, "INFO");
    assert!(host.submit_sdk_log(SdkLogLevel::Info, "native", "fatal batch"));
    host.application_event_dispatcher
        .dispatch(host.lua(), 5.0)
        .unwrap();
    let (stream, _) = accept(&server);
    respond(stream, 503, "unavailable");
    assert!(settled(session).fatal);
    for _ in 0..2 {
        let error = host.update(0.0).unwrap_err().to_string();
        assert!(
            error.contains("SDK log thread terminated: identity status 503"),
            "{error}"
        );
    }
    session.detach_store();
    session.sdk_logger.configure(&config, "DEBUG");
    assert!(
        host.update(0.0).is_err(),
        "provider replacement cannot erase a completed fatal error"
    );
}

#[test]
fn sdk_log_logout_retires_inflight_result_as_host_cancellation() {
    let (session, config, server) = setup();
    let logger = &session.sdk_logger;
    logger.configure(&config, "INFO");
    logger.submit_at(&session, SdkLogLevel::Info, "native", "old owner", 1);
    logger.flush_timer(&session, 0);
    let (stream, _) = accept(&server);
    session.logout().unwrap();
    respond(stream, 403, "retired response");
    let state = settled(&session);
    assert_eq!(state.cancelled_batches, 1);
    assert_eq!(state.failed_batches, 0);
    assert!(!state.fatal);
    assert!(state.listening);
}

#[test]
fn sdk_log_batches_serialize_and_reset_cancels_a_waiting_worker_before_http() {
    let (session, config, server) = setup();
    let logger = &session.sdk_logger;
    logger.configure(&config, "INFO");
    for index in 0..10 {
        assert!(logger.submit_at(&session, SdkLogLevel::Info, "native", "first", index));
    }
    let (stream, _) = accept(&server); // first worker owns the send mutex
    for index in 0..10 {
        assert!(logger.submit_at(&session, SdkLogLevel::Info, "native", "second", index));
    }
    assert_eq!(logger.snapshot().in_flight_batches, 2);
    assert_eq!(logger.snapshot().queued_records, 0);
    session.detach_store();
    respond(stream, 200, "{}");
    // Serialize a current third worker behind both old workers. Its completed
    // result proves old workers cannot later publish/send a retired batch.
    session.install_flat(&AccessResponse {
        access_token: "new-access".into(),
        refresh_token: "new-refresh".into(),
        absolute_expiry: 0,
        segment: None,
    });
    logger.configure(&config, "INFO");
    logger.submit_at(&session, SdkLogLevel::Info, "native", "third", 3);
    logger.flush_timer(&session, 1);
    let (stream, request) = accept(&server);
    let body: Value = serde_json::from_slice(&request.body).unwrap();
    assert_eq!(body["logs"].as_array().unwrap().len(), 1);
    assert_eq!(body["logs"][0]["message"], "third");
    assert_eq!(request.headers["x-access-token"], "new-access");
    respond(stream, 200, "{}");
    let state = settled(&session);
    assert_eq!(state.completed_batches, 1);
    assert_eq!(state.failed_batches, 0);
    assert!(!state.fatal);
}
