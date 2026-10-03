//! Native binding and ordered transport regressions, using isolated files and
//! 127.0.0.1 with synthetic identities only.

use super::*;
use serde_json::{Value as Json, json};
use session::{MemoryRefreshStore, RefreshStore};
use std::{
    io::{Read, Write},
    net::{TcpListener, TcpStream},
    sync::atomic::{AtomicU64, Ordering},
    thread,
    time::Instant,
};

struct Fixture {
    lua: Lua,
    runtime: SkynestAccountRuntime,
    store: Arc<MemoryRefreshStore>,
    directory: PathBuf,
}

impl Fixture {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let directory = std::env::temp_dir().join(format!(
            "stella-unregister-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&directory).unwrap();
        fs::write(directory.join("avatar.bin"), b"synthetic cached avatar").unwrap();
        let store = Arc::new(MemoryRefreshStore::default());
        let mut state = OfflineState::new(directory.join("local-services.json"));
        state.identity_session = IdentitySession::with_refresh_store(store.clone());
        state.local_provider = true;
        state.logged_in = true;
        state.keys.insert("saved-key".into(), "keep-value".into());
        state
            .storage_hashes
            .insert("saved-key".into(), "keep-hash".into());
        state.cloud_settings = Some(json!({"keep":true}));
        state.persist().unwrap();
        let lua = Lua::new();
        let globals = lua.globals();
        globals.set("uniqueDeviceId", "synthetic-device").unwrap();
        let runtime = install(
            &lua,
            &globals,
            Arc::new(Mutex::new(state)),
            ApplicationEventScheduler::default(),
        )
        .unwrap();
        lua.load(
            r#"
            login_calls, ignored_calls, nickname_calls = 0, 0, 0
            SkynestAccount.onLoginSuccess=function() login_calls=login_calls+1 end
            SkynestAccount.onLoginFailure=function() login_calls=login_calls+1 end
        "#,
        )
        .exec()
        .unwrap();
        // An unrelated login job must not be completed by unregistration.
        runtime.begin_login_job().unwrap();
        let fixture = Self {
            lua,
            runtime,
            store,
            directory,
        };
        fixture.replace("fixture-account", json!([]));
        fixture
    }

    fn configure(&self, listener: &TcpListener) {
        *self.runtime.compatible_url.lock().unwrap() = Some(
            IdentityEndpoint::parse(&format!(
                "http://{}/proxy/identity/3.0",
                listener.local_addr().unwrap()
            ))
            .unwrap(),
        );
        *self.runtime.client_id.lock().unwrap() = "synthetic-client".into();
        *self.runtime.client_signing.lock().unwrap() = ClientSigning::literal(
            "fixture-client-signature".into(),
            "fixture-client-salt".into(),
        );
        // Retain the injected memory registry instead of rebinding it when
        // the public Lua binding prepares this synthetic provider.
        *self.runtime.bound_registry.borrow_mut() = Some(registry_path(
            &self.runtime.registry_root,
            &self.runtime.online_config().unwrap(),
        ));
    }

    fn replace(&self, id: &str, external: Json) {
        self.runtime.session.install_flat(&AccessResponse {
            access_token: format!("{id}-access"),
            refresh_token: format!("{id}-refresh"),
            absolute_expiry: i64::MAX,
            segment: Some("fixture-segments".into()),
        });
        let mut profile = session::parse_profile_value(&profile_json(id, external));
        profile.avatar_paths.insert(
            3,
            self.directory.join("avatar.bin").to_string_lossy().into(),
        );
        assert!(
            self.runtime
                .session
                .install_profile_if_epoch(self.runtime.session.epoch(), &profile,)
                .unwrap()
        );
    }

    fn call(&self) {
        let results: MultiValue = self
            .lua
            .load(
                r#"return SkynestAccount.native_unRegister(nil, true, "ignored",
                function() ignored_calls=ignored_calls+1 end)"#,
            )
            .eval()
            .unwrap();
        assert!(results.is_empty(), "native adapter returns no Lua values");
    }

    fn finish_online(&self) {
        let deadline = Instant::now() + Duration::from_secs(5);
        while self.runtime.online_completions.lock().unwrap().is_empty() {
            assert!(Instant::now() < deadline, "unregistration worker timed out");
            thread::sleep(Duration::from_millis(1));
        }
        dispatch_online_completion(&self.lua, &self.runtime).unwrap();
        assert!(self.runtime.online_completions.lock().unwrap().is_empty());
        assert_eq!(self.lua.globals().get::<i64>("login_calls").unwrap(), 0);
        assert_eq!(self.lua.globals().get::<i64>("ignored_calls").unwrap(), 0);
    }

    fn snapshot(&self) -> Json {
        let state = self.runtime.state.lock().unwrap();
        let tokens = self.runtime.session.level2_tokens();
        let profile = self.runtime.session.profile().unwrap();
        json!({
            "keys": state.keys, "hashes":state.storage_hashes, "cloud":state.cloud_settings,
            "logged_in":state.logged_in, "progress":state.login_in_progress,
            "job":self.runtime.active_login_job.get(), "epoch":self.runtime.session.epoch(),
            "owner":format!("{:?}", self.runtime.session.request_owner(ProviderLevel::Level2)),
            "tokens":[tokens.access_token,tokens.refresh_token,tokens.segment,tokens.absolute_expiry.to_string()],
            "profile":profile.raw, "avatar_paths":profile.avatar_paths,
            "persisted_refresh":self.store.load().unwrap(), "cached_profile":self.store.load_profile().unwrap(),
            "local_file":fs::read(&state.persistence_path).unwrap(),
            "avatar_file":fs::read(self.directory.join("avatar.bin")).unwrap(),
        })
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.directory).unwrap();
    }
}

fn profile_json(id: &str, external: Json) -> Json {
    json!({
        "publicAccountId":id,
        "personal":{"nickName":"Synthetic","email":"fixture@example.invalid"},
        "externalNetworks":external,
    })
}

struct Request {
    start: String,
    headers: BTreeMap<String, String>,
    body: String,
}

fn listener() -> TcpListener {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    listener
}

fn accept(listener: &TcpListener) -> (TcpStream, Request) {
    let deadline = Instant::now() + Duration::from_secs(5);
    let mut stream = loop {
        match listener.accept() {
            Ok((stream, _)) => break stream,
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                assert!(Instant::now() < deadline, "loopback request timed out");
                thread::sleep(Duration::from_millis(1));
            }
            Err(error) => panic!("loopback accept failed: {error}"),
        }
    };
    stream.set_nonblocking(false).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(5)))
        .unwrap();
    let mut bytes = Vec::new();
    let mut chunk = [0; 2048];
    loop {
        let read = stream.read(&mut chunk).unwrap();
        assert_ne!(read, 0, "incomplete request");
        bytes.extend_from_slice(&chunk[..read]);
        assert!(bytes.len() < 65_536, "unexpectedly large request");
        if let Some(end) = bytes.windows(4).position(|part| part == b"\r\n\r\n") {
            let text = std::str::from_utf8(&bytes[..end]).unwrap();
            let mut lines = text.lines();
            let start = lines.next().unwrap().to_owned();
            let headers: BTreeMap<_, _> = lines
                .map(|line| {
                    let (key, value) = line.split_once(':').unwrap();
                    (key.to_ascii_lowercase(), value.trim().to_owned())
                })
                .collect();
            let len = headers
                .get("content-length")
                .map(|value| value.parse::<usize>().unwrap())
                .unwrap_or(0);
            if bytes.len() >= end + 4 + len {
                return (
                    stream,
                    Request {
                        start,
                        headers,
                        body: String::from_utf8(bytes[end + 4..end + 4 + len].to_vec()).unwrap(),
                    },
                );
            }
        }
    }
}

fn reply(mut stream: TcpStream, status: u16, body: &str) {
    write!(
        stream,
        "HTTP/1.1 {status} Fixture\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len(),
    )
    .unwrap();
}

fn serve(
    listener: TcpListener,
    responses: Vec<(u16, String)>,
) -> thread::JoinHandle<(TcpListener, Vec<Request>)> {
    thread::spawn(move || {
        let requests = responses
            .into_iter()
            .map(|(status, body)| {
                let (stream, request) = accept(&listener);
                reply(stream, status, &body);
                request
            })
            .collect();
        (listener, requests)
    })
}

fn assert_no_request(listener: &TcpListener) {
    assert!(
        matches!(listener.accept(), Err(error) if error.kind() == std::io::ErrorKind::WouldBlock)
    );
}

fn assert_external(
    request: &Request,
    operation: &str,
    provider: &str,
    access: &str,
    segment: &str,
) {
    assert_eq!(
        request.start,
        format!("POST /proxy/identity/2.0/{operation} HTTP/1.1")
    );
    assert_eq!(request.body, format!("provider={provider}"));
    assert_eq!(
        request.headers["content-type"],
        "application/x-www-form-urlencoded"
    );
    assert_eq!(request.headers["x-access-token"], access);
    assert_eq!(request.headers["rovio-sgs"], segment);
}

#[test]
fn unregister_local_retains_account_and_saved_data_without_consuming_other_callbacks() {
    let fixture = Fixture::new();
    let before = fixture.snapshot();
    fixture.call();
    assert_eq!(fixture.snapshot(), before);
    let callback = fixture
        .lua
        .load("return function() nickname_calls=nickname_calls+1 end")
        .eval::<mlua::Function>()
        .unwrap();
    fixture
        .runtime
        .queue_nickname_validation(fixture.lua.create_registry_value(callback).unwrap(), true);
    dispatch_local_completion(&fixture.lua, &fixture.runtime).unwrap();
    assert_eq!(
        fixture.lua.globals().get::<i64>("nickname_calls").unwrap(),
        0
    );
    assert_eq!(fixture.snapshot(), before);
    dispatch_local_completion(&fixture.lua, &fixture.runtime).unwrap();
    assert_eq!(
        fixture.lua.globals().get::<i64>("nickname_calls").unwrap(),
        1
    );
    assert_eq!(fixture.lua.globals().get::<i64>("login_calls").unwrap(), 0);
    assert_eq!(
        fixture.lua.globals().get::<i64>("ignored_calls").unwrap(),
        0
    );
    assert_eq!(fixture.snapshot(), before);
}

#[test]
fn unregister_original_lua_saves_settings_before_separate_logout_and_preserves_saved_keys() {
    let fixture = Fixture::new();
    let before = fixture.snapshot();
    let state = fixture.runtime.state.clone();
    fixture
        .lua
        .globals()
        .set(
            "probe_before_logout",
            fixture
                .lua
                .create_function(move |_, ()| {
                    let state = state.lock().unwrap();
                    Ok((state.logged_in, state.keys.get("saved-key").cloned()))
                })
                .unwrap(),
        )
        .unwrap();
    let environment = fixture.lua.create_table().unwrap();
    let metatable = fixture.lua.create_table().unwrap();
    metatable.set("__index", fixture.lua.globals()).unwrap();
    environment.set_metatable(Some(metatable)).unwrap();
    fixture
        .lua
        .load(
            r#"
        order,settings_saves={},{}
        events=setmetatable({}, {__index=function(_,name) return name end})
        eventManager={addEventListener=function() end,
            notify=function(_,event) table.insert(order,event.id) end}
        ui={Frame={inherit=function() return {} end}}
        RASettingsManager={saveRuntimeSettings=function()
            local logged_in,key=probe_before_logout()
            table.insert(settings_saves,{logged_in=logged_in,key=key})
            table.insert(order,"saveRuntimeSettings")
        end}
    "#,
        )
        .set_environment(environment.clone())
        .exec()
        .unwrap();
    let source = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../runtime/data/scripts_common/cloud/rovioid/SkynestAccount.lua");
    let prepared = stella_assets::lua::prepare_for_host(&fs::read(source).unwrap()).unwrap();
    fixture
        .lua
        .load(prepared)
        .set_environment(environment.clone())
        .exec()
        .unwrap();
    fixture
        .lua
        .load("SkynestAccount.unRegister()")
        .set_environment(environment.clone())
        .exec()
        .unwrap();
    let saves = environment.get::<mlua::Table>("settings_saves").unwrap();
    assert_eq!(saves.raw_len(), 2);
    assert!(
        saves
            .get::<mlua::Table>(1)
            .unwrap()
            .get::<bool>("logged_in")
            .unwrap()
    );
    assert!(
        !saves
            .get::<mlua::Table>(2)
            .unwrap()
            .get::<bool>("logged_in")
            .unwrap()
    );
    for i in 1..=2 {
        assert_eq!(
            saves
                .get::<mlua::Table>(i)
                .unwrap()
                .get::<String>("key")
                .unwrap(),
            "keep-value"
        );
    }
    let order = environment
        .get::<mlua::Table>("order")
        .unwrap()
        .sequence_values::<String>()
        .collect::<LuaResult<Vec<_>>>()
        .unwrap();
    assert_eq!(
        order,
        [
            "saveRuntimeSettings",
            "EID_ROVIO_ID_LOGOUT",
            "saveRuntimeSettings"
        ]
    );
    assert!(fixture.runtime.session.profile().is_none());
    assert_eq!(fixture.store.load().unwrap(), "");
    let state = fixture.runtime.state.lock().unwrap();
    assert!(!state.logged_in);
    assert!(
        state.login_in_progress,
        "shipped logout starts the next login separately"
    );
    assert_eq!(json!(state.keys), before["keys"]);
    assert_eq!(json!(state.storage_hashes), before["hashes"]);
    assert_eq!(json!(state.cloud_settings), before["cloud"]);
    assert_eq!(
        json!(fs::read(&state.persistence_path).unwrap()),
        before["local_file"]
    );
    drop(state);
    let next_job = fixture.runtime.active_login_job.get();
    assert!(next_job.is_some());
    // The prior empty unregistration completion must not end the new login.
    dispatch_local_completion(&fixture.lua, &fixture.runtime).unwrap();
    assert_eq!(fixture.runtime.active_login_job.get(), next_job);
    assert!(fixture.runtime.state.lock().unwrap().login_in_progress);
}

#[test]
fn unregister_uses_first_external_provider_and_ordered_2xx_posts_without_profile_refresh() {
    for provider in ["facebook", "sinaweibo", "gamecenter", "kakaotalk"] {
        for id in ["selected-id", ""] {
            let fixture = Fixture::new();
            fixture.replace(
                "fixture-account",
                json!([
                    {"provider":provider,"id":id}, {"provider":"facebook","id":"later-id"}
                ]),
            );
            let listener = listener();
            fixture.configure(&listener);
            let before = fixture.snapshot();
            let worker = serve(
                listener,
                vec![(201, "not JSON".into()), (204, String::new())],
            );
            fixture.call();
            fixture.finish_online();
            let (listener, requests) = worker.join().unwrap();
            assert_eq!(requests.len(), 2);
            assert_external(
                &requests[0],
                "external/remove",
                provider,
                "fixture-account-access",
                "fixture-segments",
            );
            assert_external(
                &requests[1],
                "external/disconnect",
                provider,
                "fixture-account-access",
                "fixture-segments",
            );
            assert_no_request(&listener);
            assert_eq!(fixture.snapshot(), before);
        }
    }
}

#[test]
fn unregister_unknown_or_absent_first_external_does_not_acquire_a_session_or_scan_later_entries() {
    for external in [
        json!([]),
        json!([null,{"provider":"facebook","id":"later"}]),
        json!([{"provider":"unknown","id":"first"},{"provider":"facebook","id":"later"}]),
        json!([{"provider":3,"id":"first"},{"provider":"facebook","id":"later"}]),
    ] {
        let fixture = Fixture::new();
        fixture.replace("fixture-account", external);
        // Empty access forces any accidental HTTP path to acquire a session.
        fixture.runtime.session.install_flat(&AccessResponse {
            access_token: String::new(),
            refresh_token: "fixture-refresh".into(),
            absolute_expiry: 0,
            segment: None,
        });
        let listener = listener();
        fixture.configure(&listener);
        let before = fixture.snapshot();
        fixture.call();
        fixture.finish_online();
        assert_no_request(&listener);
        assert_eq!(fixture.snapshot(), before);
    }
}

#[test]
fn unregister_non_2xx_aborts_remaining_requests_without_lua_login_failure_or_clearing_data() {
    for responses in [
        vec![(400, "remove rejected".into())],
        vec![(200, "ignored".into()), (500, "disconnect rejected".into())],
    ] {
        let fixture = Fixture::new();
        fixture.replace(
            "fixture-account",
            json!([{"provider":"facebook","id":"selected"}]),
        );
        let listener = listener();
        fixture.configure(&listener);
        let before = fixture.snapshot();
        let count = responses.len();
        let worker = serve(listener, responses);
        fixture.call();
        fixture.finish_online();
        let (listener, requests) = worker.join().unwrap();
        assert_eq!(requests.len(), count);
        assert_external(
            &requests[0],
            "external/remove",
            "facebook",
            "fixture-account-access",
            "fixture-segments",
        );
        if count == 2 {
            assert_external(
                &requests[1],
                "external/disconnect",
                "facebook",
                "fixture-account-access",
                "fixture-segments",
            );
        }
        assert_no_request(&listener);
        assert_eq!(fixture.snapshot(), before);
    }
}

#[test]
fn unregister_401_renews_once_and_replays_frozen_form_then_disconnects_with_new_headers() {
    let fixture = Fixture::new();
    let external = json!([{"provider":"facebook","id":"selected"}]);
    fixture.replace("fixture-account", external.clone());
    let listener = listener();
    fixture.configure(&listener);
    let before = fixture.snapshot();
    let renewed = json!({
        "userAuth":{"accessToken":"renewed-access","refreshToken":"renewed-refresh","expiresIn":3600},
        "segments":[3,8], "profile":profile_json("fixture-account",external), "config":{},
    });
    let worker = serve(
        listener,
        vec![
            (401, String::new()),
            (200, renewed.to_string()),
            (201, "ignored".into()),
            (204, String::new()),
        ],
    );
    fixture.call();
    fixture.finish_online();
    let (listener, requests) = worker.join().unwrap();
    assert_eq!(requests.len(), 4);
    assert_external(
        &requests[0],
        "external/remove",
        "facebook",
        "fixture-account-access",
        "fixture-segments",
    );
    assert_eq!(
        requests[1].start,
        "POST /proxy/session/1/apps/synthetic-client/sessions HTTP/1.1"
    );
    let acquisition: Json = serde_json::from_str(&requests[1].body).unwrap();
    assert_eq!(
        acquisition["refresh"],
        json!({"token":"fixture-account-refresh"})
    );
    assert_external(
        &requests[2],
        "external/remove",
        "facebook",
        "renewed-access",
        "3, 8",
    );
    assert_external(
        &requests[3],
        "external/disconnect",
        "facebook",
        "renewed-access",
        "3, 8",
    );
    assert_no_request(&listener);
    let after = fixture.snapshot();
    for key in [
        "keys",
        "hashes",
        "cloud",
        "local_file",
        "avatar_file",
        "logged_in",
        "progress",
        "job",
        "epoch",
    ] {
        assert_eq!(after[key], before[key], "unregistration changed {key}");
    }
    assert_eq!(fixture.store.load().unwrap(), "renewed-refresh");
    assert_eq!(
        fixture.store.load_profile().unwrap(),
        Some(renewed["profile"].clone())
    );
}

#[test]
fn unregister_held_response_cannot_disconnect_or_clear_replaced_or_logged_out_identity() {
    for logout in [false, true] {
        let fixture = Fixture::new();
        fixture.replace(
            "fixture-account",
            json!([{"provider":"facebook","id":"selected"}]),
        );
        let listener = listener();
        fixture.configure(&listener);
        fixture.call();
        let (stream, request) = accept(&listener);
        assert_external(
            &request,
            "external/remove",
            "facebook",
            "fixture-account-access",
            "fixture-segments",
        );
        if logout {
            fixture.runtime.session.logout().unwrap();
        }
        fixture.replace(
            "replacement-account",
            json!([{"provider":"gamecenter","id":"new"}]),
        );
        let before = fixture.snapshot();
        reply(stream, 200, "ignored");
        fixture.finish_online();
        assert_no_request(&listener);
        assert_eq!(fixture.snapshot(), before);
    }
}
