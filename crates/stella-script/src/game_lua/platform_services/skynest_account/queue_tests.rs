//! Account queue ownership/terminal-state tests; no provider or player files.

use super::*;
use session::{MemoryRefreshStore, RefreshStore};
use std::{
    io::{Read, Write},
    net::{TcpListener, TcpStream},
    thread,
    time::{Duration, Instant},
};

struct Fixture {
    lua: Lua,
    runtime: SkynestAccountRuntime,
    store: Arc<MemoryRefreshStore>,
}

impl Fixture {
    fn new() -> Self {
        let store = Arc::new(MemoryRefreshStore::default());
        let mut state = OfflineState::new(PathBuf::from("synthetic-queue/no-file-access.json"));
        state.identity_session = IdentitySession::with_refresh_store(store.clone());
        let runtime = SkynestAccountRuntime::new(
            Arc::new(Mutex::new(state)),
            ApplicationEventScheduler::default(),
            crate::game_lua::platform_services::skynest_account::identifiers::Identifiers::synthetic().into(),
        );
        let lua = Lua::new();
        let state = runtime.state.clone();
        lua.globals()
            .set(
                "probe_progress",
                lua.create_function(move |_, ()| Ok(state.lock().unwrap().login_in_progress))
                    .unwrap(),
            )
            .unwrap();
        let active = runtime.active_login_job.clone();
        lua.globals()
            .set(
                "probe_active",
                lua.create_function(move |_, ()| Ok(active.get())).unwrap(),
            )
            .unwrap();
        lua.load(
            r#"
            successes, failures, nickname_calls = 0, 0, 0
            SkynestAccount = {
                onLoginSuccess=function(_,details)
                    successes=successes+1; delivered_id=details.id
                    seen_progress=probe_progress(); seen_active=probe_active()
                end,
                onLoginFailure=function()
                    failures=failures+1
                    seen_progress=probe_progress(); seen_active=probe_active()
                end,
            }
            weak_callbacks=setmetatable({}, {__mode="v"})
        "#,
        )
        .exec()
        .unwrap();
        Self {
            lua,
            runtime,
            store,
        }
    }

    fn replace(&self, id: &str) {
        self.runtime.session.install_flat(&AccessResponse {
            access_token: format!("{id}-access"),
            refresh_token: format!("{id}-refresh"),
            absolute_expiry: i64::MAX,
            segment: Some("fixture-segments".to_owned()),
        });
        assert!(
            self.runtime
                .session
                .install_profile_if_epoch(self.runtime.session.epoch(), &profile(id),)
                .unwrap()
        );
    }

    fn count(&self, name: &str) -> i64 {
        self.lua.globals().get(name).unwrap()
    }

    fn queue_login(&self, online: bool, owner: RequestOwner, login_job: u64, success: bool) {
        if online {
            self.runtime
                .online_completions
                .lock()
                .unwrap()
                .push_back(Queued {
                    owner,
                    value: OnlineCompletion::Login {
                        login_job,
                        result: if success {
                            Ok(profile("account-a"))
                        } else {
                            Err("synthetic failure".to_owned())
                        },
                    },
                });
            self.runtime
                .application_events
                .post(ApplicationEvent::SkynestAccountOnline);
        } else {
            self.runtime.queue_local_owned(
                owner,
                if success {
                    Completion::LoginSucceeded { login_job }
                } else {
                    Completion::LoginUnavailable { login_job }
                },
            );
        }
    }

    fn dispatch(&self, online: bool) {
        if online {
            dispatch_online_completion(&self.lua, &self.runtime).unwrap();
        } else {
            dispatch_local_completion(&self.lua, &self.runtime).unwrap();
        }
    }

    fn assert_finished(&self) {
        assert!(!self.runtime.state.lock().unwrap().login_in_progress);
        assert_eq!(self.runtime.active_login_job.get(), None);
    }

    fn callback(&self) -> RegistryKey {
        let callback: mlua::Function = self
            .lua
            .load(
                r##"
            local callback=function(...)
                nickname_calls=nickname_calls+1
                nickname_args=select("#",...)
                nickname_success,nickname_valid=...
            end
            weak_callbacks[1]=callback
            return callback
        "##,
            )
            .eval()
            .unwrap();
        self.lua.create_registry_value(callback).unwrap()
    }

    fn callback_collected(&self) -> bool {
        self.lua.gc_collect().unwrap();
        self.lua.gc_collect().unwrap();
        matches!(
            self.lua
                .globals()
                .get::<mlua::Table>("weak_callbacks")
                .unwrap()
                .get::<Value>(1)
                .unwrap(),
            Value::Nil
        )
    }
}

fn profile(id: &str) -> ProfileResponse {
    let raw = serde_json::json!({"publicAccountId":id,"personal":{"email":format!("{id}@example.invalid")}});
    let mut profile: ProfileResponse = serde_json::from_value(raw.clone()).unwrap();
    profile.raw = raw;
    profile
}

#[test]
fn account_queue_stale_auto_result_finishes_its_original_job_without_logging_in() {
    for online in [false, true] {
        for success in [false, true] {
            let fixture = Fixture::new();
            fixture.replace("account-a");
            let owner = fixture.runtime.session.request_owner(ProviderLevel::Level2);
            let job = fixture.runtime.begin_login_job().unwrap();
            fixture.queue_login(online, owner, job, success);
            let epoch = fixture.runtime.session.epoch();
            fixture.replace("account-b");
            assert_eq!(fixture.runtime.session.epoch(), epoch);
            fixture.dispatch(online);
            fixture.assert_finished();
            assert!(!fixture.runtime.state.lock().unwrap().logged_in);
            assert_eq!(fixture.count("successes"), 0);
            assert_eq!(fixture.count("failures"), 0);
            assert_eq!(
                fixture.runtime.session.profile().unwrap().public_account_id,
                "account-b"
            );
        }
    }
}

#[test]
fn account_queue_old_auto_result_cannot_finish_a_new_login_job() {
    for online in [false, true] {
        for replace in [false, true] {
            for success in [false, true] {
                let fixture = Fixture::new();
                fixture.replace("account-a");
                let owner = fixture.runtime.session.request_owner(ProviderLevel::Level2);
                let old_job = fixture.runtime.begin_login_job().unwrap();
                fixture.queue_login(online, owner, old_job, success);
                if replace {
                    fixture.replace("account-b");
                }
                let current_job = fixture.runtime.begin_login_job().unwrap();
                fixture.dispatch(online);
                assert_eq!(fixture.runtime.active_login_job.get(), Some(current_job));
                assert!(fixture.runtime.state.lock().unwrap().login_in_progress);
                if replace {
                    assert_eq!(fixture.count("successes"), 0);
                    assert_eq!(fixture.count("failures"), 0);
                }
                // If native-compatible delivery retains a same-owner old
                // callback, it still cannot finish the newer job's progress.
                let prior_successes = fixture.count("successes");
                let prior_failures = fixture.count("failures");
                fixture.queue_login(
                    online,
                    fixture.runtime.session.request_owner(ProviderLevel::Level2),
                    current_job,
                    true,
                );
                fixture.dispatch(online);
                fixture.assert_finished();
                assert_eq!(fixture.count("successes"), prior_successes + 1);
                assert_eq!(fixture.count("failures"), prior_failures);
            }
        }
    }
}

#[test]
fn account_queue_normal_auto_completion_clears_active_job_before_lua_callback() {
    for online in [false, true] {
        for success in [false, true] {
            let fixture = Fixture::new();
            fixture.replace("account-a");
            let job = fixture.runtime.begin_login_job().unwrap();
            fixture.queue_login(
                online,
                fixture.runtime.session.request_owner(ProviderLevel::Level2),
                job,
                success,
            );
            fixture.dispatch(online);
            fixture.assert_finished();
            assert_eq!(fixture.count("successes"), i64::from(success));
            assert_eq!(fixture.count("failures"), i64::from(!success));
            assert!(!fixture.lua.globals().get::<bool>("seen_progress").unwrap());
            assert!(matches!(
                fixture.lua.globals().get::<Value>("seen_active").unwrap(),
                Value::Nil
            ));
            if success {
                assert!(fixture.runtime.state.lock().unwrap().logged_in);
                assert_eq!(
                    fixture.lua.globals().get::<String>("delivered_id").unwrap(),
                    "account-a"
                );
            }
        }
    }
}

#[test]
fn account_queue_stale_nickname_results_release_registry_reference_without_calling_lua() {
    for online in [false, true] {
        for success in [false, true] {
            let fixture = Fixture::new();
            fixture.replace("account-a");
            let owner = fixture.runtime.session.request_owner(ProviderLevel::Level2);
            let callback = fixture.callback();
            if online {
                fixture.runtime.callbacks.borrow_mut().insert(17, callback);
                fixture
                    .runtime
                    .online_completions
                    .lock()
                    .unwrap()
                    .push_back(Queued {
                        owner,
                        value: OnlineCompletion::ValidateNickname {
                            request_id: 17,
                            result: if success {
                                Ok(NicknameResponse {
                                    is_valid: true,
                                    validation_message: String::new(),
                                })
                            } else {
                                Err("synthetic nickname failure".to_owned())
                            },
                        },
                    });
            } else {
                fixture.runtime.queue_local_owned(
                    owner,
                    Completion::ValidateNickname {
                        callback,
                        is_valid: success,
                    },
                );
            }
            assert!(
                !fixture.callback_collected(),
                "queued callback must still be retained"
            );
            fixture.replace("account-b");
            fixture.dispatch(online);
            assert_eq!(fixture.count("nickname_calls"), 0);
            assert!(fixture.runtime.callbacks.borrow().is_empty());
            assert!(
                fixture.callback_collected(),
                "stale registry reference was not removed"
            );
        }
    }
}

#[test]
fn account_queue_current_nickname_results_keep_native_argument_counts_and_release_callbacks() {
    for online in [false, true] {
        for success in [false, true] {
            let fixture = Fixture::new();
            fixture.replace("account-a");
            let owner = fixture.runtime.session.request_owner(ProviderLevel::Level2);
            let callback = fixture.callback();
            if online {
                fixture.runtime.callbacks.borrow_mut().insert(18, callback);
                fixture
                    .runtime
                    .online_completions
                    .lock()
                    .unwrap()
                    .push_back(Queued {
                        owner,
                        value: OnlineCompletion::ValidateNickname {
                            request_id: 18,
                            result: if success {
                                Ok(NicknameResponse {
                                    is_valid: false,
                                    validation_message: String::new(),
                                })
                            } else {
                                Err("synthetic nickname failure".to_owned())
                            },
                        },
                    });
            } else {
                fixture.runtime.queue_local_owned(
                    owner,
                    Completion::ValidateNickname {
                        callback,
                        is_valid: success,
                    },
                );
            }
            fixture.dispatch(online);
            assert_eq!(fixture.count("nickname_calls"), 1);
            assert_eq!(
                fixture.count("nickname_args"),
                if online && !success { 1 } else { 2 }
            );
            assert_eq!(
                fixture
                    .lua
                    .globals()
                    .get::<bool>("nickname_success")
                    .unwrap(),
                !online || success
            );
            assert!(fixture.callback_collected());
        }
    }
}

fn accept(listener: &TcpListener) -> (TcpStream, String) {
    let deadline = Instant::now() + Duration::from_secs(5);
    let mut stream = loop {
        match listener.accept() {
            Ok((stream, _)) => break stream,
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                assert!(Instant::now() < deadline, "auto-login loopback timed out");
                thread::sleep(Duration::from_millis(1));
            }
            Err(error) => panic!("loopback accept failed: {error}"),
        }
    };
    stream.set_nonblocking(false).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(5)))
        .unwrap();
    let mut request = Vec::new();
    loop {
        let mut chunk = [0; 2048];
        let count = stream.read(&mut chunk).unwrap();
        assert_ne!(count, 0);
        request.extend_from_slice(&chunk[..count]);
        if let Some(end) = request.windows(4).position(|bytes| bytes == b"\r\n\r\n") {
            let headers = std::str::from_utf8(&request[..end]).unwrap();
            let len = headers
                .lines()
                .find_map(|line| {
                    let (name, value) = line.split_once(':')?;
                    name.eq_ignore_ascii_case("content-length")
                        .then(|| value.trim().parse::<usize>().unwrap())
                })
                .unwrap_or(0);
            if request.len() >= end + 4 + len {
                return (stream, String::from_utf8(request).unwrap());
            }
        }
        assert!(request.len() < 65536);
    }
}

#[test]
fn account_queue_real_initial_acquire_adopts_its_published_owner_and_delivers_new_profile() {
    let fixture = Fixture::new();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let config = IdentityConfig {
        identifiers:
            crate::game_lua::platform_services::skynest_account::identifiers::Identifiers::synthetic().into(),
        endpoint: IdentityEndpoint::parse(&format!(
            "http://{}/identity/3.0",
            listener.local_addr().unwrap()
        ))
        .unwrap(),
        client_id: "queue-fixture".to_owned(),
        signing: ClientSigning::default(),
    };
    // Select synthetic configuration without invoking RegistryStore::open.
    *fixture.runtime.compatible_url.lock().unwrap() = Some(config.endpoint.clone());
    *fixture.runtime.client_id.lock().unwrap() = config.client_id.clone();
    *fixture.runtime.bound_registry.borrow_mut() =
        Some(registry_path(&fixture.runtime.registry_root, &config));
    fixture.store.store("cached-refresh").unwrap();
    fixture
        .store
        .store_profile(Some(&profile("cached-account").raw))
        .unwrap();
    fixture
        .runtime
        .session
        .bind_store(fixture.store.clone())
        .unwrap();
    let original = fixture.runtime.session.request_owner(ProviderLevel::Level2);
    fixture.runtime.begin_login().unwrap();
    let (mut stream, request) = accept(&listener);
    assert_eq!(
        request.lines().next(),
        Some("POST /session/1/apps/queue-fixture/sessions HTTP/1.1")
    );
    let body: serde_json::Value =
        serde_json::from_str(request.split_once("\r\n\r\n").unwrap().1).unwrap();
    assert_eq!(
        body["refresh"],
        serde_json::json!({"token":"cached-refresh"})
    );
    let response=serde_json::json!({
        "userAuth":{"accessToken":"resolved-access","refreshToken":"resolved-refresh","expiresIn":3600},
        "segments":[2,4],"config":{},"profile":profile("resolved-account").raw,
    }).to_string();
    write!(
        stream,
        "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{response}",
        response.len()
    )
    .unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    while fixture
        .runtime
        .online_completions
        .lock()
        .unwrap()
        .is_empty()
    {
        assert!(
            Instant::now() < deadline,
            "auto-login result did not reach queue"
        );
        thread::sleep(Duration::from_millis(1));
    }
    let queued_owner = fixture
        .runtime
        .online_completions
        .lock()
        .unwrap()
        .front()
        .unwrap()
        .owner;
    assert!(!fixture.runtime.session.request_owner_is_current(original));
    assert!(
        fixture
            .runtime
            .session
            .request_owner_is_current(queued_owner)
    );
    assert_eq!(fixture.count("successes"), 0);
    fixture.dispatch(true);
    fixture.assert_finished();
    assert_eq!(fixture.count("successes"), 1);
    assert_eq!(fixture.count("failures"), 0);
    assert_eq!(
        fixture.lua.globals().get::<String>("delivered_id").unwrap(),
        "resolved-account"
    );
    assert_eq!(fixture.store.load().unwrap(), "resolved-refresh");
    assert!(matches!(listener.accept(),Err(error) if error.kind()==std::io::ErrorKind::WouldBlock));
}
