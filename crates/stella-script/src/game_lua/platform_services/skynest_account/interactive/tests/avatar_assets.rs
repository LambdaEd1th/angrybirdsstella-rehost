//! Real own-profile worker and Lua completion queue, with isolated registry.

use super::*;
use session::RegistryStore;
use std::{
    fs,
    sync::atomic::{AtomicU64, Ordering},
};

#[test]
fn own_profile_avatar_assets_gate_tokens_events_and_the_actual_login_callback() {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    for outcome in ["success", "status", "size", "logout"] {
        let root = std::env::temp_dir().join(format!(
            "stella-interactive-avatar-{}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&root).unwrap();
        let store = Arc::new(RegistryStore::open(root.join("fusion.registry")).unwrap());
        let (lua, runtime) = fixture_with_store(store.clone());
        let (config, listener) = bind();
        let asset_listener = TcpListener::bind("127.0.0.1:0").unwrap();
        asset_listener.set_nonblocking(true).unwrap();
        let url = format!(
            "http://{}/avatars/personal.bin",
            asset_listener.local_addr().unwrap()
        );
        let mut profile = raw_profile("account-b");
        profile["personal"]["imageAssets"] = serde_json::json!([
            {"url":url,"hash":"opaque-version","dimension":64,"size":6}
        ]);
        runtime
            .dispatch_interactive(
                &lua,
                runtime.session.request_owner(ProviderLevel::Level2),
                InteractiveCompletion::Tokens {
                    login_job: runtime.begin_login_job().unwrap(),
                    config,
                    access: access("account-b"),
                },
            )
            .unwrap();
        let mut stream = accept_profile(&listener);
        let body = profile.to_string();
        write!(
            stream,
            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        )
        .unwrap();
        drop(stream);
        let deadline = Instant::now() + Duration::from_secs(5);
        let mut stream = loop {
            match asset_listener.accept() {
                Ok((stream, _)) => break stream,
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                    assert!(
                        Instant::now() < deadline,
                        "asset GET missing after own profile"
                    );
                    thread::sleep(Duration::from_millis(1));
                }
                Err(error) => panic!("{error}"),
            }
        };
        stream.set_nonblocking(false).unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(5)))
            .unwrap();
        let mut request = Vec::new();
        while !request.windows(4).any(|v| v == b"\r\n\r\n") {
            let mut buffer = [0; 1024];
            let count = stream.read(&mut buffer).unwrap();
            assert_ne!(count, 0);
            request.extend_from_slice(&buffer[..count]);
        }
        let request = String::from_utf8(request).unwrap().to_ascii_lowercase();
        assert!(request.starts_with("get /avatars/personal.bin "));
        assert!(!request.contains("x-access-token") && !request.contains("rovio-sgs"));
        assert_eq!(store.load_profile().unwrap(), Some(profile.clone()));
        assert_eq!(runtime.session.profile().unwrap().raw, profile);
        assert_eq!(
            runtime.session.level2_tokens().access_token,
            access("account-a").access_token
        );
        assert_eq!(store.load().unwrap(), access("account-a").refresh_token);
        assert!(runtime.pop_session_success().is_none());
        assert!(runtime.online_completions.lock().unwrap().is_empty());
        assert_eq!(lua.globals().get::<u32>("login_successes").unwrap(), 0);
        if outcome == "logout" {
            runtime.session.logout().unwrap();
        }
        let status = if outcome == "status" { 503 } else { 200 };
        let bytes = if outcome == "size" { "short" } else { "abcdef" };
        write!(
            stream,
            "HTTP/1.1 {status} Fixture\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{bytes}",
            bytes.len()
        )
        .unwrap();
        drop(stream);
        await_profile_result(&runtime);
        let success = outcome == "success";
        assert_eq!(runtime.pop_session_success().is_some(), success);
        assert!(runtime.pop_session_success().is_none());
        assert_eq!(
            store.load_avatar_version("personal.bin").unwrap(),
            if success { "opaque-version" } else { "" }
        );
        if outcome != "logout" {
            assert_eq!(
                fs::read(root.join("avatarAssets/personal.bin")).unwrap(),
                bytes.as_bytes()
            );
            assert_eq!(store.load_profile().unwrap(), Some(profile));
            assert_eq!(
                runtime.session.level2_tokens().access_token,
                access(if success { "account-b" } else { "account-a" }).access_token
            );
            assert_eq!(
                store.load().unwrap(),
                access(if success { "account-b" } else { "account-a" }).refresh_token
            );
            assert_eq!(
                runtime
                    .session
                    .profile()
                    .unwrap()
                    .avatar_paths
                    .get(&64)
                    .map(String::as_str),
                if success {
                    Some("avatarAssets/personal.bin")
                } else {
                    None
                }
            );
        } else {
            assert!(runtime.session.profile().is_none());
            assert!(
                fs::read(root.join("avatarAssets/personal.bin"))
                    .unwrap()
                    .is_empty()
            );
        }
        super::super::super::dispatch_online_completion(&lua, &runtime).unwrap();
        assert_eq!(lua.globals().get::<u32>("login_successes").unwrap(), 0);
        assert_eq!(lua.globals().get::<u32>("login_failures").unwrap(), 0);
        super::super::super::dispatch_local_completion(&lua, &runtime).unwrap();
        assert_eq!(
            lua.globals().get::<u32>("login_successes").unwrap(),
            u32::from(success)
        );
        assert_eq!(
            lua.globals().get::<u32>("login_failures").unwrap(),
            u32::from(!success && outcome != "logout")
        );
        assert!(!runtime.state.lock().unwrap().login_in_progress);
        super::super::super::dispatch_local_completion(&lua, &runtime).unwrap();
        assert_eq!(
            lua.globals().get::<u32>("login_successes").unwrap(),
            u32::from(success)
        );
        assert_eq!(
            lua.globals().get::<u32>("login_failures").unwrap(),
            u32::from(!success && outcome != "logout")
        );
        fs::remove_dir_all(root).unwrap();
    }
}
