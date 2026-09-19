use super::*;
use crate::{
    FacebookSystemAccountAdapter, FacebookSystemAccountCompletion, FacebookSystemAuthorization,
};
use std::{net::TcpStream, sync::mpsc, thread::ThreadId, time::Instant};

type Tasks = Arc<Mutex<VecDeque<SocialPlatformTask>>>;

mod batch;
mod failures;
mod rest;
mod retirement;

struct CacheFile(std::path::PathBuf);
impl Drop for CacheFile {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

struct HttpAccount {
    root: String,
    without_ui: bool,
    events: Mutex<Vec<(&'static str, ThreadId)>>,
}

impl HttpAccount {
    fn event(&self, name: &'static str) {
        self.events
            .lock()
            .unwrap()
            .push((name, thread::current().id()));
    }
    fn assert_events(&self, expected: &[&str]) {
        let events = self.events.lock().unwrap();
        assert_eq!(
            events.iter().map(|(name, _)| *name).collect::<Vec<_>>(),
            expected
        );
        assert!(events.iter().all(|(_, id)| *id == thread::current().id()));
    }
}

impl FacebookSystemAccountAdapter for HttpAccount {
    fn can_request_access_without_ui(&self) -> bool {
        self.event("can_without_ui");
        self.without_ui
    }
    fn renew_system_authorization(
        &self,
        completion: FacebookSystemAccountCompletion<FacebookSystemAuthorization>,
    ) {
        self.event("renew");
        let url = format!("{}/renew", self.root);
        thread::spawn(move || {
            let result = wire::send(&url, None)
                .and_then(wire::Response::into_result)
                .and_then(|value| match value.get("result").and_then(Value::as_i64) {
                    Some(0) => Ok(FacebookSystemAuthorization::Renewed),
                    Some(1) => Ok(FacebookSystemAuthorization::Rejected),
                    Some(2) => Ok(FacebookSystemAuthorization::Failed),
                    _ => Err(SocialPlatformError::InvalidResponse),
                });
            completion(result);
        });
    }
    fn restore_account_access(
        &self,
        app_id: &str,
        audience: i32,
        completion: FacebookSystemAccountCompletion<String>,
    ) {
        self.event("access");
        assert_eq!(app_id, "12345");
        assert_eq!(audience, 0);
        let url = format!("{}/access?app_id={app_id}&audience={audience}", self.root);
        thread::spawn(move || {
            let result = wire::send(&url, None)
                .and_then(wire::Response::into_result)
                .and_then(|value| {
                    value
                        .get("token")
                        .and_then(Value::as_str)
                        .map(str::to_owned)
                        .ok_or(SocialPlatformError::InvalidResponse)
                });
            completion(result);
        });
    }
    fn set_force_blocking_renew(&self, force: bool) {
        assert!(force);
        self.event("force_blocking");
    }
}

fn listener() -> TcpListener {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    listener
}

fn accept(listener: &TcpListener) -> TcpStream {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        match listener.accept() {
            Ok((stream, _)) => {
                stream.set_nonblocking(false).unwrap();
                stream
                    .set_read_timeout(Some(Duration::from_secs(5)))
                    .unwrap();
                return stream;
            }
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                assert!(
                    Instant::now() < deadline,
                    "account fixture request never arrived"
                );
                thread::sleep(Duration::from_millis(1));
            }
            Err(error) => panic!("{error}"),
        }
    }
}

fn reply(stream: &mut TcpStream, status: u16, value: Value) {
    let body = value.to_string();
    write!(
        stream,
        "HTTP/1.1 {status} Fixture\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    )
    .unwrap();
}

fn drain(tasks: &Tasks) {
    loop {
        let task = tasks.lock().unwrap().pop_front();
        let Some(task) = task else {
            break;
        };
        task.run();
    }
}

fn pump_until(tasks: &Tasks, mut condition: impl FnMut() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        drain(tasks);
        if condition() {
            return;
        }
        assert!(
            Instant::now() < deadline,
            "system-account completion did not arrive"
        );
        thread::sleep(Duration::from_millis(1));
    }
}

fn session(
    graph: &TcpListener,
    account: &TcpListener,
    login_type: i32,
    age: f64,
    without_ui: bool,
) -> (
    Arc<FacebookOAuthSession>,
    Arc<HttpAccount>,
    Tasks,
    CacheFile,
) {
    session_with_rest(graph, account, login_type, age, without_ui, None)
}

fn session_with_rest(
    graph: &TcpListener,
    account: &TcpListener,
    login_type: i32,
    age: f64,
    without_ui: bool,
    rest_root: Option<String>,
) -> (
    Arc<FacebookOAuthSession>,
    Arc<HttpAccount>,
    Tasks,
    CacheFile,
) {
    let now = token_cache::now();
    let grants = vec![
        "public_profile".into(),
        "email".into(),
        "user_friends".into(),
    ];
    let mut token = token_cache::CachedToken::from_response(
        "synthetic-system-old".into(),
        grants.clone(),
        &BTreeMap::new(),
        login_type,
        now - age,
    );
    token.refresh_permissions(grants, now - age);
    let cache_file = CacheFile(std::env::temp_dir().join(format!(
            "stella-system-account-{}-{}.plist",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        )));
    assert!(!cache_file.0.exists());
    let cache = FacebookTokenCache::open(&cache_file.0).unwrap();
    cache.cache(&token);
    let session = Arc::new(
        FacebookOAuthSession::new_with_cache(
            FacebookOAuthConfig {
                graph_root: format!("http://{}/v2.0", graph.local_addr().unwrap()),
                rest_root,
                authorization_url: "http://127.0.0.1:9/oauth".into(),
                app_id: "12345".into(),
                url_scheme_suffix: String::new(),
                request_birthday: false,
            },
            cache,
            |_| panic!("cached system token must not launch a browser"),
        )
        .unwrap(),
    );
    assert_eq!(session.session_state(), FacebookSessionState::Open);
    let adapter = Arc::new(HttpAccount {
        root: format!("http://{}", account.local_addr().unwrap()),
        without_ui,
        events: Mutex::new(Vec::new()),
    });
    session.set_system_account_adapter(adapter.clone());
    let tasks = Tasks::default();
    let sink = tasks.clone();
    session.set_application_dispatcher(Arc::new(move |task| sink.lock().unwrap().push_back(task)));
    (session, adapter, tasks, cache_file)
}

#[test]
fn facebook_system_account_repairs_on_main_returns_retry_error_without_replaying() {
    let graph = listener();
    let account = listener();
    let (session, adapter, tasks, cache_file) = session(&graph, &account, 1, 0.0, true);
    let graph_server = thread::spawn(move || {
        let mut stream = accept(&graph);
        let (request, _) = test_wire::read_request(&mut stream);
        assert!(request.starts_with("GET /v2.0/me?"));
        assert!(request.contains("access_token=synthetic-system-old"));
        reply(
            &mut stream,
            400,
            json!({"error":{"code":190,"error_subcode":463}}),
        );
        graph
    });
    let (seen_tx, seen_rx) = mpsc::channel();
    let (release_tx, release_rx) = mpsc::channel();
    let account_server = thread::spawn(move || {
        for (path, response) in [
            ("GET /renew ", json!({"result":0})),
            (
                "GET /access?app_id=12345&audience=0 ",
                json!({"token":"synthetic-system-new"}),
            ),
        ] {
            let mut stream = accept(&account);
            let (request, _) = test_wire::read_request(&mut stream);
            assert!(request.starts_with(path));
            seen_tx.send(()).unwrap();
            release_rx.recv_timeout(Duration::from_secs(5)).unwrap();
            reply(&mut stream, 200, response);
        }
        account
    });
    let request = session.clone().prepare_user_profile();
    let worker = thread::spawn(move || request.execute());
    for _ in 0..2 {
        pump_until(&tasks, || seen_rx.try_recv().is_ok());
        assert!(
            !worker.is_finished(),
            "original consumer must wait for its repair chain"
        );
        assert_eq!(session.session_state(), FacebookSessionState::Open);
        assert_eq!(
            session.token_cache.admitted(&[]).unwrap().unwrap().token,
            "synthetic-system-old"
        );
        release_tx.send(()).unwrap();
    }
    pump_until(&tasks, || worker.is_finished());
    assert!(matches!(
        worker.join().unwrap(),
        Err(SocialPlatformError::GraphRetryRequired)
    ));
    drain(&tasks);
    let account = account_server.join().unwrap();
    let graph = graph_server.join().unwrap();
    assert!(matches!(graph.accept(), Err(e) if e.kind() == std::io::ErrorKind::WouldBlock));
    assert!(matches!(account.accept(), Err(e) if e.kind() == std::io::ErrorKind::WouldBlock));
    assert_eq!(
        session.session_state(),
        FacebookSessionState::OpenTokenExtended
    );
    let token = session.token_cache.admitted(&[]).unwrap().unwrap();
    assert_eq!(token.token, "synthetic-system-new");
    assert!(!token.is_system_account());
    let serialized = plist::Value::from_file(&cache_file.0).unwrap();
    let token = serialized.as_dictionary().unwrap()["FBAccessTokenInformationKey"]
        .as_dictionary()
        .unwrap();
    let date: SystemTime = token["com.facebook.sdk:TokenInformationExpirationDateKey"]
        .as_date()
        .unwrap()
        .into();
    assert_eq!(
        date.duration_since(UNIX_EPOCH).unwrap().as_secs(),
        64_092_211_200,
    );
    adapter.assert_events(&["can_without_ui", "renew", "access"]);
    let server = thread::spawn(move || {
        let mut stream = accept(&graph);
        let (request, _) = test_wire::read_request(&mut stream);
        assert!(request.contains("access_token=synthetic-system-new"));
        reply(
            &mut stream,
            200,
            json!({"id":"actual-user","name":"Actual Name"}),
        );
    });
    let request = session.clone().prepare_user_profile();
    let worker = thread::spawn(move || request.execute());
    pump_until(&tasks, || worker.is_finished());
    let profile = session
        .publish_completed_profile(&worker.join().unwrap().unwrap())
        .unwrap();
    assert_eq!(profile.user.id, "actual-user");
    assert_eq!(profile.access_token, "synthetic-system-new");
    server.join().unwrap();
}
