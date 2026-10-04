use super::*;

macro_rules! native_png_fixture {
    ($name:literal) => {
        include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../stella-assets/src/native_image/png/fixtures/",
            $name
        ))
        .to_vec()
    };
}

// Deterministic 17x13 RGBA PNG. Full pixel decoding is tested, with no bundled
// avatar or filename extension available to stand in for the downloaded file.
fn avatar_png() -> Vec<u8> {
    use base64::Engine;
    base64::engine::general_purpose::STANDARD.decode("iVBORw0KGgoAAAANSUhEUgAAABEAAAANCAYAAABPeYUaAAAAGElEQVR4nGMQtj2/llLMMGrIqCGjhpCFAd1JjSwPI2qaAAAAAElFTkSuQmCC").unwrap()
}

fn avatar_bmp() -> Vec<u8> {
    let mut bytes = vec![0u8; 58];
    bytes[..2].copy_from_slice(b"BM");
    bytes[2..6].copy_from_slice(&58u32.to_le_bytes());
    bytes[10..14].copy_from_slice(&54u32.to_le_bytes());
    bytes[14..18].copy_from_slice(&40u32.to_le_bytes());
    bytes[18..22].copy_from_slice(&1i32.to_le_bytes());
    bytes[22..26].copy_from_slice(&1i32.to_le_bytes());
    bytes[26..28].copy_from_slice(&1u16.to_le_bytes());
    bytes[28..30].copy_from_slice(&24u16.to_le_bytes());
    bytes[54..58].copy_from_slice(&[7, 31, 203, 0]);
    bytes
}

fn avatar_tga() -> Vec<u8> {
    let mut bytes = vec![0u8; 18];
    bytes[2] = 2;
    bytes[12..14].copy_from_slice(&1u16.to_le_bytes());
    bytes[14..16].copy_from_slice(&1u16.to_le_bytes());
    bytes[16] = 32;
    bytes[17] = 0x28;
    bytes.extend_from_slice(&[7, 31, 203, 117]);
    bytes
}

fn avatar_indexed_tga(rle: bool) -> Vec<u8> {
    let mut bytes = vec![0u8; 18];
    bytes[1] = 1;
    bytes[2] = if rle { 9 } else { 1 };
    bytes[5..7].copy_from_slice(&1u16.to_le_bytes());
    bytes[7] = 32;
    bytes[12..14].copy_from_slice(&1u16.to_le_bytes());
    bytes[14..16].copy_from_slice(&1u16.to_le_bytes());
    bytes[16] = 8;
    bytes[17] = 0x28;
    bytes.extend_from_slice(&[7, 31, 203, 117]);
    if rle {
        bytes.push(0x80);
    }
    bytes.push(0);
    bytes
}

fn avatar_scanline_tga(depth: u8) -> Vec<u8> {
    let mut bytes = vec![0u8; 18];
    bytes[2] = 10;
    bytes[12..14].copy_from_slice(&2u16.to_le_bytes());
    bytes[14..16].copy_from_slice(&2u16.to_le_bytes());
    bytes[16] = depth;
    bytes[17] = 0x10; // Bottom origin, horizontal bit ignored, no attribute bits.
    bytes.push(2);
    for pixel in [[7, 31, 203, 41], [83, 17, 29, 173], [255; 4]] {
        bytes.extend_from_slice(&pixel[..usize::from(depth / 8)]);
    }
    bytes.push(1);
    for pixel in [[19, 211, 61, 0], [151, 43, 97, 255]] {
        bytes.extend_from_slice(&pixel[..usize::from(depth / 8)]);
    }
    bytes
}

fn avatar_native_bmp(dib: u32, indexed: bool) -> Vec<u8> {
    let offset = 14 + dib as usize + if indexed { 12 } else { 0 };
    let mut bytes = vec![0; offset];
    bytes[..2].copy_from_slice(b"BM");
    bytes[10..14].copy_from_slice(&(offset as u32).to_le_bytes());
    bytes[14..18].copy_from_slice(&dib.to_le_bytes());
    bytes[18..22].copy_from_slice(&0x10002u32.to_le_bytes());
    bytes[22..26].copy_from_slice(&0x10002u32.to_le_bytes());
    bytes[26..28].copy_from_slice(&1u16.to_le_bytes());
    bytes[28..30].copy_from_slice(&(if indexed { 8u16 } else { 24 }).to_le_bytes());
    if indexed {
        bytes[46..50].copy_from_slice(&3u32.to_le_bytes());
        bytes[14 + dib as usize..]
            .copy_from_slice(&[7, 31, 203, 0, 83, 17, 29, 173, 19, 211, 61, 0]);
        bytes.extend_from_slice(&[200, 1, 91, 92, 2, 0, 93, 94]);
    } else {
        bytes.extend_from_slice(&[
            7, 31, 203, 83, 17, 29, 91, 92, 19, 211, 61, 151, 43, 97, 93, 94,
        ]);
    }
    let length = bytes.len() as u32;
    bytes[2..6].copy_from_slice(&length.to_le_bytes());
    bytes
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
        .unwrap()
        .snapshot_image()
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
fn social_avatar_downloaded_bmp_and_tga_publish_real_pixels_and_native_layouts() {
    use stella_assets::{native_image::ImageSurfaceLayout, surface_format::SurfaceFormat};
    let clear = [0; 4];
    let white = [255; 4];
    let gray_pixels = [
        clear, white, clear, white, white, white, clear, white, clear, clear, clear, clear, white,
        white, clear,
    ]
    .concat();
    for (name, bytes, suffix, expected_size, expected_pixels, expected_layout) in [
        (
            "bmp-misnamed-png",
            avatar_bmp(),
            "png",
            (1, 1),
            vec![203, 31, 7, 255],
            ImageSurfaceLayout::direct(SurfaceFormat::R8G8B8),
        ),
        (
            "tga",
            avatar_tga(),
            "tga",
            (1, 1),
            vec![203, 31, 7, 117],
            ImageSurfaceLayout::direct(SurfaceFormat::A8R8G8B8),
        ),
        (
            "tga-indexed-raw",
            avatar_indexed_tga(false),
            "tga",
            (1, 1),
            vec![203, 31, 7, 255],
            ImageSurfaceLayout {
                pixels: SurfaceFormat::P8,
                palette: Some(SurfaceFormat::X8B8G8R8),
            },
        ),
        (
            "tga-indexed-rle",
            avatar_indexed_tga(true),
            "tga",
            (1, 1),
            vec![203, 31, 7, 255],
            ImageSurfaceLayout {
                pixels: SurfaceFormat::P8,
                palette: Some(SurfaceFormat::X8B8G8R8),
            },
        ),
        (
            "tga-24-native-scanlines",
            avatar_scanline_tga(24),
            "tga",
            (2, 2),
            vec![
                61, 211, 19, 255, 97, 43, 151, 255, 203, 31, 7, 255, 29, 17, 83, 255,
            ],
            ImageSurfaceLayout::direct(SurfaceFormat::R8G8B8),
        ),
        (
            "tga-32-native-scanlines",
            avatar_scanline_tga(32),
            "tga",
            (2, 2),
            vec![
                61, 211, 19, 0, 97, 43, 151, 255, 203, 31, 7, 41, 29, 17, 83, 173,
            ],
            ImageSurfaceLayout::direct(SurfaceFormat::A8R8G8B8),
        ),
        (
            "bmp-native-info-low-words",
            avatar_native_bmp(40, false),
            "png",
            (2, 2),
            vec![
                61, 211, 19, 255, 97, 43, 151, 255, 203, 31, 7, 255, 29, 17, 83, 255,
            ],
            ImageSurfaceLayout::direct(SurfaceFormat::R8G8B8),
        ),
        (
            "bmp-native-os2-palette",
            avatar_native_bmp(64, true),
            "png",
            (2, 2),
            vec![
                61, 211, 19, 255, 203, 31, 7, 255, 0, 0, 0, 255, 29, 17, 83, 255,
            ],
            ImageSurfaceLayout {
                pixels: SurfaceFormat::P8,
                palette: Some(SurfaceFormat::X8B8G8R8),
            },
        ),
        (
            "png-native-gray-expanded-alpha",
            native_png_fixture!("gray1-trns.png"),
            "png",
            (5, 3),
            gray_pixels.clone(),
            ImageSurfaceLayout::direct(SurfaceFormat::A8L8),
        ),
        (
            "png-native-gray-adam7-expanded-alpha",
            native_png_fixture!("gray1-trns-adam7.png"),
            "png",
            (5, 3),
            gray_pixels,
            ImageSurfaceLayout::direct(SurfaceFormat::A8L8),
        ),
        (
            "png-native-low-bit-palette-direct",
            native_png_fixture!("palette4-opaque.png"),
            "png",
            (3, 2),
            vec![
                61, 211, 19, 255, 29, 17, 83, 255, 203, 31, 7, 255, 203, 31, 7, 255, 61, 211, 19,
                255, 29, 17, 83, 255,
            ],
            ImageSurfaceLayout::direct(SurfaceFormat::B8G8R8),
        ),
        (
            "png-native-eight-bit-palette-white",
            native_png_fixture!("palette8-alpha.png"),
            "png",
            (3, 2),
            vec![
                255, 255, 255, 255, 29, 17, 83, 173, 203, 31, 7, 0, 203, 31, 7, 0, 61, 211, 19,
                255, 29, 17, 83, 173,
            ],
            ImageSurfaceLayout {
                pixels: SurfaceFormat::P8,
                palette: Some(SurfaceFormat::A8R8G8B8),
            },
        ),
    ] {
        let sandbox = ShippedDataSandbox::new(name);
        let (asset_root, asset_rx, asset_worker) = spawn_sequence_responses(vec![(200, bytes)]);
        let asset_url = format!("{asset_root}.{suffix}");
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
        runtime
            .execute_source("_G.SocialManager.native_loadAvatar('own')")
            .unwrap();
        assert_eq!(
            env.get::<mlua::Table>("avatar_loaded").unwrap().raw_len(),
            1
        );
        let retained = runtime
            .resource_runtime
            .lock()
            .unwrap()
            .active_atlas_catalog_region("AVATAR_own", runtime.data_root())
            .unwrap()
            .snapshot_image()
            .unwrap();
        let image = retained.decoded_image.as_ref().unwrap();
        assert_eq!((image.width, image.height), expected_size);
        assert_eq!(image.rgba, expected_pixels);
        assert_eq!(image.layout, expected_layout);
    }
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
fn social_avatar_readable_unsupported_textures_stay_cached_without_success() {
    let mut tga16 = avatar_tga();
    tga16[16] = 16;
    tga16[17] = 0x20;
    tga16.truncate(18);
    tga16.extend_from_slice(&[0, 0x7c]);
    let mut bmp16 = avatar_bmp();
    bmp16[28..30].copy_from_slice(&16u16.to_le_bytes());
    bmp16[54..58].copy_from_slice(&[0, 0x7c, 0, 0]);
    let mut bmp32 = avatar_bmp();
    bmp32[28..30].copy_from_slice(&32u16.to_le_bytes());
    for (name, suffix, bytes, readable, reason) in [
        (
            "social-tga16-rejected",
            "tga",
            tga16,
            true,
            "Unsupported texture format: R5G5B5",
        ),
        (
            "social-bmp16-rejected",
            "bmp",
            bmp16,
            true,
            "Unsupported texture format: R5G5B5",
        ),
        (
            "social-bmp32-rejected",
            "bmp",
            bmp32,
            true,
            "Unsupported texture format: X8R8G8B8",
        ),
        (
            "social-png-gray-key-rejected",
            "png",
            native_png_fixture!("gray8-key.png"),
            false,
            "native color-key transparency has no alpha table",
        ),
        (
            "social-png-rgb-key-rejected",
            "png",
            native_png_fixture!("rgb8-key.png"),
            false,
            "native color-key transparency has no alpha table",
        ),
    ] {
        assert_eq!(
            stella_assets::native_image::decode_native_image(&bytes, Some(suffix)).is_ok(),
            readable
        );
        let sandbox = ShippedDataSandbox::new(name);
        let (asset_url, asset_rx, asset_worker) = spawn_sequence_responses(vec![(200, bytes)]);
        let (url, connect_rx, connect_worker) = spawn_sequence_responses(vec![(
            200,
            connect_response(&format!("{asset_url}.{suffix}")),
        )]);
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
            let error = runtime
                .execute_source("_G.SocialManager.native_loadAvatar('own')")
                .unwrap_err();
            assert!(error.to_string().contains(reason), "{error}");
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
