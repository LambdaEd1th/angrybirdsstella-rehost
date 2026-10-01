//! Headless host-contract integration: actual local browser/dialog loads and
//! original Lua consumers. This fixture does not provide a graphical web view.
use super::*;
use stella_script::{
    FacebookLoginDialogAdapter, FacebookLoginDialogEvent, FacebookLoginDialogRequest,
    FacebookTokenCache,
};
mod host;
use host::{Event, View};

fn logger(req: &str) -> &str {
    req.split("%220_auth_logger_id%22%3A%22")
        .nth(1)
        .unwrap()
        .split("%22")
        .next()
        .unwrap()
}
pub(super) fn run(data: PathBuf, mode: &str, path: PathBuf) {
    let restore = mode == "inline-restore";
    assert_eq!(path.exists(), restore);
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let addr = listener.local_addr().unwrap();
    let server = thread::spawn(move || {
        let mut auth_id = String::new();
        for step in 0..if restore { 1 } else { 9 } {
            let deadline = std::time::Instant::now() + Duration::from_secs(20);
            let mut stream = loop {
                match listener.accept() {
                    Ok((stream, _)) => break stream,
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        assert!(
                            std::time::Instant::now() < deadline,
                            "inline probe request timed out"
                        );
                        thread::sleep(Duration::from_millis(2));
                    }
                    Err(error) => panic!("inline accept: {error}"),
                }
            };
            let req = request(&mut stream);
            if restore {
                assert!(req.starts_with("GET /v2.0/me?"));
                assert!(req.contains("access_token=synthetic-inline-release"));
                assert!(!req.contains("extendSSOAccessToken"));
                reply(
                    &mut stream,
                    200,
                    r#"{"id":"inline-restored-user","name":"Inline Restored User"}"#,
                );
            } else if matches!(step, 0 | 3 | 5 | 7) {
                assert!(req.starts_with("GET /oauth?"));
                assert!(req.contains("browser_auth"));
                auth_id = logger(&req).to_owned();
                reply(&mut stream, 404, ""); // Actual unavailable browser route.
            } else if step == 2 {
                facebook_oauth_batch_probe::reply_batch(
                    &mut stream,
                    &req,
                    "synthetic-inline-release",
                    r#"{"id":"inline-user","name":"Inline User"}"#,
                    false,
                    true,
                );
            } else {
                assert!(req.starts_with("GET /dialog/oauth?"));
                assert!(req.contains("fallback_auth"));
                assert!(req.contains("redirect_uri=fbconnect%3A%2F%2Fsuccess"));
                assert!(
                    req.contains("scope=public_profile%2Cemail%2Cuser_friends%2Cuser_birthday")
                );
                assert_eq!(logger(&req), auth_id);
                if step == 1 {
                    reply(
                        &mut stream,
                        200,
                        "fbconnect://success#access_token=synthetic-inline-release&expires_in=3600",
                    );
                } else if step == 4 {
                    reply(&mut stream, 200, "fbconnect://cancel");
                } else if step == 8 {
                    reply(
                        &mut stream,
                        200,
                        "fbconnect://success#access_token=%FF&expires_in=3600",
                    );
                } else {
                    // Truncated body produces a real std::io UnexpectedEof.
                    write!(stream, "HTTP/1.1 200 Fixture\r\nContent-Length: 100\r\nConnection: close\r\n\r\npartial").unwrap();
                }
            }
        }
        listener
    });
    let cache = FacebookTokenCache::open(&path).unwrap();
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
            move |url| {
                assert!(!restore, "restored inline session requested authorization");
                let local = url.strip_prefix(&format!("http://{addr}")).unwrap();
                let mut stream = TcpStream::connect(addr).unwrap();
                stream
                    .set_read_timeout(Some(Duration::from_secs(10)))
                    .unwrap();
                write!(
                    stream,
                    "GET {local} HTTP/1.1\r\nHost: {addr}\r\nConnection: close\r\n\r\n"
                )
                .unwrap();
                let mut response = String::new();
                stream.read_to_string(&mut response).unwrap();
                assert!(response.starts_with("HTTP/1.1 404 "));
                Ok(false)
            },
        )
        .unwrap(),
    );
    let view = Arc::new(View {
        addr,
        owner: Arc::downgrade(&provider),
        events: Mutex::new(Vec::new()),
        dismissed: Mutex::new(Vec::new()),
    });
    provider
        .set_login_dialog_adapter(&format!("http://{addr}/dialog/oauth"), view.clone())
        .unwrap();
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
    game.execute_source("if not menuManager or not menuManager:getRoot() then error('original inline scene missing') end").unwrap();
    if restore {
        match provider.clone().prepare_user_profile() {
            SocialProfileRequest::Ready(Ok(profile)) => {
                assert_eq!(profile.access_token, "synthetic-inline-release");
                assert_eq!(profile.user.id, "inline-restored-user");
            }
            _ => panic!("restored inline profile missing"),
        }
        assert_eq!(provider.session_state(), FacebookSessionState::Open);
        assert!(view.events.lock().unwrap().is_empty());
        std::fs::copy(&path, path.with_extension("restored.plist")).unwrap();
        provider.logout().unwrap();
    } else {
        for phase in 0..4 {
            assert!(matches!(
                provider.clone().prepare_login(),
                SocialLoginRequest::AwaitingCallback
            ));
            let (request, event) = view.events.lock().unwrap().pop().unwrap();
            game.post_application_resumed();
            frame(&game, &mut clock);
            assert_eq!(provider.session_state(), FacebookSessionState::Opening);
            assert_eq!(provider.take_login_completion(), None);
            match event {
                Event::Redirect(url) => {
                    assert!(
                        game.handle_platform_login_dialog_event(
                            &FacebookLoginDialogEvent::Navigation {
                                request_id: request.request_id.clone(),
                                url,
                                link_clicked: false
                            }
                        )
                        .unwrap()
                    );
                }
                Event::LoadFailure(domain, code) => {
                    assert_eq!(phase, 2);
                    assert_eq!(domain, "std::io::UnexpectedEof");
                    assert!(
                        game.handle_platform_login_dialog_event(
                            &FacebookLoginDialogEvent::LoadFailure {
                                request_id: request.request_id.clone(),
                                domain,
                                code
                            }
                        )
                        .unwrap()
                    );
                }
            }
            for _ in 0..119 {
                frame(&game, &mut clock);
                thread::sleep(Duration::from_millis(1));
            }
            assert_eq!(
                provider.take_login_completion(),
                Some(match phase {
                    0 => Ok(()),
                    1 => Err(SocialPlatformError::Cancelled),
                    _ => Err(SocialPlatformError::Cancelled),
                })
            );
            if phase == 0 {
                match provider.clone().prepare_user_profile() {
                    SocialProfileRequest::Ready(Ok(profile)) => {
                        assert_eq!(profile.access_token, "synthetic-inline-release");
                        assert_eq!(profile.user.id, "inline-user");
                    }
                    _ => panic!("inline profile missing"),
                }
                std::fs::copy(&path, path.with_extension("inline.plist")).unwrap();
                provider.logout().unwrap();
            } else {
                assert_eq!(
                    provider.session_state(),
                    FacebookSessionState::ClosedLoginFailed
                );
            }
        }
        assert_eq!(
            *view.dismissed.lock().unwrap(),
            [
                (true, FacebookSessionState::Open),
                (false, FacebookSessionState::Opening),
                (false, FacebookSessionState::Opening),
                (false, FacebookSessionState::Opening),
                (false, FacebookSessionState::ClosedLoginFailed)
            ]
        );
    }
    assert_eq!(cache.take_error(), None);
    assert!(game.fallback_calls().is_empty());
    assert!(game.compatibility_bindings().is_empty());
    let listener = server.join().unwrap();
    assert!(matches!(listener.accept(), Err(e) if e.kind() == std::io::ErrorKind::WouldBlock));
    println!(
        "{mode}: frames={}; browser={}; dialog={}; graph=1; cache_error=none; fallback_calls=0; compatibility_bindings=0",
        if restore { 600 } else { 1080 },
        if restore { 0 } else { 4 },
        if restore { 0 } else { 4 }
    );
}
