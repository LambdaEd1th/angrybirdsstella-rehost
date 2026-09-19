//! Recovered FacebookService Graph requests for an explicitly supplied session.
//! This does not obtain credentials or synthesize an OAuth login. See the
//! native social-platform audit for SDK/session features still outstanding.

use crate::{
    SocialFriendDetails, SocialPlatformError, SocialPlatformFriends, SocialPlatformProfile,
    SocialPlatformProvider, SocialPlatformUser,
};
use serde_json::Value;
use std::{
    fmt::{self, Write as _},
    io::Read,
    sync::{Arc, Mutex},
    time::Duration,
};

pub(crate) mod wire;

pub(crate) type GraphTransport = dyn Fn(&str, &str, &crate::SocialPlatformRequestOwner) -> Result<Value, SocialPlatformError>
    + Send
    + Sync;

const MAX_RESPONSE_BYTES: u64 = 8 * 1024 * 1024;

/// An authenticated session supplied by the embedding host. `graph_root` is an
/// explicit Graph origin/version root, e.g. a controlled replacement's `/v2.0`.
/// Credentials stay in memory; requests never borrow Skynest's access token.
pub struct FacebookGraphSession {
    graph_root: String,
    state: Mutex<GraphSessionState>,
    owner: crate::SocialPlatformRequestOwner,
    transport: Option<Arc<GraphTransport>>,
}

struct GraphSessionState {
    access_token: String,
    open: bool,
    profile: Option<SocialPlatformProfile>,
}

impl fmt::Debug for FacebookGraphSession {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("FacebookGraphSession")
    }
}

impl FacebookGraphSession {
    pub fn new(graph_root: &str, access_token: &str) -> Result<Self, SocialPlatformError> {
        let root = graph_root.trim().trim_end_matches('/');
        let uri: ureq::http::Uri = root
            .parse()
            .map_err(|_| SocialPlatformError::InvalidConfiguration)?;
        if !matches!(uri.scheme_str(), Some("http" | "https"))
            || uri
                .authority()
                .is_none_or(|a| a.host().is_empty() || a.as_str().contains('@'))
            || uri.query().is_some()
            || root.contains(['#', '\\'])
            || root.chars().any(char::is_control)
        {
            return Err(SocialPlatformError::InvalidConfiguration);
        }
        if access_token.is_empty() {
            return Err(SocialPlatformError::NotLoggedIn);
        }
        Ok(Self {
            graph_root: root.to_owned(),
            owner: Default::default(),
            transport: None,
            state: Mutex::new(GraphSessionState {
                access_token: access_token.to_owned(),
                open: true,
                profile: None,
            }),
        })
    }

    pub(crate) fn with_transport(mut self, transport: Arc<GraphTransport>) -> Self {
        self.transport = Some(transport);
        self
    }

    pub(crate) fn owns_request(&self, owner: &crate::SocialPlatformRequestOwner) -> bool {
        self.owner.matches(owner)
    }

    pub(crate) fn replace_token(&self, token: &str) {
        let mut state = self.state.lock().expect("Graph session lock poisoned");
        state.access_token = token.to_owned();
        if let Some(profile) = &mut state.profile {
            profile.access_token = token.to_owned();
        }
    }

    pub fn close(&self) {
        self.state.lock().expect("Graph session lock poisoned").open = false;
    }

    pub(crate) fn current_token(&self) -> Result<String, SocialPlatformError> {
        let state = self
            .state
            .lock()
            .map_err(|_| SocialPlatformError::Transport)?;
        if !state.open {
            return Err(SocialPlatformError::NotLoggedIn);
        }
        Ok(state.access_token.clone())
    }

    #[cfg(test)]
    pub(crate) fn has_access_token(&self, token: &str) -> bool {
        self.state
            .lock()
            .is_ok_and(|state| state.open && state.access_token == token)
    }

    pub(crate) fn cached_user(&self) -> Option<SocialPlatformUser> {
        // The FacebookService owns this cache independently of FBSession.
        // A closed session's cached user survives until an open logout clears it.
        self.state
            .lock()
            .expect("Graph session lock poisoned")
            .profile
            .as_ref()
            .map(|profile| profile.user.clone())
    }

    fn request(&self, graph_path: &str) -> Result<Value, SocialPlatformError> {
        let token = self.current_token()?;
        // 1002DA720 adds format/json, sdk/ios and the platform token. The native
        // path is appended verbatim to the root's slash: getFriends passes
        // /me/friends, while requestForMe passes me. Do not collapse the former
        // double slash or change the token's %20 encoding into form '+' syntax.
        let url = format!(
            "{}/{graph_path}?format=json&sdk=ios&access_token={}",
            self.graph_root,
            encode_query(&token)
        );
        let response = if let Some(transport) = &self.transport {
            transport(graph_path, &token, &self.owner)
        } else {
            wire::send(&url, None).and_then(wire::Response::into_result)
        };
        if !self.is_logged_in() {
            return Err(SocialPlatformError::Cancelled);
        }
        response
    }
}

impl SocialPlatformProvider for FacebookGraphSession {
    fn prepare_login(self: std::sync::Arc<Self>) -> crate::SocialLoginRequest {
        // This object represents a host-supplied token, not an OAuth launcher.
        // A closed token session cannot manufacture a fresh platform login.
        crate::SocialLoginRequest::Ready(if self.is_logged_in() {
            Ok(())
        } else {
            Err(SocialPlatformError::NotLoggedIn)
        })
    }

    fn is_logged_in(&self) -> bool {
        self.state.lock().expect("Graph session lock poisoned").open
    }

    fn logout(&self) -> Result<(), SocialPlatformError> {
        // FacebookService779720 only clears an open session. A separately
        // closed session is a no-op here, including its completed cache.
        let mut state = self
            .state
            .lock()
            .map_err(|_| SocialPlatformError::Transport)?;
        if state.open {
            state.open = false;
            state.access_token.clear();
            state.profile = None;
        }
        Ok(())
    }

    fn user_profile(&self) -> Result<SocialPlatformProfile, SocialPlatformError> {
        let state = self
            .state
            .lock()
            .map_err(|_| SocialPlatformError::Transport)?;
        if !state.open {
            return Err(SocialPlatformError::NotLoggedIn);
        }
        if let Some(profile) = &state.profile {
            return Ok(profile.clone());
        }
        // 100779840 has no in-flight waiter list: concurrent misses send me.
        drop(state);
        let profile = self.fetch_profile()?;
        self.publish_user_profile(&profile)?;
        Ok(profile)
    }

    fn prepare_user_profile(self: std::sync::Arc<Self>) -> crate::SocialProfileRequest {
        let cached = self
            .state
            .lock()
            .map_err(|_| SocialPlatformError::Transport)
            .and_then(|state| {
                if state.open {
                    Ok(state.profile.clone())
                } else {
                    Err(SocialPlatformError::NotLoggedIn)
                }
            });
        match cached {
            Ok(Some(profile)) => crate::SocialProfileRequest::Ready(Ok(profile)),
            Err(error) => crate::SocialProfileRequest::Ready(Err(error)),
            Ok(None) => {
                crate::SocialProfileRequest::Pending(Box::new(move || self.fetch_profile()))
            }
        }
    }

    fn publish_user_profile(
        &self,
        profile: &SocialPlatformProfile,
    ) -> Result<(), SocialPlatformError> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| SocialPlatformError::Transport)?;
        // Check and write under one lock so logout cannot be undone by a
        // completion that observed open before token/profile clearing.
        if !state.open {
            return Err(SocialPlatformError::Cancelled);
        }
        let mut profile = profile.clone();
        profile.request_owner = Some(self.owner.clone());
        state.profile = Some(profile);
        Ok(())
    }

    fn friends(
        &self,
        details: SocialFriendDetails,
    ) -> Result<SocialPlatformFriends, SocialPlatformError> {
        parse_friends(&self.request("/me/friends")?, details)
    }
}

impl FacebookGraphSession {
    fn fetch_profile(&self) -> Result<SocialPlatformProfile, SocialPlatformError> {
        let value = self.request("me")?;
        if !value.is_object() {
            return Err(SocialPlatformError::InvalidResponse);
        }
        let id = optional_string(&value, "id")?;
        // 77998C constructs NSArray(name), which raises for nil. Represent
        // malformed native input as an explicit host error, never empty success.
        let name = optional_string(&value, "name")?.ok_or(SocialPlatformError::InvalidResponse)?;
        let user = SocialPlatformUser {
            id: c_string(id.unwrap_or_default()),
            username: c_string(optional_string(&value, "username")?.unwrap_or_default()),
            name: c_string(name),
            avatar_url: id
                .map(|id| {
                    c_string(&format!(
                        "https://graph.facebook.com/{id}/picture?type=large"
                    ))
                })
                .unwrap_or_default(),
            ..Default::default()
        };
        Ok(SocialPlatformProfile {
            user,
            access_token: self.current_token()?,
            client_id: String::new(),
            request_owner: Some(self.owner.clone()),
        })
    }
}

pub(crate) fn encode_query(value: &str) -> String {
    let mut output = String::new();
    for byte in value.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~') {
            output.push(char::from(byte));
        } else {
            write!(output, "%{byte:02X}").expect("String formatting");
        }
    }
    output
}

fn optional_string<'a>(
    value: &'a Value,
    key: &str,
) -> Result<Option<&'a str>, SocialPlatformError> {
    match value.get(key) {
        None => Ok(None),
        Some(Value::String(text)) => Ok(Some(text)),
        Some(_) => Err(SocialPlatformError::InvalidResponse),
    }
}

// Native SocialServiceUtils100787634 copies UTF8String using strlen.
fn c_string(value: &str) -> String {
    value.split('\0').next().unwrap_or_default().to_owned()
}

fn parse_friends(
    value: &Value,
    details: SocialFriendDetails,
) -> Result<SocialPlatformFriends, SocialPlatformError> {
    // 77B1DC sends data/count/enumeration to nil and publishes an empty
    // successful result. The wire layer uses Null for that absent graph object.
    if !value.is_object() && !value.is_null() {
        return Err(SocialPlatformError::InvalidResponse);
    }
    let data = match value.get("data") {
        None => &[][..],
        Some(Value::Array(values)) => values,
        // An empty NSDictionary responds to count/enumeration without yielding
        // a non-user key. Nonempty dictionaries fail the subsequent user getter.
        Some(Value::Object(values)) if values.is_empty() => &[],
        Some(_) => return Err(SocialPlatformError::InvalidResponse),
    };
    let mut users = Vec::with_capacity(data.len());
    for value in data {
        if !value.is_object() {
            return Err(SocialPlatformError::InvalidResponse);
        }
        let id = optional_string(value, "id")?;
        let mut user = SocialPlatformUser {
            id: c_string(id.unwrap_or_default()),
            ..Default::default()
        };
        if details == SocialFriendDetails::Profiles {
            user.name = c_string(optional_string(value, "name")?.unwrap_or_default());
            user.username = c_string(optional_string(value, "username")?.unwrap_or_default());
            user.avatar_url = c_string(&format!(
                "https://graph.facebook.com/{}/picture?type=normal",
                id.unwrap_or("(null)")
            ));
        }
        users.push(user);
    }
    let next_page = if users.len() >= 5000 {
        match value.get("paging") {
            None => String::new(),
            Some(value) if value.is_object() => {
                c_string(optional_string(value, "next")?.unwrap_or_default())
            }
            Some(_) => return Err(SocialPlatformError::InvalidResponse),
        }
    } else {
        String::new()
    };
    Ok(SocialPlatformFriends { users, next_page })
}

#[cfg(test)]
pub(crate) mod test_wire;
#[cfg(test)]
mod tests;
