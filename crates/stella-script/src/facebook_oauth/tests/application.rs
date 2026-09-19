use super::*;
use crate::{
    FacebookSystemAccountAdapter, FacebookSystemAccountCompletion, FacebookSystemAuthorization,
};
use serde_json::Value;
use std::sync::atomic::{AtomicBool, Ordering};

struct NoInitialSystem;
impl FacebookSystemAccountAdapter for NoInitialSystem {
    fn can_request_access_without_ui(&self) -> bool {
        panic!("Purple disables initial system auth");
    }
    fn renew_system_authorization(
        &self,
        _: FacebookSystemAccountCompletion<FacebookSystemAuthorization>,
    ) {
        panic!("unexpected renewal");
    }
    fn restore_account_access(&self, _: &str, _: i32, _: FacebookSystemAccountCompletion<String>) {
        panic!("unexpected initial system access");
    }
    fn set_force_blocking_renew(&self, _: bool) {
        panic!("unexpected system policy");
    }
}

fn params(config: &FacebookOAuthConfig, url: &str) -> BTreeMap<String, String> {
    let query = url.split_once('?').unwrap().1;
    protocol::callback_params(
        config,
        &format!(
            "fb{}{}://authorize?{query}",
            config.app_id, config.url_scheme_suffix
        ),
    )
    .unwrap()
    .unwrap()
}

fn client_state(params: &BTreeMap<String, String>) -> Value {
    serde_json::from_str(&params["state"]).unwrap()
}

struct CacheFile(std::path::PathBuf);
impl CacheFile {
    fn new() -> Self {
        let file = Self(std::env::temp_dir().join(format!(
                "stella-app-auth-{}-{}.plist",
                std::process::id(),
                SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap()
                    .as_nanos()
            )));
        assert!(!file.0.exists());
        file
    }
    fn login_type(&self) -> i64 {
        let value = plist::Value::from_file(&self.0).unwrap();
        value.as_dictionary().unwrap()["FBAccessTokenInformationKey"]
            .as_dictionary()
            .unwrap()["com.facebook.sdk:TokenInformationLoginTypeLoginKey"]
            .as_signed_integer()
            .unwrap()
    }
}
impl Drop for CacheFile {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

#[test]
fn facebook_oauth_application_protocol_uses_native_scheme_state_and_cached_login_type() {
    for suffix in ["", "test"] {
        let mut cfg = config();
        cfg.url_scheme_suffix = suffix.into();
        let file = CacheFile::new();
        let cache = FacebookTokenCache::open(&file.0).unwrap();
        let session = Arc::new(
            FacebookOAuthSession::new_with_cache(cfg.clone(), cache, |_| {
                panic!("accepted app must not launch browser")
            })
            .unwrap(),
        );
        session.set_system_account_adapter(Arc::new(NoInitialSystem));
        let urls = Arc::new(Mutex::new(Vec::new()));
        let output = urls.clone();
        let owner = Arc::downgrade(&session);
        session.set_facebook_application_launcher(move |url| {
            assert_eq!(
                owner
                    .upgrade()
                    .unwrap()
                    .state
                    .lock()
                    .unwrap()
                    .pending_login_type,
                2
            );
            output.lock().unwrap().push(url.to_owned());
            Ok(true)
        });
        assert!(matches!(
            session.clone().prepare_login(),
            SocialLoginRequest::AwaitingCallback
        ));
        assert_eq!(session.take_login_completion(), None);
        let url = urls.lock().unwrap()[0].clone();
        assert!(url.starts_with(if suffix.is_empty() {
            "fbauth://authorize?"
        } else {
            "fbauth2://authorize?"
        }));
        let values = params(&cfg, &url);
        assert_eq!(values["redirect_uri"], "fbconnect://success");
        assert_eq!(values["client_id"], "12345");
        assert_eq!(
            values["scope"],
            "public_profile,email,user_friends,user_birthday"
        );
        assert_eq!(values["sdk_version"], "3.14.1");
        assert_eq!(values["response_type"], "token");
        assert_eq!(
            values.get("local_client_id").map(String::as_str),
            (!suffix.is_empty()).then_some(suffix)
        );
        assert_eq!(client_state(&values)["3_method"], "fb_application_web_auth");
        assert!(
            client_state(&values)["0_auth_logger_id"]
                .as_str()
                .unwrap()
                .len()
                > 20
        );
        assert!(!values.contains_key("access_token"));
        assert!(
            session
                .handle_open_url(&format!(
                    "fb12345{suffix}://authorize#access_token=synthetic-app-result&expires_in=3600"
                ))
                .unwrap()
        );
        session.application_resumed();
        assert_eq!(session.take_login_completion(), Some(Ok(())));
        assert_eq!(session.session_state(), FacebookSessionState::Open);
        assert_eq!(file.login_type(), 2);
        assert!(matches!(
            session.clone().prepare_user_profile(),
            SocialProfileRequest::Pending(_)
        ));
        assert!(matches!(
            session.clone().prepare_login(),
            SocialLoginRequest::Ready(Ok(()))
        ));
        assert_eq!(
            urls.lock().unwrap().len(),
            1,
            "cache hit relaunched app auth"
        );
    }
}

#[test]
fn facebook_oauth_declined_application_falls_back_once_with_same_logger() {
    for app_error in [false, true] {
        for browser_accepts in [false, true] {
            let urls = Arc::new(Mutex::new(Vec::new()));
            let browser_urls = urls.clone();
            let session = Arc::new(
                FacebookOAuthSession::new(config(), move |url| {
                    browser_urls.lock().unwrap().push(url.to_owned());
                    Ok(browser_accepts)
                })
                .unwrap(),
            );
            let app_urls = urls.clone();
            session.set_facebook_application_launcher(move |url| {
                app_urls.lock().unwrap().push(url.to_owned());
                if app_error {
                    Err(SocialPlatformError::Transport)
                } else {
                    Ok(false)
                }
            });
            let admission = session.clone().prepare_login();
            assert!(if browser_accepts {
                matches!(admission, SocialLoginRequest::AwaitingCallback)
            } else {
                matches!(
                    admission,
                    SocialLoginRequest::Ready(Err(SocialPlatformError::Unavailable))
                )
            });
            let urls = urls.lock().unwrap();
            assert_eq!(urls.len(), 2);
            assert!(urls[0].starts_with("fbauth2://authorize?"));
            assert!(urls[1].starts_with("http://127.0.0.1:9/oauth?"));
            let app = params(&config(), &urls[0]);
            let browser = params(&config(), &urls[1]);
            assert_eq!(
                client_state(&app)["0_auth_logger_id"],
                client_state(&browser)["0_auth_logger_id"]
            );
            assert_eq!(client_state(&browser)["3_method"], "browser_auth");
            assert_eq!(browser["redirect_uri"], "fb12345test://authorize");
            assert_eq!(
                session.state.lock().unwrap().pending_login_type,
                if browser_accepts { 3 } else { 0 }
            );
        }
    }
}

#[test]
fn facebook_oauth_application_reentrant_callbacks_do_not_double_launch_or_erase_success() {
    for case in 0..5 {
        let browsers = Arc::new(Mutex::new(Vec::new()));
        let output = browsers.clone();
        let session = Arc::new(
            FacebookOAuthSession::new(config(), move |url| {
                output.lock().unwrap().push(url.to_owned());
                Ok(case != 2)
            })
            .unwrap(),
        );
        let owner = Arc::downgrade(&session);
        session.set_facebook_application_launcher(move |_| {
            let session = owner.upgrade().unwrap();
            match case {
                0 => {
                    session
                        .handle_open_url(
                            "fb12345test://authorize#access_token=synthetic-reentrant-app",
                        )
                        .unwrap();
                }
                1 | 2 => {
                    session
                        .handle_open_url(
                            "fb12345test://authorize#error=service_disabled_use_browser",
                        )
                        .unwrap();
                }
                3 => session.application_resumed(),
                4 => {
                    session
                        .handle_open_url("fb12345test://authorize#error=service_disabled")
                        .unwrap();
                }
                _ => unreachable!(),
            }
            Ok(false)
        });
        let result = session.clone().prepare_login();
        match case {
            0 => {
                assert!(matches!(result, SocialLoginRequest::Ready(Ok(()))));
                assert!(session.is_logged_in());
            }
            1 => {
                assert!(matches!(result, SocialLoginRequest::AwaitingCallback));
                assert_eq!(session.state.lock().unwrap().pending_login_type, 3);
            }
            2 | 4 => assert!(matches!(
                result,
                SocialLoginRequest::Ready(Err(SocialPlatformError::Unavailable))
            )),
            3 => {
                assert!(matches!(result, SocialLoginRequest::Ready(Ok(()))));
                assert!(!session.is_logged_in());
            }
            _ => unreachable!(),
        }
        assert_eq!(
            browsers.lock().unwrap().len(),
            usize::from(matches!(case, 1 | 2))
        );
    }
}

#[test]
fn facebook_oauth_replaced_application_launch_cannot_close_newer_login() {
    let browser_count = Arc::new(Mutex::new(0));
    let count = browser_count.clone();
    let session = Arc::new(
        FacebookOAuthSession::new(config(), move |_| {
            *count.lock().unwrap() += 1;
            Ok(true)
        })
        .unwrap(),
    );
    let entered = Arc::new(AtomicBool::new(false));
    let owner = Arc::downgrade(&session);
    session.set_facebook_application_launcher(move |_| {
        if !entered.swap(true, Ordering::SeqCst) {
            assert!(matches!(
                owner.upgrade().unwrap().prepare_login(),
                SocialLoginRequest::AwaitingCallback
            ));
        }
        Ok(false)
    });
    assert!(matches!(
        session.clone().prepare_login(),
        SocialLoginRequest::Ready(Err(SocialPlatformError::Cancelled))
    ));
    assert_eq!(session.session_state(), FacebookSessionState::Opening);
    assert_eq!(session.state.lock().unwrap().pending_login_type, 3);
    assert_eq!(*browser_count.lock().unwrap(), 1);
}
