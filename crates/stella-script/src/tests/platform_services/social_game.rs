//! Original native bindings, synthetic identity/Facebook and real loopback HTTP.
use super::*;
use serde_json::{Value, json};
use std::{
    collections::VecDeque,
    net::TcpStream,
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
    time::Instant,
};

pub(super) struct Sandbox {
    root: std::path::PathBuf,
    pub(super) data_root: std::path::PathBuf,
}

impl Sandbox {
    pub(super) fn new(label: &str) -> Self {
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!("stella-{label}-{unique}"));
        let data_root = root.join("data");
        // This fixture uses only synthetic Lua and services. An empty data
        // directory keeps the complete wire/lifetime suite portable in CI.
        fs::create_dir_all(&data_root).unwrap();
        fs::create_dir_all(root.join("appdata")).unwrap();
        Self { root, data_root }
    }
}

impl Drop for Sandbox {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

struct CachedFacebook(AtomicBool);
impl CachedFacebook {
    fn profile() -> SocialPlatformProfile {
        SocialPlatformProfile {
            user: SocialPlatformUser {
                id: "facebook-own".to_owned(),
                name: "Own".to_owned(),
                ..Default::default()
            },
            access_token: "synthetic-facebook-game".to_owned(),
            ..Default::default()
        }
    }
}
impl SocialPlatformProvider for CachedFacebook {
    fn is_logged_in(&self) -> bool {
        self.0.load(Ordering::Acquire)
    }
    fn prepare_login(self: Arc<Self>) -> SocialLoginRequest {
        SocialLoginRequest::Ready(Ok(()))
    }
    fn logout(&self) -> Result<(), SocialPlatformError> {
        self.0.store(false, Ordering::Release);
        Ok(())
    }
    fn user_profile(&self) -> Result<SocialPlatformProfile, SocialPlatformError> {
        Ok(Self::profile())
    }
    fn prepare_user_profile(self: Arc<Self>) -> SocialProfileRequest {
        SocialProfileRequest::Ready(Ok(Self::profile()))
    }
    fn publish_user_profile(&self, _: &SocialPlatformProfile) -> Result<(), SocialPlatformError> {
        Ok(())
    }
    fn friends(
        &self,
        _: SocialFriendDetails,
    ) -> Result<SocialPlatformFriends, SocialPlatformError> {
        Ok(SocialPlatformFriends::default())
    }
}

pub(super) struct Rule {
    route: &'static str,
    status: u16,
    body: String,
    pub(super) hold: Option<mpsc::Receiver<()>>,
}

pub(super) fn rule(route: &'static str, status: u16, body: &str) -> Rule {
    Rule {
        route,
        status,
        body: body.to_owned(),
        hold: None,
    }
}

fn profile(linked: bool, renewed: bool) -> Value {
    let mut value = json!({"accountId":"private-own","publicAccountId":"own","personal":{"nickName":if renewed {"Renamed"} else {"Own"}}});
    if linked {
        value["externalNetworks"] = json!([{"provider":"facebook","id":"facebook-own"}]);
        value["socialNetworks"] =
            json!([{"provider":"facebook","id":"facebook-own","socialAttributes":{"name":"Own"}}]);
    }
    value
}

fn reply(stream: &mut TcpStream, status: u16, body: &str) {
    write!(
        stream,
        "HTTP/1.1 {status} Synthetic\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    )
    .unwrap();
}

fn read_request(stream: &mut TcpStream) -> String {
    stream.set_nonblocking(false).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(5)))
        .unwrap();
    let mut bytes = Vec::new();
    loop {
        let mut block = [0_u8; 4096];
        let n = stream.read(&mut block).unwrap();
        assert_ne!(n, 0);
        bytes.extend_from_slice(&block[..n]);
        if let Some(end) = bytes.windows(4).position(|part| part == b"\r\n\r\n") {
            let headers = std::str::from_utf8(&bytes[..end]).unwrap();
            let size = headers
                .lines()
                .find_map(|line| {
                    let (name, value) = line.split_once(':')?;
                    name.eq_ignore_ascii_case("Content-Length")
                        .then(|| value.trim().parse::<usize>().unwrap())
                })
                .unwrap_or(0);
            if bytes.len() >= end + 4 + size {
                return String::from_utf8(bytes).unwrap();
            }
        }
    }
}

pub(super) struct Server {
    pub(super) origin: String,
    requests: mpsc::Receiver<String>,
    stop: Arc<AtomicBool>,
    thread: Option<thread::JoinHandle<()>>,
    profiles: Arc<AtomicUsize>,
    sessions: Arc<AtomicUsize>,
}

impl Server {
    pub(super) fn new(linked: bool, rules: Vec<Rule>) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let origin = format!("http://{}/proxy", listener.local_addr().unwrap());
        let stop = Arc::new(AtomicBool::new(false));
        let stopped = stop.clone();
        let (tx, requests) = mpsc::channel();
        let profiles = Arc::new(AtomicUsize::new(0));
        let fetched_profiles = profiles.clone();
        let sessions = Arc::new(AtomicUsize::new(0));
        let acquired_sessions = sessions.clone();
        let worker = thread::spawn(move || {
            let mut rules = VecDeque::from(rules);
            let mut session_count = 0;
            let mut held = Vec::new();
            while !stopped.load(Ordering::Acquire) {
                let mut stream = match listener.accept() {
                    Ok((stream, _)) => stream,
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                        thread::sleep(Duration::from_millis(1));
                        continue;
                    }
                    Err(e) => panic!("synthetic game accept: {e}"),
                };
                let request = read_request(&mut stream);
                let route = request.lines().next().unwrap();
                if route.contains("/leaderboard/1.0/")
                    || route.starts_with("POST /proxy/storage/1.0/state ")
                    || route.starts_with("GET /proxy/storage/1.0/state?")
                {
                    let rule = rules.pop_front().expect("unexpected native game request");
                    assert!(route.starts_with(rule.route), "{route}");
                    tx.send(request).unwrap();
                    if let Some(release) = rule.hold {
                        held.push(thread::spawn(move || {
                            release.recv_timeout(Duration::from_secs(10)).unwrap();
                            reply(&mut stream, rule.status, &rule.body);
                        }));
                    } else {
                        reply(&mut stream, rule.status, &rule.body);
                    }
                } else if route.starts_with("POST /proxy/session/1/apps/game-fixture/sessions ") {
                    session_count += 1;
                    acquired_sessions.fetch_add(1, Ordering::AcqRel);
                    let body=json!({"userAuth":{"accessToken":if session_count==1 {"synthetic-game-access"} else {"renewed-game-access"},"refreshToken":"synthetic-game-refresh","expiresIn":3600},"segments":[8,2],"config":{},"profile":profile(linked,session_count>1)}).to_string();
                    reply(&mut stream, 200, &body);
                } else if route.starts_with("GET /proxy/identity/2.0/friends ") {
                    reply(&mut stream, 503, "");
                } else if route.starts_with("POST /proxy/identity/2.0/external/connect ") {
                    let connection: Value =
                        serde_json::from_str(storage_session::body(&request)).unwrap();
                    assert_eq!(connection["provider"], "facebook");
                    assert_eq!(connection["externalAttributes"]["userId"], "facebook-own");
                    reply(&mut stream, 204, "");
                } else if route.starts_with("GET /proxy/identity/3.0/profile/own ") {
                    reply(
                        &mut stream,
                        200,
                        &profile(linked, session_count > 1).to_string(),
                    );
                    fetched_profiles.fetch_add(1, Ordering::AcqRel);
                } else if route.starts_with("POST /proxy/storage/2.0/states/query ")
                    || route.starts_with("POST /proxy/storage/1.0/states/query ")
                {
                    reply(&mut stream, 200, r#"{"result":[]}"#);
                } else if route.contains("/log/") {
                    reply(&mut stream, 200, "");
                } else {
                    reply(&mut stream, 404, "");
                    panic!("unexpected synthetic game route: {route}");
                }
            }
            for worker in held {
                worker.join().unwrap();
            }
            assert!(
                rules.is_empty(),
                "unexecuted native game requests: {}",
                rules.len()
            );
        });
        Self {
            origin,
            requests,
            stop,
            thread: Some(worker),
            profiles,
            sessions,
        }
    }

    pub(super) fn request(&self) -> String {
        self.requests.recv_timeout(Duration::from_secs(5)).unwrap()
    }

    pub(super) fn session_requests(&self) -> usize {
        self.sessions.load(Ordering::Acquire)
    }

    pub(super) fn finish(mut self) {
        self.stop.store(true, Ordering::Release);
        self.thread.take().unwrap().join().unwrap();
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
    }
}

pub(super) fn runtime(sandbox: &Sandbox, server: &Server, linked: bool) -> StellaLua {
    let runtime = StellaLua::new(sandbox.data_root.clone()).unwrap();
    runtime
        .set_identity_url(&format!("{}/identity/2.0", server.origin))
        .unwrap();
    runtime
        .set_identity_client(Some("game-fixture"), Some("synthetic-signature"), None)
        .unwrap();
    runtime.execute_source(r#"
        update=function() end
        game_login=0; game_connected=0; game_posts={}; game_boards={}
        _G.SkynestAccount.onLoginSuccess=function() game_login=game_login+1 end
        _G.SocialManager.onSocialNetworkConnected=function() game_connected=game_connected+1 end
        _G.SocialManager.onFriendsProgressUpdated=function() end
        _G.SocialManager.onScorePosted=function(...) local p={...};p.n=select('#',...);game_posts[#game_posts+1]=p end
        _G.SocialManager.onLeaderboardFetched=function(...) local p={...};p.n=select('#',...);game_boards[#game_boards+1]=p end
    "#).unwrap();
    if linked {
        runtime
            .set_facebook_session(Some(Arc::new(CachedFacebook(AtomicBool::new(true)))))
            .unwrap();
    }
    runtime
}

pub(super) fn wait(runtime: &StellaLua, predicate: impl Fn(&mlua::Table) -> bool) {
    let env = game_environment(runtime.lua()).unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        dispatch_registered_application_events(runtime.lua()).unwrap();
        if predicate(&env) {
            return;
        }
        assert!(Instant::now() < deadline, "native game event timeout");
        thread::sleep(Duration::from_millis(1));
    }
}

pub(super) fn login(runtime: &StellaLua, linked: bool) {
    runtime
        .execute_source("_G.SkynestAccount.native_login(false,false,false)")
        .unwrap();
    wait(runtime, |env| {
        env.get::<i32>("game_login").unwrap() == 1
            && (!linked
                || (env.get::<i32>("game_connected").unwrap() > 0
                    && runtime.social.platform_state_for_test() == (0, false, true)))
    });
}

fn count(runtime: &StellaLua, table: &str, expected: usize) {
    wait(runtime, |env| {
        env.get::<mlua::Table>(table).unwrap().raw_len() == expected
    });
}

fn callback(runtime: &StellaLua, table: &str, index: usize) -> mlua::Table {
    game_environment(runtime.lua())
        .unwrap()
        .get::<mlua::Table>(table)
        .unwrap()
        .raw_get(index)
        .unwrap()
}

fn header(request: &str, name: &str) -> Option<String> {
    request
        .split("\r\n\r\n")
        .next()
        .unwrap()
        .lines()
        .find_map(|line| {
            let (key, value) = line.split_once(':')?;
            key.eq_ignore_ascii_case(name)
                .then(|| value.trim().to_owned())
        })
}

pub(super) fn decode(request: &str, access: &str) -> String {
    use aes::{
        Aes128,
        cipher::{BlockModeDecrypt, KeyIvInit, block_padding::Pkcs7},
    };
    use base64::{Engine as _, engine::general_purpose::STANDARD};
    let vectors: Value = serde_json::from_str(include_str!(
        "../../game_lua/platform_services/social/game_client/protocol/vectors.json"
    ))
    .unwrap();
    let hex = vectors["keys"][access].as_str().unwrap();
    let bytes: Vec<u8> = (0..hex.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&hex[i..i + 2], 16).unwrap())
        .collect();
    let key: &[u8; 16] = bytes.as_slice().try_into().unwrap();
    let bytes = STANDARD.decode(storage_session::body(request)).unwrap();
    String::from_utf8(
        cbc::Decryptor::<Aes128>::new(key.into(), (&[0_u8; 16]).into())
            .decrypt_padded_vec::<Pkcs7>(&bytes)
            .unwrap(),
    )
    .unwrap()
}

#[test]
fn native_game_wire_401_frozen_cipher_leaderboard_nocache_schema_and_lua_arity() {
    let sandbox = Sandbox::new("native-game-wire");
    let server = Server::new(
        true,
        vec![
            rule("POST /proxy/leaderboard/1.0/score ", 401, ""),
            rule("POST /proxy/leaderboard/1.0/score ", 204, ""),
            rule(
                "GET /proxy/leaderboard/1.0/level-S01L01/query/friends?nocache=1 ",
                200,
                "{}",
            ),
            rule(
                "GET /proxy/leaderboard/1.0/level-S01L01/query/friends?nocache=1 ",
                200,
                r#"{"scores":[{"accountId":"own","score":{"points":16777217},"ranking":{"rank":4294967295}},{"accountId":"friend","score":{"points":3},"ranking":{"rank":2}},{"accountId":"unknown"},{"accountId":"friend"}]}"#,
            ),
            rule(
                "GET /proxy/leaderboard/1.0/level-S01L01/query/friends ",
                201,
                "junk",
            ),
        ],
    );
    let runtime = runtime(&sandbox, &server, true);
    runtime.skynest_account.seed_friends_cache_for_test(
        "own",
        r#"{"friends":[{"accountId":"friend","nickName":"Cached Friend"}]}"#,
    );
    login(&runtime, true);
    runtime
        .execute_source(r#"_G.SocialManager.native_postScores("S01L01",50000.9,"score/request")"#)
        .unwrap();
    let first = server.request();
    let replay = server.request();
    storage_session::assert_auth(&first, "synthetic-game-access", "8, 2");
    storage_session::assert_auth(&replay, "renewed-game-access", "8, 2");
    assert_eq!(
        storage_session::body(&first),
        storage_session::body(&replay)
    );
    assert_eq!(header(&first, "EM"), Some("1".to_owned()));
    assert_eq!(
        header(&first, "Content-Type"),
        Some("application/json".to_owned())
    );
    let golden: Value = serde_json::from_str(include_str!(
        "../../game_lua/platform_services/social/game_client/protocol/vectors.json"
    ))
    .unwrap();
    assert_eq!(
        storage_session::body(&first),
        golden["encryption"][0]["base64"].as_str().unwrap()
    );
    count(&runtime, "game_posts", 1);
    let posted = callback(&runtime, "game_posts", 1);
    assert!(posted.raw_get::<bool>(1).unwrap());
    assert_eq!(posted.get::<i32>("n").unwrap(), 3);
    assert_eq!(posted.raw_get::<String>(3).unwrap(), "score/request");
    // Session renewal independently queues SDK social resynchronization.
    // Finish that real own-profile refresh before admitting new board jobs;
    // otherwise a deliberate host ownership check cancels the superseded job.
    wait(&runtime, |_| {
        server.profiles.load(Ordering::Acquire) >= 2
            && runtime.social.platform_state_for_test() == (0, false, true)
    });
    for index in 1..=3 {
        runtime
            .execute_source(r#"_G.SocialManager.native_fetchLeaderboard("S01L01","board/request")"#)
            .unwrap();
        let request = server.request();
        storage_session::assert_auth(&request, "renewed-game-access", "8, 2");
        count(&runtime, "game_boards", index);
    }
    for index in [1, 3] {
        let failed = callback(&runtime, "game_boards", index);
        assert!(!failed.raw_get::<bool>(1).unwrap());
        assert_eq!(failed.get::<i32>("n").unwrap(), 2);
    }
    let board = callback(&runtime, "game_boards", 2);
    assert_eq!(board.get::<i32>("n").unwrap(), 4);
    assert_eq!(board.raw_get::<String>(4).unwrap(), "board/request");
    let rows = board.raw_get::<mlua::Table>(3).unwrap();
    assert_eq!(rows.raw_len(), 4);
    let own = rows.raw_get::<mlua::Table>(1).unwrap();
    assert_eq!(own.get::<f64>("points").unwrap(), 16777216.0);
    assert_eq!(own.get::<f64>("rank").unwrap(), -1.0);
    assert_eq!(own.get::<String>("nickname").unwrap(), "Renamed");
    assert!(own.get::<bool>("localPlayer").unwrap());
    let friend = rows.raw_get::<mlua::Table>(2).unwrap();
    assert_eq!(friend.get::<String>("nickname").unwrap(), "Cached Friend");
    assert!(matches!(
        friend.get::<mlua::Value>("localPlayer").unwrap(),
        mlua::Value::Nil
    ));
    assert_eq!(
        rows.raw_get::<mlua::Table>(3)
            .unwrap()
            .get::<String>("nickname")
            .unwrap(),
        "n/a"
    );
    server.finish();
}

#[test]
fn native_game_pending_failures_survive_vm_restart_and_flush_before_friends_query_once() {
    let sandbox = Sandbox::new("native-game-restart");
    let server = Server::new(
        true,
        vec![
            rule("POST /proxy/leaderboard/1.0/score ", 503, ""),
            rule("POST /proxy/leaderboard/1.0/score ", 500, ""),
            rule("POST /proxy/leaderboard/1.0/score ", 201, "ignored"),
            rule(
                "GET /proxy/leaderboard/1.0/level-S01L01/query/friends?nocache=1 ",
                200,
                r#"{"scores":[]}"#,
            ),
        ],
    );
    let first = runtime(&sandbox, &server, true);
    let cached_score =
        json!({"accountId":"old","level":"S01L02","points":8,"properties":{"note":"preserved"}})
            .to_string();
    first.skynest_account.seed_game_cache_for_test("private-own",&json!({"transactionId":0,"scoresToSend":[],"cachedScores":[{"leaderBoardId":"level","rank":7,"score":cached_score}]}).to_string());
    login(&first, true);
    for (points, id) in [(7, "first"), (3, "second")] {
        first
            .execute_source(&format!(
                "_G.SocialManager.native_postScores('S01L01',{points},'{id}')"
            ))
            .unwrap();
        let request = server.request();
        let plaintext = decode(&request, "synthetic-game-access");
        assert_eq!(
            plaintext,
            if points == 7 {
                r#"{"level":"S01L01","score":{"points":7}}"#
            } else {
                r#"{"level":"S01L01","score":{"points":7}}{"level":"S01L01","score":{"points":3}}"#
            }
        );
        count(&first, "game_posts", if points == 7 { 1 } else { 2 });
    }
    assert!(
        !callback(&first, "game_posts", 1)
            .raw_get::<bool>(1)
            .unwrap()
    );
    assert!(
        !callback(&first, "game_posts", 2)
            .raw_get::<bool>(1)
            .unwrap()
    );
    drop(first);
    let second = runtime(&sandbox, &server, true);
    let cache: Value = serde_json::from_str(
        &second
            .skynest_account
            .read_game_cache_for_test("private-own"),
    )
    .unwrap();
    assert_eq!(cache["transactionId"], 2);
    assert_eq!(cache["scoresToSend"].as_array().unwrap().len(), 2);
    assert!(
        second
            .skynest_account
            .read_game_cache_for_test("own")
            .is_empty()
    );
    login(&second, true);
    second
        .execute_source("_G.SocialManager.native_fetchLeaderboard('S01L01','restored')")
        .unwrap();
    let replay = server.request();
    let query = server.request();
    assert_eq!(
        decode(&replay, "renewed-game-access"),
        r#"{"level":"S01L01","score":{"points":7}}{"level":"S01L01","score":{"points":3}}"#
    );
    assert!(query.starts_with("GET "));
    count(&second, "game_boards", 1);
    assert_eq!(
        game_environment(second.lua())
            .unwrap()
            .get::<mlua::Table>("game_posts")
            .unwrap()
            .raw_len(),
        0
    );
    wait(&second, |_| {
        let text = second
            .skynest_account
            .read_game_cache_for_test("private-own");
        serde_json::from_str::<Value>(&text).is_ok_and(|v| {
            v["transactionId"] == 3 && v["scoresToSend"].as_array().unwrap().is_empty()
        })
    });
    let cache: Value = serde_json::from_str(
        &second
            .skynest_account
            .read_game_cache_for_test("private-own"),
    )
    .unwrap();
    assert_eq!(cache["cachedScores"].as_array().unwrap().len(), 1);
    assert_eq!(cache["cachedScores"][0]["rank"], 7);
    server.finish();
}

#[test]
fn native_game_4xx_drops_pending_invalid_scores_and_disconnected_query_have_native_gates() {
    let sandbox = Sandbox::new("native-game-reject");
    let server = Server::new(
        false,
        vec![
            rule("POST /proxy/leaderboard/1.0/score ", 400, ""),
            rule("POST /proxy/leaderboard/1.0/score ", 200, "junk"),
        ],
    );
    let runtime = runtime(&sandbox, &server, false);
    login(&runtime, false);
    runtime.execute_source("_G.SocialManager.native_fetchLeaderboard('S01L01','disconnected');_G.SocialManager.native_postScores('',42,'empty');_G.SocialManager.native_postScores('S01L01',-1.9,'negative')").unwrap();
    count(&runtime, "game_posts", 2);
    assert!(server.requests.try_recv().is_err());
    assert_eq!(
        game_environment(runtime.lua())
            .unwrap()
            .get::<mlua::Table>("game_boards")
            .unwrap()
            .raw_len(),
        0
    );
    runtime
        .execute_source("_G.SocialManager.native_postScores('S01L01',7,'bad')")
        .unwrap();
    server.request();
    count(&runtime, "game_posts", 3);
    runtime
        .execute_source("_G.SocialManager.native_postScores('S01L01',-0.9,'zero')")
        .unwrap();
    let request = server.request();
    assert_eq!(
        decode(&request, "synthetic-game-access"),
        r#"{"level":"S01L01","score":{"points":0}}"#
    );
    count(&runtime, "game_posts", 4);
    assert!(
        callback(&runtime, "game_posts", 4)
            .raw_get::<bool>(1)
            .unwrap()
    );
    server.finish();
}

#[test]
fn native_game_held_score_logout_provider_switch_and_runtime_drop_retire_response_writes() {
    for operation in ["logout", "local", "drop"] {
        let sandbox = Sandbox::new("native-game-held");
        let (release, resume) = mpsc::channel();
        let mut held = rule("POST /proxy/leaderboard/1.0/score ", 200, "");
        held.hold = Some(resume);
        let server = Server::new(false, vec![held]);
        let runtime = runtime(&sandbox, &server, false);
        login(&runtime, false);
        let probe = runtime.skynest_account.clone();
        let queued = runtime.social.online_completion_count_probe();
        let seed = r#"{"cachedScores":[],"scoresToSend":[],"transactionId":0}"#;
        probe.seed_game_cache_for_test("private-own", seed);
        runtime
            .execute_source("_G.SocialManager.native_postScores('S01L01',7,'held')")
            .unwrap();
        server.request();
        if operation == "drop" {
            drop(runtime);
            let text = probe.read_game_cache_for_test("private-own");
            let cache: Value = serde_json::from_str(&text).unwrap();
            assert_eq!(cache["transactionId"], 1);
            assert_eq!(cache["scoresToSend"].as_array().unwrap().len(), 1);
            release.send(()).unwrap();
            server.finish();
            assert_eq!(probe.read_game_cache_for_test("private-own"), text);
        } else {
            if operation == "logout" {
                runtime
                    .execute_source("_G.SkynestAccount.native_logout()")
                    .unwrap();
            } else {
                runtime.social.enable_local_provider().unwrap();
            }
            let baseline = queued();
            release.send(()).unwrap();
            server.finish();
            for _ in 0..10 {
                dispatch_registered_application_events(runtime.lua()).unwrap();
                thread::sleep(Duration::from_millis(2));
            }
            assert_eq!(
                game_environment(runtime.lua())
                    .unwrap()
                    .get::<mlua::Table>("game_posts")
                    .unwrap()
                    .raw_len(),
                0
            );
            assert_eq!(probe.read_game_cache_for_test("private-own"), seed);
            assert!(queued() <= baseline);
        }
    }
}
