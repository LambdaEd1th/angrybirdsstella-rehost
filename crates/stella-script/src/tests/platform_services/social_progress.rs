//! Native silent progress writes share the SDK storage path and ownership.
use super::{social_game::*, storage_session::*, *};
use std::time::Instant;

const POST: &str = "POST /proxy/storage/1.0/state ";
const GET: &str = "GET /proxy/storage/1.0/state?key=%5Bmy%5D%2F%5Bclient%5D%2Fprogress ";

fn configure(runtime: &StellaLua, server: &Server) {
    runtime
        .set_storage_url(&format!("{}/storage/1.0", server.origin))
        .unwrap();
    runtime
        .execute_source(
            r#"
        generic_done=0
        notifyEventManager=function(name)
            if string.match(name,"^EID_SYNC_CLOUD") then error("progress must not emit cloud events") end
        end
        _G.SkynestStorage.cloudDataNewDataAvailable=function() error("progress must not merge cloud settings") end
    "#,
        )
        .unwrap();
}

fn post(runtime: &StellaLua, value: &str) {
    game_environment(runtime.lua())
        .unwrap()
        .set("progress_value", value)
        .unwrap();
    runtime
        .execute_source(
            "assert(select('#',_G.SocialManager.native_setProgress(progress_value))==0)",
        )
        .unwrap();
}

fn fields(request: &str, value: &str, hash: &str) {
    assert!(request.lines().next().unwrap().starts_with(POST));
    let fields = form_fields(body(request));
    assert_eq!(fields.len(), 5);
    assert_eq!(fields["key"], "[my]/[client]/progress");
    assert_eq!(fields["encoding"], "SDKv2");
    assert_eq!(fields["force"], "false");
    assert_eq!(fields["hash"], hash);
    assert_eq!(decode_sdkv2(&fields["value"]), value);
}

fn wait_hash(runtime: &StellaLua, hash: &str) {
    wait(runtime, |_| {
        runtime
            .skynest_storage
            .cached_key_for_test("progress")
            .1
            .as_deref()
            == Some(hash)
    });
}

fn wait_worker(queued: impl Fn() -> usize) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while queued() == 0 {
        assert!(
            Instant::now() < deadline,
            "native progress worker did not finish"
        );
        thread::sleep(Duration::from_millis(1));
    }
}

fn silent(runtime: &StellaLua) {
    let environment = game_environment(runtime.lua()).unwrap();
    for name in ["game_posts", "game_boards"] {
        assert_eq!(environment.get::<mlua::Table>(name).unwrap().raw_len(), 0);
    }
    assert_eq!(
        runtime.skynest_storage.retained_callback_count_for_test(),
        0
    );
    runtime
        .execute_source("assert(not _G.SkynestStorage.native_isTransactionInProcess())")
        .unwrap();
}

#[test]
fn native_social_progress_acquires_without_social_login_replays_frozen_sdkv2_and_shares_hash() {
    let sandbox = Sandbox::new("native-progress-wire");
    let server = Server::new(
        false,
        vec![
            rule(POST, 401, ""),
            rule(POST, 200, r#"[{"hash":"progress-first"}]"#),
            rule(POST, 200, r#"[{"hash":"progress-second"}]"#),
        ],
    );
    let runtime = runtime(&sandbox, &server, false);
    configure(&runtime, &server);
    let value = "星/42\nthis is an opaque progress string, not JSON";
    post(&runtime, value);
    let initial = server.request();
    let replay = server.request();
    fields(&initial, value, "");
    assert_eq!(body(&initial), body(&replay));
    assert_auth(&initial, "synthetic-game-access", "8, 2");
    assert_auth(&replay, "renewed-game-access", "8, 2");
    wait_hash(&runtime, "progress-first");
    assert_eq!(server.session_requests(), 2);
    assert_eq!(runtime.social.platform_state_for_test(), (0, false, false));
    assert_eq!(
        game_environment(runtime.lua())
            .unwrap()
            .get::<i64>("game_login")
            .unwrap(),
        0
    );
    silent(&runtime);
    runtime
        .execute_source(
            r#"_G.SkynestStorage.native_setKey('progress','ordinary-value',function(...)
                assert(select('#',...)==0);generic_done=generic_done+1
            end)"#,
        )
        .unwrap();
    fields(&server.request(), "ordinary-value", "progress-first");
    wait(&runtime, |environment| {
        environment.get::<i64>("generic_done").unwrap() == 1
    });
    assert_eq!(
        runtime.skynest_storage.cached_key_for_test("progress"),
        (
            Some("ordinary-value".to_owned()),
            Some("progress-second".to_owned())
        )
    );
    silent(&runtime);
    server.finish();
}

#[test]
fn native_social_progress_conflict_fetch_preserves_value_and_other_callback_without_auto_overwrite()
{
    let sandbox = Sandbox::new("native-progress-conflict");
    let (release, resume) = mpsc::channel();
    let mut held = rule(POST, 200, r#"[{"hash":"unrelated-hash"}]"#);
    held.hold = Some(resume);
    let server = Server::new(
        false,
        vec![
            rule(POST, 200, r#"[{"hash":"seed"}]"#),
            held,
            rule(POST, 409, ""),
            rule(
                GET,
                200,
                r#"[{"hash":"remote","value":"opaque remote progress","encoding":"SDKv1"}]"#,
            ),
            rule(POST, 200, r#"[{"hash":"last"}]"#),
        ],
    );
    let runtime = runtime(&sandbox, &server, false);
    configure(&runtime, &server);
    post(&runtime, "seed-value");
    server.request();
    wait_hash(&runtime, "seed");
    runtime
        .execute_source(
            r#"_G.SkynestStorage.native_setKey('unrelated','unrelated-value',function(...)
                assert(select('#',...)==0);generic_done=generic_done+1
            end)"#,
        )
        .unwrap();
    let unrelated = server.request();
    assert_eq!(
        form_fields(body(&unrelated))["key"],
        "[my]/[client]/unrelated"
    );
    post(&runtime, "rejected-local-value");
    fields(&server.request(), "rejected-local-value", "seed");
    wait(&runtime, |_| {
        runtime
            .skynest_storage
            .cached_key_for_test("progress")
            .1
            .as_deref()
            == Some("remote")
    });
    let fetched = server.request();
    assert!(fetched.lines().next().unwrap().starts_with(GET));
    assert_eq!(
        runtime.skynest_storage.retained_callback_count_for_test(),
        1
    );
    assert_eq!(
        runtime.skynest_storage.cached_key_for_test("progress"),
        (Some("seed-value".to_owned()), Some("remote".to_owned()))
    );
    post(&runtime, "explicit-next-value");
    fields(&server.request(), "explicit-next-value", "remote");
    wait_hash(&runtime, "last");
    assert_eq!(
        runtime.skynest_storage.retained_callback_count_for_test(),
        1
    );
    release.send(()).unwrap();
    server.finish();
    wait(&runtime, |environment| {
        environment.get::<i64>("generic_done").unwrap() == 1
    });
    silent(&runtime);
}

#[test]
fn native_social_progress_rejected_status_and_schema_preserve_prior_cache_without_lua_completion() {
    for (status, response) in [
        (201, r#"[{"hash":"must-not-publish"}]"#),
        (204, ""),
        (400, "private server diagnostic"),
        (503, ""),
        (200, "{}"),
        (200, r#"[{"hash":false}]"#),
    ] {
        let sandbox = Sandbox::new("native-progress-errors");
        let server = Server::new(
            false,
            vec![
                rule(POST, 200, r#"[{"hash":"seed"}]"#),
                rule(POST, status, response),
            ],
        );
        let runtime = runtime(&sandbox, &server, false);
        configure(&runtime, &server);
        post(&runtime, "seed-value");
        server.request();
        wait_hash(&runtime, "seed");
        let queued = runtime.skynest_storage.online_completion_count_probe();
        post(&runtime, "rejected-value");
        fields(&server.request(), "rejected-value", "seed");
        wait_worker(&queued);
        dispatch_registered_application_events(runtime.lua()).unwrap();
        assert_eq!(
            runtime.skynest_storage.cached_key_for_test("progress"),
            (Some("seed-value".to_owned()), Some("seed".to_owned()))
        );
        silent(&runtime);
        server.finish();
    }
}

#[test]
fn native_social_progress_unavailable_and_empty_inputs_do_not_acquire_session_or_send_http() {
    let sandbox = Sandbox::new("native-progress-empty");
    let server = Server::new(false, vec![]);
    let runtime = runtime(&sandbox, &server, false);
    post(&runtime, "unavailable-storage");
    assert_eq!(server.session_requests(), 0);
    configure(&runtime, &server);
    let queued = runtime.skynest_storage.online_completion_count_probe();
    post(&runtime, "");
    wait_worker(&queued);
    dispatch_registered_application_events(runtime.lua()).unwrap();
    runtime
        .execute_source(
            r#"
        _G.SkynestStorage.native_setKey('','value',function(...) assert(select('#',...)==0);generic_done=generic_done+1 end)
        _G.SkynestStorage.native_setKey('key','',function(...) assert(select('#',...)==0);generic_done=generic_done+1 end)
    "#,
        )
        .unwrap();
    wait(&runtime, |environment| {
        environment.get::<i64>("generic_done").unwrap() == 2
    });
    assert_eq!(server.session_requests(), 0);
    assert_eq!(
        runtime.skynest_storage.cached_key_for_test("progress"),
        (None, None)
    );
    silent(&runtime);
    server.finish();
}

#[test]
fn native_social_progress_held_post_and_conflict_get_retire_on_context_change_or_drop() {
    for conflict in [false, true] {
        for operation in ["logout", "identity", "storage", "drop"] {
            let sandbox = Sandbox::new("native-progress-held");
            let (release, resume) = mpsc::channel();
            let mut held = if conflict {
                rule(
                    GET,
                    200,
                    r#"[{"hash":"late-hash","value":"late-remote","encoding":"SDKv1"}]"#,
                )
            } else {
                rule(POST, 200, r#"[{"hash":"late-hash"}]"#)
            };
            held.hold = Some(resume);
            let rules = if conflict {
                vec![rule(POST, 409, ""), held]
            } else {
                vec![held]
            };
            let server = Server::new(false, rules);
            let runtime = runtime(&sandbox, &server, false);
            configure(&runtime, &server);
            let cache = runtime
                .skynest_storage
                .cached_key_probe_for_test("progress");
            let queued = runtime.skynest_storage.online_completion_count_probe();
            post(&runtime, "held-local");
            server.request();
            if conflict {
                wait_worker(&queued);
                dispatch_registered_application_events(runtime.lua()).unwrap();
                server.request();
            }
            if operation == "drop" {
                drop(runtime);
                release.send(()).unwrap();
                server.finish();
                wait_worker(&queued);
                assert_eq!(cache(), (None, None));
            } else {
                match operation {
                    "logout" => runtime
                        .execute_source("_G.SkynestAccount.native_logout()")
                        .unwrap(),
                    "identity" => runtime
                        .set_identity_url("http://127.0.0.1:9/replaced/identity/2.0")
                        .unwrap(),
                    "storage" => runtime
                        .set_storage_url(&format!("{}/replaced/storage/1.0", server.origin))
                        .unwrap(),
                    _ => unreachable!(),
                }
                release.send(()).unwrap();
                server.finish();
                wait_worker(&queued);
                dispatch_registered_application_events(runtime.lua()).unwrap();
                assert_eq!(cache(), (None, None));
                silent(&runtime);
            }
        }
    }
}
