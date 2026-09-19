//! Actual identity login -> synchronous native store -> Lua/avatar callbacks.
use super::*;
use serde_json::json;
mod errors;
mod platform;

fn identity_with_failed_friends(
    accounts: Vec<&'static str>,
) -> (
    String,
    std::sync::mpsc::Receiver<Vec<u8>>,
    thread::JoinHandle<()>,
) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}/identity/2.0", listener.local_addr().unwrap());
    let (tx, rx) = std::sync::mpsc::channel();
    let server = thread::spawn(move || {
        for account in accounts {
            let (mut stream, request) =
                identity_routes::accept_request_including_friends(&listener);
            assert!(request.starts_with("POST /session/1/apps/friends-fixture/sessions HTTP/1.1"));
            tx.send(request.into_bytes()).unwrap();
            let body = session_response(account);
            write!(
                stream,
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                body.len()
            )
            .unwrap();
            stream.write_all(&body).unwrap();
            drop(stream);
            let (mut stream, request) =
                identity_routes::accept_request_including_friends(&listener);
            assert!(
                request.starts_with("GET /identity/2.0/friends HTTP/1.1"),
                "{request}"
            );
            storage_session::assert_auth(&request, "synthetic-friend-access", "8, 2");
            write!(
                stream,
                "HTTP/1.1 503 Unavailable\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
            )
            .unwrap();
        }
    });
    (url, rx, server)
}

fn session_response(account: &str) -> Vec<u8> {
    serde_json::to_vec(&json!({
        "userAuth":{"accessToken":"synthetic-friend-access","refreshToken":"synthetic-friend-refresh","expiresIn":3600},
        "segments":[8,2],"config":{},"profile":{"publicAccountId":account,"personal":{"nickName":"Own"}}
    })).unwrap()
}

fn configure(runtime: &StellaLua, url: &str) {
    runtime.set_identity_url(url).unwrap();
    runtime
        .set_identity_client(Some("friends-fixture"), Some("synthetic-signature"), None)
        .unwrap();
    runtime.execute_source(r#"
        update = function() end
        friend_login_count = 0; avatar_cached = {}; avatar_loaded = {}
        _G.SkynestAccount.onLoginSuccess = function()
            friend_login_count = friend_login_count + 1
            friends_at_login = _G.SocialManager.native_getFriends()
            connected_at_login = _G.SocialManager.native_isConnectedToSocialNetwork()
        end
        _G.SocialManager.onAvatarDownloadedToCache = function(id) avatar_cached[#avatar_cached+1] = id end
        _G.SocialManager.onAvatarImageLoaded = function(id) avatar_loaded[#avatar_loaded+1] = id end
    "#).unwrap();
}

fn login(runtime: &StellaLua, count: i32) {
    runtime
        .execute_source("_G.SkynestAccount.native_login(false,false,false)")
        .unwrap();
    for _ in 0..500 {
        dispatch_registered_application_events(runtime.lua()).unwrap();
        if game_environment(runtime.lua())
            .unwrap()
            .get::<i32>("friend_login_count")
            .unwrap()
            == count
        {
            return;
        }
        thread::sleep(Duration::from_millis(2));
    }
    panic!("native friend-store account login timed out");
}

fn raw_reply(stream: &mut std::net::TcpStream, status: u16, body: &str) {
    write!(
        stream,
        "HTTP/1.1 {status} Fixture\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    )
    .unwrap();
}

fn wait_progress(runtime: &StellaLua, count: i32) {
    for _ in 0..1000 {
        dispatch_registered_application_events(runtime.lua()).unwrap();
        if game_environment(runtime.lua())
            .unwrap()
            .get::<i32>("native_progress_count")
            .unwrap()
            == count
        {
            return;
        }
        thread::sleep(Duration::from_millis(2));
    }
    panic!("native friends progress timed out");
}

#[test]
fn native_friends_pipeline_401_search_persistence_and_storage_progress_are_real_requests() {
    let sandbox = ShippedDataSandbox::new("native-friends-pipeline");
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let origin = format!("http://{}", listener.local_addr().unwrap());
    let server = thread::spawn(move || {
        let (mut stream, request) = identity_routes::accept_request_including_friends(&listener);
        assert!(
            request.starts_with("POST /proxy/session/1/apps/friends-fixture/sessions HTTP/1.1")
        );
        raw_reply(
            &mut stream,
            200,
            &String::from_utf8(session_response("own")).unwrap(),
        );
        drop(stream);
        let (mut stream, first) = identity_routes::accept_request_including_friends(&listener);
        assert!(first.starts_with("GET /proxy/identity/2.0/friends HTTP/1.1"));
        storage_session::assert_auth(&first, "synthetic-friend-access", "8, 2");
        raw_reply(&mut stream, 401, "");
        drop(stream);
        let (mut stream, request) = identity_routes::accept_request_including_friends(&listener);
        assert!(
            request.starts_with("POST /proxy/session/1/apps/friends-fixture/sessions HTTP/1.1")
        );
        let body: serde_json::Value =
            serde_json::from_str(storage_session::body(&request)).unwrap();
        assert_eq!(body["refresh"]["token"], "synthetic-friend-refresh");
        let mut renewed: serde_json::Value =
            serde_json::from_slice(&session_response("own")).unwrap();
        renewed["userAuth"]["accessToken"] = json!("renewed-friend-access");
        raw_reply(&mut stream, 200, &renewed.to_string());
        drop(stream);
        let (mut stream, replay) = identity_routes::accept_request_including_friends(&listener);
        assert_eq!(first.lines().next(), replay.lines().next());
        assert_eq!(
            storage_session::body(&first),
            storage_session::body(&replay)
        );
        storage_session::assert_auth(&replay, "renewed-friend-access", "8, 2");
        let relation = |id: &str, name: &str| json!({"id":id,"socialNetworks":[{"networkId":format!("rel-{id}"),"provider":"kakaotalk","socialAttributes":{"name":name}}]});
        raw_reply(&mut stream,200,&json!({"socialFriends":[relation("z","First Z"),relation("a +/","Alpha"),relation("z","Last Z"),relation("","Empty ID"),{"id":"hidden"}]}).to_string());
        drop(stream);
        let (mut stream, request) = identity_routes::accept_request_including_friends(&listener);
        assert!(request.starts_with("POST /proxy/identity/2.0/profile/search HTTP/1.1"));
        storage_session::assert_auth(&request, "renewed-friend-access", "8, 2");
        assert_eq!(
            storage_session::body(&request),
            "publicAccountId=z&publicAccountId=a+%2B%2F&publicAccountId=z&publicAccountId=&publicAccountId=hidden"
        );
        let profile = |url: &str| json!({"publicAccountId":"z","personal":{"nickName":"Must not replace relation name","imageAssets":[{"url":url,"hash":"version","dimension":64,"size":1}]},"socialNetworks":[]});
        raw_reply(
            &mut stream,
            200,
            &format!(
                "\n{}\n{}",
                profile("first-personal-asset"),
                profile("wrong-second-asset")
            ),
        );
        drop(stream);
        for status in [204, 200] {
            let (mut stream, request) =
                identity_routes::accept_request_including_friends(&listener);
            assert!(request.starts_with("POST /proxy/storage/2.0/states/query HTTP/1.1"));
            storage_session::assert_auth(&request, "renewed-friend-access", "8, 2");
            let body: serde_json::Value =
                serde_json::from_str(storage_session::body(&request)).unwrap();
            assert_eq!(
                body,
                json!({"keys":["[my]/[client]/progress"],"accountIds":["","a +/","hidden","z"]})
            );
            let item = |id: &str, value: &str| json!({"accountId":id,"states":[{"value":value,"encoding":"SDKv1"}]});
            let response = json!({"result":[item("z",""),item("unknown","ignored"),item("hidden","unnamed"),item("","empty-id-progress"),item("a +/","alpha-progress")]});
            let response = response.to_string();
            raw_reply(
                &mut stream,
                status,
                if status == 200 { &response } else { "" },
            );
        }
    });
    let runtime = StellaLua::new(&sandbox.data_root).unwrap();
    configure(&runtime, &format!("{origin}/proxy/identity/3.0"));
    runtime
        .set_storage_url(&format!("{origin}/proxy/storage/2.0"))
        .unwrap();
    runtime.skynest_account.seed_friends_cache_for_test(
        "own",
        &json!({"friends":[{"accountId":"old","nickName":"Old"}],
        "socialNetworkFriends":[{"socialNetwork":3,"uid":"retained-disk-platform","name":"Prior"}]})
        .to_string(),
    );
    runtime
        .skynest_account
        .seed_friends_cache_for_test("another", r#"{"friends":[{"accountId":"untouched"}]}"#);
    runtime
        .execute_source(
            r#"
        native_progress_count=0
        _G.SocialManager.onFriendsProgressUpdated=function(success,friends)
            native_progress_count=native_progress_count+1
            native_progress_success=success; native_progress_friends=friends
        end
    "#,
        )
        .unwrap();
    login(&runtime, 1);
    let env = game_environment(runtime.lua()).unwrap();
    assert_eq!(
        env.get::<mlua::Table>("friends_at_login")
            .unwrap()
            .raw_get::<mlua::Table>(1)
            .unwrap()
            .get::<String>("accountId")
            .unwrap(),
        "old"
    );
    wait_progress(&runtime, 1);
    assert!(!env.get::<bool>("native_progress_success").unwrap());
    assert_eq!(
        env.get::<mlua::Table>("native_progress_friends")
            .unwrap()
            .raw_len(),
        0
    );
    runtime
        .execute_source("_G.SocialManager.native_getFriendsProgress()")
        .unwrap();
    wait_progress(&runtime, 2);
    server.join().unwrap();
    assert!(env.get::<bool>("native_progress_success").unwrap());
    let friends = env.get::<mlua::Table>("native_progress_friends").unwrap();
    assert_eq!(friends.raw_len(), 3);
    for (index, id, name, progress) in [
        (1, "", "Empty ID", "empty-id-progress"),
        (2, "a +/", "Alpha", "alpha-progress"),
        (3, "z", "Last Z", ""),
    ] {
        let friend = friends.raw_get::<mlua::Table>(index).unwrap();
        assert_eq!(friend.get::<String>("accountId").unwrap(), id);
        assert_eq!(friend.get::<String>("nickname").unwrap(), name);
        assert_eq!(friend.get::<String>("progress").unwrap(), progress);
    }
    let saved: serde_json::Value =
        serde_json::from_str(&runtime.skynest_account.read_friends_cache_for_test("own")).unwrap();
    assert_eq!(saved["friends"].as_array().unwrap().len(), 4);
    assert_eq!(
        saved["friends"][3]["socialNetworkProfiles"][0]["name"],
        "Last Z"
    );
    assert!(!saved.to_string().contains("first-personal-asset"));
    assert_eq!(
        saved["socialNetworkFriends"][0]["uid"],
        "retained-disk-platform"
    );
    assert_eq!(
        runtime
            .skynest_account
            .read_friends_cache_for_test("another"),
        r#"{"friends":[{"accountId":"untouched"}]}"#
    );
}

#[test]
fn native_friends_store_login_loads_scoped_cache_before_lua_without_platform_connection() {
    let sandbox = ShippedDataSandbox::new("native-friends-store-login");
    let (url, rx, server) = identity_with_failed_friends(vec!["own", "other"]);
    let runtime = StellaLua::new(&sandbox.data_root).unwrap();
    configure(&runtime, &url);
    runtime.skynest_account.seed_friends_cache_for_test(
        "own",
        &json!({"friends":[
            {"accountId":"z","nickName":"Last"},
            {"accountId":"a","nickName":"Personal","socialNetworkProfiles":[{"name":"Social"}]},
            {"accountId":"hidden"}, {"accountId":"","nickName":"Empty ID"}
        ]})
        .to_string(),
    );
    runtime.skynest_account.seed_friends_cache_for_test(
        "other",
        &json!({"friends":[{"accountId":"other-friend","nickName":"Other"}]}).to_string(),
    );
    runtime
        .execute_source("friends_before_login = _G.SocialManager.native_getFriends()")
        .unwrap();
    let env = game_environment(runtime.lua()).unwrap();
    assert_eq!(
        env.get::<mlua::Table>("friends_before_login")
            .unwrap()
            .raw_len(),
        0
    );
    login(&runtime, 1);
    let request = String::from_utf8(rx.recv_timeout(Duration::from_secs(5)).unwrap()).unwrap();
    assert!(request.starts_with("POST /session/1/apps/friends-fixture/sessions HTTP/1.1"));
    assert!(!env.get::<bool>("connected_at_login").unwrap());
    let friends = env.get::<mlua::Table>("friends_at_login").unwrap();
    assert_eq!(friends.raw_len(), 3);
    for (index, id, name) in [(1, "", "Empty ID"), (2, "a", "Social"), (3, "z", "Last")] {
        let friend = friends.raw_get::<mlua::Table>(index).unwrap();
        assert_eq!(friend.get::<String>("accountId").unwrap(), id);
        assert_eq!(friend.get::<String>("name").unwrap(), name);
    }
    // Native manager initializes only once; a repeated account success does
    // not silently reread changed disk state or fabricate platform readiness.
    runtime
        .skynest_account
        .seed_friends_cache_for_test("own", r#"{"friends":[]}"#);
    login(&runtime, 2);
    assert_eq!(
        env.get::<mlua::Table>("friends_at_login")
            .unwrap()
            .raw_len(),
        3
    );
    drop(runtime);
    let restarted = StellaLua::new(&sandbox.data_root).unwrap();
    configure(&restarted, &url);
    login(&restarted, 1);
    rx.recv_timeout(Duration::from_secs(5)).unwrap();
    server.join().unwrap();
    let env = game_environment(restarted.lua()).unwrap();
    let friends = env.get::<mlua::Table>("friends_at_login").unwrap();
    assert_eq!(friends.raw_len(), 1);
    assert_eq!(
        friends
            .raw_get::<mlua::Table>(1)
            .unwrap()
            .get::<String>("accountId")
            .unwrap(),
        "other-friend"
    );
    assert!(!env.get::<bool>("connected_at_login").unwrap());
}

#[test]
fn native_friends_store_cached_avatar_uses_real_download_without_social_provider() {
    use base64::Engine;
    let sandbox = ShippedDataSandbox::new("native-friends-store-avatar");
    let png = base64::engine::general_purpose::STANDARD.decode("iVBORw0KGgoAAAANSUhEUgAAABEAAAANCAYAAABPeYUaAAAAGElEQVR4nGMQtj2/llLMMGrIqCGjhpCFAd1JjSwPI2qaAAAAAElFTkSuQmCC").unwrap();
    let (asset_url, asset_rx, asset_server) = spawn_sequence_responses(vec![(200, png)]);
    let (url, rx, server) = identity_with_failed_friends(vec!["own"]);
    let runtime = StellaLua::new(&sandbox.data_root).unwrap();
    configure(&runtime, &url);
    runtime.skynest_account.seed_friends_cache_for_test("own",&json!({"friends":[
        {"accountId":"cached","socialNetworkProfiles":[{"socialNetwork":3,"uid":"game-id","name":"Cached","avatarUrl":asset_url}]}
    ]}).to_string());
    login(&runtime, 1);
    rx.recv_timeout(Duration::from_secs(5)).unwrap();
    server.join().unwrap();
    runtime
        .execute_source("_G.SocialManager.native_loadAvatar('cached')")
        .unwrap();
    let request =
        String::from_utf8(asset_rx.recv_timeout(Duration::from_secs(5)).unwrap()).unwrap();
    asset_server.join().unwrap();
    assert!(request.starts_with("GET "));
    assert!(!request.to_lowercase().contains("x-access-token:"));
    let env = game_environment(runtime.lua()).unwrap();
    for _ in 0..500 {
        dispatch_registered_application_events(runtime.lua()).unwrap();
        if env.get::<mlua::Table>("avatar_cached").unwrap().raw_len() == 1 {
            break;
        }
        thread::sleep(Duration::from_millis(2));
    }
    assert_eq!(
        env.get::<mlua::Table>("avatar_cached").unwrap().raw_len(),
        1
    );
    assert_eq!(
        env.get::<mlua::Table>("avatar_loaded").unwrap().raw_len(),
        0
    );
    runtime
        .execute_source("_G.SocialManager.native_loadAvatar('cached')")
        .unwrap();
    assert_eq!(
        env.get::<mlua::Table>("avatar_loaded").unwrap().raw_len(),
        1
    );
    let region = runtime
        .resource_runtime
        .lock()
        .unwrap()
        .active_atlas_catalog_region("AVATAR_cached", runtime.data_root())
        .unwrap();
    assert_eq!(
        (
            region.sprite.width,
            region.sprite.height,
            region.sprite.pivot_x,
            region.sprite.pivot_y
        ),
        (17, 13, 8, 6)
    );
    assert!(region.decoded_image.is_some());
    assert!(
        runtime
            .skynest_account
            .registry_path_for_test()
            .parent()
            .unwrap()
            .join("SkynestUserAvatars")
            .is_dir()
    );
    assert!(!env.get::<bool>("connected_at_login").unwrap());
}
