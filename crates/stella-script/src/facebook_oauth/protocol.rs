use super::*;

pub(super) fn validate_config(config: &FacebookOAuthConfig) -> Result<(), SocialPlatformError> {
    for root in [&config.graph_root, &config.authorization_url]
        .into_iter()
        .chain(config.rest_root.iter())
    {
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
    }
    if config.app_id.is_empty()
        || !config.app_id.bytes().all(|b| b.is_ascii_digit())
        || !config
            .url_scheme_suffix
            .bytes()
            .all(|b| b.is_ascii_alphanumeric())
    {
        return Err(SocialPlatformError::InvalidConfiguration);
    }
    Ok(())
}

pub(super) fn permissions(config: &FacebookOAuthConfig) -> Vec<String> {
    let mut permissions = vec![
        "public_profile".into(),
        "email".into(),
        "user_friends".into(),
    ];
    if config.request_birthday {
        permissions.push("user_birthday".into());
    }
    permissions
}

/// FBSession31EA7C retains previous denials not mentioned by this response,
/// removes newly granted names, and never adds the two implicit permissions.
pub(super) fn update_declined(
    declined: &mut Vec<String>,
    requested: &[String],
    granted: &[String],
) {
    declined.retain(|permission| !granted.contains(permission));
    for permission in requested {
        if !granted.contains(permission)
            && !declined.contains(permission)
            && !matches!(permission.as_str(), "basic_info" | "public_profile")
        {
            declined.push(permission.clone());
        }
    }
}

fn app_base_url(config: &FacebookOAuthConfig) -> String {
    format!(
        "fb{}{}://authorize",
        config.app_id, config.url_scheme_suffix
    )
}

pub(super) fn authorization_url(
    config: &FacebookOAuthConfig,
) -> Result<(String, String), SocialPlatformError> {
    let mut random = [0_u8; 16];
    getrandom::fill(&mut random).map_err(|_| SocialPlatformError::Transport)?;
    random[6] = (random[6] & 0x0f) | 0x40;
    random[8] = (random[8] & 0x3f) | 0x80;
    let hex: String = random.iter().map(|b| format!("{b:02X}")).collect();
    let logger_id = format!(
        "{}-{}-{}-{}-{}",
        &hex[..8],
        &hex[8..12],
        &hex[12..16],
        &hex[16..20],
        &hex[20..]
    );
    let url = retry_authorization_url(config, &logger_id)?;
    Ok((url, logger_id))
}

pub(super) fn retry_authorization_url(
    config: &FacebookOAuthConfig,
    logger_id: &str,
) -> Result<String, SocialPlatformError> {
    Ok(authorization_url_with_context(
        config,
        logger_id,
        current_time_ms()?,
        Route::Browser,
    ))
}

pub(super) fn application_authorization_url(
    config: &FacebookOAuthConfig,
    logger_id: &str,
) -> Result<String, SocialPlatformError> {
    Ok(authorization_url_with_context(
        config,
        logger_id,
        current_time_ms()?,
        Route::Application,
    ))
}

pub(super) fn dialog_authorization_url(
    config: &FacebookOAuthConfig,
    logger_id: &str,
    endpoint: &str,
) -> Result<String, SocialPlatformError> {
    Ok(authorization_url_with_context(
        config,
        logger_id,
        current_time_ms()?,
        Route::Dialog(endpoint),
    ))
}
#[derive(Clone, Copy)]
enum Route<'a> {
    Browser,
    Application,
    Dialog(&'a str),
}

fn current_time_ms() -> Result<u64, SocialPlatformError> {
    Ok((SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| SocialPlatformError::Transport)?
        .as_secs_f64()
        * 1000.0)
        .round() as u64)
}

fn authorization_url_with_context(
    config: &FacebookOAuthConfig,
    logger_id: &str,
    time: u64,
    route: Route<'_>,
) -> String {
    let state = serde_json::json!({
        "com.facebook.sdk_client_state": true, "is_open_session": true,
        "is_active_session": true, "0_auth_logger_id": logger_id,
        "3_method": match route { Route::Application => "fb_application_web_auth", Route::Browser => "browser_auth", Route::Dialog(_) => "fallback_auth" }
    });
    let mut params = BTreeMap::from([
        ("client_id", config.app_id.clone()),
        ("response_type", "token".into()),
        (
            "redirect_uri",
            if matches!(route, Route::Browser) {
                app_base_url(config)
            } else {
                "fbconnect://success".into()
            },
        ),
        ("display", "touch".into()),
        ("sdk", "ios".into()),
        ("return_scopes", "true".into()),
        ("sdk_version", "3.14.1".into()),
        ("legacy_override", "v2.0".into()),
        ("scope", permissions(config).join(",")),
        ("state", state.to_string()),
        ("e2e", serde_json::json!({"init": time}).to_string()),
    ]);
    if !config.url_scheme_suffix.is_empty() {
        params.insert("local_client_id", config.url_scheme_suffix.clone());
    }
    format!(
        "{}?{}",
        match route {
            Route::Application => {
                if config.url_scheme_suffix.is_empty() {
                    "fbauth://authorize"
                } else {
                    "fbauth2://authorize"
                }
            }
            Route::Browser => &config.authorization_url,
            Route::Dialog(endpoint) => endpoint,
        },
        params
            .iter()
            .map(|(key, value)| format!("{}={}", encode_query(key), encode_query(value)))
            .collect::<Vec<_>>()
            .join("&")
    )
}

pub(super) fn callback_params(
    config: &FacebookOAuthConfig,
    url: &str,
) -> Result<Option<BTreeMap<String, String>>, SocialPlatformError> {
    // Native2FE308 uses absoluteString.hasPrefix(appBaseUrl), not a modern
    // authorization-code/PKCE exchange. Query2F7E90 precedes fragment so the
    // fragment wins duplicate names. These callbacks never perform HTTP.
    if !url.starts_with(&app_base_url(config)) {
        return Ok(None);
    }
    url_params(url).map(Some)
}

pub(super) fn url_params(url: &str) -> Result<BTreeMap<String, String>, SocialPlatformError> {
    let (base, fragment) = url
        .split_once('#')
        .map_or((url, None), |(a, b)| (a, Some(b)));
    let mut params = BTreeMap::new();
    if let Some((_, query)) = base.split_once('?') {
        parse_part(&mut params, query)?;
    }
    if let Some(fragment) = fragment {
        parse_part(&mut params, fragment)?;
    }
    Ok(params)
}

fn parse_part(
    params: &mut BTreeMap<String, String>,
    text: &str,
) -> Result<(), SocialPlatformError> {
    for pair in text.split('&').filter(|pair| !pair.is_empty()) {
        let (key, value) = pair.split_once('=').unwrap_or((pair, ""));
        params.insert(decode(key)?, decode(value)?);
    }
    Ok(())
}

fn decode(text: &str) -> Result<String, SocialPlatformError> {
    percent_decode(text, true)
}

pub(super) fn percent_decode(
    text: &str,
    plus_as_space: bool,
) -> Result<String, SocialPlatformError> {
    let mut output = Vec::with_capacity(text.len());
    let mut bytes = text.bytes();
    while let Some(byte) = bytes.next() {
        output.push(match byte {
            b'+' if plus_as_space => b' ',
            b'%' => {
                let high = bytes.next().and_then(|b| char::from(b).to_digit(16));
                let low = bytes.next().and_then(|b| char::from(b).to_digit(16));
                let (Some(high), Some(low)) = (high, low) else {
                    return Err(SocialPlatformError::InvalidResponse);
                };
                ((high << 4) | low) as u8
            }
            byte => byte,
        });
    }
    String::from_utf8(output).map_err(|_| SocialPlatformError::InvalidResponse)
}
