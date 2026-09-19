use super::*;
use std::{
    net::TcpListener,
    sync::atomic::{AtomicU64, Ordering},
};

struct Fixture {
    root: PathBuf,
    cache: AvatarCache,
}
impl Fixture {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let root = std::env::temp_dir().join(format!(
            "stella-social-cache-{}-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        let cache = AvatarCache::new(root.clone(), SdkLogSink::detached_for_test()).unwrap();
        Self { root, cache }
    }
    fn prepare(&self) {
        self.cache
            .backend
            .prepare(epoch_seconds(SystemTime::now()))
            .unwrap();
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        self.cache.retire();
        let _ = fs::remove_dir_all(&self.root);
    }
}

fn read_request(stream: &mut std::net::TcpStream) -> String {
    stream
        .set_read_timeout(Some(Duration::from_secs(5)))
        .unwrap();
    let mut bytes = Vec::new();
    while !bytes.ends_with(b"\r\n\r\n") {
        let mut byte = [0];
        stream.read_exact(&mut byte).unwrap();
        bytes.push(byte[0]);
    }
    String::from_utf8(bytes).unwrap()
}

#[test]
fn social_avatar_cache_filename_uses_full_url_sha1_and_short_literal_suffix() {
    assert_eq!(
        cache_basename("abc"),
        "A9993E364706816ABA3E25717850C26C9CD0D89D"
    );
    for (url, suffix) in [
        ("http://x/avatar.jpg", ".jpg"),
        ("http://x/avatar.jpeg", ".jpeg"),
        ("http://x/avatar.png?q", ""),
        ("http://x/a.p?x", ".p?x"),
        ("http://x/a.", ""),
        ("http://x/a.longer", ""),
    ] {
        assert_eq!(
            cache_basename(url),
            format!("{}{suffix}", upper_hex(&sha1_digest(url.as_bytes())))
        );
    }
}

#[test]
fn social_avatar_cache_expires_at_seven_whole_days_and_keeps_future_timestamp() {
    let f = Fixture::new();
    let backend = &f.cache.backend;
    backend.prepare(1000000).unwrap();
    let sentinel = backend.directory.join("sentinel");
    fs::write(&sentinel, b"old").unwrap();
    backend.prepare(1000000 + 7 * 86400 - 1).unwrap();
    assert!(sentinel.exists());
    backend.prepare(1000000 + 7 * 86400).unwrap();
    assert!(!sentinel.exists());
    assert_eq!(
        backend.registry.creation_time(DIRECTORY).unwrap(),
        1000000 + 7 * 86400
    );
    fs::write(&sentinel, b"future").unwrap();
    backend.prepare(0).unwrap();
    assert!(sentinel.exists());
    backend.registry.set_creation_time("other", 19).unwrap();
    backend.registry.set_creation_time(DIRECTORY, 22).unwrap();
    assert_eq!(backend.registry.creation_time("other").unwrap(), 19);
    let bytes = fs::read(f.root.join("fusion.registry")).unwrap();
    assert!(!String::from_utf8_lossy(&bytes).contains("SkynestAvatarCreationTime"));
}

#[test]
fn social_avatar_cache_evicts_oldest_unretained_without_reserving_incoming_bytes() {
    let f = Fixture::new();
    f.prepare();
    let dir = &f.cache.backend.directory;
    let mut paths = Vec::new();
    for (index, name) in ["retained", "old", "new"].into_iter().enumerate() {
        let path = dir.join(name);
        fs::write(&path, [0; 4]).unwrap();
        fs::File::options()
            .write(true)
            .open(&path)
            .unwrap()
            .set_modified(UNIX_EPOCH + Duration::from_secs(100 + index as u64))
            .unwrap();
        paths.push(path);
    }
    let retained = BTreeSet::from([paths[0].clone()]);
    evict(dir, &retained, 12).unwrap();
    assert!(paths.iter().all(|p| p.exists()));
    evict(dir, &retained, 8).unwrap();
    assert!(paths[0].exists());
    assert!(!paths[1].exists());
    assert!(paths[2].exists());
    evict(dir, &retained, 0).unwrap();
    assert!(paths[0].exists());
    assert!(!paths[2].exists());
}

#[test]
fn social_avatar_cache_streams_tmp_then_renames_and_sends_plain_get() {
    let f = Fixture::new();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}/opaque", listener.local_addr().unwrap());
    let path = f.cache.backend.directory.join(cache_basename(&url));
    let tmp = path.with_file_name(format!("{}.tmp", cache_basename(&url)));
    let (start_tx, start_rx) = mpsc::channel();
    let (finish_tx, finish_rx) = mpsc::channel();
    let worker = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let request = read_request(&mut stream);
        stream
            .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 6\r\nConnection: close\r\n\r\nabc")
            .unwrap();
        stream.flush().unwrap();
        start_tx.send(request).unwrap();
        finish_rx.recv_timeout(Duration::from_secs(5)).unwrap();
        stream.write_all(b"def").unwrap();
    });
    let (tx, rx) = mpsc::channel();
    f.cache
        .request(url.clone(), move |r| tx.send(r).unwrap())
        .unwrap();
    let request = start_rx
        .recv_timeout(Duration::from_secs(5))
        .unwrap()
        .to_ascii_lowercase();
    assert!(request.starts_with("get /opaque http/1.1"));
    for header in ["authorization:", "x-access-token:", "rovio-sgs"] {
        assert!(!request.contains(header));
    }
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    while fs::metadata(&tmp).map(|m| m.len()).unwrap_or(0) != 3
        && std::time::Instant::now() < deadline
    {
        std::thread::sleep(Duration::from_millis(2));
    }
    assert_eq!(fs::read(&tmp).unwrap(), b"abc");
    assert!(!path.exists());
    finish_tx.send(()).unwrap();
    assert_eq!(
        rx.recv_timeout(Duration::from_secs(5)).unwrap().unwrap(),
        path
    );
    worker.join().unwrap();
    assert_eq!(fs::read(&path).unwrap(), b"abcdef");
    assert!(!tmp.exists());
    // The listener has closed: this succeeds only by taking the disk hit.
    assert_eq!(f.cache.backend.fetch(&url).unwrap(), path);
}

#[test]
fn social_avatar_cache_rejects_non200_and_empty_download_removing_both_files() {
    for (status, body, message) in [
        ("201 Created", "bad", "Created"),
        ("503 Service Unavailable", "bad", "Service Unavailable"),
        ("200 OK", "", "Empty response"),
    ] {
        let f = Fixture::new();
        f.prepare();
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}/asset.png", listener.local_addr().unwrap());
        let path = f.cache.backend.directory.join(cache_basename(&url));
        let worker = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            read_request(&mut stream);
            write!(
                stream,
                "HTTP/1.1 {status}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            )
            .unwrap();
        });
        assert_eq!(
            f.cache.backend.fetch(&url),
            Err(CacheError::Failed(message.to_owned()))
        );
        worker.join().unwrap();
        assert!(!path.exists());
        assert!(
            !path
                .with_file_name(format!("{}.tmp", cache_basename(&url)))
                .exists()
        );
        fs::write(&path, []).unwrap();
        assert_eq!(
            f.cache.backend.fetch(&url).unwrap(),
            path,
            "empty existing files are valid cache hits"
        );
    }
}

#[test]
fn social_avatar_cache_retirement_prevents_remaining_stream_writes_and_publication() {
    let f = Fixture::new();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}/asset", listener.local_addr().unwrap());
    let path = f.cache.backend.directory.join(cache_basename(&url));
    let tmp = path.with_file_name(format!("{}.tmp", cache_basename(&url)));
    let (start_tx, start_rx) = mpsc::channel();
    let (finish_tx, finish_rx) = mpsc::channel();
    let worker = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        read_request(&mut stream);
        stream
            .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 6\r\nConnection: close\r\n\r\nabc")
            .unwrap();
        stream.flush().unwrap();
        start_tx.send(()).unwrap();
        finish_rx.recv_timeout(Duration::from_secs(5)).unwrap();
        stream.write_all(b"def").unwrap();
    });
    let (tx, rx) = mpsc::channel();
    f.cache.request(url, move |r| tx.send(r).unwrap()).unwrap();
    start_rx.recv_timeout(Duration::from_secs(5)).unwrap();
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    while fs::metadata(&tmp).map(|m| m.len()).unwrap_or(0) != 3
        && std::time::Instant::now() < deadline
    {
        std::thread::sleep(Duration::from_millis(2));
    }
    assert_eq!(fs::read(&tmp).unwrap(), b"abc");
    f.cache.retire();
    finish_tx.send(()).unwrap();
    assert_eq!(
        rx.recv_timeout(Duration::from_secs(5)).unwrap(),
        Err(CacheError::Cancelled)
    );
    worker.join().unwrap();
    assert_eq!(fs::read(&tmp).unwrap(), b"abc");
    assert!(!path.exists());
}
