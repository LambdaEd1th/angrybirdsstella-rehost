//! Real loopback name lookup, native callback order and retired owners.

use super::*;
use crate::SocialPlatformProvider;
use serde_json::{Value as JsonValue, json};
use std::{
    net::TcpStream,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
};

struct ObservedGraph {
    graph: Arc<FacebookGraphSession>,
    admissions: AtomicUsize,
}

impl SocialPlatformProvider for ObservedGraph {
    fn is_logged_in(&self) -> bool {
        self.graph.is_logged_in()
    }
    fn prepare_login(self: Arc<Self>) -> SocialLoginRequest {
        self.graph.clone().prepare_login()
    }
    fn logout(&self) -> Result<(), SocialPlatformError> {
        self.graph.logout()
    }
    fn user_profile(&self) -> Result<SocialPlatformProfile, SocialPlatformError> {
        self.graph.user_profile()
    }
    fn prepare_user_profile(self: Arc<Self>) -> SocialProfileRequest {
        self.admissions.fetch_add(1, Ordering::SeqCst);
        self.graph.clone().prepare_user_profile()
    }
    fn publish_user_profile(
        &self,
        profile: &SocialPlatformProfile,
    ) -> Result<(), SocialPlatformError> {
        self.graph.publish_user_profile(profile)
    }
    fn friends(
        &self,
        details: SocialFriendDetails,
    ) -> Result<SocialPlatformFriends, SocialPlatformError> {
        self.graph.friends(details)
    }
}

fn external_profile(provider: &str, name: &str) -> JsonValue {
    json!({"publicAccountId":"name-account","personal":{"nickName":"Not a fallback"},
        "externalNetworks":[{"provider":provider,"id":"external-user"}],
        "socialNetworks":[{"provider":provider,"id":"external-user",
            "socialAttributes":{"name":name}}]})
}

fn session_body(profile: JsonValue) -> String {
    json!({"userAuth":{"accessToken":"name-identity-access","refreshToken":"name-identity-refresh","expiresIn":3600},
        "segments":[],"config":{},"profile":profile}).to_string()
}

fn reply(stream: &mut TcpStream, status: u16, body: &str) {
    write!(
        stream,
        "HTTP/1.1 {status} Fixture\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    )
    .unwrap();
}

fn listener() -> (TcpListener, String) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let origin = format!("http://{}", listener.local_addr().unwrap());
    (listener, origin)
}

fn configured_runtime(
    sandbox: &ShippedDataSandbox,
    origin: &str,
    installed: bool,
) -> (StellaLua, Arc<ObservedGraph>) {
    let runtime = StellaLua::new(&sandbox.data_root).unwrap();
    runtime
        .set_identity_url(&format!("{origin}/identity/3.0"))
        .unwrap();
    runtime
        .set_identity_client(Some("name-fixture"), Some("synthetic-name-signature"), None)
        .unwrap();
    let provider = Arc::new(ObservedGraph {
        graph: Arc::new(
            FacebookGraphSession::new(&format!("{origin}/v2.0"), "synthetic-name-platform-token")
                .unwrap(),
        ),
        admissions: AtomicUsize::new(0),
    });
    if installed {
        runtime
            .set_facebook_session(Some(provider.clone()))
            .unwrap();
    }
    let observed = provider.clone();
    runtime
        .lua()
        .globals()
        .set(
            "profile_admissions_probe",
            runtime
                .lua()
                .create_function(move |_, ()| Ok(observed.admissions.load(Ordering::SeqCst)))
                .unwrap(),
        )
        .unwrap();
    runtime
        .execute_source(
            r#"
        update=function() end
        account_successes, account_failures, name_responses=0,0,0
        profile_sequence={}
        _G.SkynestAccount.onLoginSuccess=function(guest, details)
            account_successes=account_successes+1
            account_guest, account_details=guest,details
            admission_at_login=profile_admissions_probe()
            profile_sequence[#profile_sequence+1]="login"
        end
        _G.SkynestAccount.onLoginFailure=function() account_failures=account_failures+1 end
        _G.SkynestAccount.onUserProfileResponse=function(...)
            name_responses=name_responses+1
            name_arg_count=select('#',...)
            returned_name=...
            account_details.name=returned_name
            profile_sequence[#profile_sequence+1]="name"
        end
    "#,
        )
        .unwrap();
    (runtime, provider)
}

fn begin_login(runtime: &StellaLua, successes: i64) {
    runtime
        .execute_source("_G.SkynestAccount.native_login(false,false,false)")
        .unwrap();
    let env = game_environment(runtime.lua()).unwrap();
    wait_until(runtime, || {
        env.get::<i64>("account_successes").unwrap() == successes
    });
}

fn wait_for_queue(probe: &impl Fn() -> usize, count: usize) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while probe() < count {
        assert!(
            Instant::now() < deadline,
            "profile workers did not complete"
        );
        thread::sleep(Duration::from_millis(1));
    }
}

fn assert_no_request(listener: &TcpListener) {
    assert!(matches!(listener.accept(), Err(e) if e.kind()==std::io::ErrorKind::WouldBlock));
}

fn assert_sequence(runtime: &StellaLua, expected: &[&str]) {
    let sequence = game_environment(runtime.lua())
        .unwrap()
        .get::<Table>("profile_sequence")
        .unwrap()
        .sequence_values::<String>()
        .collect::<mlua::Result<Vec<_>>>()
        .unwrap();
    assert_eq!(sequence, expected);
}

struct HeldName {
    origin: String,
    start: mpsc::Sender<()>,
    ready: mpsc::Receiver<()>,
    release: mpsc::Sender<()>,
    worker: thread::JoinHandle<TcpListener>,
}

impl HeldName {
    fn new(status: u16, body: &'static str) -> Self {
        let (listener, origin) = listener();
        let (start, accept) = mpsc::channel();
        let (arrived, ready) = mpsc::channel();
        let (release, resume) = mpsc::channel();
        let worker = thread::spawn(move || {
            // Original-script boot can precede HTTP by much more than the
            // request fixture's five-second socket deadline.
            accept.recv_timeout(Duration::from_secs(60)).unwrap();
            let (mut stream, request) =
                identity_routes::accept_request_including_friends(&listener);
            assert!(request.starts_with("POST /session/1/apps/name-fixture/sessions "));
            // The account lookup is fixed to Facebook even for this selected
            // Sina Weibo profile. No linked Facebook Friends check is involved.
            reply(
                &mut stream,
                200,
                &session_body(external_profile("sinaweibo", "")),
            );
            drop(stream);
            let mut name = None;
            let mut friends = false;
            while name.is_none() || !friends {
                let (mut stream, request) =
                    identity_routes::accept_request_including_friends(&listener);
                if request.starts_with("GET /identity/2.0/friends ") {
                    assert!(!friends);
                    friends = true;
                    reply(&mut stream, 503, "");
                } else {
                    assert!(request.starts_with("GET /v2.0/me?"), "{request}");
                    assert!(request.contains("access_token=synthetic-name-platform-token"));
                    assert!(name.is_none());
                    name = Some(stream);
                }
            }
            arrived.send(()).unwrap();
            resume.recv_timeout(Duration::from_secs(8)).unwrap();
            reply(&mut name.unwrap(), status, body);
            listener
        });
        Self {
            origin,
            start,
            ready,
            release,
            worker,
        }
    }

    fn admitted(&self, runtime: &StellaLua) {
        self.start.send(()).unwrap();
        begin_login(runtime, 1);
        self.ready.recv_timeout(Duration::from_secs(5)).unwrap();
        wait_until(runtime, || {
            runtime.social.native_friends_completions_for_test() == 1
        });
        let env = game_environment(runtime.lua()).unwrap();
        assert_eq!(env.get::<i64>("name_responses").unwrap(), 0);
        assert_eq!(env.get::<i64>("admission_at_login").unwrap(), 0);
        assert!(
            env.get::<Table>("account_details")
                .unwrap()
                .get::<bool>("isConnectedToSocialNetwork")
                .unwrap()
        );
        assert_eq!(
            env.get::<Table>("account_details")
                .unwrap()
                .get::<String>("name")
                .unwrap(),
            ""
        );
        assert_eq!(runtime.social.online_completion_count_probe()(), 0);
    }

    fn finish(self, runtime: &StellaLua) -> TcpListener {
        self.release.send(()).unwrap();
        let listener = self.worker.join().unwrap();
        let queued = runtime.social.online_completion_count_probe();
        wait_for_queue(&queued, 1);
        // The actual HTTP result is queued, but may not change Lua or the
        // provider cache before the application event is dispatched.
        assert_eq!(
            game_environment(runtime.lua())
                .unwrap()
                .get::<i64>("name_responses")
                .unwrap(),
            0
        );
        drain(runtime);
        assert_eq!(queued(), 0);
        listener
    }
}

#[test]
fn account_profile_name_real_http_then_cached_result_follows_login_success() {
    for (body, name) in [
        (
            r#"{"id":"different-facebook-id","name":"星光 🌟"}"#,
            "星光 🌟",
        ),
        (r#"{"id":"different-facebook-id","name":""}"#, ""),
    ] {
        let sandbox = ShippedDataSandbox::new("account-profile-name-http");
        let held = HeldName::new(200, body);
        let (runtime, provider) = configured_runtime(&sandbox, &held.origin, true);
        held.admitted(&runtime);
        assert_eq!(provider.admissions.load(Ordering::SeqCst), 1);
        assert!(provider.graph.cached_user().is_none());
        let listener = held.finish(&runtime);
        let env = game_environment(runtime.lua()).unwrap();
        assert_eq!(env.get::<i64>("name_responses").unwrap(), 1);
        assert_eq!(env.get::<i64>("name_arg_count").unwrap(), 1);
        assert_eq!(env.get::<String>("returned_name").unwrap(), name);
        assert_sequence(&runtime, &["login", "name"]);
        assert_eq!(provider.graph.cached_user().unwrap().name, name);
        assert_eq!(env.get::<i64>("account_failures").unwrap(), 0);
        // The native callback changes the Lua projection, not the identity's
        // source profile. A second login therefore still admits the lookup.
        let raw = runtime.skynest_account.friends_profile_for_test().unwrap();
        assert_eq!(raw["socialNetworks"][0]["socialAttributes"]["name"], "");
        begin_login(&runtime, 2);
        assert_eq!(env.get::<i64>("name_responses").unwrap(), 2);
        assert_eq!(env.get::<i64>("admission_at_login").unwrap(), 1);
        assert_eq!(provider.admissions.load(Ordering::SeqCst), 2);
        assert_eq!(runtime.social.online_completion_count_probe()(), 0);
        assert_sequence(&runtime, &["login", "name", "login", "name"]);
        assert_no_request(&listener);
    }
}

#[test]
fn account_profile_name_failed_http_and_graph_results_do_not_fail_login() {
    for (status, body) in [
        (503, ""),
        (200, "[]"),
        (
            400,
            r#"{"error":{"message":"Synthetic rejection","code":1}}"#,
        ),
    ] {
        let sandbox = ShippedDataSandbox::new("account-profile-name-errors");
        let held = HeldName::new(status, body);
        let (runtime, provider) = configured_runtime(&sandbox, &held.origin, true);
        held.admitted(&runtime);
        let listener = held.finish(&runtime);
        let env = game_environment(runtime.lua()).unwrap();
        assert_eq!(env.get::<i64>("account_successes").unwrap(), 1);
        assert_eq!(env.get::<i64>("account_failures").unwrap(), 0);
        assert_eq!(env.get::<i64>("name_responses").unwrap(), 0);
        assert!(provider.graph.cached_user().is_none());
        assert_sequence(&runtime, &["login"]);
        assert_no_request(&listener);
    }
}

#[test]
fn account_profile_name_late_result_cannot_publish_after_owner_retirement() {
    for retirement in ["logout", "platform", "identity"] {
        let sandbox = ShippedDataSandbox::new("account-profile-name-retirement");
        let held = HeldName::new(200, r#"{"id":"late-id","name":"Must not publish"}"#);
        let (runtime, provider) = configured_runtime(&sandbox, &held.origin, true);
        held.admitted(&runtime);
        match retirement {
            "logout" => runtime
                .execute_source("_G.SkynestAccount.native_logout()")
                .unwrap(),
            "platform" => runtime.set_facebook_session(None).unwrap(),
            _ => runtime
                .set_identity_client(Some("replacement-name-client"), None, None)
                .unwrap(),
        }
        let listener = held.finish(&runtime);
        assert_eq!(
            game_environment(runtime.lua())
                .unwrap()
                .get::<i64>("name_responses")
                .unwrap(),
            0
        );
        assert!(provider.graph.cached_user().is_none());
        assert_sequence(&runtime, &["login"]);
        assert_no_request(&listener);
    }
}

fn serve_without_name(
    listener: TcpListener,
    profile: JsonValue,
) -> thread::JoinHandle<TcpListener> {
    thread::spawn(move || {
        let (mut stream, request) = identity_routes::accept_request_including_friends(&listener);
        assert!(request.starts_with("POST /session/1/apps/name-fixture/sessions "));
        reply(&mut stream, 200, &session_body(profile));
        drop(stream);
        let (mut stream, request) = identity_routes::accept_request_including_friends(&listener);
        assert!(request.starts_with("GET /identity/2.0/friends "));
        reply(&mut stream, 503, "");
        listener
    })
}

#[test]
fn account_profile_name_known_or_unconnected_profiles_do_not_admit_a_lookup() {
    for profile in [
        external_profile("sinaweibo", "Already Known"),
        json!({"publicAccountId":"email-account","personal":{"email":"person@example.invalid"}}),
        json!({"publicAccountId":"guest-account"}),
    ] {
        let sandbox = ShippedDataSandbox::new("account-profile-name-not-needed");
        let (listener, origin) = listener();
        let server = serve_without_name(listener, profile);
        let (runtime, provider) = configured_runtime(&sandbox, &origin, true);
        begin_login(&runtime, 1);
        let listener = server.join().unwrap();
        wait_until(&runtime, || {
            runtime.social.native_friends_completions_for_test() == 1
        });
        assert_eq!(provider.admissions.load(Ordering::SeqCst), 0);
        assert_eq!(
            game_environment(runtime.lua())
                .unwrap()
                .get::<i64>("name_responses")
                .unwrap(),
            0
        );
        assert_no_request(&listener);
    }
}

#[test]
fn account_profile_name_absent_or_closed_platform_does_not_authenticate() {
    for installed in [false, true] {
        let sandbox = ShippedDataSandbox::new("account-profile-name-unavailable");
        let (listener, origin) = listener();
        let server = serve_without_name(listener, external_profile("sinaweibo", ""));
        let (runtime, provider) = configured_runtime(&sandbox, &origin, installed);
        provider.graph.close();
        begin_login(&runtime, 1);
        let listener = server.join().unwrap();
        wait_until(&runtime, || {
            runtime.social.native_friends_completions_for_test() == 1
        });
        let env = game_environment(runtime.lua()).unwrap();
        assert_eq!(env.get::<i64>("account_failures").unwrap(), 0);
        assert_eq!(env.get::<i64>("name_responses").unwrap(), 0);
        assert_eq!(
            provider.admissions.load(Ordering::SeqCst),
            usize::from(installed)
        );
        assert!(!provider.is_logged_in());
        assert_no_request(&listener);
    }
}

#[test]
fn account_profile_name_reentrant_logout_prevents_lookup_admission() {
    let sandbox = ShippedDataSandbox::new("account-profile-name-reentrant");
    let (listener, origin) = listener();
    let (stop, finished) = mpsc::channel();
    let server = thread::spawn(move || {
        let (mut stream, request) = identity_routes::accept_request_including_friends(&listener);
        assert!(request.starts_with("POST /session/1/apps/name-fixture/sessions "));
        reply(
            &mut stream,
            200,
            &session_body(external_profile("sinaweibo", "")),
        );
        drop(stream);
        // Reentrant logout can cancel the already-started Friends job before
        // its HTTP admission. Serve it only if it actually reaches the wire.
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            match listener.accept() {
                Ok((mut stream, _)) => {
                    stream.set_nonblocking(false).unwrap();
                    stream
                        .set_read_timeout(Some(Duration::from_secs(5)))
                        .unwrap();
                    let mut request = Vec::new();
                    let mut bytes = [0; 4096];
                    while !request.windows(4).any(|w| w == b"\r\n\r\n") {
                        let count = stream.read(&mut bytes).unwrap();
                        assert_ne!(count, 0);
                        request.extend_from_slice(&bytes[..count]);
                    }
                    assert!(request.starts_with(b"GET /identity/2.0/friends "));
                    reply(&mut stream, 503, "");
                }
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    if finished.try_recv().is_ok() {
                        break;
                    }
                    assert!(Instant::now() < deadline);
                    thread::sleep(Duration::from_millis(1));
                }
                Err(e) => panic!("optional Friends accept failed: {e}"),
            }
        }
        listener
    });
    let (runtime, provider) = configured_runtime(&sandbox, &origin, true);
    runtime
        .execute_source(
            r#"
        _G.SkynestAccount.onLoginSuccess=function()
            account_successes=account_successes+1
            _G.SkynestAccount.native_logout()
        end
    "#,
        )
        .unwrap();
    begin_login(&runtime, 1);
    let queued = runtime.social.online_completion_count_probe();
    wait_for_queue(&queued, 1);
    // A retired provider generation discards this result before the Friends
    // completion counter runs. Observe and drain the actual worker queue.
    drain(&runtime);
    assert_eq!(queued(), 0);
    stop.send(()).unwrap();
    let listener = server.join().unwrap();
    // The original Friends worker may already be queued, but no profile
    // request can be admitted after the login callback retires the account.
    assert_eq!(provider.admissions.load(Ordering::SeqCst), 0);
    assert_eq!(
        game_environment(runtime.lua())
            .unwrap()
            .get::<i64>("name_responses")
            .unwrap(),
        0
    );
    assert_no_request(&listener);
}

#[test]
fn account_profile_name_facebook_lookup_is_independent_of_initial_friends_checks() {
    let sandbox = ShippedDataSandbox::new("account-profile-name-facebook");
    let (listener, origin) = listener();
    let (arrived, ready) = mpsc::channel();
    let (release, resume) = mpsc::channel();
    let server = thread::spawn(move || {
        let (mut stream, request) = identity_routes::accept_request_including_friends(&listener);
        assert!(request.starts_with("POST /session/1/apps/name-fixture/sessions "));
        reply(
            &mut stream,
            200,
            &session_body(external_profile("facebook", "")),
        );
        drop(stream);
        let mut profiles = Vec::new();
        for _ in 0..3 {
            let (stream, request) = identity_routes::accept_request_including_friends(&listener);
            assert!(request.starts_with("GET /v2.0/me?"), "{request}");
            profiles.push(stream);
        }
        arrived.send(()).unwrap();
        resume.recv_timeout(Duration::from_secs(8)).unwrap();
        // HTTP arrival order need not match admission order. Every response
        // has a different ID, so both Friends consumers reject the match while
        // the account consumer independently accepts the returned name.
        for stream in &mut profiles {
            reply(
                stream,
                200,
                r#"{"id":"different-user","name":"Facebook Name"}"#,
            );
        }
        listener
    });
    let (runtime, provider) = configured_runtime(&sandbox, &origin, true);
    begin_login(&runtime, 1);
    ready.recv_timeout(Duration::from_secs(5)).unwrap();
    assert_eq!(
        game_environment(runtime.lua())
            .unwrap()
            .get::<i64>("admission_at_login")
            .unwrap(),
        2
    );
    assert_eq!(provider.admissions.load(Ordering::SeqCst), 3);
    release.send(()).unwrap();
    let listener = server.join().unwrap();
    let queued = runtime.social.online_completion_count_probe();
    wait_for_queue(&queued, 3);
    drain(&runtime);
    let env = game_environment(runtime.lua()).unwrap();
    assert_eq!(env.get::<i64>("account_successes").unwrap(), 1);
    assert_eq!(env.get::<i64>("name_responses").unwrap(), 1);
    assert_eq!(env.get::<String>("returned_name").unwrap(), "Facebook Name");
    assert_sequence(&runtime, &["login", "name"]);
    assert_no_request(&listener);
}

#[test]
fn account_profile_name_shipped_callback_updates_the_original_id_popup() {
    let sandbox = ShippedDataSandbox::new("account-profile-name-shipped-popup");
    let held = HeldName::new(
        200,
        r#"{"id":"different-facebook-id","name":"Restored Platform Name"}"#,
    );
    let (runtime, provider) = configured_runtime(&sandbox, &held.origin, true);
    runtime.boot("scripts/game.lua").unwrap();
    assert!(runtime.gamelogic_loaded());
    held.start.send(()).unwrap();
    let env = game_environment(runtime.lua()).unwrap();
    let facade = env.get::<Table>("SkynestAccount").unwrap();
    wait_until(&runtime, || {
        facade.get::<String>("loginState").unwrap() == "LOGGED_IN"
    });
    held.ready.recv_timeout(Duration::from_secs(5)).unwrap();
    wait_until(&runtime, || {
        runtime.social.native_friends_completions_for_test() == 1
    });
    assert_eq!(
        facade
            .get::<Table>("profile")
            .unwrap()
            .get::<String>("name")
            .unwrap(),
        ""
    );
    assert!(provider.graph.cached_user().is_none());
    // MapScreen.onEntry uses this original public flag to unblock ID popups.
    // Exercise that UI boundary explicitly without assigning a profile/name
    // or replacing either shipped account callback.
    runtime
        .execute_source(
            r#"
        SkynestAccount.setIDPopupBlocked(false)
        shipped_name_popup_before = notificationsFrame:getChild("IDPopup") ~= nil
    "#,
        )
        .unwrap();
    assert!(!env.get::<bool>("shipped_name_popup_before").unwrap());
    let listener = held.finish(&runtime);
    assert_eq!(
        facade
            .get::<Table>("profile")
            .unwrap()
            .get::<String>("name")
            .unwrap(),
        "Restored Platform Name"
    );
    runtime
        .execute_source(
            r#"
        local popup = notificationsFrame:getChild("IDPopup")
        shipped_name_popup_exists = popup ~= nil
        if popup then
            shipped_name_popup_account = popup.accountName
            shipped_name_popup_description = popup:getChild("description").completeText
        end
    "#,
        )
        .unwrap();
    assert!(env.get::<bool>("shipped_name_popup_exists").unwrap());
    assert_eq!(
        env.get::<String>("shipped_name_popup_account").unwrap(),
        "Restored Platform Name"
    );
    assert_eq!(
        env.get::<String>("shipped_name_popup_description").unwrap(),
        "Restored Platform Name"
    );
    assert_no_request(&listener);
}
