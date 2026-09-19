//! Native session JSON contracts, without credential-bearing diagnostic text.

use super::{AccessResponse, ProfileResponse, SessionError, Tokens};
use serde_json::{Map, Value};
use std::{
    collections::BTreeMap,
    io::Read,
    time::{SystemTime, UNIX_EPOCH},
};
use ureq::{Body, http::Response};

pub(super) struct ParsedSession {
    pub(super) tokens: Tokens,
    pub(super) profile: ProfileResponse,
    pub(super) config: BTreeMap<String, String>,
}

pub(super) fn unix_seconds() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs() as i64)
        .unwrap_or_else(|before| -(before.duration().as_secs() as i64))
}

fn read_native_200(response: Response<Body>) -> Result<Value, SessionError> {
    // Both 100745440 (app sessions) and 10067CA34 (Level1 access) require
    // exactly 200, unlike the common authorized request wrapper's full 2xx.
    if response.status() != 200 {
        return Err(SessionError::http(response.status().as_u16()));
    }
    read_body(response)
}

fn read_body(mut response: Response<Body>) -> Result<Value, SessionError> {
    // Host resource boundary, not a recovered native protocol size limit.
    // An untrusted opt-in provider may not allocate an unbounded JSON body.
    const MAX_BODY: u64 = 8 * 1024 * 1024;
    let mut body = Vec::new();
    response
        .body_mut()
        .as_reader()
        .take(MAX_BODY + 1)
        .read_to_end(&mut body)
        .map_err(|_| SessionError::transport())?;
    if body.len() as u64 > MAX_BODY {
        return Err(SessionError::transport());
    }
    serde_json::from_slice(&body).map_err(|_| SessionError::transport())
}

pub(in super::super) fn parse_access_response(
    response: Response<Body>,
) -> Result<AccessResponse, SessionError> {
    // login/register/upgrade have already used the common authorized wrapper;
    // its accepted 2xx range is not the Level1-acquisition exact-200 contract.
    if !response.status().is_success() {
        return Err(SessionError::http(response.status().as_u16()));
    }
    access_value(&read_body(response)?, unix_seconds())
}

fn access_value(json: &Value, now: i64) -> Result<AccessResponse, SessionError> {
    let object = object(json)?;
    let access_token = string(object, "accessToken")?;
    let refresh_token = string(object, "refreshToken")?;
    if access_token.is_empty() || refresh_token.is_empty() {
        return Err(SessionError::transport());
    }
    let seconds = signed_number(required(object, "expiresIn")?)? as i32;
    Ok(AccessResponse {
        access_token,
        refresh_token,
        absolute_expiry: if seconds > 0 {
            now.wrapping_add(i64::from(seconds))
        } else {
            0
        },
        segment: object
            .get("segment")
            .and_then(Value::as_str)
            .map(str::to_owned),
    })
}

pub(super) fn parse_flat_tokens(response: Response<Body>) -> Result<Tokens, SessionError> {
    flat_tokens(&read_native_200(response)?, unix_seconds())
}

pub(super) fn parse_session(response: Response<Body>) -> Result<ParsedSession, SessionError> {
    session(&read_native_200(response)?, unix_seconds())
}

pub(in super::super) fn parse_profile_response(
    response: Response<Body>,
) -> Result<ProfileResponse, SessionError> {
    // 100672054 is a direct own-profile GET with its own exact-200 check,
    // not the common GET wrapper and not an additional 401 renewal loop.
    Ok(parse_profile_value(&read_native_200(response)?))
}

pub(in super::super) fn parse_profile_value(value: &Value) -> ProfileResponse {
    // 100675F9C's predicates are typed checks, not mere key existence checks.
    // A null/scalar/missing profile is the native default-empty UserProfile.
    // 100676088 and 100676098 fill the SAME map: personal overrides abid.
    let mut personal = BTreeMap::<String, String>::new();
    for group in ["abid", "personal"] {
        if let Some(values) = value.get(group).and_then(Value::as_object) {
            for (key, value) in values {
                let text = match value {
                    Value::String(value) => value.clone(),
                    Value::Bool(value) => value.to_string(),
                    _ => String::new(),
                };
                personal.insert(key.clone(), text);
            }
        }
    }
    let mut social_networks: Vec<Value> = value
        .get("socialNetworks")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter(|entry| {
            entry.get("provider").is_some_and(Value::is_string)
                && entry.get("id").is_some_and(Value::is_string)
        })
        .cloned()
        .collect();
    // Only the first external item selects the active profile. Provider and
    // id have separate typed checks; the default is (0, ""), which may itself
    // match an unknown-provider/empty-id social entry. No extra validity gate.
    let active = value
        .get("externalNetworks")
        .and_then(Value::as_array)
        .and_then(|entries| entries.first());
    let provider = active
        .and_then(|entry| entry.get("provider"))
        .and_then(Value::as_str)
        .unwrap_or_default();
    let active_external_id = active
        .and_then(|entry| entry.get("id"))
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_owned();
    let provider_id = social_provider(provider);
    let matched = social_networks.iter().find(|entry| {
        social_provider(entry["provider"].as_str().unwrap()) == provider_id
            && entry["id"].as_str().unwrap() == active_external_id
    });
    let mut connected_to_social_network = matched.is_some();
    let active_social_name = matched
        .and_then(|entry| entry.get("socialAttributes"))
        .and_then(|attributes| attributes.get("name"))
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_owned();
    // 100689ECC appends a selected known/nonempty external profile only when
    // its pair is absent. It has no name of its own: external names are not
    // decoded, and matches always take the first social entry's name.
    if !connected_to_social_network && provider_id != 0 && !active_external_id.is_empty() {
        social_networks.push(serde_json::json!({"provider":provider,"id":active_external_id}));
        connected_to_social_network = true;
    }
    ProfileResponse {
        raw: value.clone(),
        avatar_assets: super::parse_avatar_assets(value),
        avatar_paths: BTreeMap::new(),
        public_account_id: value
            .get("publicAccountId")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned(),
        personal: super::super::PersonalProfile {
            nickname: personal.remove("nickName").unwrap_or_default(),
            email: personal.remove("email").unwrap_or_default(),
        },
        social_networks,
        active_external_id,
        active_social_network: match provider_id {
            1 => Some(crate::SocialNetwork::Facebook),
            2 => Some(crate::SocialNetwork::SinaWeibo),
            3 => Some(crate::SocialNetwork::GameCenter),
            4 => Some(crate::SocialNetwork::KakaoTalk),
            _ => None,
        },
        active_social_name,
        connected_to_social_network,
    }
}

fn social_provider(provider: &str) -> u8 {
    match provider {
        "facebook" => 1,
        "sinaweibo" => 2,
        "gamecenter" => 3,
        "kakaotalk" => 4,
        _ => 0,
    }
}

fn object(value: &Value) -> Result<&Map<String, Value>, SessionError> {
    value.as_object().ok_or_else(SessionError::transport)
}
fn required<'a>(object: &'a Map<String, Value>, key: &str) -> Result<&'a Value, SessionError> {
    object.get(key).ok_or_else(SessionError::transport)
}
fn string(object: &Map<String, Value>, key: &str) -> Result<String, SessionError> {
    required(object, key)?
        .as_str()
        .map(str::to_owned)
        .ok_or_else(SessionError::transport)
}

/// JSON Number's signed integer slot: parser i64 stays exact; floating values
/// go through FCVTZS X,D before the consumers narrow to W or reinterpret u64.
pub(in super::super) fn signed_number(value: &Value) -> Result<i64, SessionError> {
    let number = value.as_number().ok_or_else(SessionError::transport)?;
    if let Some(value) = number.as_i64() {
        return Ok(value);
    }
    if number.as_u64().is_some() {
        return Err(SessionError::transport());
    }
    number
        .as_f64()
        .map(|value| value as i64)
        .ok_or_else(SessionError::transport)
}

pub(super) fn flat_tokens(json: &Value, now: i64) -> Result<Tokens, SessionError> {
    Ok(Tokens::from_flat(&access_value(json, now)?))
}

pub(super) fn session(json: &Value, now: i64) -> Result<ParsedSession, SessionError> {
    let root = object(json)?;
    let auth = object(required(root, "userAuth")?)?;
    let segments = required(root, "segments")?
        .as_array()
        .ok_or_else(SessionError::transport)?
        .iter()
        .map(|value| signed_number(value).map(|number| number.to_string()))
        .collect::<Result<Vec<_>, _>>()?
        .join(", ");
    let seconds = signed_number(required(auth, "expiresIn")?)? as i32;
    let tokens = Tokens {
        access_token: string(auth, "accessToken")?,
        refresh_token: string(auth, "refreshToken")?,
        segment: segments,
        // Unlike the flat constructor, nested session expiry has no <=0
        // sentinel branch: 100689584..100689594 adds the signed low W word.
        absolute_expiry: now.wrapping_add(i64::from(seconds)),
    };
    let profile = parse_profile_value(root.get("profile").unwrap_or(&Value::Null));
    let config = parse_config(required(root, "config")?)?;
    Ok(ParsedSession {
        tokens,
        profile,
        config,
    })
}

fn parse_config(json: &Value) -> Result<BTreeMap<String, String>, SessionError> {
    // 100662198 builds a temporary scalar-string map and then replaces the
    // whole config. Number's integer bits are printed unsigned by 100691B44.
    object(json)?
        .iter()
        .map(|(key, value)| {
            let text = match value {
                Value::Bool(value) => value.to_string(),
                Value::String(value) => value.clone(),
                Value::Number(_) => (signed_number(value)? as u64).to_string(),
                _ => return Err(SessionError::transport()),
            };
            Ok((key.clone(), text))
        })
        .collect()
}
