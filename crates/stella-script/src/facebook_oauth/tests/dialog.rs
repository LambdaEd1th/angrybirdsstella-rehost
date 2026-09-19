use super::*;
use std::sync::Weak;

type DismissRecord = (
    String,
    bool,
    FacebookSessionState,
    Option<Result<(), SocialPlatformError>>,
);
type OnShow = dyn Fn(&FacebookLoginDialogRequest) + Send + Sync;
#[derive(Default)]
struct Dialog {
    owner: Mutex<Weak<FacebookOAuthSession>>,
    requests: Mutex<Vec<FacebookLoginDialogRequest>>,
    dismisses: Mutex<Vec<DismissRecord>>,
    on_show: Mutex<Option<Box<OnShow>>>,
    show_error: Option<SocialPlatformError>,
}
impl FacebookLoginDialogAdapter for Dialog {
    fn show(&self, request: &FacebookLoginDialogRequest) -> Result<(), SocialPlatformError> {
        let session = self.owner.lock().unwrap().upgrade().unwrap();
        assert_eq!(session.session_state(), FacebookSessionState::Opening);
        assert_eq!(session.state.lock().unwrap().pending_login_type, 4);
        self.requests.lock().unwrap().push(request.clone());
        let action = self.on_show.lock().unwrap().take();
        if let Some(action) = action {
            action(request);
        }
        self.show_error.map_or(Ok(()), Err)
    }
    fn dismiss(&self, request: &FacebookLoginDialogRequest, success: bool) {
        let session = self.owner.lock().unwrap().upgrade().unwrap();
        let state = session.state.lock().unwrap();
        self.dismisses.lock().unwrap().push((
            request.request_id.clone(),
            success,
            state.status,
            state.completion,
        ));
    }
}
fn setup(show_error: Option<SocialPlatformError>) -> (Arc<FacebookOAuthSession>, Arc<Dialog>) {
    let session = Arc::new(FacebookOAuthSession::new(config(), |_| Ok(false)).unwrap());
    let host = Arc::new(Dialog {
        owner: Mutex::new(Arc::downgrade(&session)),
        show_error,
        ..Default::default()
    });
    session
        .set_login_dialog_adapter("http://127.0.0.1:9/dialog/oauth", host.clone())
        .unwrap();
    (session, host)
}
fn start(session: &Arc<FacebookOAuthSession>, host: &Dialog) -> FacebookLoginDialogRequest {
    assert!(matches!(
        session.clone().prepare_login(),
        SocialLoginRequest::AwaitingCallback
    ));
    host.requests.lock().unwrap().last().unwrap().clone()
}

#[test]
fn facebook_inline_dialog_native_request_resume_success_and_cache() {
    let path = std::env::temp_dir().join(format!(
        "stella-inline-{}-{}.plist",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    assert!(!path.exists());
    let cache = FacebookTokenCache::open(&path).unwrap();
    let session = Arc::new(
        FacebookOAuthSession::new_with_cache(config(), cache.clone(), |_| Ok(false)).unwrap(),
    );
    let host = Arc::new(Dialog {
        owner: Mutex::new(Arc::downgrade(&session)),
        ..Default::default()
    });
    session.set_facebook_application_launcher(|_| Ok(false));
    session
        .set_login_dialog_adapter("http://127.0.0.1:9/dialog/oauth", host.clone())
        .unwrap();
    let request = start(&session, &host);
    let params = protocol::url_params(&request.authorization_url).unwrap();
    assert!(
        request
            .authorization_url
            .starts_with("http://127.0.0.1:9/dialog/oauth?")
    );
    assert_eq!(params["redirect_uri"], "fbconnect://success");
    assert_eq!(params["local_client_id"], "test");
    assert_eq!(
        params["scope"],
        "public_profile,email,user_friends,user_birthday"
    );
    assert_eq!(params["sdk_version"], "3.14.1");
    let client: serde_json::Value = serde_json::from_str(&params["state"]).unwrap();
    assert_eq!(client["3_method"], "fallback_auth");
    assert_eq!(client["0_auth_logger_id"], request.request_id);
    assert!(!params.contains_key("access_token"));
    for _ in 0..2 {
        session.application_resumed();
    }
    assert_eq!(session.session_state(), FacebookSessionState::Opening);
    assert_eq!(session.take_login_completion(), None);
    assert!(
        !session
            .handle_login_dialog_navigation(&request.request_id, &request.authorization_url, true)
            .unwrap()
    );
    assert!(
        session
            .handle_login_dialog_navigation(
                &request.request_id,
                "fbconnect://success#access_token=synthetic-inline%2Btoken&expires=1",
                false
            )
            .unwrap()
    );
    assert_eq!(session.take_login_completion(), Some(Ok(())));
    assert_eq!(session.session_state(), FacebookSessionState::Open);
    assert_eq!(session.state.lock().unwrap().pending_login_type, 4);
    let dismisses = host.dismisses.lock().unwrap();
    assert_eq!(dismisses.len(), 1);
    assert!(dismisses[0].1);
    assert_eq!(dismisses[0].2, FacebookSessionState::Open);
    assert_eq!(dismisses[0].3, Some(Ok(())));
    drop(dismisses);
    let preferences = plist::Value::from_file(&path).unwrap();
    let token = preferences.as_dictionary().unwrap()["FBAccessTokenInformationKey"]
        .as_dictionary()
        .unwrap();
    let prefix = "com.facebook.sdk:TokenInformation";
    assert_eq!(
        token[&format!("{prefix}TokenKey")].as_string(),
        Some("synthetic-inline+token")
    );
    assert_eq!(
        token[&format!("{prefix}LoginTypeLoginKey")].as_signed_integer(),
        Some(4)
    );
    let expiry: SystemTime = token[&format!("{prefix}ExpirationDateKey")]
        .as_date()
        .unwrap()
        .into();
    assert_eq!(
        expiry.duration_since(UNIX_EPOCH).unwrap().as_secs(),
        64_092_211_200
    );
    assert!(matches!(
        session.clone().prepare_login(),
        SocialLoginRequest::Ready(Ok(()))
    ));
    assert_eq!(host.requests.lock().unwrap().len(), 1);
    session.logout().unwrap();
    assert!(
        plist::Value::from_file(&path)
            .unwrap()
            .as_dictionary()
            .unwrap()
            .is_empty()
    );
    assert_eq!(cache.take_error(), None);
    std::fs::remove_file(path).unwrap();
}

#[test]
fn facebook_inline_dialog_cancel_load_errors_and_dismiss_order() {
    for case in 0..8 {
        let (session, host) = setup(None);
        let request = start(&session, &host);
        let id = &request.request_id;
        let expected = match case {
            0 => {
                assert!(session.cancel_login_dialog(id).unwrap());
                SocialPlatformError::Cancelled
            }
            1 => {
                assert!(
                    session
                        .handle_login_dialog_navigation(id, "fbconnect://cancel", false)
                        .unwrap()
                );
                SocialPlatformError::Cancelled
            }
            2 => {
                assert!(
                    session
                        .handle_login_dialog_navigation(
                            id,
                            "fbconnect://success#access_token=",
                            false
                        )
                        .unwrap()
                );
                SocialPlatformError::Cancelled
            }
            3 => {
                assert!(
                    session
                        .handle_login_dialog_navigation(id, "fbconnect://success#no_token=1", false)
                        .unwrap()
                );
                SocialPlatformError::Cancelled
            }
            _ => {
                let (domain, code) = match case {
                    4 => ("NSURLErrorDomain", -1009),
                    5 => ("WebKitErrorDomain", 101),
                    6 => ("OtherDomain", -999),
                    _ => ("OtherDomain", 102),
                };
                assert!(
                    session
                        .handle_login_dialog_load_error(id, domain, code)
                        .unwrap()
                );
                SocialPlatformError::Cancelled
            }
        };
        assert_eq!(
            session.session_state(),
            FacebookSessionState::ClosedLoginFailed,
            "case{case}"
        );
        let dismisses = host.dismisses.lock().unwrap();
        assert_eq!(dismisses.len(), if matches!(case, 2 | 3) { 2 } else { 1 });
        assert!(!dismisses[0].1);
        assert_eq!(dismisses[0].2, FacebookSessionState::Opening);
        assert_eq!(dismisses[0].3, None);
        assert_eq!(session.take_login_completion(), Some(Err(expected)));
        assert!(!session.cancel_login_dialog(id).unwrap());
    }
    let (session, host) = setup(None);
    let request = start(&session, &host);
    for (domain, code) in [("NSURLErrorDomain", -999), ("WebKitErrorDomain", 102)] {
        assert!(
            !session
                .handle_login_dialog_load_error(&request.request_id, domain, code)
                .unwrap()
        );
        assert_eq!(session.session_state(), FacebookSessionState::Opening);
        assert_eq!(session.take_login_completion(), None);
    }
    assert!(host.dismisses.lock().unwrap().is_empty());
    // Base-dialog error dismissal does not invoke the login delegate2E7020.
    assert_eq!(
        session.handle_login_dialog_navigation(
            &request.request_id,
            "fbconnect://cancel?error_code=4&error_msg=declined",
            false
        ),
        Err(SocialPlatformError::Graph)
    );
    assert_eq!(session.session_state(), FacebookSessionState::Opening);
    assert_eq!(session.take_login_completion(), None);
    assert_eq!(host.dismisses.lock().unwrap().len(), 1);
    session.application_resumed();
    assert_eq!(session.session_state(), FacebookSessionState::Opening);
}

#[test]
fn facebook_inline_dialog_synchronous_callbacks_and_actual_show_failure() {
    for outcome in 0..3 {
        let (session, host) = setup(Some(SocialPlatformError::Transport));
        if outcome != 0 {
            let owner = Arc::downgrade(&session);
            *host.on_show.lock().unwrap() = Some(Box::new(move |request| {
                let session = owner.upgrade().unwrap();
                if outcome == 1 {
                    session.cancel_login_dialog(&request.request_id).unwrap();
                } else {
                    session.handle_login_dialog_navigation(&request.request_id, "fbconnect://success#access_token=synthetic-inline-sync&expires_in=3600", false).unwrap();
                }
            }));
        }
        let SocialLoginRequest::Ready(result) = session.clone().prepare_login() else {
            panic!("synchronous result missing");
        };
        assert_eq!(
            result,
            match outcome {
                0 => Err(SocialPlatformError::Transport),
                1 => Err(SocialPlatformError::Cancelled),
                _ => Ok(()),
            }
        );
        assert_eq!(host.dismisses.lock().unwrap().len(), 1);
        assert_eq!(session.take_login_completion(), None);
    }
}

#[test]
fn facebook_inline_dialog_replacement_retains_host_and_rejects_late_callbacks() {
    let (session, first) = setup(None);
    let old = start(&session, &first);
    let next = Arc::new(Dialog {
        owner: Mutex::new(Arc::downgrade(&session)),
        ..Default::default()
    });
    session
        .set_login_dialog_adapter("http://127.0.0.1:9/other/oauth", next.clone())
        .unwrap();
    assert!(session.cancel_login_dialog(&old.request_id).unwrap());
    assert_eq!(first.dismisses.lock().unwrap().len(), 1);
    assert!(next.dismisses.lock().unwrap().is_empty());
    session.take_login_completion();
    let owner = Arc::downgrade(&session);
    *next.on_show.lock().unwrap() = Some(Box::new(move |request| {
        let session = owner.upgrade().unwrap();
        assert!(matches!(
            session.clone().prepare_login(),
            SocialLoginRequest::AwaitingCallback
        ));
        assert!(
            !session
                .handle_login_dialog_navigation(
                    &request.request_id,
                    "fbconnect://success#access_token=synthetic-obsolete-inline",
                    false
                )
                .unwrap()
        );
    }));
    assert!(matches!(
        session.clone().prepare_login(),
        SocialLoginRequest::Ready(Err(SocialPlatformError::Cancelled))
    ));
    let requests = next.requests.lock().unwrap();
    assert_eq!(requests.len(), 2);
    let newest = requests.last().unwrap().clone();
    assert_ne!(requests[0].request_id, newest.request_id);
    drop(requests);
    assert!(
        !session
            .handle_login_dialog_load_error(&old.request_id, "OtherDomain", 1)
            .unwrap()
    );
    assert_eq!(session.session_state(), FacebookSessionState::Opening);
    assert!(
        session
            .handle_login_dialog_navigation(
                &newest.request_id,
                "fbconnect://success#access_token=synthetic-current-inline",
                false
            )
            .unwrap()
    );
    assert_eq!(session.take_login_completion(), Some(Ok(())));
}

#[test]
fn facebook_inline_dialog_server_retry_flags_do_not_enable_fallback() {
    for browser_retry in [false, true] {
        let session = Arc::new(FacebookOAuthSession::new(config(), |_| Ok(false)).unwrap());
        session.set_facebook_application_launcher(|_| Ok(true));
        let host = Arc::new(Dialog {
            owner: Mutex::new(Arc::downgrade(&session)),
            ..Default::default()
        });
        session
            .set_login_dialog_adapter("http://127.0.0.1:9/dialog/oauth", host.clone())
            .unwrap();
        assert!(matches!(
            session.clone().prepare_login(),
            SocialLoginRequest::AwaitingCallback
        ));
        assert!(
            session
                .handle_open_url(if browser_retry {
                    "fb12345test://authorize#error=service_disabled_use_browser"
                } else {
                    "fb12345test://authorize#error=service_disabled"
                })
                .unwrap()
        );
        assert_eq!(
            session.take_login_completion(),
            Some(Err(SocialPlatformError::Unavailable))
        );
        assert!(host.requests.lock().unwrap().is_empty());
    }
}

#[test]
fn facebook_inline_dialog_first_token_extraction_and_external_navigation() {
    let calls = Arc::new(Mutex::new(Vec::new()));
    let captured = calls.clone();
    let session = Arc::new(
        FacebookOAuthSession::new(config(), move |url| {
            captured.lock().unwrap().push(url.to_owned());
            Ok(url == "https://example.invalid/help")
        })
        .unwrap(),
    );
    let host = Arc::new(Dialog {
        owner: Mutex::new(Arc::downgrade(&session)),
        ..Default::default()
    });
    session
        .set_login_dialog_adapter("http://127.0.0.1:9/dialog/oauth", host.clone())
        .unwrap();
    let request = start(&session, &host);
    assert!(
        !session
            .handle_login_dialog_navigation(
                &request.request_id,
                "https://example.invalid/redirect",
                false
            )
            .unwrap()
    );
    assert!(
        session
            .handle_login_dialog_navigation(
                &request.request_id,
                "https://example.invalid/help",
                true
            )
            .unwrap()
    );
    assert_eq!(calls.lock().unwrap().len(), 2);
    assert_eq!(
        session.handle_login_dialog_navigation(
            &request.request_id,
            "https://example.invalid/unavailable",
            true
        ),
        Err(SocialPlatformError::Unavailable)
    );
    assert_eq!(session.session_state(), FacebookSessionState::Opening);
    assert!(session.handle_login_dialog_navigation(&request.request_id, "fbconnect://success?access_token=synthetic-first+literal&expires_in=3600#access_token=synthetic-last", false).unwrap());
    assert_eq!(
        session.graph().unwrap().current_token().unwrap(),
        "synthetic-first+literal"
    );
    assert_eq!(session.take_login_completion(), Some(Ok(())));
}

#[test]
fn facebook_inline_dialog_nullable_foundation_percent_decoding() {
    let cases: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/dialog-percent-decoding.json")).unwrap();
    assert_eq!(cases.as_array().unwrap().len(), 20);
    for case in cases.as_array().unwrap() {
        let (session, host) = setup(None);
        let request = start(&session, &host);
        let url = format!(
            "fbconnect://success#access_token={}&expires_in=3600",
            case["input"].as_str().unwrap()
        );
        assert!(
            session
                .handle_login_dialog_navigation(&request.request_id, &url, false)
                .unwrap(),
            "{case}"
        );
        match case["decoded"].as_str().filter(|s| !s.is_empty()) {
            Some(token) => {
                assert_eq!(session.graph().unwrap().current_token().unwrap(), token);
                assert_eq!(session.take_login_completion(), Some(Ok(())));
                assert_eq!(host.dismisses.lock().unwrap().len(), 1);
            }
            None => {
                assert_eq!(
                    session.take_login_completion(),
                    Some(Err(SocialPlatformError::Cancelled))
                );
                assert_eq!(
                    session.session_state(),
                    FacebookSessionState::ClosedLoginFailed
                );
                assert_eq!(host.dismisses.lock().unwrap().len(), 2);
            }
        }
    }
    for invalid in ["%", "%FF", "%XX"] {
        let (session, host) = setup(None);
        let request = start(&session, &host);
        assert!(
            session
                .handle_login_dialog_navigation(
                    &request.request_id,
                    &format!("fbconnect://cancel?error_code={invalid}&error_msg=fixture"),
                    false
                )
                .unwrap()
        );
        assert_eq!(
            session.take_login_completion(),
            Some(Err(SocialPlatformError::Cancelled))
        );
        assert_eq!(host.dismisses.lock().unwrap().len(), 1);
    }
}
