use super::*;
use std::{
    net::TcpStream,
    sync::{
        atomic::{AtomicU64, Ordering},
        mpsc,
    },
    time::Instant,
};

type Tasks = Arc<Mutex<VecDeque<SocialPlatformTask>>>;

fn accept(listener: &TcpListener) -> TcpStream {
    let end = Instant::now() + Duration::from_secs(5);
    loop {
        match listener.accept() {
            Ok((stream, _)) => {
                stream.set_nonblocking(false).unwrap();
                stream
                    .set_read_timeout(Some(Duration::from_secs(5)))
                    .unwrap();
                return stream;
            }
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                assert!(Instant::now() < end, "SDK request did not arrive");
                thread::sleep(Duration::from_millis(1));
            }
            Err(error) => panic!("{error}"),
        }
    }
}

fn task(tasks: &Tasks) -> SocialPlatformTask {
    let end = Instant::now() + Duration::from_secs(5);
    loop {
        if let Some(task) = tasks.lock().unwrap().pop_front() {
            return task;
        }
        assert!(Instant::now() < end, "SDK completion did not arrive");
        thread::sleep(Duration::from_millis(1));
    }
}

fn session(
    root: String,
    api: Option<String>,
) -> (Arc<FacebookOAuthSession>, Arc<AtomicU64>, Tasks) {
    let session = Arc::new(
        FacebookOAuthSession::new(
            FacebookOAuthConfig {
                graph_root: root,
                rest_root: api,
                authorization_url: "http://127.0.0.1:9/oauth".into(),
                app_id: "12345".into(),
                url_scheme_suffix: String::new(),
                request_birthday: false,
            },
            |_| Ok(true),
        )
        .unwrap(),
    );
    assert!(matches!(
        session.clone().prepare_login(),
        SocialLoginRequest::AwaitingCallback
    ));
    session
        .handle_open_url("fb12345://authorize#access_token=synthetic-old")
        .unwrap();
    assert_eq!(session.take_login_completion(), Some(Ok(())));
    let clock = Arc::new(AtomicU64::new(200000.0f64.to_bits()));
    let time = clock.clone();
    let mut data = metadata(200000.0 - 86400.0 + 1.0);
    data.refresh_permissions(
        vec![
            "public_profile".into(),
            "email".into(),
            "user_friends".into(),
        ],
        200000.0,
    );
    {
        let mut state = session.state.lock().unwrap();
        state.refresh.install(data);
        state.refresh.clock = Some(Arc::new(move || {
            f64::from_bits(time.load(Ordering::SeqCst))
        }));
    }
    let tasks = Arc::new(Mutex::new(VecDeque::new()));
    let queue = tasks.clone();
    session.set_application_dispatcher(Arc::new(move |task| queue.lock().unwrap().push_back(task)));
    (session, clock, tasks)
}

fn graph_response(
    session: &Arc<FacebookOAuthSession>,
    clock: &AtomicU64,
    listener: TcpListener,
) -> SocialPlatformProfile {
    graph_result(session, clock, listener, 200).unwrap()
}

fn graph_result(
    session: &Arc<FacebookOAuthSession>,
    clock: &AtomicU64,
    listener: TcpListener,
    status: u16,
) -> Result<SocialPlatformProfile, SocialPlatformError> {
    let request = session.clone().prepare_user_profile();
    let worker = thread::spawn(move || request.execute());
    let mut stream = accept(&listener);
    let (headers, body) = test_wire::read_request(&mut stream);
    assert!(
        headers.starts_with(
            "GET /v2.0/me?format=json&sdk=ios&access_token=synthetic-old HTTP/1.1\r\n"
        )
    );
    assert!(body.is_empty());
    // Advance across the actual admission boundary while HTTP is in flight.
    clock.store(200002.0f64.to_bits(), Ordering::SeqCst);
    let body = r#"{"id":"profile","name":"Original response"}"#;
    write!(
        stream,
        "HTTP/1.1 {status} Fixture\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    )
    .unwrap();
    drop(stream);
    worker.join().unwrap()
}

#[test]
fn facebook_oauth_completion_extension_uses_api_origin_without_waiting_for_its_response() {
    let graph = TcpListener::bind("127.0.0.1:0").unwrap();
    graph.set_nonblocking(true).unwrap();
    let api = TcpListener::bind("127.0.0.1:0").unwrap();
    api.set_nonblocking(true).unwrap();
    let (session, clock, tasks) = session(
        format!("http://{}/v2.0", graph.local_addr().unwrap()),
        Some(format!("http://{}/v2.0", api.local_addr().unwrap())),
    );
    let (arrived, arrival) = mpsc::channel();
    let (release, resume) = mpsc::channel();
    let server = thread::spawn(move || {
        let mut stream = accept(&api);
        let (headers, body) = test_wire::read_request(&mut stream);
        assert!(headers.starts_with("GET /v2.0/method/auth.extendSSOAccessToken?format=json&sdk=ios&access_token=synthetic-old HTTP/1.1\r\n"));
        assert!(body.is_empty());
        arrived.send(()).unwrap();
        resume.recv_timeout(Duration::from_secs(5)).unwrap();
        let body = r#"{"access_token":"synthetic-new","expires_at":4102444800}"#;
        write!(
            stream,
            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        )
        .unwrap();
        api
    });
    let profile = graph_response(&session, &clock, graph);
    let began = Instant::now();
    task(&tasks).run();
    assert!(
        began.elapsed() < Duration::from_secs(1),
        "primary completion waited for REST response"
    );
    arrival.recv_timeout(Duration::from_secs(5)).unwrap();
    let published = session.publish_completed_profile(&profile).unwrap();
    assert_eq!(published.access_token, "synthetic-old");
    assert_eq!(session.session_state(), FacebookSessionState::Open);
    // A different installation must not capture this already-started REST
    // completion. Runtime lifetime flags decide whether the old sink runs it.
    let replacement_tasks: Tasks = Arc::new(Mutex::new(VecDeque::new()));
    let replacement = replacement_tasks.clone();
    session.set_application_dispatcher(Arc::new(move |task| {
        replacement.lock().unwrap().push_back(task)
    }));
    release.send(()).unwrap();
    let api = server.join().unwrap();
    let update = task(&tasks);
    assert!(replacement_tasks.lock().unwrap().is_empty());
    assert_eq!(session.session_state(), FacebookSessionState::Open);
    update.run();
    assert_eq!(
        session.session_state(),
        FacebookSessionState::OpenTokenExtended
    );
    match session.clone().prepare_user_profile() {
        SocialProfileRequest::Ready(Ok(profile)) => {
            assert_eq!(profile.access_token, "synthetic-new")
        }
        _ => panic!("independent extension discarded completed service user"),
    }
    assert_eq!(session.take_refresh_error(), None);
    assert!(matches!(api.accept(),Err(e) if e.kind()==std::io::ErrorKind::WouldBlock));
}

#[test]
fn facebook_oauth_completion_extension_requires_explicit_rest_configuration() {
    let graph = TcpListener::bind("127.0.0.1:0").unwrap();
    graph.set_nonblocking(true).unwrap();
    let (session, clock, tasks) =
        session(format!("http://{}/v2.0", graph.local_addr().unwrap()), None);
    let profile = graph_response(&session, &clock, graph);
    task(&tasks).run();
    assert_eq!(
        session
            .publish_completed_profile(&profile)
            .unwrap()
            .access_token,
        "synthetic-old"
    );
    assert_eq!(
        session.take_refresh_error(),
        Some(SocialPlatformError::Unavailable)
    );
    assert_eq!(session.take_refresh_error(), None);
    assert_eq!(session.session_state(), FacebookSessionState::Open);
}

#[test]
fn facebook_oauth_completion_extension_can_follow_a_failed_primary_request() {
    let graph = TcpListener::bind("127.0.0.1:0").unwrap();
    graph.set_nonblocking(true).unwrap();
    let api = TcpListener::bind("127.0.0.1:0").unwrap();
    api.set_nonblocking(true).unwrap();
    let (session, clock, tasks) = session(
        format!("http://{}/v2.0", graph.local_addr().unwrap()),
        Some(format!("http://{}/v2.0", api.local_addr().unwrap())),
    );
    let server = thread::spawn(move || {
        let mut stream = accept(&api);
        let (headers, body) = test_wire::read_request(&mut stream);
        assert!(headers.starts_with("GET /v2.0/method/auth.extendSSOAccessToken?"));
        assert!(body.is_empty());
        let body = r#"{"access_token":"synthetic-new","expires_at":4102444800}"#;
        write!(
            stream,
            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        )
        .unwrap();
    });
    assert!(matches!(
        graph_result(&session, &clock, graph, 503),
        Err(SocialPlatformError::Http(503))
    ));
    task(&tasks).run();
    server.join().unwrap();
    task(&tasks).run();
    assert_eq!(
        session.session_state(),
        FacebookSessionState::OpenTokenExtended
    );
    assert!(matches!(
        session.clone().prepare_user_profile(),
        SocialProfileRequest::Pending(_)
    ));
    assert_eq!(session.take_refresh_error(), None);
}

#[test]
fn facebook_oauth_completion_extension_errors_and_retirement_do_not_fake_refresh_success() {
    for retired in [false, true] {
        let graph = TcpListener::bind("127.0.0.1:0").unwrap();
        graph.set_nonblocking(true).unwrap();
        let api = TcpListener::bind("127.0.0.1:0").unwrap();
        api.set_nonblocking(true).unwrap();
        let (session, clock, tasks) = session(
            format!("http://{}/v2.0", graph.local_addr().unwrap()),
            Some(format!("http://{}/v2.0", api.local_addr().unwrap())),
        );
        let server = thread::spawn(move || {
            let mut stream = accept(&api);
            let (headers, body) = test_wire::read_request(&mut stream);
            assert!(headers.starts_with("GET /v2.0/method/auth.extendSSOAccessToken?"));
            assert!(body.is_empty());
            let body = if retired {
                r#"{"access_token":"late-token","expires_at":4102444800}"#
            } else {
                r#"{"error":{"code":190}}"#
            };
            write!(
                stream,
                "HTTP/1.1 {} Fixture\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                if retired { 200 } else { 400 },
                body.len()
            )
            .unwrap();
        });
        let profile = graph_response(&session, &clock, graph);
        task(&tasks).run();
        session.publish_completed_profile(&profile).unwrap();
        server.join().unwrap();
        let update = task(&tasks);
        if retired {
            session.logout().unwrap();
        }
        update.run();
        assert_eq!(session.session_state(), FacebookSessionState::Closed);
        assert!(session.token_cache.admitted(&[]).unwrap().is_none());
        assert_eq!(
            session.take_refresh_error(),
            if retired {
                None
            } else {
                Some(SocialPlatformError::Http(400))
            }
        );
    }
}
