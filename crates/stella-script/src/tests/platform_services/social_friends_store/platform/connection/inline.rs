use super::*;
use crate::{FacebookLoginDialogAdapter, FacebookLoginDialogEvent, FacebookLoginDialogRequest};

#[derive(Default)]
struct InlineHost(Mutex<Vec<FacebookLoginDialogRequest>>);
impl FacebookLoginDialogAdapter for InlineHost {
    fn show(&self, request: &FacebookLoginDialogRequest) -> Result<(), SocialPlatformError> {
        self.0.lock().unwrap().push(request.clone());
        Ok(())
    }
    fn dismiss(&self, request: &FacebookLoginDialogRequest, _: bool) {
        assert_eq!(self.0.lock().unwrap().last(), Some(request));
    }
}

#[test]
fn native_platform_inline_callback_starts_service_profile_and_retires_late_results() {
    for retired in [false, true] {
        let sandbox = ShippedDataSandbox::new("native-inline-profile");
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let origin = format!("http://{}", listener.local_addr().unwrap());
        let (arrived, phase) = mpsc::channel();
        let (release, resume) = mpsc::channel();
        let server = thread::spawn(move || {
            let (mut stream, request) =
                identity_routes::accept_request_including_friends(&listener);
            let (headers, body) = request.split_once("\r\n\r\n").unwrap();
            crate::facebook_graph::test_wire::assert_batch(
                &format!("{headers}\r\n\r\n"),
                body,
                "12345",
                &["me", "me/permissions"],
                "synthetic-inline-runtime",
            );
            arrived.send(()).unwrap();
            resume.recv_timeout(Duration::from_secs(8)).unwrap();
            raw_reply(
                &mut stream,
                200,
                &crate::facebook_graph::test_wire::batch_response(&[
                    (
                        200,
                        json!({"id":"inline-runtime-user","name":"Inline Runtime User"}),
                    ),
                    (
                        200,
                        json!({"data":[{"permission":"email","status":"granted"}]}),
                    ),
                ]),
            );
            listener
        });
        let provider = Arc::new(
            FacebookOAuthSession::new(
                FacebookOAuthConfig {
                    rest_root: None,
                    graph_root: format!("{origin}/v2.0"),
                    authorization_url: format!("{origin}/oauth"),
                    app_id: "12345".into(),
                    url_scheme_suffix: String::new(),
                    request_birthday: true,
                },
                |_| Ok(false),
            )
            .unwrap(),
        );
        let host = Arc::new(InlineHost::default());
        provider
            .set_login_dialog_adapter(&format!("{origin}/dialog/oauth"), host.clone())
            .unwrap();
        let runtime = platform_runtime(&sandbox, &origin);
        let queued = runtime.social.online_completion_count_probe();
        runtime
            .set_facebook_session(Some(provider.clone()))
            .unwrap();
        assert!(matches!(
            provider.clone().prepare_login(),
            SocialLoginRequest::AwaitingCallback
        ));
        let id = host.0.lock().unwrap().last().unwrap().request_id.clone();
        let event = FacebookLoginDialogEvent::Navigation {
            request_id: id,
            url: "fbconnect://success#access_token=synthetic-inline-runtime&expires_in=3600".into(),
            link_clicked: false,
        };
        assert!(runtime.handle_platform_login_dialog_event(&event).unwrap());
        wait_signal(&runtime, &phase);
        if retired {
            runtime.set_facebook_session(None).unwrap();
        }
        release.send(()).unwrap();
        let listener = server.join().unwrap();
        for _ in 0..1500 {
            if queued() >= 2 {
                break;
            }
            thread::sleep(Duration::from_millis(2));
        }
        assert!(
            queued() >= 2,
            "actual dialog SDK/profile workers did not finish"
        );
        dispatch_registered_application_events(runtime.lua()).unwrap();
        match provider.clone().prepare_user_profile() {
            SocialProfileRequest::Ready(Ok(profile)) if !retired => {
                assert_eq!(profile.user.id, "inline-runtime-user");
                assert_eq!(profile.access_token, "synthetic-inline-runtime");
            }
            SocialProfileRequest::Pending(_) if retired => {}
            _ => panic!("inline service profile ignored runtime ownership"),
        }
        assert_eq!(
            provider.granted_permissions(),
            if retired {
                vec!["public_profile", "email", "user_friends", "user_birthday"]
            } else {
                vec!["email"]
            }
        );
        assert!(provider.clone().take_login_profile_request().is_none());
        assert!(!runtime.handle_platform_login_dialog_event(&event).unwrap());
        assert!(matches!(listener.accept(), Err(e) if e.kind() == std::io::ErrorKind::WouldBlock));
    }
}
