//! SDK3.14.1 keys and typed property-list values (30CC54/311A38/312184).
use super::property_list::{Dictionary, Value};
use super::{CachedToken, FacebookTokenCacheError, is_ns_whitespace, numeric_prefix};
use plist::Value as PlistValue;

pub(super) const TOKEN: &str = "com.facebook.sdk:TokenInformationTokenKey";
pub(super) const EXPIRATION: &str = "com.facebook.sdk:TokenInformationExpirationDateKey";
pub(super) const REFRESH: &str = "com.facebook.sdk:TokenInformationRefreshDateKey";
pub(super) const LEGACY_LOGIN: &str = "com.facebook.sdk:TokenInformationIsFacebookLoginKey";
pub(super) const LOGIN_TYPE: &str = "com.facebook.sdk:TokenInformationLoginTypeLoginKey";
pub(super) const PERMISSIONS: &str = "com.facebook.sdk:TokenInformationPermissionsKey";
pub(super) const PERMISSIONS_REFRESH: &str =
    "com.facebook.sdk:TokenInformationPermissionsRefreshDateKey";

impl CachedToken {
    pub(super) fn from_dictionary(
        value: &Value,
        now: f64,
    ) -> Result<Option<Self>, FacebookTokenCacheError> {
        let Some(dict) = value.as_dictionary() else {
            return Ok(None);
        };
        // Validation does not reject expiration in the past. Rejected
        // structure returns nil, leaving the stored value untouched318D84.
        let (Some(token), Some(expiration)) = (
            dict.get(TOKEN).and_then(Value::as_string),
            dict.get(EXPIRATION).and_then(Value::unix_date),
        ) else {
            return Ok(None);
        };
        if token.trim_matches(is_ns_whitespace).is_empty() {
            return Ok(None);
        }
        let mut login_type = int_value(dict.get(LOGIN_TYPE))?;
        if bool_value(dict.get(LEGACY_LOGIN))? && login_type == 0 {
            login_type = 2;
        }
        let permissions = match dict.get(PERMISSIONS) {
            None => None,
            Some(Value::Array(values)) => Some(values.clone()),
            _ => return Err(FacebookTokenCacheError::InvalidTokenInformation),
        };
        Ok(Some(Self {
            token: token.to_owned(), // Factory validates trimmed length but copies original.
            permissions,
            expires_at: expiration,
            login_type,
            refresh_date: dict
                .get(REFRESH)
                .cloned()
                .unwrap_or_else(|| Value::date(now)),
            permissions_refresh_date: dict
                .get(PERMISSIONS_REFRESH)
                .cloned()
                .unwrap_or_else(|| Value::date(-62_135_769_600.0)),
        }))
    }

    pub(super) fn dictionary(&self) -> Value {
        let mut dict = Dictionary::new();
        dict.insert(TOKEN.into(), Value::string(self.token.clone()));
        dict.insert(EXPIRATION.into(), Value::date(self.expires_at));
        dict.insert(
            LOGIN_TYPE.into(),
            Value::Other(PlistValue::Integer(self.login_type.into())),
        );
        dict.insert(REFRESH.into(), self.refresh_date.clone());
        if let Some(permissions) = &self.permissions {
            dict.insert(PERMISSIONS.into(), Value::Array(permissions.clone()));
        }
        dict.insert(
            PERMISSIONS_REFRESH.into(),
            self.permissions_refresh_date.clone(),
        );
        Value::Dictionary(dict)
    }
}

fn int_value(value: Option<&Value>) -> Result<i32, FacebookTokenCacheError> {
    match value {
        None => Ok(0),
        Some(Value::Other(PlistValue::Integer(value))) => Ok(value.as_signed().map_or_else(
            || value.as_unsigned().unwrap_or_default() as i32,
            |v| v as i32,
        )),
        Some(Value::Other(PlistValue::Real(value))) => Ok((*value as i64) as i32),
        Some(Value::Other(PlistValue::Boolean(value))) => Ok(i32::from(*value)),
        Some(Value::Other(PlistValue::String(value))) => Ok(numeric_prefix(value, false) as i32),
        _ => Err(FacebookTokenCacheError::InvalidTokenInformation),
    }
}

fn bool_value(value: Option<&Value>) -> Result<bool, FacebookTokenCacheError> {
    match value {
        Some(Value::Other(PlistValue::String(value))) => {
            let first = value.trim_start_matches(is_ns_whitespace).chars().next();
            Ok(matches!(first, Some('Y' | 'y' | 'T' | 't')) || numeric_prefix(value, false) != 0.0)
        }
        Some(Value::Other(PlistValue::Real(value))) => Ok(*value != 0.0),
        Some(Value::Other(PlistValue::Integer(value))) => Ok(value.as_signed() != Some(0)),
        _ => Ok(int_value(value)? != 0),
    }
}
