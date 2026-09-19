//! Native friends relations and newline-separated profile/search response.
use super::*;

pub(in crate::game_lua::platform_services) fn account_ids(
    friends: &[LocalSocialFriend],
) -> Vec<String> {
    friends
        .iter()
        .map(|friend| friend.account_id.clone())
        .collect()
}

pub(in crate::game_lua::platform_services) fn parse_relations(
    text: &str,
) -> Result<Vec<LocalSocialFriend>, String> {
    let root = parse_json(text)?;
    let mut friends = Vec::new();
    if let Some(entries) = root.get("socialFriends").and_then(Value::as_array) {
        for entry in entries {
            let account_id = required_string(entry, "id")?;
            let mut profiles = Vec::new();
            if let Some(entries) = entry.get("socialNetworks").and_then(Value::as_array) {
                for entry in entries {
                    let uid = required_string(entry, "networkId")?;
                    let provider = required_string(entry, "provider")?;
                    let network = match provider.as_str() {
                        "facebook" => 1,
                        "sinaweibo" => 2,
                        "gamecenter" => 3,
                        "kakaotalk" => 4,
                        _ => 0,
                    };
                    let attributes = entry.get("socialAttributes").unwrap_or(&Value::Null);
                    let mut avatar_url = string(attributes, "avatarUrl");
                    if avatar_url.is_empty() {
                        avatar_url = default_avatar_url(network, &uid);
                    }
                    profiles.push(NetworkProfile {
                        network,
                        uid,
                        avatar_url,
                        name: string(attributes, "name"),
                    });
                }
            }
            let name = profiles
                .iter()
                .find(|p| !p.name.is_empty())
                .map(|p| p.name.clone())
                .unwrap_or_default();
            friends.push(LocalSocialFriend { account_id:account_id.clone(), name,
                profile:serde_json::json!({"publicAccountId":account_id,
                    "socialNetworks":profiles.iter().map(NetworkProfile::profile_value).collect::<Vec<_>>()}),
                ..Default::default() });
        }
    }
    Ok(friends)
}

pub(in crate::game_lua::platform_services) fn merge_avatar_profiles(
    friends: &mut [LocalSocialFriend],
    text: &str,
) -> Result<(), String> {
    let mut profiles = Vec::new();
    // getline consumes LF only, skips no blank lines, and accepts a final line
    // without LF. Empty JSON text is native null, whereas whitespace is invalid.
    for line in text.split_terminator('\n') {
        let root = parse_json(line)?;
        if !root.is_object() && !root.is_null() {
            return Err("profile/search JSON is not an object".into());
        }
        profiles.push(root);
    }
    for friend in friends {
        if let Some(profile) = profiles
            .iter()
            .find(|p| string(p, "publicAccountId") == friend.account_id)
        {
            // 1007336AC copies only AvatarAssetVec from the FIRST matching ID.
            // Name, relation order and social profiles remain from /friends.
            friend.profile["personal"] = profile.get("personal").cloned().unwrap_or(Value::Null);
        }
    }
    Ok(())
}

fn parse_json(text: &str) -> Result<Value, String> {
    if text.is_empty() {
        Ok(Value::Null)
    } else {
        serde_json::from_str(text).map_err(|e| format!("Parsing friends JSON failed: {e}"))
    }
}

fn required_string(value: &Value, key: &str) -> Result<String, String> {
    value
        .get(key)
        .and_then(Value::as_str)
        .map(str::to_owned)
        .ok_or_else(|| format!("friends JSON member {key} must be a String"))
}

pub(super) fn default_avatar_url(network: i32, uid: &str) -> String {
    match network {
        1 => format!("https://graph.facebook.com/{uid}/picture?type=normal"),
        2 => format!("http://tp1.sinaimg.cn/{uid}/180/0/1"),
        _ => String::new(),
    }
}
