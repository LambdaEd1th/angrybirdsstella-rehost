use super::*;
use serde_json::json;

#[test]
fn native_friends_relations_require_strings_and_keep_duplicates_and_fallback_urls() {
    let friends = protocol::parse_relations(&json!({"socialFriends":[
        {"id":"same","socialNetworks":[{"networkId":"fb","provider":"facebook"}]},
        {"id":"same","socialNetworks":[{"networkId":"wb","provider":"sinaweibo","socialAttributes":{"name":"Weibo"}}]},
        {"id":""}
    ]}).to_string()).unwrap();
    assert_eq!(protocol::account_ids(&friends), ["same", "same", ""]);
    assert_eq!(
        friends[0].profile["socialNetworks"][0]["socialAttributes"]["avatarUrl"],
        "https://graph.facebook.com/fb/picture?type=normal"
    );
    assert_eq!(friends[1].display_name(), "Weibo");
    assert_eq!(
        friends[1].profile["socialNetworks"][0]["socialAttributes"]["avatarUrl"],
        "http://tp1.sinaimg.cn/wb/180/0/1"
    );
    for value in [
        json!({"socialFriends":[{}]}),
        json!({"socialFriends":[{"id":3}]}),
        json!({"socialFriends":[{"id":"id","socialNetworks":[{"provider":"facebook"}]}]}),
        json!({"socialFriends":[{"id":"id","socialNetworks":[false]}]}),
    ] {
        assert!(protocol::parse_relations(&value.to_string()).is_err());
    }
}

#[test]
fn native_friends_profile_search_keeps_only_first_match_assets_and_native_line_rules() {
    let mut friends = protocol::parse_relations(r#"{"socialFriends":[{"id":"x","socialNetworks":[{"provider":"other","networkId":"rel","socialAttributes":{"name":"Relation"}}]},{"id":""}]}"#).unwrap();
    let profiles = format!(
        "\n{}\n{}\n{}",
        json!({"publicAccountId":"x","personal":{"nickName":"Ignored","imageAssets":[{"url":"first"}]},"socialNetworks":[]}),
        json!({"publicAccountId":"x","personal":{"imageAssets":[{"url":"second"}]}}),
        json!({"publicAccountId":"","personal":{"imageAssets":[{"url":"ignored-empty-ID"}]}})
    );
    protocol::merge_avatar_profiles(&mut friends, &profiles).unwrap();
    assert_eq!(friends[0].display_name(), "Relation");
    assert_eq!(friends[0].nickname, "");
    assert_eq!(friends[0].profile["socialNetworks"][0]["id"], "rel");
    assert_eq!(
        friends[0].profile["personal"]["imageAssets"][0]["url"],
        "first"
    );
    assert!(friends[1].profile["personal"].is_null());
    for text in [" \n", "42\n", "[]\n", "{\n"] {
        assert!(protocol::merge_avatar_profiles(&mut friends, text).is_err());
    }
}

fn load(value: Value) -> FriendsStore {
    FriendsStore::load(
        "own".into(),
        &value.to_string(),
        PathBuf::from("unused/skynest_friends_store_own"),
    )
    .unwrap()
}

#[test]
fn native_friends_cache_sorts_ids_keeps_last_duplicate_and_first_social_name() {
    let store = load(json!({"friends":[
        {"accountId":"z","nickName":"Personal","socialNetworkProfiles":[
            {"name":""},{"socialNetwork":1,"uid":"fb","name":"First"},{"name":"Second"}]},
        {"accountId":"a","nickName":"Replaced"},
        {"accountId":"a","nickName":"Last"},
        {"accountId":"","nickName":"Empty ID is retained"}
    ]}));
    assert_eq!(
        store.friends.keys().map(String::as_str).collect::<Vec<_>>(),
        ["", "a", "z"]
    );
    assert_eq!(store.friends["a"].display_name(), "Last");
    assert_eq!(store.friends["z"].display_name(), "First");
    assert_eq!(store.friends["z"].profile["socialNetworks"][1]["id"], "fb");
}

#[test]
fn native_friends_cache_typed_defaults_do_not_inherit_or_load_personal_assets() {
    let store = load(json!({"friends":[
        {"accountId":"named","nickName":42,"socialNetworkProfiles":[
            {"socialNetwork":4294967297i64,"uid":"fb","name":false,"avatarUrl":7},
            false,
            {"socialNetwork":4294967298.9,"name":"Name"}],
         "personal":{"imageAssets":[{"url":"must-not-load"}]},"avatarId":"ignored"},
        false
    ],"socialNetworkFriends":[{"socialNetwork":1,"uid":"fb","name":"Do not merge on load"}]}));
    let friend = &store.friends["named"];
    assert_eq!(friend.display_name(), "Name");
    assert_eq!(friend.profile["socialNetworks"][0]["provider"], "facebook");
    assert_eq!(
        friend.profile["socialNetworks"][0]["socialAttributes"]["name"],
        ""
    );
    assert_eq!(friend.profile["socialNetworks"][1]["id"], "");
    assert_eq!(friend.profile["socialNetworks"][2]["provider"], "sinaweibo");
    assert!(friend.profile.get("personal").is_none());
    assert_eq!(store.friends[""].display_name(), "");
    assert_eq!(
        store.platform_profiles[&(1, "fb".to_owned())].name,
        "Do not merge on load"
    );
}

#[test]
fn native_friends_cache_empty_is_null_but_damaged_json_is_an_error() {
    for text in ["", "null", "false", "42", "[]", r#"{"friends":false}"#] {
        assert!(
            FriendsStore::load("own".into(), text, PathBuf::new())
                .unwrap()
                .friends
                .is_empty()
        );
    }
    for text in [
        " ",
        "{",
        r#"{"friends":[{"socialNetworkProfiles":[{"socialNetwork":18446744073709551615}]}]}"#,
    ] {
        assert!(FriendsStore::load("own".into(), text, PathBuf::new()).is_err());
    }
}

#[test]
fn native_platform_friends_merge_fills_only_empty_matching_relation_fields() {
    let mut store = load(json!({"friends":[
        {"accountId":"game","nickName":"Nickname","socialNetworkProfiles":[
            {"socialNetwork":1,"uid":"fb-id","name":"","avatarUrl":""},
            {"socialNetwork":1,"uid":"keep","name":"Keep name","avatarUrl":"keep-url"},
            {"socialNetwork":2,"uid":"fb-id","name":"","avatarUrl":""}
        ]}
    ]}));
    store.merge_platform_friends(
        SocialNetwork::Facebook,
        vec![
            SocialPlatformUser {
                id: "fb-id".into(),
                name: "Platform name".into(),
                avatar_url: "platform-url".into(),
                ..Default::default()
            },
            SocialPlatformUser {
                id: "keep".into(),
                name: "Wrong replacement".into(),
                avatar_url: "wrong-url".into(),
                ..Default::default()
            },
            SocialPlatformUser {
                id: "platform-only".into(),
                name: "No game account".into(),
                ..Default::default()
            },
        ],
    );
    assert_eq!(store.friends.len(), 1);
    assert_eq!(store.friends["game"].display_name(), "Platform name");
    let profiles = &store.friends["game"].profile["socialNetworks"];
    assert_eq!(profiles[0]["socialAttributes"]["avatarUrl"], "platform-url");
    assert_eq!(profiles[1]["socialAttributes"]["name"], "Keep name");
    assert_eq!(profiles[1]["socialAttributes"]["avatarUrl"], "keep-url");
    assert_eq!(profiles[2]["socialAttributes"]["name"], "");
    assert_eq!(profiles[2]["socialAttributes"]["avatarUrl"], "");
    assert_eq!(store.platform_profiles.len(), 3);
    let disk = store.cache_value();
    assert_eq!(disk["socialNetworkFriends"].as_array().unwrap().len(), 3);
    let restarted = load(disk);
    assert_eq!(restarted.friends["game"].display_name(), "Platform name");
    assert_eq!(
        restarted.platform_profiles[&(1, "platform-only".into())].name,
        "No game account"
    );
}

#[test]
fn native_platform_friends_duplicates_fallbacks_and_empty_ids_follow_native_map_rules() {
    let mut store = load(
        json!({"friends":[{"accountId":"game","nickName":"Fallback","socialNetworkProfiles":[{"socialNetwork":1,"uid":""}]}]}),
    );
    store.merge_platform_friends(
        SocialNetwork::Facebook,
        vec![
            SocialPlatformUser {
                id: "".into(),
                name: "First duplicate".into(),
                ..Default::default()
            },
            SocialPlatformUser {
                id: "".into(),
                username: "Username fallback".into(),
                ..Default::default()
            },
        ],
    );
    assert_eq!(store.platform_profiles.len(), 1);
    assert_eq!(store.friends["game"].display_name(), "Username fallback");
    assert_eq!(
        store.platform_profiles[&(1, String::new())].avatar_url,
        "https://graph.facebook.com//picture?type=normal"
    );
    // A later successful response replaces platform data, but previously filled
    // game values are now nonempty and remain unchanged until relations refresh.
    store.merge_platform_friends(
        SocialNetwork::Facebook,
        vec![SocialPlatformUser {
            id: "".into(),
            name: "Later name".into(),
            avatar_url: "later-url".into(),
            ..Default::default()
        }],
    );
    assert_eq!(
        store.platform_profiles[&(1, String::new())].name,
        "Later name"
    );
    assert_eq!(store.friends["game"].display_name(), "Username fallback");
    assert_eq!(
        store.friends["game"].profile["socialNetworks"][0]["socialAttributes"]["avatarUrl"],
        "https://graph.facebook.com//picture?type=normal"
    );
}
