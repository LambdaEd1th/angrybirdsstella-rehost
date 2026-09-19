//! FBSession319114 cache admission and FBUtility2FE618 response expiration.
use std::collections::BTreeMap;

mod dictionary;
mod error;
mod property_list;
mod store;
pub use error::FacebookTokenCacheError;
pub use store::FacebookTokenCache;
#[cfg(test)]
mod storage_tests;

#[derive(Clone)]
pub(super) struct CachedToken {
    pub token: String,
    permissions: Option<Vec<property_list::Value>>,
    expires_at: f64,
    login_type: i32,
    refresh_date: property_list::Value,
    permissions_refresh_date: property_list::Value,
}

impl CachedToken {
    pub(super) fn is_system_account(&self) -> bool {
        self.login_type == 1
    }

    pub(super) fn should_extend(&self, now: f64) -> Result<bool, crate::SocialPlatformError> {
        Ok(matches!(self.login_type, 1..=3)
            && now
                - self
                    .refresh_date
                    .unix_date()
                    .ok_or(crate::SocialPlatformError::InvalidResponse)?
                > 86400.0)
    }

    pub(super) fn should_refresh_permissions(
        &self,
        now: f64,
    ) -> Result<bool, crate::SocialPlatformError> {
        Ok(now
            - self
                .permissions_refresh_date
                .unix_date()
                .ok_or(crate::SocialPlatformError::InvalidResponse)?
            > 86400.0)
    }

    pub(super) fn extend(&mut self, token: Option<&str>, expiry: f64, now: f64) {
        if let Some(token) = token {
            self.token = token.to_owned();
        }
        self.expires_at = expiry;
        self.refresh_date = property_list::Value::date(now);
        self.login_type = 0; // 31D9AC deliberately resets the login type.
    }

    pub(super) fn refresh_permissions(&mut self, permissions: Vec<String>, now: f64) {
        self.permissions = Some(
            permissions
                .into_iter()
                .map(property_list::Value::string)
                .collect(),
        );
        self.permissions_refresh_date = property_list::Value::date(now);
    }
    pub(super) fn from_response(
        token: String,
        permissions: Vec<String>,
        params: &BTreeMap<String, String>,
        login_type: i32,
        now: f64,
    ) -> Self {
        Self {
            token,
            permissions: Some(
                permissions
                    .into_iter()
                    .map(property_list::Value::string)
                    .collect(),
            ),
            expires_at: expiration(params, now),
            login_type,
            refresh_date: property_list::Value::date(now),
            permissions_refresh_date: property_list::Value::date(-62_135_769_600.0),
        }
    }

    pub(super) fn granted_permissions(&self) -> Vec<String> {
        self.permissions
            .as_deref()
            .unwrap_or_default()
            .iter()
            .filter_map(|value| value.as_string().map(str::to_owned))
            .collect()
    }

    pub fn admits(&self, requested: &[String], now: f64) -> bool {
        self.expires_at > now
            && requested.iter().all(|permission| {
                self.permissions
                    .as_deref()
                    .unwrap_or_default()
                    .iter()
                    .any(|value| value.as_string() == Some(permission.as_str()))
            })
    }
}

pub(super) fn now() -> f64 {
    super::SystemTime::now()
        .duration_since(super::UNIX_EPOCH)
        .map_or_else(|e| -e.duration().as_secs_f64(), |d| d.as_secs_f64())
}

pub(super) fn expiration(params: &BTreeMap<String, String>, now: f64) -> f64 {
    let date = if let Some(value) = params.get("expires_in") {
        // NSString.intValue consumes a signed decimal prefix and saturates.
        let value = numeric_prefix(value, false).clamp(i32::MIN as f64, i32::MAX as f64);
        (value != 0.0).then_some(now + value)
    } else {
        let value = params
            .get("expires")
            .map_or(0.0, |v| numeric_prefix(v, true));
        (value != 0.0).then_some(value)
    };
    date.unwrap_or(64_092_211_200.0) // NSDate.distantFuture
}

pub(super) fn numeric_prefix(value: &str, floating: bool) -> f64 {
    let value = value.trim_start_matches(is_ns_whitespace);
    let bytes = value.as_bytes();
    let mut end = usize::from(matches!(bytes.first(), Some(b'+' | b'-')));
    let start = end;
    while bytes.get(end).is_some_and(u8::is_ascii_digit) {
        end += 1;
    }
    let mut digits = end - start;
    if floating && bytes.get(end) == Some(&b'.') {
        end += 1;
        let start = end;
        while bytes.get(end).is_some_and(u8::is_ascii_digit) {
            end += 1;
        }
        digits += end - start;
    }
    if digits == 0 {
        return 0.0;
    }
    if floating && matches!(bytes.get(end), Some(b'e' | b'E')) {
        let exponent = end;
        end += 1;
        if matches!(bytes.get(end), Some(b'+' | b'-')) {
            end += 1;
        }
        let start = end;
        while bytes.get(end).is_some_and(u8::is_ascii_digit) {
            end += 1;
        }
        if start == end {
            end = exponent;
        }
    }
    value[..end].parse().unwrap_or(0.0)
}

// NSCharacterSet.whitespaceCharacterSet includes U+200B but excludes line
// separators. Rust's char::is_whitespace has different membership.
fn is_ns_whitespace(c: char) -> bool {
    matches!(
        c,
        '\t' | ' ' | '\u{a0}' | '\u{1680}' | '\u{2000}'
            ..='\u{200b}' | '\u{202f}' | '\u{205f}' | '\u{3000}'
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn facebook_oauth_expiration_matches_native_precedence_and_numeric_prefixes() {
        for (relative, absolute, expected) in [
            (None, None, 64_092_211_200.0),
            (Some(""), Some("123"), 64_092_211_200.0),
            (Some("0"), Some("123"), 64_092_211_200.0),
            (Some("120s"), None, 1120.0),
            (Some("1.5"), None, 1001.0),
            (Some("1e3"), None, 1001.0),
            (Some("2147483648"), None, 2147484647.0),
            (Some("-2147483649"), None, -2147482648.0),
            (None, Some("1.5suffix"), 1.5),
            (None, Some("1e3"), 1000.0),
            (None, Some("-1"), -1.0),
            (Some("\n12"), None, 64_092_211_200.0),
            (None, Some("\u{200b}12.5"), 12.5),
        ] {
            let mut params = BTreeMap::new();
            if let Some(value) = relative {
                params.insert("expires_in".into(), value.into());
            }
            if let Some(value) = absolute {
                params.insert("expires".into(), value.into());
            }
            assert_eq!(expiration(&params, 1000.0), expected, "{params:?}");
        }
    }

    #[test]
    fn facebook_oauth_cache_requires_strict_future_and_permission_subset() {
        let token = CachedToken {
            token: "synthetic".into(),
            permissions: Some(vec![property_list::Value::string("email")]),
            expires_at: 1000.0,
            login_type: 3,
            refresh_date: property_list::Value::date(999.0),
            permissions_refresh_date: property_list::Value::date(-62_135_769_600.0),
        };
        assert!(token.admits(&["email".into(), "email".into()], 999.0));
        assert!(!token.admits(&[], 1000.0));
        assert!(!token.admits(&[], 1001.0));
        assert!(!token.admits(&["Email".into()], 999.0));
        assert!(!token.admits(&["user_birthday".into()], 999.0));
    }
}
