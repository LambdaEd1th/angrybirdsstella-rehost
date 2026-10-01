//! Real loopback account adapter driven by original-script application updates.
use super::*;
use std::sync::atomic::{AtomicUsize, Ordering};
mod observer;
use stella_script::{
    FacebookSystemAccountAdapter, FacebookSystemAccountCompletion, FacebookSystemAuthorization,
    FacebookTokenCache,
};

struct Account {
    addr: std::net::SocketAddr,
    thread: thread::ThreadId,
    calls: Mutex<Vec<&'static str>>,
}

impl Account {
    fn called(&self, name: &'static str) {
        assert_eq!(thread::current().id(), self.thread);
        self.calls.lock().unwrap().push(name);
    }
    fn fetch(addr: std::net::SocketAddr, path: &str) -> Result<String, SocialPlatformError> {
        let mut stream = TcpStream::connect_timeout(&addr, Duration::from_secs(10))
            .map_err(|_| SocialPlatformError::Transport)?;
        stream
            .set_read_timeout(Some(Duration::from_secs(10)))
            .unwrap();
        write!(
            stream,
            "GET {path} HTTP/1.1\r\nHost: {addr}\r\nConnection: close\r\n\r\n"
        )
        .map_err(|_| SocialPlatformError::Transport)?;
        let mut response = String::new();
        stream
            .read_to_string(&mut response)
            .map_err(|_| SocialPlatformError::Transport)?;
        let (headers, body) = response
            .split_once("\r\n\r\n")
            .ok_or(SocialPlatformError::InvalidResponse)?;
        if !headers.starts_with("HTTP/1.1 200 ") {
            return Err(SocialPlatformError::Transport);
        }
        Ok(body.to_owned())
    }
}

impl FacebookSystemAccountAdapter for Account {
    fn can_request_access_without_ui(&self) -> bool {
        self.called("can_without_ui");
        true
    }
    fn renew_system_authorization(
        &self,
        completion: FacebookSystemAccountCompletion<FacebookSystemAuthorization>,
    ) {
        self.called("renew");
        let addr = self.addr;
        thread::spawn(move || {
            completion(
                Self::fetch(addr, "/renew").and_then(|body| match body.as_str() {
                    "0" => Ok(FacebookSystemAuthorization::Renewed),
                    "1" => Ok(FacebookSystemAuthorization::Rejected),
                    "2" => Ok(FacebookSystemAuthorization::Failed),
                    _ => Err(SocialPlatformError::InvalidResponse),
                }),
            )
        });
    }
    fn restore_account_access(
        &self,
        app_id: &str,
        audience: i32,
        completion: FacebookSystemAccountCompletion<String>,
    ) {
        self.called("access");
        assert_eq!((app_id, audience), ("12345", 0));
        let addr = self.addr;
        thread::spawn(move || completion(Self::fetch(addr, "/access?app_id=12345&audience=0")));
    }
    fn set_force_blocking_renew(&self, _force: bool) {
        panic!("unexpected password-change branch");
    }
}

pub(super) fn run(data: PathBuf, path: PathBuf) {
    assert!(path.is_file());
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let addr = listener.local_addr().unwrap();
    let graph_count = Arc::new(AtomicUsize::new(0));
    let count = graph_count.clone();
    let server = thread::spawn(move || {
        for step in 0..4 {
            let end = std::time::Instant::now() + Duration::from_secs(20);
            let mut stream = loop {
                match listener.accept() {
                    Ok((stream, _)) => break stream,
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        assert!(
                            std::time::Instant::now() < end,
                            "system fixture request timed out"
                        );
                        thread::sleep(Duration::from_millis(1));
                    }
                    Err(error) => panic!("{error}"),
                }
            };
            let request = request(&mut stream);
            assert!(!request.to_ascii_lowercase().contains("x-access-token"));
            match step {
                0 => {
                    assert!(request.starts_with(
                        "GET /v2.0/me?format=json&sdk=ios&access_token=synthetic-persisted-release "
                    ));
                    count.fetch_add(1, Ordering::SeqCst);
                    reply(
                        &mut stream,
                        400,
                        r#"{"error":{"code":190,"error_subcode":463}}"#,
                    );
                }
                1 => {
                    assert!(request.starts_with("GET /renew "));
                    reply(&mut stream, 200, "0");
                }
                2 => {
                    assert!(request.starts_with("GET /access?app_id=12345&audience=0 "));
                    reply(&mut stream, 200, "synthetic-system-release");
                }
                3 => {
                    assert!(request.starts_with(
                        "GET /v2.0/me?format=json&sdk=ios&access_token=synthetic-system-release "
                    ));
                    count.fetch_add(1, Ordering::SeqCst);
                    reply(
                        &mut stream,
                        200,
                        r#"{"id":"system-release-user","name":"System Release User"}"#,
                    );
                }
                _ => unreachable!(),
            }
        }
        listener
    });
    let cache = FacebookTokenCache::open(&path).unwrap();
    let provider = Arc::new(
        FacebookOAuthSession::new_with_cache(
            FacebookOAuthConfig {
                graph_root: format!("http://{addr}/v2.0"),
                rest_root: None,
                authorization_url: format!("http://{addr}/oauth"),
                app_id: "12345".into(),
                url_scheme_suffix: String::new(),
                request_birthday: true,
            },
            cache.clone(),
            |_| panic!("repair must not launch browser auth"),
        )
        .unwrap(),
    );
    let adapter = Arc::new(Account {
        addr,
        thread: thread::current().id(),
        calls: Mutex::new(Vec::new()),
    });
    provider.set_system_account_adapter(adapter.clone());
    let observed = Arc::new(observer::Observed {
        inner: provider.clone(),
        results: Default::default(),
    });
    let game = StellaLua::new_with_missing_global_diagnostics(data).unwrap();
    game.enable_local_services().unwrap();
    game.set_facebook_session(Some(observed.clone())).unwrap();
    game.boot("scripts/game.lua").unwrap();
    let mut clock = AudioOutputClock::default();
    let mut commands = 0;
    for _ in 0..600 {
        commands = frame(&game, &mut clock);
        thread::sleep(Duration::from_millis(1));
    }
    assert!(commands > 0);
    game.execute_source("if not menuManager or not menuManager:getRoot() then error('original system repair scene missing') end").unwrap();
    assert_eq!(
        provider.session_state(),
        FacebookSessionState::OpenTokenExtended
    );
    assert!(
        matches!(
            provider.clone().prepare_user_profile(),
            SocialProfileRequest::Pending(_)
        ),
        "failed startup request fabricated a cached profile"
    );
    assert_eq!(
        graph_count.load(Ordering::SeqCst),
        1,
        "default behavior replayed original request"
    );
    assert_eq!(
        *observed.results.lock().unwrap(),
        [Err(SocialPlatformError::GraphRetryRequired)],
        "startup worker did not deliver its actual repair failure"
    );
    assert_eq!(
        *adapter.calls.lock().unwrap(),
        ["can_without_ui", "renew", "access"]
    );
    println!(
        "system-repair: SDK_state=514; original_profile=failed; automatic_replay=0; callbacks=application_thread"
    );
    let retry = provider.clone().prepare_user_profile();
    let worker = thread::spawn(move || retry.execute());
    for _ in 0..120 {
        frame(&game, &mut clock);
        thread::sleep(Duration::from_millis(1));
    }
    assert!(
        worker.is_finished(),
        "explicit profile request did not finish"
    );
    let profile = provider
        .publish_completed_profile(&worker.join().unwrap().unwrap())
        .unwrap();
    assert_eq!(profile.user.id, "system-release-user");
    assert_eq!(profile.access_token, "synthetic-system-release");
    std::fs::copy(&path, path.with_extension("repaired.plist")).unwrap();
    provider.logout().unwrap();
    assert_eq!(cache.take_error(), None);
    assert!(game.fallback_calls().is_empty());
    assert!(game.compatibility_bindings().is_empty());
    let listener = server.join().unwrap();
    assert!(
        matches!(listener.accept(), Err(error) if error.kind() == std::io::ErrorKind::WouldBlock)
    );
    assert_eq!(graph_count.load(Ordering::SeqCst), 2);
    println!(
        "system-repair: frames=720; auth=0; graph=2; renew=1; access=1; current_token=true; cache_error=none; fallback_calls=0; compatibility_bindings=0"
    );
}
