//! Original Lua integration with actual loopback host launch results.
use super::*;
use stella_script::FacebookTokenCache;

fn exchange(
    addr: std::net::SocketAddr,
    url: &str,
    application: bool,
    inbox: &Mutex<Vec<String>>,
) -> Result<bool, SocialPlatformError> {
    let mut stream = TcpStream::connect(addr).map_err(|_| SocialPlatformError::Transport)?;
    stream
        .set_read_timeout(Some(Duration::from_secs(10)))
        .unwrap();
    if application {
        write!(stream, "POST /application HTTP/1.1\r\nHost: {addr}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{url}", url.len()).unwrap();
    } else {
        let origin = format!("http://{addr}");
        let path = url.strip_prefix(&origin).unwrap();
        write!(
            stream,
            "GET {path} HTTP/1.1\r\nHost: {addr}\r\nConnection: close\r\n\r\n"
        )
        .unwrap();
    }
    let mut response = String::new();
    stream
        .read_to_string(&mut response)
        .map_err(|_| SocialPlatformError::Transport)?;
    let (headers, body) = response
        .split_once("\r\n\r\n")
        .ok_or(SocialPlatformError::InvalidResponse)?;
    if headers.starts_with("HTTP/1.1 200 ") {
        inbox.lock().unwrap().push(body.to_owned());
        Ok(true)
    } else if headers.starts_with("HTTP/1.1 404 ") {
        Ok(false)
    } else {
        Err(SocialPlatformError::Transport)
    }
}
fn logger(req: &str) -> &str {
    req.split("%220_auth_logger_id%22%3A%22")
        .nth(1)
        .unwrap()
        .split("%22")
        .next()
        .unwrap()
}
pub(super) fn run(data: PathBuf, path: PathBuf) {
    assert!(!path.exists());
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let addr = listener.local_addr().unwrap();
    let server = thread::spawn(move || {
        let mut fallback_logger = String::new();
        for step in 0..5 {
            let deadline = std::time::Instant::now() + Duration::from_secs(20);
            let mut stream = loop {
                match listener.accept() {
                    Ok((stream, _)) => break stream,
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        assert!(
                            std::time::Instant::now() < deadline,
                            "app probe request timed out"
                        );
                        thread::sleep(Duration::from_millis(2));
                    }
                    Err(error) => panic!("accept: {error}"),
                }
            };
            let req = request(&mut stream);
            match step {
                0 | 2 => {
                    assert!(req.starts_with("POST /application HTTP/1.1"));
                    assert!(req.contains("\r\n\r\nfbauth://authorize?"));
                    for part in [
                        "redirect_uri=fbconnect%3A%2F%2Fsuccess",
                        "fb_application_web_auth",
                        "sdk_version=3.14.1",
                        "scope=public_profile%2Cemail%2Cuser_friends%2Cuser_birthday",
                    ] {
                        assert!(req.contains(part), "missing {part}");
                    }
                    assert!(!req.contains("access_token="));
                    if step == 0 {
                        reply(
                            &mut stream,
                            200,
                            "fb12345://authorize#access_token=synthetic-application-release&expires_in=3600",
                        );
                    } else {
                        fallback_logger = logger(&req).to_owned();
                        reply(&mut stream, 404, "");
                    }
                }
                3 => {
                    assert!(req.starts_with("GET /oauth?"));
                    assert!(req.contains("redirect_uri=fb12345%3A%2F%2Fauthorize"));
                    assert!(req.contains("browser_auth"));
                    assert_eq!(logger(&req), fallback_logger);
                    reply(
                        &mut stream,
                        200,
                        "fb12345://authorize#access_token=synthetic-browser-fallback-release&expires_in=3600",
                    );
                }
                1 | 4 => facebook_oauth_batch_probe::reply_batch(
                    &mut stream,
                    &req,
                    if step == 1 {
                        "synthetic-application-release"
                    } else {
                        "synthetic-browser-fallback-release"
                    },
                    if step == 1 {
                        r#"{"id":"application-user","name":"Application User"}"#
                    } else {
                        r#"{"id":"browser-user","name":"Browser User"}"#
                    },
                    false,
                    true,
                ),
                _ => unreachable!(),
            }
        }
        listener
    });
    let cache = FacebookTokenCache::open(&path).unwrap();
    let callbacks = Arc::new(Mutex::new(Vec::new()));
    let browser_inbox = callbacks.clone();
    let provider = Arc::new(
        FacebookOAuthSession::new_with_cache(
            FacebookOAuthConfig {
                rest_root: None,
                graph_root: format!("http://{addr}/v2.0"),
                authorization_url: format!("http://{addr}/oauth"),
                app_id: "12345".into(),
                url_scheme_suffix: String::new(),
                request_birthday: true,
            },
            cache.clone(),
            move |url| exchange(addr, url, false, &browser_inbox),
        )
        .unwrap(),
    );
    let app_inbox = callbacks.clone();
    provider.set_facebook_application_launcher(move |url| exchange(addr, url, true, &app_inbox));
    let game = StellaLua::new_with_missing_global_diagnostics(data).unwrap();
    game.enable_local_services().unwrap();
    game.set_facebook_session(Some(provider.clone())).unwrap();
    game.boot("scripts/game.lua").unwrap();
    let mut clock = AudioOutputClock::default();
    let mut commands = 0;
    for _ in 0..600 {
        commands = frame(&game, &mut clock);
    }
    assert!(commands > 0);
    game.execute_source("if not menuManager or not menuManager:getRoot() then error('original application scene missing') end").unwrap();
    for (suffix, token, id) in [
        ("app", "synthetic-application-release", "application-user"),
        (
            "browser",
            "synthetic-browser-fallback-release",
            "browser-user",
        ),
    ] {
        assert!(matches!(
            provider.clone().prepare_login(),
            SocialLoginRequest::AwaitingCallback
        ));
        assert_eq!(provider.session_state(), FacebookSessionState::Opening);
        let callback = callbacks.lock().unwrap().pop().unwrap();
        assert!(game.handle_platform_open_url(&callback).unwrap());
        game.post_application_resumed();
        for _ in 0..120 {
            frame(&game, &mut clock);
            thread::sleep(Duration::from_millis(1));
        }
        assert_eq!(provider.take_login_completion(), Some(Ok(())));
        match provider.clone().prepare_user_profile() {
            SocialProfileRequest::Ready(Ok(profile)) => {
                assert_eq!(profile.access_token, token);
                assert_eq!(profile.user.id, id);
            }
            _ => panic!("application service profile missing"),
        }
        assert_eq!(provider.session_state(), FacebookSessionState::Open);
        std::fs::copy(&path, path.with_extension(format!("{suffix}.plist"))).unwrap();
        provider.logout().unwrap();
    }
    assert!(callbacks.lock().unwrap().is_empty());
    assert_eq!(cache.take_error(), None);
    assert!(game.fallback_calls().is_empty());
    assert!(game.compatibility_bindings().is_empty());
    let listener = server.join().unwrap();
    assert!(matches!(listener.accept(), Err(e) if e.kind() == std::io::ErrorKind::WouldBlock));
    println!(
        "application-auth: frames=840; application=2; browser=1; graph=2; cache_error=none; fallback_calls=0; compatibility_bindings=0"
    );
}
