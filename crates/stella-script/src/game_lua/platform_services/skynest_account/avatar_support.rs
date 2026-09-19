//! Shared native UserProfile selection, logger and scoped registry adapters.

use super::*;
use serde_json::Value;

#[derive(Clone)]
pub(in crate::game_lua::platform_services) struct SdkLogSink(IdentitySession);

impl std::fmt::Debug for SdkLogSink {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("SdkLogSink")
    }
}

impl SdkLogSink {
    #[cfg(test)]
    pub(in crate::game_lua::platform_services) fn detached_for_test() -> Self {
        Self(IdentitySession::default())
    }
    pub(in crate::game_lua::platform_services) fn submit(
        &self,
        level: SdkLogLevel,
        tag: &str,
        message: &str,
    ) {
        self.0.sdk_logger.submit(&self.0, level, tag, message);
    }
}

impl SkynestAccountRuntime {
    pub(in crate::game_lua::platform_services) fn sdk_log_sink(&self) -> SdkLogSink {
        SdkLogSink(self.session.clone())
    }

    pub(in crate::game_lua::platform_services) fn avatar_profile(
        &self,
        account: &str,
    ) -> Option<Value> {
        self.session
            .profile()
            .filter(|profile| profile.public_account_id == account)
            .map(|profile| profile.raw)
    }
}

pub(in crate::game_lua::platform_services) struct AvatarCacheRegistry(session::RegistryStore);

impl AvatarCacheRegistry {
    pub(in crate::game_lua::platform_services) fn open(path: PathBuf) -> Result<Self, String> {
        session::RegistryStore::open(path)
            .map(Self)
            .map_err(|e| e.to_string())
    }
    pub(in crate::game_lua::platform_services) fn creation_time(
        &self,
        directory: &str,
    ) -> Result<i64, String> {
        self.0
            .avatar_creation_time(directory)
            .map_err(|e| e.to_string())
    }
    pub(in crate::game_lua::platform_services) fn set_creation_time(
        &self,
        directory: &str,
        now: i64,
    ) -> Result<(), String> {
        self.0
            .set_avatar_creation_time(directory, now)
            .map_err(|e| e.to_string())
    }
}

/// 100684E64: preference1 social-first, preference0 personal-first.
pub(in crate::game_lua::platform_services) fn avatar_url(
    raw: &Value,
    preference: i32,
    dimension: i32,
) -> Result<String, String> {
    let profile = session::parse_profile_value(raw);
    let personal = || {
        for asset in &profile.avatar_assets {
            let size = asset
                .dimension
                .ok_or("avatar dimension is indeterminate in native input")?;
            if size >= dimension {
                return Ok(asset.url.clone());
            }
        }
        Ok(String::new())
    };
    let social = || {
        let mut chosen = String::new();
        for entry in &profile.social_networks {
            let id = entry["id"].as_str().unwrap_or_default();
            let provider = entry["provider"].as_str().unwrap_or_default();
            if provider == "facebook" {
                let size = dimension.max(0);
                chosen = format!(
                    "https://graph.facebook.com/{id}/picture?height={size}&type=normal&width={size}"
                );
                continue;
            }
            let explicit = entry
                .get("socialAttributes")
                .and_then(Value::as_object)
                .and_then(|attributes| attributes.get("avatarUrl"))
                .and_then(Value::as_str)
                .unwrap_or_default();
            if !explicit.is_empty() {
                return explicit.to_owned();
            }
            if provider == "sinaweibo" {
                return format!("http://tp1.sinaimg.cn/{id}/180/0/1");
            }
        }
        chosen
    };
    match preference {
        1 => {
            let url = social();
            if url.is_empty() { personal() } else { Ok(url) }
        }
        0 => {
            let url = personal()?;
            if url.is_empty() {
                Ok(social())
            } else {
                Ok(url)
            }
        }
        _ => Ok(String::new()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn social_avatar_profile_selection_preserves_input_order_and_social_preference() {
        let raw = json!({"personal":{"imageAssets":[
            {"url":"first","hash":"a","dimension":128,"size":1},
            {"url":"second","hash":"b","dimension":64,"size":2}
        ]},"socialNetworks":[
            {"provider":"facebook","id":"fb","socialAttributes":{"avatarUrl":"ignored"}},
            {"provider":"other","id":"id","socialAttributes":{"avatarUrl":"override"}}
        ]});
        assert_eq!(avatar_url(&raw, 0, 64).unwrap(), "first");
        assert_eq!(avatar_url(&raw, 1, 64).unwrap(), "override");
        assert_eq!(avatar_url(&raw, 9, 64).unwrap(), "");
        let mut fb = raw.clone();
        fb["socialNetworks"].as_array_mut().unwrap().pop();
        assert_eq!(
            avatar_url(&fb, 1, -3).unwrap(),
            "https://graph.facebook.com/fb/picture?height=0&type=normal&width=0"
        );
        fb["socialNetworks"] = json!([]);
        assert_eq!(avatar_url(&fb, 1, 64).unwrap(), "first");
        assert_eq!(avatar_url(&fb, 0, 129).unwrap(), "");
    }

    #[test]
    fn social_avatar_selection_inherits_personal_fields_and_requires_typed_social_entries() {
        let raw = json!({"personal":{"imageAssets":[
            {"url":"inherited","hash":"v","dimension":32,"size":1},
            {"url":"not-adopted","dimension":64},
            false
        ]},"socialNetworks":[{"provider":"facebook","id":3},{"provider":"sinaweibo","id":"42"}]});
        assert_eq!(avatar_url(&raw, 0, 64).unwrap(), "inherited");
        assert_eq!(
            avatar_url(&raw, 1, 64).unwrap(),
            "http://tp1.sinaimg.cn/42/180/0/1"
        );
        assert!(avatar_url(&json!({"personal":{"imageAssets":[{}]}}), 0, 64).is_err());
        assert_eq!(avatar_url(&json!({}), 1, 64).unwrap(), "");
    }
}
