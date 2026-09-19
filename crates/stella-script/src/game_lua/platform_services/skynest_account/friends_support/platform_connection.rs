//! 100726F10 external/connect -> nonempty platform IDs -> direct own profile.
use super::*;

impl FriendsClient {
    pub(in crate::game_lua::platform_services) fn has_access_token(&self) -> bool {
        self.is_current() && !self.session.level2_tokens().access_token.is_empty()
    }

    pub(in crate::game_lua::platform_services) fn current_social_user(
        &self,
    ) -> Option<(String, serde_json::Value)> {
        if !self.context_is_current() {
            return None;
        }
        self.session.profile().map(|p| (p.public_account_id, p.raw))
    }

    pub(in crate::game_lua::platform_services) fn linked_platform_id(
        &self,
        network: SocialNetwork,
    ) -> Option<String> {
        if !self.is_current() {
            return None;
        }
        let provider = provider_name(network);
        self.session
            .profile()?
            .social_networks
            .iter()
            .find(|p| p["provider"].as_str() == Some(provider))
            .and_then(|p| p["id"].as_str())
            .filter(|id| !id.is_empty())
            .map(str::to_owned)
    }

    pub(in crate::game_lua::platform_services) fn connect_platform(
        &mut self,
        network: SocialNetwork,
        profile: &SocialPlatformProfile,
        ids: &[String],
        active: &Mutex<bool>,
    ) -> Result<(), String> {
        let current = || *active.lock().expect("platform lease lock poisoned");
        let check = || {
            if current() {
                Ok(())
            } else {
                Err("platform social request was cancelled".to_owned())
            }
        };
        check()?;
        let body = serde_json::to_vec(&connection_body(network, profile))
            .map_err(|_| "social authentication encoding failed")?;
        // 725794 and726F10 add no exact200 test after the common2xx executor.
        self.session
            .execute_platform_request(
                &self.config,
                &mut self.owner,
                "external/connect",
                ("application/json", &body),
                &current,
            )
            .map_err(|e| e.to_string())?;
        if !ids.is_empty() {
            check()?;
            let mut fields = ids
                .iter()
                .map(|id| format!("networkId={}", form_component(id)))
                .collect::<Vec<_>>();
            fields.push(format!("networkProvider={}", provider_name(network)));
            let body = fields.join("&");
            self.session
                .execute_platform_request(
                    &self.config,
                    &mut self.owner,
                    "friends",
                    ("application/x-www-form-urlencoded", body.as_bytes()),
                    &current,
                )
                .map_err(|e| e.to_string())?;
        }
        check()?;
        let access = self
            .session
            .platform_profile_access(&self.config, &mut self.owner)
            .map_err(|e| e.to_string())?;
        let mut owner = self
            .session
            .own_profile_owner_for_request(self.owner)
            .ok_or("identity request cancelled")?;
        let before = self
            .session
            .login_profile_identity(owner)
            .ok_or("identity request cancelled")?;
        let response = agent()
            .get(self.config.endpoint.request_url("profile/own"))
            .header("X-Access-Token", access)
            .call()
            .map_err(|_| "identity profile transport error")?;
        let profile = session::parse_profile_response(response).map_err(|e| e.to_string())?;
        check()?;
        // This refresh has no flat-token continuation, UUID rotation or SDK
        // session-success publication. It still publishes profile before assets.
        {
            let active = active.lock().map_err(|_| "platform lease lock poisoned")?;
            if !*active {
                return Err("platform social request was cancelled".to_owned());
            }
            self.session
                .prepare_login_profile(&mut owner, &profile, before)
                .map_err(|e| e.to_string())?
                .ok_or("identity request cancelled")?;
        }
        self.owner = owner.request_owner();
        self.session
            .fetch_platform_avatar_assets(owner, &profile.avatar_assets, active)?;
        check()?;
        if !self.is_current() {
            return Err("identity request cancelled".to_owned());
        }
        Ok(())
    }
}

fn provider_name(network: SocialNetwork) -> &'static str {
    match network {
        SocialNetwork::Facebook => "facebook",
        SocialNetwork::SinaWeibo => "sinaweibo",
        SocialNetwork::GameCenter => "gamecenter",
        SocialNetwork::KakaoTalk => "kakaotalk",
    }
}

// 100724894 appends only nonempty attributes. Username/customParams do not
// belong to the authentication document. The separate platform token is used.
fn connection_body(network: SocialNetwork, profile: &SocialPlatformProfile) -> serde_json::Value {
    let attrs: serde_json::Map<String, serde_json::Value> = [
        ("accessToken", &profile.access_token),
        ("clientId", &profile.client_id),
        ("userId", &profile.user.id),
        ("name", &profile.user.name),
        ("avatarUrl", &profile.user.avatar_url),
    ]
    .into_iter()
    .filter(|(_, value)| !value.is_empty())
    .map(|(key, value)| (key.to_owned(), serde_json::Value::String(value.clone())))
    .collect();
    serde_json::json!({"provider":provider_name(network),"externalAttributes":attrs})
}
