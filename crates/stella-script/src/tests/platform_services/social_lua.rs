//! Shipped, unmodified Lua facade over native services and loopback HTTP.
use super::{social_game::*, storage_session::*, *};

fn load_original(runtime: &StellaLua) {
    runtime
        .execute_source(
            r#"
        social_events,social_listeners={},{}
        events=setmetatable({}, {__index=function(_,name) return name end})
        RovioCloudManager={isServiceAvailable=function() return false end}
        adSystem={}
        eventManager={
            addEventListener=function(_,id,listener)
                social_listeners[id]=listener
            end,
            notify=function(first,second)
                local event=second or first
                social_events[#social_events+1]=event
                local listener=social_listeners[event.id]
                if listener then listener:eventTriggered(event) end
            end
        }
        notifyEventManager=function(name)
            if string.match(name,"^EID_SYNC_CLOUD") then error("progress must not emit cloud events") end
        end
        _G.SkynestStorage.cloudDataNewDataAvailable=function() error("progress must not merge cloud settings") end
    "#,
        )
        .unwrap();
    let source = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../runtime/data/scripts_common/cloud/SocialManager.lua");
    let prepared = stella_assets::lua::prepare_for_host(&fs::read(source).unwrap()).unwrap();
    runtime
        .lua()
        .load(prepared)
        .set_environment(game_environment(runtime.lua()).unwrap())
        .exec()
        .unwrap();
    // Observe the argument list without replacing the shipped callback body.
    runtime
        .execute_source(
            r#"
        social_board_callbacks={}
        local original=_G.SocialManager.onLeaderboardFetched
        _G.SocialManager.onLeaderboardFetched=function(...)
            local arguments={...};arguments.n=select('#',...)
            social_board_callbacks[#social_board_callbacks+1]=arguments
            return original(...)
        end
    "#,
        )
        .unwrap();
}

fn announce(runtime: &StellaLua) {
    crate::announce_cloud_service_registrations(runtime.lua()).unwrap();
}

fn events(runtime: &StellaLua, id: &str) -> Vec<mlua::Table> {
    game_environment(runtime.lua())
        .unwrap()
        .get::<mlua::Table>("social_events")
        .unwrap()
        .sequence_values::<mlua::Table>()
        .map(Result::unwrap)
        .filter(|event| event.get::<String>("id").unwrap() == id)
        .collect()
}

#[test]
fn native_social_original_lua_progress_waits_for_registration_then_uploads_level_without_connection()
 {
    let sandbox = Sandbox::new("original-social-progress");
    let server = Server::new(
        false,
        vec![rule(
            "POST /proxy/storage/1.0/state ",
            200,
            r#"[{"hash":"original-progress"}]"#,
        )],
    );
    let runtime = runtime(&sandbox, &server, false);
    runtime
        .set_storage_url(&format!("{}/storage/1.0", server.origin))
        .unwrap();
    load_original(&runtime);
    login(&runtime, false);
    runtime
        .execute_source(
            r#"eventManager:notify({id=events.EID_PROGRESS_UPDATED,progress={level="before-enable"}})"#,
        )
        .unwrap();
    assert_eq!(
        runtime.skynest_storage.cached_key_for_test("progress"),
        (None, None)
    );
    announce(&runtime);
    runtime
        .execute_source(
            r#"
        assert(not SocialManager.isConnected())
        eventManager:notify({id=events.EID_PROGRESS_UPDATED,progress={level="S01L07",unrelated="ignored"}})
    "#,
        )
        .unwrap();
    let request = server.request();
    assert!(request.starts_with("POST /proxy/storage/1.0/state "));
    let fields = form_fields(body(&request));
    assert_eq!(fields.len(), 5);
    assert_eq!(fields["key"], "[my]/[client]/progress");
    assert_eq!(fields["encoding"], "SDKv2");
    assert_eq!(fields["force"], "false");
    assert_eq!(fields["hash"], "");
    assert_eq!(decode_sdkv2(&fields["value"]), "S01L07");
    wait(&runtime, |_| {
        runtime
            .skynest_storage
            .cached_key_for_test("progress")
            .1
            .as_deref()
            == Some("original-progress")
    });
    assert_eq!(runtime.social.platform_state_for_test(), (0, false, false));
    assert_eq!(
        runtime.skynest_storage.retained_callback_count_for_test(),
        0
    );
    assert!(events(&runtime, "EID_SOCIAL_SCORE_POSTED").is_empty());
    assert!(events(&runtime, "EID_SOCIAL_LEADERBOARD_FETCHED").is_empty());
    server.finish();
}

#[test]
fn native_social_original_lua_score_and_board_preserve_ids_and_publish_only_success_events() {
    let sandbox = Sandbox::new("original-social-score-board");
    let server = Server::new(
        true,
        vec![
            rule("POST /proxy/leaderboard/1.0/score ", 200, ""),
            rule(
                "GET /proxy/leaderboard/1.0/level-S01L01/query/friends?nocache=1 ",
                200,
                r#"{"scores":[{"accountId":"own","score":{"points":21},"ranking":{"rank":1}},{"accountId":"other","score":{"points":19},"ranking":{"rank":2}}]}"#,
            ),
            rule(
                "GET /proxy/leaderboard/1.0/level-S01L01/query/friends ",
                201,
                "",
            ),
        ],
    );
    let runtime = runtime(&sandbox, &server, true);
    load_original(&runtime);
    runtime
        .execute_source("_G.SkynestAccount.native_login(false,false,false)")
        .unwrap();
    wait(&runtime, |env| {
        env.get::<i32>("game_login").unwrap() == 1
            && runtime.social.platform_state_for_test() == (0, false, true)
    });
    announce(&runtime);
    runtime
        .execute_source(
            r#"
        assert(SocialManager.isConnected())
        score_request=SocialManager.postScore("S01L01",21.9)
        assert(type(score_request)=="string")
    "#,
        )
        .unwrap();
    let request = server.request();
    assert!(request.starts_with("POST /proxy/leaderboard/1.0/score "));
    let payload: serde_json::Value =
        serde_json::from_str(&decode(&request, "synthetic-game-access")).unwrap();
    assert_eq!(payload["score"]["points"], 21);
    wait(&runtime, |_| {
        !events(&runtime, "EID_SOCIAL_SCORE_POSTED").is_empty()
    });
    runtime
        .execute_source(
            r#"
        board_request=SocialManager.fetchLeaderboard("S01L01")
        assert(type(board_request)=="string" and score_request~=board_request)
    "#,
        )
        .unwrap();
    assert!(
        server
            .request()
            .starts_with("GET /proxy/leaderboard/1.0/level-S01L01/query/friends?nocache=1 ")
    );
    wait(&runtime, |_| {
        !events(&runtime, "EID_SOCIAL_LEADERBOARD_FETCHED").is_empty()
    });
    let environment = game_environment(runtime.lua()).unwrap();
    let posted = events(&runtime, "EID_SOCIAL_SCORE_POSTED");
    assert_eq!(posted.len(), 1);
    assert_eq!(posted[0].get::<String>("levelName").unwrap(), "S01L01");
    assert_eq!(
        posted[0].get::<String>("requestId").unwrap(),
        environment.get::<String>("score_request").unwrap()
    );
    let boards = events(&runtime, "EID_SOCIAL_LEADERBOARD_FETCHED");
    assert_eq!(boards.len(), 1);
    assert_eq!(boards[0].get::<String>("levelName").unwrap(), "S01L01");
    assert_eq!(
        boards[0].get::<String>("requestId").unwrap(),
        environment.get::<String>("board_request").unwrap()
    );
    let rows = boards[0].get::<mlua::Table>("leaderboard").unwrap();
    assert_eq!(rows.raw_len(), 2);
    assert_eq!(
        rows.raw_get::<mlua::Table>(1)
            .unwrap()
            .get::<String>("nickname")
            .unwrap(),
        "Own"
    );
    assert_eq!(
        rows.raw_get::<mlua::Table>(2)
            .unwrap()
            .get::<String>("nickname")
            .unwrap(),
        "n/a"
    );
    runtime
        .execute_source(
            r#"
        original_board=SocialManager.getCurrentLevelCachedLeaderboard()
        assert(#original_board==2)
        failed_board_request=SocialManager.fetchLeaderboard("S01L01")
    "#,
        )
        .unwrap();
    assert!(
        server
            .request()
            .starts_with("GET /proxy/leaderboard/1.0/level-S01L01/query/friends ")
    );
    wait(&runtime, |env| {
        env.get::<mlua::Table>("social_board_callbacks")
            .unwrap()
            .raw_len()
            == 2
    });
    runtime
        .execute_source(
            r#"
        assert(social_board_callbacks[1].n==4 and social_board_callbacks[1][1]==true)
        assert(social_board_callbacks[2].n==2 and social_board_callbacks[2][1]==false)
        assert(SocialManager.getCurrentLevelCachedLeaderboard()==original_board)
    "#,
        )
        .unwrap();
    assert_eq!(events(&runtime, "EID_SOCIAL_SCORE_POSTED").len(), 1);
    assert_eq!(events(&runtime, "EID_SOCIAL_LEADERBOARD_FETCHED").len(), 1);
    server.finish();
}
