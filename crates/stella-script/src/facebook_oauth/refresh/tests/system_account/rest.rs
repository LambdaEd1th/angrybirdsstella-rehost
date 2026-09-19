use super::*;
use std::sync::atomic::{AtomicU64, Ordering};

#[test]
fn facebook_system_account_independent_rest_extension_uses_the_same_repair_chain() {
    let graph = listener();
    let rest = listener();
    let account = listener();
    let (session, adapter, tasks, _cache_file) = session_with_rest(
        &graph,
        &account,
        1,
        0.0,
        true,
        Some(format!("http://{}/v1.0", rest.local_addr().unwrap())),
    );
    let now = token_cache::now();
    let clock = Arc::new(AtomicU64::new(now.to_bits()));
    let time = clock.clone();
    session.state.lock().unwrap().refresh.clock = Some(Arc::new(move || {
        f64::from_bits(time.load(Ordering::SeqCst))
    }));
    let (graph_seen_tx, graph_seen_rx) = mpsc::channel();
    let (graph_release_tx, graph_release_rx) = mpsc::channel();
    let graph_server = thread::spawn(move || {
        let mut stream = accept(&graph);
        let (request, _) = test_wire::read_request(&mut stream);
        assert!(request.starts_with("GET /v2.0/me?"));
        graph_seen_tx.send(()).unwrap();
        graph_release_rx
            .recv_timeout(Duration::from_secs(5))
            .unwrap();
        reply(
            &mut stream,
            200,
            json!({"id":"rest-user","name":"REST User"}),
        );
        graph
    });
    let (rest_seen_tx, rest_seen_rx) = mpsc::channel();
    let (rest_release_tx, rest_release_rx) = mpsc::channel();
    let rest_server = thread::spawn(move || {
        let mut stream = accept(&rest);
        let (request, _) = test_wire::read_request(&mut stream);
        assert!(request.starts_with("GET /v1.0/method/auth.extendSSOAccessToken?format=json&sdk=ios&access_token=synthetic-system-old "));
        rest_seen_tx.send(()).unwrap();
        rest_release_rx
            .recv_timeout(Duration::from_secs(5))
            .unwrap();
        reply(
            &mut stream,
            400,
            json!({"error":{"code":190,"error_subcode":463}}),
        );
        rest
    });
    let account_server = thread::spawn(move || {
        for response in [
            json!({"result":0}),
            json!({"token":"synthetic-rest-repaired"}),
        ] {
            let mut stream = accept(&account);
            test_wire::read_request(&mut stream);
            reply(&mut stream, 200, response);
        }
        account
    });
    let request = session.clone().prepare_user_profile();
    let worker = thread::spawn(move || request.execute());
    graph_seen_rx.recv_timeout(Duration::from_secs(5)).unwrap();
    clock.store((now + 90000.0).to_bits(), Ordering::SeqCst);
    graph_release_tx.send(()).unwrap();
    pump_until(&tasks, || rest_seen_rx.try_recv().is_ok());
    pump_until(&tasks, || worker.is_finished());
    let original = worker.join().unwrap().unwrap();
    assert_eq!(
        session
            .publish_completed_profile(&original)
            .unwrap()
            .access_token,
        "synthetic-system-old"
    );
    adapter.assert_events(&[]);
    rest_release_tx.send(()).unwrap();
    pump_until(&tasks, || {
        session.session_state() == FacebookSessionState::OpenTokenExtended
    });
    pump_until(&tasks, || {
        session.take_refresh_error() == Some(SocialPlatformError::GraphRetryRequired)
    });
    assert_eq!(
        session
            .publish_completed_profile(&original)
            .unwrap()
            .access_token,
        "synthetic-rest-repaired"
    );
    adapter.assert_events(&["can_without_ui", "renew", "access"]);
    for socket in [
        graph_server.join().unwrap(),
        rest_server.join().unwrap(),
        account_server.join().unwrap(),
    ] {
        assert!(matches!(socket.accept(), Err(e) if e.kind() == std::io::ErrorKind::WouldBlock));
    }
}
