use super::property_list::{self, Dictionary, Value};
use super::{dictionary::*, *};
use crate::{
    FacebookOAuthConfig, FacebookOAuthSession, FacebookSessionState, SocialLoginRequest,
    SocialPlatformProvider, SocialProfileRequest,
};
use plist::Value as PlistValue;
use std::{
    fs,
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
};

const KEY: &str = "FBAccessTokenInformationKey";

struct Fixture(PathBuf);
impl Fixture {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "stella-facebook-token-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        Self(path)
    }
    fn path(&self) -> PathBuf {
        self.0.join("preferences.plist")
    }
    fn write(&self, value: Value) {
        let mut prefs = Dictionary::new();
        prefs.insert("unrelated".into(), Value::Other(PlistValue::Boolean(true)));
        prefs.insert(KEY.into(), value);
        fs::write(
            self.path(),
            property_list::write(&Value::Dictionary(prefs)).unwrap(),
        )
        .unwrap();
    }
    fn read(&self) -> Dictionary {
        property_list::read(&fs::read(self.path()).unwrap())
            .unwrap()
            .into_dictionary()
            .unwrap()
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn config() -> FacebookOAuthConfig {
    FacebookOAuthConfig {
        rest_root: None,
        graph_root: "http://127.0.0.1:9/v2.0".into(),
        authorization_url: "http://127.0.0.1:9/oauth".into(),
        app_id: "12345".into(),
        url_scheme_suffix: String::new(),
        request_birthday: true,
    }
}
fn token() -> CachedToken {
    CachedToken {
        token: "  synthetic-original-token  ".into(),
        permissions: Some(
            super::super::protocol::permissions(&config())
                .into_iter()
                .map(Value::string)
                .collect(),
        ),
        expires_at: 64_092_211_200.0,
        login_type: 3,
        refresh_date: Value::date(1000.25),
        permissions_refresh_date: Value::date(-62_135_769_600.0),
    }
}

#[test]
fn facebook_oauth_token_dictionary_preserves_native_types_defaults_and_legacy_login() {
    let dict = token().dictionary();
    let decoded = CachedToken::from_dictionary(&dict, 2000.0)
        .unwrap()
        .unwrap();
    assert_eq!(decoded.token, "  synthetic-original-token  ");
    assert_eq!(decoded.expires_at, 64_092_211_200.0);
    assert_eq!(decoded.refresh_date, Value::date(1000.25));
    assert_eq!(
        decoded.permissions_refresh_date,
        Value::date(-62_135_769_600.0)
    );
    let mut dict = dict.into_dictionary().unwrap();
    assert!(matches!(dict.get(EXPIRATION), Some(Value::Date(_))));
    assert!(matches!(dict.get(REFRESH), Some(Value::Date(_))));
    assert!(!dict.contains_key(LEGACY_LOGIN));
    dict.remove(REFRESH);
    dict.remove(PERMISSIONS_REFRESH);
    dict.remove(PERMISSIONS);
    dict.remove(LOGIN_TYPE);
    dict.insert(LEGACY_LOGIN.into(), Value::string(" yes"));
    let decoded = CachedToken::from_dictionary(&Value::Dictionary(dict.clone()), 2000.0)
        .unwrap()
        .unwrap();
    // The factory copies supplied optional date objects; it does not add
    // class validation before cache admission. Refresh consumers interpret them.
    dict.insert(REFRESH.into(), Value::string("opaque-optional-metadata"));
    let opaque = CachedToken::from_dictionary(&Value::Dictionary(dict.clone()), 2000.0)
        .unwrap()
        .unwrap();
    assert_eq!(
        opaque.refresh_date.as_string(),
        Some("opaque-optional-metadata")
    );
    assert_eq!(decoded.login_type, 2);
    assert_eq!(decoded.refresh_date, Value::date(2000.0));
    assert_eq!(
        decoded.permissions_refresh_date,
        Value::date(-62_135_769_600.0)
    );
    assert!(decoded.permissions.is_none());
    assert!(
        !decoded
            .dictionary()
            .as_dictionary()
            .unwrap()
            .contains_key(PERMISSIONS)
    );
    dict.insert(LOGIN_TYPE.into(), Value::string("3suffix"));
    assert_eq!(
        CachedToken::from_dictionary(&Value::Dictionary(dict), 0.0)
            .unwrap()
            .unwrap()
            .login_type,
        3
    );
}

#[test]
fn facebook_oauth_malformed_cache_is_preserved_but_rejected_valid_token_is_cleared() {
    let fixture = Fixture::new();
    let valid = token().dictionary().into_dictionary().unwrap();
    for malformed in [
        Value::string("wrong-structure"),
        {
            let mut dict = valid.clone();
            dict.insert(
                EXPIRATION.into(),
                Value::Other(PlistValue::Real(64_092_211_200.0)),
            );
            Value::Dictionary(dict)
        },
        {
            let mut dict = valid.clone();
            dict.insert(TOKEN.into(), Value::string("\t \u{200b}"));
            Value::Dictionary(dict)
        },
        {
            let mut dict = valid.clone();
            dict.insert(TOKEN.into(), Value::string(String::new()));
            Value::Dictionary(dict)
        },
    ] {
        fixture.write(malformed);
        let before = fs::read(fixture.path()).unwrap();
        let cache = FacebookTokenCache::open(fixture.path()).unwrap();
        let session = FacebookOAuthSession::new_with_cache(config(), cache.clone(), |_| {
            panic!("constructor opened browser")
        })
        .unwrap();
        assert_eq!(session.session_state(), FacebookSessionState::Created);
        assert_eq!(fs::read(fixture.path()).unwrap(), before);
        assert_eq!(cache.take_error(), None);
    }
    for expired in [false, true] {
        let mut invalid = token();
        if expired {
            invalid.expires_at = -1.0;
        } else {
            invalid.permissions = Some(vec![Value::string("email")]);
        }
        fixture.write(invalid.dictionary());
        let cache = FacebookTokenCache::open(fixture.path()).unwrap();
        let session = FacebookOAuthSession::new_with_cache(config(), cache.clone(), |_| {
            panic!("constructor opened browser")
        })
        .unwrap();
        assert_eq!(session.session_state(), FacebookSessionState::Created);
        let remaining = fixture.read();
        assert!(!remaining.contains_key(KEY));
        assert_eq!(
            remaining.get("unrelated"),
            Some(&Value::Other(PlistValue::Boolean(true)))
        );
        assert_eq!(cache.take_error(), None);
    }
}

#[test]
fn facebook_oauth_cache_factory_distinguishes_whitespace_from_newline() {
    let mut dict = token().dictionary().into_dictionary().unwrap();
    dict.insert(TOKEN.into(), Value::string("\n"));
    assert_eq!(
        CachedToken::from_dictionary(&Value::Dictionary(dict), 0.0)
            .unwrap()
            .unwrap()
            .token,
        "\n"
    );
}

#[test]
fn facebook_oauth_cache_keeps_opaque_permission_elements_and_number_narrowing() {
    let mut data = token();
    data.permissions
        .as_mut()
        .unwrap()
        .push(Value::Other(PlistValue::Integer(42.into())));
    let dict = data.dictionary();
    let decoded = CachedToken::from_dictionary(&dict, 1000.0)
        .unwrap()
        .unwrap();
    assert!(decoded.admits(&super::super::protocol::permissions(&config()), 1000.0));
    assert_eq!(decoded.dictionary(), dict);
    let mut dict = dict.into_dictionary().unwrap();
    for (value, expected) in [
        (2147483648.0, i32::MIN),
        (4294967296.0, 0),
        (-2147483649.0, i32::MAX),
    ] {
        dict.insert(LOGIN_TYPE.into(), Value::Other(PlistValue::Real(value)));
        assert_eq!(
            CachedToken::from_dictionary(&Value::Dictionary(dict.clone()), 1000.0)
                .unwrap()
                .unwrap()
                .login_type,
            expected
        );
    }
}

#[test]
fn facebook_oauth_cache_survives_provider_replacement_and_file_reload() {
    let fixture = Fixture::new();
    let cache = FacebookTokenCache::open(fixture.path()).unwrap();
    let session = Arc::new(
        FacebookOAuthSession::new_with_cache(config(), cache.clone(), |_| Ok(true)).unwrap(),
    );
    assert!(
        !fixture.path().exists(),
        "opening absent preferences must not write"
    );
    assert!(matches!(
        session.clone().prepare_login(),
        SocialLoginRequest::AwaitingCallback
    ));
    session
        .handle_open_url("fb12345://authorize#access_token=synthetic-persisted&expires_in=3600")
        .unwrap();
    assert_eq!(session.take_login_completion(), Some(Ok(())));
    assert_eq!(cache.take_error(), None);
    assert!(fs::read(fixture.path()).unwrap().starts_with(b"bplist00"));
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            fs::metadata(fixture.path()).unwrap().permissions().mode() & 0o777,
            0o600
        );
    }
    let before = fs::read(fixture.path()).unwrap();
    for cache in [
        cache.clone(),
        FacebookTokenCache::open(fixture.path()).unwrap(),
    ] {
        let restored = Arc::new(
            FacebookOAuthSession::new_with_cache(config(), cache, |_| {
                panic!("cached constructor opened browser")
            })
            .unwrap(),
        );
        assert!(restored.is_logged_in());
        assert_eq!(restored.session_state(), FacebookSessionState::Open);
        assert!(restored.take_login_completion().is_none());
        assert!(matches!(
            restored.clone().take_login_profile_request(),
            Some(SocialProfileRequest::Pending(_))
        ));
        assert!(restored.clone().take_login_profile_request().is_none());
        restored.close();
        restored.logout().unwrap();
    }
    assert_eq!(
        fs::read(fixture.path()).unwrap(),
        before,
        "cached open/closed logout must not rewrite metadata"
    );
    session.logout().unwrap();
    assert!(!fixture.read().contains_key(KEY));
    let restored = FacebookOAuthSession::new_with_cache(
        config(),
        FacebookTokenCache::open(fixture.path()).unwrap(),
        |_| panic!("empty cache opened browser"),
    )
    .unwrap();
    assert_eq!(restored.session_state(), FacebookSessionState::Created);
}

#[test]
fn facebook_oauth_cache_updates_only_its_preference_key() {
    let fixture = Fixture::new();
    fixture.write(Value::string("unrelated original cache"));
    let first = FacebookTokenCache::open(fixture.path()).unwrap();
    let second = FacebookTokenCache::open_with_key(fixture.path(), "custom-token-key").unwrap();
    first.cache(&token());
    second.cache(&token());
    first.clear();
    let preferences = fixture.read();
    assert_eq!(
        preferences.get("unrelated"),
        Some(&Value::Other(PlistValue::Boolean(true)))
    );
    assert!(!preferences.contains_key(KEY));
    assert!(preferences.contains_key("custom-token-key"));
    assert_eq!(first.take_error(), None);
    assert_eq!(second.take_error(), None);
}

#[test]
fn facebook_oauth_persistence_failure_preserves_memory_login_and_exposes_error() {
    let fixture = Fixture::new();
    let cache = FacebookTokenCache::open(fixture.path()).unwrap();
    let session = Arc::new(
        FacebookOAuthSession::new_with_cache(config(), cache.clone(), |_| Ok(true)).unwrap(),
    );
    fs::remove_dir(&fixture.0).unwrap();
    assert!(matches!(
        session.clone().prepare_login(),
        SocialLoginRequest::AwaitingCallback
    ));
    session
        .handle_open_url("fb12345://authorize#access_token=synthetic-memory-after-io-error")
        .unwrap();
    assert_eq!(session.take_login_completion(), Some(Ok(())));
    assert!(session.is_logged_in());
    assert_eq!(
        session.take_token_cache_error(),
        Some(FacebookTokenCacheError::Io(std::io::ErrorKind::NotFound))
    );
    assert_eq!(cache.take_error(), None);
    let restored = FacebookOAuthSession::new_with_cache(config(), cache.clone(), |_| {
        panic!("memory cache lost on sync failure")
    })
    .unwrap();
    assert!(restored.is_logged_in());
    restored.logout().unwrap();
    assert!(cache.take_error().is_some());
    assert!(
        cache
            .admitted(&super::super::protocol::permissions(&config()))
            .unwrap()
            .is_none()
    );
}

#[test]
fn facebook_oauth_corrupt_preferences_and_optional_metadata_errors_are_observable() {
    let fixture = Fixture::new();
    fs::write(fixture.path(), b"broken plist").unwrap();
    assert_eq!(
        FacebookTokenCache::open(fixture.path()).unwrap_err(),
        FacebookTokenCacheError::InvalidPreferences
    );
    assert_eq!(fs::read(fixture.path()).unwrap(), b"broken plist");
    let mut dict = token().dictionary().into_dictionary().unwrap();
    dict.insert(PERMISSIONS.into(), Value::string("not an array"));
    fixture.write(Value::Dictionary(dict));
    let before = fs::read(fixture.path()).unwrap();
    let cache = FacebookTokenCache::open(fixture.path()).unwrap();
    assert!(
        FacebookOAuthSession::new_with_cache(config(), cache.clone(), |_| panic!(
            "malformed optional metadata authorized"
        ))
        .is_err()
    );
    assert_eq!(
        cache.take_error(),
        Some(FacebookTokenCacheError::InvalidTokenInformation)
    );
    assert_eq!(fs::read(fixture.path()).unwrap(), before);
}
