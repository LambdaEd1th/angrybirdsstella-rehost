use super::*;

// Deterministic 17x13 RGBA PNG. Full pixel decoding is tested, with no bundled
// avatar or filename extension available to stand in for the downloaded file.
fn avatar_png() -> Vec<u8> {
    use base64::Engine;
    base64::engine::general_purpose::STANDARD.decode("iVBORw0KGgoAAAANSUhEUgAAABEAAAANCAYAAABPeYUaAAAAGElEQVR4nGMQtj2/llLMMGrIqCGjhpCFAd1JjSwPI2qaAAAAAElFTkSuQmCC").unwrap()
}

fn setup_avatar_runtime(runtime: &StellaLua, url: &str) {
    runtime.set_social_url(url).unwrap();
    runtime
        .execute_source(
            r#"
        update = function() end
        avatar_connected = false; avatar_cached = {}; avatar_loaded = {}
        local s = _G.SocialManager
        s.onSocialNetworkConnected = function() avatar_connected = true end
        s.onAvatarDownloadedToCache = function(id) avatar_cached[#avatar_cached+1] = id end
        s.onAvatarImageLoaded = function(id) avatar_loaded[#avatar_loaded+1] = id end
        s.native_connectToSocialNetwork()
    "#,
        )
        .unwrap();
    let env = game_environment(runtime.lua()).unwrap();
    for _ in 0..500 {
        runtime.update(1.0 / 60.0).unwrap();
        if env.get::<bool>("avatar_connected").unwrap() {
            return;
        }
        thread::sleep(Duration::from_millis(2));
    }
    panic!("social connect callback timed out");
}

fn connect_response(asset_url: &str) -> Vec<u8> {
    let profile = serde_json::json!({"personal":{"imageAssets":[{"url":asset_url,"hash":"opaque","dimension":64,"size":1}]}});
    serde_json::to_vec(&serde_json::json!({"localPlayer":{"accountId":"own","profile":profile},"friends":[{"accountId":"friend","name":"Friend","profile":profile}]})).unwrap()
}

#[test]
fn social_avatar_online_coalesces_accounts_decodes_second_call_and_retains_pixels_after_unload() {
    let sandbox = ShippedDataSandbox::new("social-avatar-coalesced");
    let (asset_url, asset_rx, asset_worker) = spawn_sequence_responses(vec![(200, avatar_png())]);
    let (url, connect_rx, connect_worker) =
        spawn_sequence_responses(vec![(200, connect_response(&asset_url))]);
    let runtime = StellaLua::new(&sandbox.data_root).unwrap();
    setup_avatar_runtime(&runtime, &url);
    connect_rx.recv_timeout(Duration::from_secs(5)).unwrap();
    connect_worker.join().unwrap();
    runtime.execute_source("_G.SocialManager.native_loadAvatar('own'); _G.SocialManager.native_loadAvatar('friend'); _G.SocialManager.native_loadAvatar('own')").unwrap();
    let env = game_environment(runtime.lua()).unwrap();
    let cached = env.get::<mlua::Table>("avatar_cached").unwrap();
    let loaded = env.get::<mlua::Table>("avatar_loaded").unwrap();
    assert_eq!(cached.raw_len(), 0);
    let request =
        String::from_utf8(asset_rx.recv_timeout(Duration::from_secs(5)).unwrap()).unwrap();
    assert!(request.starts_with("GET "));
    assert!(!request.to_lowercase().contains("authorization:"));
    asset_worker.join().unwrap();
    for _ in 0..500 {
        runtime.update(1.0 / 60.0).unwrap();
        if cached.raw_len() == 2 {
            break;
        }
        thread::sleep(Duration::from_millis(2));
    }
    assert_eq!(cached.raw_len(), 2);
    assert_eq!(cached.raw_get::<String>(1).unwrap(), "own");
    assert_eq!(cached.raw_get::<String>(2).unwrap(), "friend");
    assert_eq!(loaded.raw_len(), 0);
    assert!(
        runtime
            .resource_runtime
            .lock()
            .unwrap()
            .active_native_sprite_metrics("AVATAR_own")
            .is_none()
    );
    runtime.execute_source("_G.SocialManager.native_loadAvatar('own'); _G.SocialManager.native_loadAvatar('friend')").unwrap();
    assert_eq!(loaded.raw_len(), 2);
    let retained = runtime
        .resource_runtime
        .lock()
        .unwrap()
        .active_atlas_catalog_region("AVATAR_own", runtime.data_root())
        .unwrap();
    assert_eq!(
        (
            retained.sprite.width,
            retained.sprite.height,
            retained.sprite.pivot_x,
            retained.sprite.pivot_y
        ),
        (17, 13, 8, 6)
    );
    let decoded = retained.decoded_image.as_ref().unwrap();
    assert!(
        decoded
            .rgba
            .as_chunks::<4>()
            .0
            .iter()
            .all(|p| *p == [19, 61, 207, 173])
    );
    let path = std::path::PathBuf::from(stella_assets::image_source::image_source_path(
        &retained.texture_source,
    ));
    assert!(path.is_absolute());
    runtime
        .execute_source("_G.SocialManager.native_unloadAllAvatars()")
        .unwrap();
    fs::remove_file(&path).unwrap();
    assert_eq!(retained.decoded_image.as_ref().unwrap().rgba, decoded.rgba);
    assert!(
        runtime
            .execute_source("_G.SocialManager.native_loadAvatar('own')")
            .is_err()
    );
    assert_eq!(
        loaded.raw_len(),
        2,
        "missing pixels must not publish an image-loaded callback"
    );
}

#[test]
fn social_avatar_online_download_failure_is_silent_and_retry_uses_real_pixels() {
    let sandbox = ShippedDataSandbox::new("social-avatar-retry");
    let (asset_url, asset_rx, asset_worker) =
        spawn_sequence_responses(vec![(503, b"unavailable".to_vec()), (200, avatar_png())]);
    let (url, connect_rx, connect_worker) =
        spawn_sequence_responses(vec![(200, connect_response(&asset_url))]);
    let runtime = StellaLua::new(&sandbox.data_root).unwrap();
    setup_avatar_runtime(&runtime, &url);
    connect_rx.recv_timeout(Duration::from_secs(5)).unwrap();
    connect_worker.join().unwrap();
    let env = game_environment(runtime.lua()).unwrap();
    let cached = env.get::<mlua::Table>("avatar_cached").unwrap();
    runtime
        .execute_source("_G.SocialManager.native_loadAvatar('own')")
        .unwrap();
    asset_rx.recv_timeout(Duration::from_secs(5)).unwrap();
    for _ in 0..100 {
        runtime.update(1.0 / 60.0).unwrap();
        thread::sleep(Duration::from_millis(2));
    }
    assert_eq!(cached.raw_len(), 0);
    runtime
        .execute_source("_G.SocialManager.native_loadAvatar('own')")
        .unwrap();
    asset_rx.recv_timeout(Duration::from_secs(5)).unwrap();
    asset_worker.join().unwrap();
    for _ in 0..500 {
        runtime.update(1.0 / 60.0).unwrap();
        if cached.raw_len() == 1 {
            break;
        }
        thread::sleep(Duration::from_millis(2));
    }
    assert_eq!(cached.raw_len(), 1);
    runtime
        .execute_source("_G.SocialManager.native_loadAvatar('own')")
        .unwrap();
    assert_eq!(
        env.get::<mlua::Table>("avatar_loaded").unwrap().raw_len(),
        1
    );
}

#[test]
fn social_avatar_online_corrupt_pixels_never_publish_image_loaded() {
    let sandbox = ShippedDataSandbox::new("social-avatar-corrupt");
    let (asset_url, asset_rx, asset_worker) =
        spawn_sequence_responses(vec![(200, avatar_png()[..33].to_vec())]);
    let (url, connect_rx, connect_worker) =
        spawn_sequence_responses(vec![(200, connect_response(&asset_url))]);
    let runtime = StellaLua::new(&sandbox.data_root).unwrap();
    setup_avatar_runtime(&runtime, &url);
    connect_rx.recv_timeout(Duration::from_secs(5)).unwrap();
    connect_worker.join().unwrap();
    runtime
        .execute_source("_G.SocialManager.native_loadAvatar('own')")
        .unwrap();
    asset_rx.recv_timeout(Duration::from_secs(5)).unwrap();
    asset_worker.join().unwrap();
    let env = game_environment(runtime.lua()).unwrap();
    let cached = env.get::<mlua::Table>("avatar_cached").unwrap();
    for _ in 0..500 {
        runtime.update(1.0 / 60.0).unwrap();
        if cached.raw_len() == 1 {
            break;
        }
        thread::sleep(Duration::from_millis(2));
    }
    assert_eq!(cached.raw_len(), 1);
    for _ in 0..2 {
        assert!(
            runtime
                .execute_source("_G.SocialManager.native_loadAvatar('own')")
                .is_err()
        );
    }
    assert_eq!(
        env.get::<mlua::Table>("avatar_loaded").unwrap().raw_len(),
        0
    );
    assert!(
        runtime
            .resource_runtime
            .lock()
            .unwrap()
            .active_native_sprite_metrics("AVATAR_own")
            .is_none()
    );
}

#[test]
fn social_avatar_provider_replacement_discards_old_connect_and_download_callbacks() {
    for local in [false, true] {
        let sandbox = ShippedDataSandbox::new("social-avatar-owner");
        let (asset_url, asset_rx, asset_worker) =
            spawn_sequence_responses(vec![(200, avatar_png())]);
        let (url, connect_rx, connect_worker) =
            spawn_sequence_responses(vec![(200, connect_response(&asset_url))]);
        let runtime = StellaLua::new(&sandbox.data_root).unwrap();
        setup_avatar_runtime(&runtime, &url);
        connect_rx.recv_timeout(Duration::from_secs(5)).unwrap();
        connect_worker.join().unwrap();
        runtime
            .execute_source("_G.SocialManager.native_loadAvatar('own')")
            .unwrap();
        asset_rx.recv_timeout(Duration::from_secs(5)).unwrap();
        asset_worker.join().unwrap();
        if local {
            runtime.enable_local_services().unwrap();
        } else {
            runtime
                .set_social_url("http://127.0.0.1:1/replaced")
                .unwrap();
        }
        for _ in 0..50 {
            runtime.update(1.0 / 60.0).unwrap();
            thread::sleep(Duration::from_millis(2));
        }
        let env = game_environment(runtime.lua()).unwrap();
        assert_eq!(
            env.get::<mlua::Table>("avatar_cached").unwrap().raw_len(),
            0
        );
        assert_eq!(
            env.get::<mlua::Table>("avatar_loaded").unwrap().raw_len(),
            0
        );
        assert!(
            runtime
                .resource_runtime
                .lock()
                .unwrap()
                .active_native_sprite_metrics("AVATAR_own")
                .is_none()
        );
        if local {
            runtime.execute_source("_G.SocialManager.native_connectToSocialNetwork(); _G.SocialManager.native_loadAvatar('local-player')").unwrap();
            for _ in 0..3 {
                runtime.update(1.0 / 60.0).unwrap();
            }
            assert_eq!(
                env.get::<mlua::Table>("avatar_cached")
                    .unwrap()
                    .raw_get::<String>(1)
                    .unwrap(),
                "local-player"
            );
        }
    }
    let sandbox = ShippedDataSandbox::new("social-connect-owner");
    let (url, rx, worker) =
        spawn_sequence_responses(vec![(200, connect_response("http://127.0.0.1:1/unused"))]);
    let runtime = StellaLua::new(&sandbox.data_root).unwrap();
    runtime.set_social_url(&url).unwrap();
    runtime.execute_source("update=function() end; stale_connects=0; _G.SocialManager.onSocialNetworkConnected=function() stale_connects=stale_connects+1 end; _G.SocialManager.native_connectToSocialNetwork()").unwrap();
    rx.recv_timeout(Duration::from_secs(5)).unwrap();
    worker.join().unwrap();
    runtime.set_social_url("http://127.0.0.1:1/new").unwrap();
    for _ in 0..50 {
        runtime.update(1.0 / 60.0).unwrap();
        thread::sleep(Duration::from_millis(2));
    }
    assert_eq!(
        game_environment(runtime.lua())
            .unwrap()
            .get::<i64>("stale_connects")
            .unwrap(),
        0
    );
}

#[test]
fn social_avatar_runtime_drop_retires_inflight_cache_writes() {
    let sandbox = ShippedDataSandbox::new("social-avatar-dropped");
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let asset_url = format!("http://{}/opaque", listener.local_addr().unwrap());
    let (started_tx, started_rx) = mpsc::channel();
    let (finish_tx, finish_rx) = mpsc::channel();
    let worker = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(5)))
            .unwrap();
        let mut bytes = Vec::new();
        while !bytes.ends_with(b"\r\n\r\n") {
            let mut byte = [0];
            stream.read_exact(&mut byte).unwrap();
            bytes.push(byte[0]);
        }
        stream
            .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 6\r\nConnection: close\r\n\r\nabc")
            .unwrap();
        stream.flush().unwrap();
        started_tx.send(()).unwrap();
        finish_rx.recv_timeout(Duration::from_secs(5)).unwrap();
        stream.write_all(b"def").unwrap();
    });
    let (url, connect_rx, connect_worker) =
        spawn_sequence_responses(vec![(200, connect_response(&asset_url))]);
    let runtime = StellaLua::new(&sandbox.data_root).unwrap();
    setup_avatar_runtime(&runtime, &url);
    connect_rx.recv_timeout(Duration::from_secs(5)).unwrap();
    connect_worker.join().unwrap();
    runtime
        .execute_source("_G.SocialManager.native_loadAvatar('own')")
        .unwrap();
    started_rx.recv_timeout(Duration::from_secs(5)).unwrap();
    let scope = crate::game_lua::sha1_upper_hex(url.as_bytes());
    let name = crate::game_lua::sha1_upper_hex(asset_url.as_bytes());
    let directory = sandbox
        .root
        .join("appdata/social-providers")
        .join(scope)
        .join("SkynestUserAvatars");
    let tmp = directory.join(format!("{name}.tmp"));
    let path = directory.join(name);
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    while fs::metadata(&tmp).map(|m| m.len()).unwrap_or(0) != 3
        && std::time::Instant::now() < deadline
    {
        thread::sleep(Duration::from_millis(2));
    }
    assert_eq!(fs::read(&tmp).unwrap(), b"abc");
    // Keep only the worker's completion queue alive, exactly as an outstanding
    // HTTP operation does. Destroy the actual Lua/service owner before EOF.
    let completed = runtime.social.online_completion_count_probe();
    assert_eq!(completed(), 0);
    drop(runtime);
    finish_tx.send(()).unwrap();
    worker.join().unwrap();
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    while completed() == 0 && std::time::Instant::now() < deadline {
        thread::sleep(Duration::from_millis(2));
    }
    assert_eq!(
        completed(),
        1,
        "the real retired worker must finish before checking disk"
    );
    assert!(!path.exists());
    assert_eq!(fs::read(&tmp).unwrap(), b"abc");
}
