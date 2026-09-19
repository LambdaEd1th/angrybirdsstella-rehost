use super::super::super::{ClientSigning, IdentityEndpoint, identifiers::Identifiers};
use super::*;
use serde_json::json;
use std::{
    collections::BTreeMap,
    net::{TcpListener, TcpStream},
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

struct Fixture {
    root: std::path::PathBuf,
    session: IdentitySession,
    store: Arc<RegistryStore>,
}

impl Fixture {
    fn new() -> Self {
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let root = std::env::temp_dir().join(format!(
            "stella-avatar-assets-{}-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos(),
            NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        ));
        fs::create_dir(&root).unwrap();
        let store = Arc::new(RegistryStore::open(root.join("fusion.registry")).unwrap());
        let session = IdentitySession::with_refresh_store(store.clone());
        session.bind_success_events(crate::ApplicationEventScheduler::default());
        session.install_flat(&access("old"));
        session
            .install_profile_if_epoch(
                session.epoch(),
                &protocol::parse_profile_value(&json!({"publicAccountId":"same-account"})),
            )
            .unwrap();
        Self {
            root,
            session,
            store,
        }
    }

    fn stage(&self, images: Value) -> (OwnProfileOwner, PreparedLoginProfile, ProfileResponse) {
        let profile = protocol::parse_profile_value(&json!({
            "publicAccountId":"same-account", "personal":{"email":"synthetic@example.invalid","avatarId":"native-avatar", "imageAssets":images}
        }));
        let mut owner = self
            .session
            .own_profile_owner_for_request(self.session.request_owner(ProviderLevel::Level2))
            .unwrap();
        let before = self.session.login_profile_identity(owner).unwrap();
        let prepared = self
            .session
            .prepare_login_profile(&mut owner, &profile, before)
            .unwrap()
            .unwrap();
        (owner, prepared, profile)
    }

    fn path(&self, basename: &str) -> std::path::PathBuf {
        self.root.join("avatarAssets").join(basename)
    }

    fn start(
        &self,
        owner: OwnProfileOwner,
        profile: &ProfileResponse,
    ) -> thread::JoinHandle<Result<(), String>> {
        let session = self.session.clone();
        let assets = profile.avatar_assets.clone();
        thread::spawn(move || session.fetch_avatar_assets(owner, &assets))
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

fn access(label: &str) -> AccessResponse {
    AccessResponse {
        access_token: format!("{label}-access"),
        refresh_token: format!("{label}-refresh"),
        absolute_expiry: 0,
        segment: Some(format!("{label}-segment")),
    }
}

fn server() -> (TcpListener, String) {
    let server = TcpListener::bind("127.0.0.1:0").unwrap();
    server.set_nonblocking(true).unwrap();
    let url = format!("http://{}", server.local_addr().unwrap());
    (server, url)
}

struct Request {
    line: String,
    headers: BTreeMap<String, String>,
    body: Vec<u8>,
}

fn accept(server: &TcpListener) -> (TcpStream, Request) {
    let deadline = Instant::now() + Duration::from_secs(5);
    let mut stream = loop {
        match server.accept() {
            Ok((stream, _)) => break stream,
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                assert!(Instant::now() < deadline, "asset request missing");
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
    let mut buffer = [0; 2048];
    let boundary = loop {
        let count = stream.read(&mut buffer).unwrap();
        assert_ne!(count, 0);
        bytes.extend_from_slice(&buffer[..count]);
        if let Some(index) = bytes.windows(4).position(|part| part == b"\r\n\r\n") {
            break index + 4;
        }
    };
    let head = std::str::from_utf8(&bytes[..boundary]).unwrap();
    let mut lines = head.lines();
    let line = lines.next().unwrap().to_owned();
    let headers: BTreeMap<_, _> = lines
        .filter_map(|line| line.split_once(':'))
        .map(|(k, v)| (k.to_ascii_lowercase(), v.trim().to_owned()))
        .collect();
    let size: usize = headers
        .get("content-length")
        .map_or(0, |s| s.parse().unwrap());
    while bytes.len() < boundary + size {
        let n = stream.read(&mut buffer).unwrap();
        assert_ne!(n, 0);
        bytes.extend_from_slice(&buffer[..n]);
    }
    (
        stream,
        Request {
            line,
            headers,
            body: bytes[boundary..boundary + size].to_vec(),
        },
    )
}

fn respond(mut stream: TcpStream, status: u16, bytes: &[u8]) {
    write!(
        stream,
        "HTTP/1.1 {status} Response\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        bytes.len()
    )
    .unwrap();
    stream.write_all(bytes).unwrap();
}

fn image(url: &str, hash: &str, size: i64, dimension: i32) -> Value {
    json!({"url":url,"hash":hash,"size":size,"dimension":dimension})
}

#[test]
fn avatar_asset_parser_preserves_native_field_inheritance_and_number_narrowing() {
    let entries = parse_avatar_assets(&json!({"personal":{"avatarId":"id","imageAssets":[
        {"url":"one","hash":"hash1","dimension":64.9,"size":3.9},
        {"url":"ignored without paired hash","dimension":-1,"size":true},
        null,
        {"url":"","hash":"","dimension":4294967295i64,"size":-5}
    ]}}));
    assert_eq!(entries.len(), 4);
    assert_eq!(
        entries[0],
        AvatarAsset {
            avatar_id: "id".into(),
            url: "one".into(),
            hash: "hash1".into(),
            dimension: Some(64),
            size: Some(3)
        }
    );
    assert_eq!(entries[1].url, "one");
    assert_eq!(entries[1].hash, "hash1");
    assert_eq!(entries[1].dimension, Some(-1));
    assert_eq!(entries[1].size, Some(3));
    assert_eq!(entries[2], entries[1]);
    assert_eq!(entries[3].url, "");
    assert_eq!(entries[3].hash, "");
    assert_eq!(entries[3].dimension, Some(-1));
    assert_eq!(entries[3].size, Some(-5));
    assert!(parse_avatar_assets(&json!({"personal":[]})).is_empty());
    assert!(parse_avatar_assets(&json!({"personal":{"imageAssets":{}}})).is_empty());
    let empty = parse_avatar_assets(&json!({"personal":{"imageAssets":[{}]}}));
    assert_eq!(empty[0].dimension, None);
    assert_eq!(empty[0].size, None);
}

#[test]
fn avatar_asset_download_is_between_profile_and_tokens_and_has_no_identity_headers() {
    let fixture = Fixture::new();
    let (server, url) = server();
    let (mut owner, prepared, profile) = fixture.stage(json!([image(
        &format!("{url}/image.bin?rev=2"),
        "opaque-version",
        3,
        64
    )]));
    let worker = fixture.start(owner, &profile);
    let (stream, request) = accept(&server);
    assert_eq!(request.line, "GET /image.bin?rev=2 HTTP/1.1");
    for key in ["authorization", "x-access-token", "rovio-sgs"] {
        assert!(!request.headers.contains_key(key));
    }
    assert!(request.body.is_empty());
    assert_eq!(
        fixture.store.load_profile().unwrap(),
        Some(profile.raw.clone())
    );
    assert_eq!(fixture.session.level2_tokens().access_token, "old-access");
    assert!(fixture.session.pop_success_owner().is_none());
    assert_eq!(fs::read(fixture.path("image.bin?rev=2")).unwrap(), b"");
    respond(stream, 200, b"abc");
    worker.join().unwrap().unwrap();
    assert_eq!(fs::read(fixture.path("image.bin?rev=2")).unwrap(), b"abc");
    assert_eq!(
        fixture
            .store
            .load_avatar_version("image.bin?rev=2")
            .unwrap(),
        "opaque-version"
    );
    assert_eq!(
        fixture.session.profile().unwrap().avatar_paths[&64],
        "avatarAssets/image.bin?rev=2"
    );
    assert_eq!(fixture.session.level2_tokens().access_token, "old-access");
    assert!(
        fixture
            .session
            .finish_login_profile(
                &mut owner,
                &access("new"),
                &Identifiers::synthetic(),
                prepared
            )
            .unwrap()
    );
    assert_eq!(fixture.session.level2_tokens().access_token, "new-access");
    assert!(fixture.session.pop_success_owner().is_some());
}

#[test]
fn avatar_asset_cache_uses_opaque_version_and_size_without_a_content_hash_or_http() {
    let fixture = Fixture::new();
    let (server, url) = server();
    fs::create_dir(fixture.root.join("avatarAssets")).unwrap();
    fs::write(fixture.path("same.bin"), b"not a digest-verified image").unwrap();
    fixture
        .store
        .store_avatar_version("same.bin", "arbitrary-version")
        .unwrap();
    let (owner, _, profile) = fixture.stage(json!([image(
        &format!("{url}/same.bin"),
        "arbitrary-version",
        27,
        64
    )]));
    fixture
        .session
        .fetch_avatar_assets(owner, &profile.avatar_assets)
        .unwrap();
    assert_eq!(
        fixture.session.profile().unwrap().avatar_paths[&64],
        "avatarAssets/same.bin"
    );
    assert!(matches!(server.accept(),Err(error) if error.kind()==std::io::ErrorKind::WouldBlock));
}

#[test]
fn avatar_asset_cache_miss_warns_through_actual_sdk_listener_and_uploads_native_record() {
    let fixture = Fixture::new();
    let (server, url) = server();
    fixture
        .store
        .store_avatar_version("missing.bin", "v1")
        .unwrap();
    let config = IdentityConfig {
        endpoint: IdentityEndpoint::parse(&format!("{url}/identity/3.0")).unwrap(),
        client_id: "synthetic-client".into(),
        signing: ClientSigning::default(),
        identifiers: Identifiers::synthetic().into(),
    };
    fixture.session.sdk_logger.configure(&config, "WARN");
    let (owner, _, profile) =
        fixture.stage(json!([image(&format!("{url}/missing.bin"), "v1", 3, 64)]));
    let worker = fixture.start(owner, &profile);
    let (stream, request) = accept(&server);
    assert_eq!(request.line, "GET /missing.bin HTTP/1.1");
    respond(stream, 200, b"abc");
    worker.join().unwrap().unwrap();
    assert_eq!(fixture.session.sdk_logger.snapshot().queued_records, 1);
    fixture.session.sdk_logger.flush_timer(&fixture.session, 0);
    let (stream, request) = accept(&server);
    assert!(request.line.contains("/test_devices/"));
    assert_eq!(request.headers["x-access-token"], "old-access");
    let value: Value = serde_json::from_slice(&request.body).unwrap();
    assert_eq!(value["logs"].as_array().unwrap().len(), 1);
    let record = &value["logs"][0];
    assert_eq!(record["level"], "WARN");
    assert_eq!(record["tag"], "");
    assert!(record["message"].as_str().unwrap().starts_with("Unable to open local file while it was supposed to exist: Failed to open file avatarAssets/missing.bin : "));
    assert!(record["message"].as_str().unwrap().ends_with(' '));
    respond(stream, 200, b"{}");
    let deadline = Instant::now() + Duration::from_secs(5);
    while fixture.session.sdk_logger.snapshot().in_flight_batches != 0 {
        assert!(Instant::now() < deadline);
        thread::sleep(Duration::from_millis(1));
    }
    let snapshot = fixture.session.sdk_logger.snapshot();
    assert_eq!(snapshot.completed_batches, 1);
    assert!(!snapshot.fatal);
}

#[test]
fn avatar_asset_bad_status_preserves_partial_file_old_version_and_prechecked_path() {
    let fixture = Fixture::new();
    let (server, url) = server();
    fs::create_dir(fixture.root.join("avatarAssets")).unwrap();
    fs::write(fixture.path("old.bin"), b"x").unwrap();
    fixture.store.store_avatar_version("old.bin", "v1").unwrap();
    let (owner, _, profile) = fixture.stage(json!([image(&format!("{url}/old.bin"), "v1", 3, 64)]));
    let worker = fixture.start(owner, &profile);
    let (stream, _) = accept(&server);
    assert_eq!(
        fixture.session.profile().unwrap().avatar_paths[&64],
        "avatarAssets/old.bin"
    );
    respond(stream, 201, b"bad");
    assert_eq!(
        worker.join().unwrap().unwrap_err(),
        "avatar asset HTTP status 201"
    );
    assert_eq!(fs::read(fixture.path("old.bin")).unwrap(), b"bad");
    assert_eq!(fixture.store.load_avatar_version("old.bin").unwrap(), "v1");
    assert_eq!(fixture.session.level2_tokens().access_token, "old-access");
    assert!(fixture.session.pop_success_owner().is_none());
}

#[test]
fn avatar_asset_later_size_failure_keeps_earlier_commit_and_new_profile_with_old_tokens() {
    let fixture = Fixture::new();
    let (server, url) = server();
    let (owner, _, profile) = fixture.stage(json!([
        image(&format!("{url}/first.bin"), "one", 3, 32),
        image(&format!("{url}/second.bin"), "two", 5, 64)
    ]));
    let worker = fixture.start(owner, &profile);
    let (stream, _) = accept(&server);
    respond(stream, 200, b"abc");
    let (stream, _) = accept(&server);
    respond(stream, 200, b"bad");
    assert_eq!(worker.join().unwrap().unwrap_err(), "Incorrect filesize");
    assert_eq!(
        fixture.store.load_avatar_version("first.bin").unwrap(),
        "one"
    );
    assert_eq!(fixture.store.load_avatar_version("second.bin").unwrap(), "");
    assert_eq!(fs::read(fixture.path("second.bin")).unwrap(), b"bad");
    let current = fixture.session.profile().unwrap();
    assert_eq!(current.raw, profile.raw);
    assert_eq!(current.avatar_paths.len(), 1);
    assert_eq!(fixture.store.load_profile().unwrap(), Some(profile.raw));
    assert_eq!(fixture.session.level2_tokens().access_token, "old-access");
    assert!(fixture.session.pop_success_owner().is_none());
}

#[test]
fn avatar_asset_empty_version_and_zero_length_still_perform_get_then_accept_native_size() {
    let fixture = Fixture::new();
    let (server, url) = server();
    fs::create_dir(fixture.root.join("avatarAssets")).unwrap();
    fs::write(fixture.path("empty.bin"), b"").unwrap();
    let (owner, _, profile) = fixture.stage(json!([image(&format!("{url}/empty.bin"), "", 0, 64)]));
    let worker = fixture.start(owner, &profile);
    let (stream, _) = accept(&server);
    respond(stream, 200, b"");
    worker.join().unwrap().unwrap();
    assert_eq!(
        fixture.session.profile().unwrap().avatar_paths[&64],
        "avatarAssets/empty.bin"
    );
}

#[test]
fn avatar_asset_retired_owner_cannot_write_remaining_response_or_publish_tokens() {
    let fixture = Fixture::new();
    let (server, url) = server();
    let (mut owner, prepared, profile) =
        fixture.stage(json!([image(&format!("{url}/slow.bin"), "v1", 6, 64)]));
    let worker = fixture.start(owner, &profile);
    let (mut stream, _) = accept(&server);
    stream
        .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 6\r\nConnection: close\r\n\r\nabc")
        .unwrap();
    stream.flush().unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    while fs::metadata(fixture.path("slow.bin")).unwrap().len() != 3 {
        assert!(Instant::now() < deadline);
        thread::sleep(Duration::from_millis(1));
    }
    fixture.session.logout().unwrap();
    stream.write_all(b"def").unwrap();
    drop(stream);
    assert!(worker.join().unwrap().is_err());
    assert_eq!(fs::read(fixture.path("slow.bin")).unwrap(), b"abc");
    assert_eq!(fixture.store.load_avatar_version("slow.bin").unwrap(), "");
    assert!(
        !fixture
            .session
            .finish_login_profile(
                &mut owner,
                &access("new"),
                &Identifiers::synthetic(),
                prepared
            )
            .unwrap()
    );
    assert!(fixture.session.pop_success_owner().is_none());
}

#[test]
fn avatar_asset_native_indeterminate_numeric_slot_is_an_explicit_host_error() {
    let fixture = Fixture::new();
    let (server, url) = server();
    let (owner, _, profile) =
        fixture.stage(json!([{"url":format!("{url}/image.bin"),"hash":"v1","size":3}]));
    assert_eq!(
        fixture
            .session
            .fetch_avatar_assets(owner, &profile.avatar_assets)
            .unwrap_err(),
        "avatar dimension is indeterminate in native input"
    );
    assert!(matches!(server.accept(),Err(error) if error.kind()==std::io::ErrorKind::WouldBlock));
    assert_eq!(fixture.session.level2_tokens().access_token, "old-access");
}
