use std::{
    io::{Read, Write},
    net::{TcpListener, TcpStream},
    path::PathBuf,
    sync::{Arc, Mutex},
    thread,
    time::Duration,
};
use stella_script::{
    AudioOutputClock, FacebookOAuthConfig, FacebookOAuthSession, FacebookSessionState,
    SocialLoginRequest, SocialPlatformError, SocialPlatformProvider, SocialProfileRequest,
    StellaLua,
};
mod facebook_oauth_application_probe;
mod facebook_oauth_batch_probe;
mod facebook_oauth_cache_probe;
mod facebook_oauth_inline_probe;
mod facebook_oauth_system_probe;

fn request(stream: &mut TcpStream) -> String {
    stream.set_nonblocking(false).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(10)))
        .unwrap();
    let mut bytes = Vec::new();
    while !bytes.ends_with(b"\r\n\r\n") {
        let mut b = [0];
        stream.read_exact(&mut b).unwrap();
        bytes.push(b[0]);
        assert!(bytes.len() < 16384);
    }
    let headers = String::from_utf8(bytes).unwrap();
    let length = headers
        .lines()
        .find_map(|line| {
            let (name, value) = line.split_once(':')?;
            name.eq_ignore_ascii_case("content-length")
                .then(|| value.trim().parse::<usize>().unwrap())
        })
        .unwrap_or(0);
    assert!(length < 65536);
    let mut body = vec![0; length];
    stream.read_exact(&mut body).unwrap();
    headers + &String::from_utf8(body).unwrap()
}
fn reply(stream: &mut TcpStream, status: u16, body: &str) {
    write!(
        stream,
        "HTTP/1.1 {status} Fixture\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    )
    .unwrap();
}
fn frame(game: &StellaLua, clock: &mut AudioOutputClock) -> usize {
    let transitions =
        clock.synchronize(&game.audio_output_state(), Duration::from_nanos(16_666_667));
    game.apply_audio_playback_transitions(&transitions);
    game.update(1.0 / 60.0).unwrap();
    let drew = game.draw().unwrap();
    let commands = game.take_render_commands().len();
    if !drew {
        assert_eq!(commands, 0);
    }
    let transitions = clock.synchronize(&game.audio_output_state(), Duration::ZERO);
    game.apply_audio_playback_transitions(&transitions);
    commands
}
fn main() {
    let data = PathBuf::from(std::env::args().nth(1).expect("isolated data path"));
    if let Some(mode) = std::env::args().nth(2) {
        if matches!(mode.as_str(), "inline-auth" | "inline-restore") {
            facebook_oauth_inline_probe::run(
                data,
                &mode,
                PathBuf::from(std::env::args().nth(3).expect("isolated inline cache path")),
            );
            return;
        }
        if mode == "application-auth" {
            facebook_oauth_application_probe::run(
                data,
                PathBuf::from(
                    std::env::args()
                        .nth(3)
                        .expect("isolated application cache path"),
                ),
            );
            return;
        }
        if mode == "system-repair" {
            facebook_oauth_system_probe::run(
                data,
                PathBuf::from(std::env::args().nth(3).expect("isolated system cache path")),
            );
            return;
        }
        facebook_oauth_cache_probe::run(
            data,
            &mode,
            PathBuf::from(std::env::args().nth(3).expect("isolated cache path")),
        );
        return;
    }
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    let origin = format!("http://{addr}");
    let server = thread::spawn(move || {
        let mut auth = 0;
        let mut graph = 0;
        for _ in 0..5 {
            let (mut stream, _) = listener.accept().unwrap();
            let req = request(&mut stream);
            assert!(!req.to_ascii_lowercase().contains("x-access-token"));
            if req.starts_with("GET /oauth?") {
                auth += 1;
                for part in [
                    "response_type=token",
                    "redirect_uri=fb12345%3A%2F%2Fauthorize",
                    "sdk_version=3.14.1",
                    "browser_auth",
                    "scope=public_profile%2Cemail%2Cuser_friends%2Cuser_birthday",
                ] {
                    assert!(req.contains(part));
                }
                assert!(!req.contains("access_token="));
                if auth == 1 {
                    reply(
                        &mut stream,
                        200,
                        "fb12345://authorize#error=service_disabled_use_browser",
                    );
                } else if auth == 2 {
                    reply(
                        &mut stream,
                        200,
                        "fb12345://authorize#access_token=synthetic-release-oauth&granted_scopes=public_profile,email,user_friends",
                    );
                } else if auth == 3 {
                    reply(
                        &mut stream,
                        200,
                        "fb12345://authorize#access_token=synthetic-release-reopened&expires_in=3600",
                    );
                } else {
                    reply(&mut stream, 204, "");
                }
            } else {
                graph += 1;
                facebook_oauth_batch_probe::reply_batch(
                    &mut stream,
                    &req,
                    "synthetic-release-oauth",
                    r#"{"id":"oauth-release-user","name":"OAuth Release Fixture"}"#,
                    false,
                    false,
                );
            }
        }
        assert_eq!((auth, graph), (4, 1));
        (auth, graph)
    });
    let callbacks = Arc::new(Mutex::new(Vec::new()));
    let inbox = callbacks.clone();
    let provider = Arc::new(
        FacebookOAuthSession::new(
            FacebookOAuthConfig {
                rest_root: None,
                graph_root: format!("{origin}/v2.0"),
                authorization_url: format!("{origin}/oauth"),
                app_id: "12345".into(),
                url_scheme_suffix: String::new(),
                request_birthday: true,
            },
            move |url| {
                let path = url
                    .strip_prefix(&format!("http://{addr}"))
                    .ok_or(SocialPlatformError::InvalidConfiguration)?;
                let mut stream =
                    TcpStream::connect(addr).map_err(|_| SocialPlatformError::Transport)?;
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
                if headers.starts_with("HTTP/1.1 200 ") {
                    inbox.lock().unwrap().push(body.to_owned());
                    Ok(true)
                } else if headers.starts_with("HTTP/1.1 204 ") {
                    Ok(true)
                } else {
                    Err(SocialPlatformError::Transport)
                }
            },
        )
        .unwrap(),
    );
    let game = StellaLua::new_with_missing_global_diagnostics(data).unwrap();
    game.enable_local_services().unwrap();
    game.set_facebook_session(Some(provider.clone())).unwrap();
    game.boot("scripts/game.lua").unwrap();
    let mut clock = AudioOutputClock::default();
    let mut last_commands = 0;
    let mut empty_frames = 0;
    for _ in 0..600 {
        last_commands = frame(&game, &mut clock);
        if last_commands == 0 {
            empty_frames += 1;
        }
    }
    assert!(last_commands > 0, "startup never reached a rendered scene");
    game.execute_source("if not menuManager or not menuManager:getRoot() then error('original scene was not constructed') end").unwrap();
    println!("startup_empty_frames={empty_frames}; final_render_commands={last_commands}");
    println!("original_boot_frames=600");
    game.set_application_active(false).unwrap();
    game.set_application_audio_active(false).unwrap();
    assert!(matches!(
        provider.clone().prepare_login(),
        SocialLoginRequest::AwaitingCallback
    ));
    let redirect = callbacks.lock().unwrap().pop().unwrap();
    assert!(game.handle_platform_open_url(&redirect).unwrap());
    assert_eq!(provider.session_state(), FacebookSessionState::Opening);
    assert_eq!(provider.take_login_completion(), None);
    println!("browser_retry=opening; interim_completion=none");
    let callback = callbacks.lock().unwrap().pop().unwrap();
    assert!(game.handle_platform_open_url(&callback).unwrap());
    game.set_application_active(true).unwrap();
    game.post_application_resumed();
    game.set_application_audio_active(true).unwrap();
    for _ in 0..120 {
        frame(&game, &mut clock);
        thread::sleep(Duration::from_millis(1));
    }
    assert_eq!(provider.session_state(), FacebookSessionState::Open);
    assert_eq!(provider.take_login_completion(), Some(Ok(())));
    match provider.clone().prepare_user_profile() {
        SocialProfileRequest::Ready(Ok(p)) => assert_eq!(p.user.id, "oauth-release-user"),
        _ => panic!("service profile did not publish"),
    }
    assert_eq!(provider.declined_permissions(), ["user_birthday"]);
    println!("url_before_resume=open; service_profile=published");
    provider.close();
    provider.logout().unwrap();
    game.set_application_active(false).unwrap();
    game.set_application_audio_active(false).unwrap();
    assert!(matches!(
        provider.clone().prepare_login(),
        SocialLoginRequest::AwaitingCallback
    ));
    let reopened = callbacks.lock().unwrap().pop().unwrap();
    assert!(game.handle_platform_open_url(&reopened).unwrap());
    assert_eq!(provider.take_login_completion(), Some(Ok(())));
    match provider.clone().prepare_user_profile() {
        SocialProfileRequest::Ready(Ok(profile)) => {
            assert_eq!(profile.user.id, "oauth-release-user");
            assert_eq!(profile.access_token, "synthetic-release-reopened");
        }
        _ => panic!("reopening discarded the service profile"),
    }
    game.set_application_active(true).unwrap();
    game.post_application_resumed();
    game.set_application_audio_active(true).unwrap();
    for _ in 0..120 {
        frame(&game, &mut clock);
    }
    println!("reopened_cache=current_token; old_user=retained");
    provider.close();
    provider.logout().unwrap(); // Closed logout preserves valid cached token.
    assert!(matches!(
        provider.clone().prepare_login(),
        SocialLoginRequest::Ready(Ok(()))
    ));
    assert_eq!(provider.session_state(), FacebookSessionState::Open);
    match provider.clone().take_login_profile_request() {
        Some(SocialProfileRequest::Ready(Ok(profile))) => {
            assert_eq!(profile.access_token, "synthetic-release-reopened");
            assert_eq!(profile.user.id, "oauth-release-user");
        }
        _ => panic!("cached-token open did not schedule retained profile"),
    }
    println!("sdk_token_cache=ready_without_browser; retained_profile=true");
    provider.logout().unwrap(); // Explicitly clear token before cancellation test.
    game.set_application_active(false).unwrap();
    game.set_application_audio_active(false).unwrap();
    assert!(matches!(
        provider.clone().prepare_login(),
        SocialLoginRequest::AwaitingCallback
    ));
    assert!(callbacks.lock().unwrap().is_empty());
    game.set_application_active(true).unwrap();
    game.post_application_resumed();
    game.set_application_audio_active(true).unwrap();
    assert_eq!(provider.session_state(), FacebookSessionState::Opening);
    frame(&game, &mut clock);
    assert_eq!(
        provider.session_state(),
        FacebookSessionState::ClosedLoginFailed
    );
    assert_eq!(provider.take_login_completion(), Some(Ok(())));
    assert!(matches!(
        provider.clone().prepare_user_profile(),
        SocialProfileRequest::Ready(Err(SocialPlatformError::NotLoggedIn))
    ));
    assert!(!game.handle_platform_open_url(&callback).unwrap());
    for _ in 0..119 {
        frame(&game, &mut clock);
    }
    assert!(
        game.fallback_calls().is_empty(),
        "{:?}",
        game.fallback_calls()
    );
    assert!(
        game.compatibility_bindings().is_empty(),
        "{:?}",
        game.compatibility_bindings()
    );
    assert_eq!(server.join().unwrap(), (4, 1));
    println!("implicit_resume=closed_login_failed; nil_error=true; late_callback=false");
    println!("original_frames=960; http=5; fallback_calls=0; compatibility_bindings=0");
}
