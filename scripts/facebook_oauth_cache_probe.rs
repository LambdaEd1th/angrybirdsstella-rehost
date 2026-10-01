//! Separate-process producer/restorer for the original-script release audit.
use super::*;
use stella_script::FacebookTokenCache;

pub(super) fn run(data: PathBuf, mode: &str, path: PathBuf) {
    let seed = mode == "cache-seed";
    let extension = mode == "cache-extend";
    let restore = mode == "cache-restore" || extension;
    assert!(seed || restore || mode == "cache-empty");
    assert_eq!(path.exists(), !seed);
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let addr = listener.local_addr().unwrap();
    let origin = format!("http://{addr}");
    let server = thread::spawn(move || {
        let mut auth = 0;
        let mut graph = 0;
        for _ in 0..if seed { 2 } else { usize::from(restore) } {
            let deadline = std::time::Instant::now() + Duration::from_secs(20);
            let mut stream = loop {
                match listener.accept() {
                    Ok((stream, _)) => break stream,
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        assert!(
                            std::time::Instant::now() < deadline,
                            "cache probe request timed out"
                        );
                        thread::sleep(Duration::from_millis(2));
                    }
                    Err(error) => panic!("cache fixture accept failed: {error}"),
                }
            };
            let req = request(&mut stream);
            if req.starts_with("GET /oauth?") {
                assert!(seed, "restored constructor attempted browser authorization");
                auth += 1;
                reply(
                    &mut stream,
                    200,
                    "fb12345://authorize#access_token=synthetic-persisted-release&expires_in=3600",
                );
            } else {
                graph += 1;
                let profile = if seed {
                    r#"{"id":"cache-seed-user","name":"Seed Profile"}"#
                } else {
                    r#"{"id":"cache-restored-user","name":"Fresh Restored Profile"}"#
                };
                if seed || extension {
                    facebook_oauth_batch_probe::reply_batch(
                        &mut stream,
                        &req,
                        "synthetic-persisted-release",
                        profile,
                        extension,
                        true,
                    );
                } else {
                    assert!(req.starts_with("GET /v2.0/me?"));
                    assert!(req.contains("access_token=synthetic-persisted-release"));
                    reply(&mut stream, 200, profile);
                }
            }
        }
        (listener, auth, graph)
    });
    let cache = FacebookTokenCache::open(&path).unwrap();
    let callbacks = Arc::new(Mutex::new(Vec::new()));
    let inbox = callbacks.clone();
    let provider = Arc::new(
        FacebookOAuthSession::new_with_cache(
            FacebookOAuthConfig {
                rest_root: None,
                graph_root: format!("{origin}/v2.0"),
                authorization_url: format!("{origin}/oauth"),
                app_id: "12345".into(),
                url_scheme_suffix: String::new(),
                request_birthday: true,
            },
            cache.clone(),
            move |url| {
                assert!(seed, "cache startup opened browser");
                let path = url.strip_prefix(&format!("http://{addr}")).unwrap();
                let mut stream = TcpStream::connect(addr).unwrap();
                stream
                    .set_read_timeout(Some(Duration::from_secs(10)))
                    .unwrap();
                write!(
                    stream,
                    "GET {path} HTTP/1.1\r\nHost: {addr}\r\nConnection: close\r\n\r\n"
                )
                .unwrap();
                let mut response = String::new();
                stream.read_to_string(&mut response).unwrap();
                let (headers, body) = response.split_once("\r\n\r\n").unwrap();
                assert!(headers.starts_with("HTTP/1.1 200 "));
                inbox.lock().unwrap().push(body.to_owned());
                Ok(true)
            },
        )
        .unwrap(),
    );
    assert_eq!(
        provider.session_state(),
        if restore {
            FacebookSessionState::Open
        } else {
            FacebookSessionState::Created
        }
    );
    if restore {
        assert!(
            matches!(
                provider.clone().prepare_user_profile(),
                SocialProfileRequest::Pending(_)
            ),
            "service user must not be persisted with the SDK token"
        );
    }
    let game = StellaLua::new_with_missing_global_diagnostics(data).unwrap();
    game.enable_local_services().unwrap();
    game.set_facebook_session(Some(provider.clone())).unwrap();
    game.boot("scripts/game.lua").unwrap();
    let mut clock = AudioOutputClock::default();
    let mut commands = 0;
    for _ in 0..600 {
        commands = frame(&game, &mut clock);
        if restore {
            thread::sleep(Duration::from_millis(1));
        }
    }
    assert!(commands > 0);
    game.execute_source("if not menuManager or not menuManager:getRoot() then error('original cache scene missing') end").unwrap();
    if seed {
        assert!(matches!(
            provider.clone().prepare_login(),
            SocialLoginRequest::AwaitingCallback
        ));
        assert!(
            game.handle_platform_open_url(&callbacks.lock().unwrap().pop().unwrap())
                .unwrap()
        );
        for _ in 0..120 {
            frame(&game, &mut clock);
            thread::sleep(Duration::from_millis(1));
        }
        assert_eq!(provider.take_login_completion(), Some(Ok(())));
    }
    if seed || restore {
        match provider.clone().prepare_user_profile() {
            SocialProfileRequest::Ready(Ok(profile)) => {
                assert_eq!(
                    profile.access_token,
                    if extension {
                        "synthetic-extended-release"
                    } else {
                        "synthetic-persisted-release"
                    }
                );
                assert_eq!(
                    provider.session_state(),
                    if extension {
                        FacebookSessionState::OpenTokenExtended
                    } else {
                        FacebookSessionState::Open
                    }
                );
                assert_eq!(
                    profile.user.id,
                    if seed {
                        "cache-seed-user"
                    } else {
                        "cache-restored-user"
                    }
                );
            }
            _ => panic!("cache service profile was not published by the runtime"),
        }
    }
    if extension {
        std::fs::copy(&path, path.with_extension("extended.plist")).unwrap();
        println!("cache-extend: SDK_state=514; profile_token=current; refresh_cache=persisted");
    }
    if restore {
        provider.logout().unwrap();
    }
    assert_eq!(cache.take_error(), None);
    assert!(game.fallback_calls().is_empty());
    assert!(game.compatibility_bindings().is_empty());
    let (listener, auth, graph) = server.join().unwrap();
    assert_eq!(
        (auth, graph),
        if seed {
            (1, 1)
        } else if restore {
            (0, 1)
        } else {
            (0, 0)
        }
    );
    assert!(matches!(listener.accept(), Err(e) if e.kind() == std::io::ErrorKind::WouldBlock));
    println!(
        "{mode}: frames={}; auth={auth}; graph={graph}; cache_error=none; fallback_calls=0; compatibility_bindings=0",
        if seed { 720 } else { 600 }
    );
}
