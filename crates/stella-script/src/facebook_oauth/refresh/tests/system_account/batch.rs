use super::*;

#[test]
fn facebook_system_account_auxiliary_repair_does_not_hold_primary_profile() {
    let graph = listener();
    let account = listener();
    let (session, adapter, tasks, _cache_file) = session(&graph, &account, 1, 90000.0, true);
    let graph_server = thread::spawn(move || {
        let mut stream = accept(&graph);
        let (headers, body) = test_wire::read_request(&mut stream);
        test_wire::assert_batch(
            &headers,
            &body,
            "12345",
            &["me", "method/auth.extendSSOAccessToken", "me/permissions"],
            "synthetic-system-old",
        );
        let body = test_wire::batch_response(&[
            (200, json!({"id":"original","name":"Original User"})),
            (400, json!({"error":{"code":190,"error_subcode":463}})),
            (
                200,
                json!({"data":[{"public_profile":true,"email":true,"user_friends":true}]}),
            ),
        ]);
        write!(
            stream,
            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        )
        .unwrap();
        graph
    });
    let (seen_tx, seen_rx) = mpsc::channel();
    let (release_tx, release_rx) = mpsc::channel();
    let account_server = thread::spawn(move || {
        let mut stream = accept(&account);
        let (request, _) = test_wire::read_request(&mut stream);
        assert!(request.starts_with("GET /renew "));
        seen_tx.send(()).unwrap();
        release_rx.recv_timeout(Duration::from_secs(5)).unwrap();
        reply(&mut stream, 200, json!({"result":0}));
        let mut stream = accept(&account);
        let (request, _) = test_wire::read_request(&mut stream);
        assert!(request.starts_with("GET /access?app_id=12345&audience=0 "));
        reply(
            &mut stream,
            200,
            json!({"token":"synthetic-batch-repaired"}),
        );
        account
    });
    let request = session.clone().prepare_user_profile();
    let worker = thread::spawn(move || request.execute());
    pump_until(&tasks, || seen_rx.try_recv().is_ok());
    pump_until(&tasks, || worker.is_finished());
    let original = worker.join().unwrap().unwrap();
    let profile = session.publish_completed_profile(&original).unwrap();
    assert_eq!(profile.access_token, "synthetic-system-old");
    assert_eq!(profile.user.id, "original");
    assert_eq!(session.session_state(), FacebookSessionState::Open);
    release_tx.send(()).unwrap();
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
        "synthetic-batch-repaired"
    );
    assert_eq!(session.user_profile().unwrap().user.id, "original");
    adapter.assert_events(&["can_without_ui", "renew", "access"]);
    for socket in [graph_server.join().unwrap(), account_server.join().unwrap()] {
        assert!(matches!(socket.accept(), Err(e) if e.kind() == std::io::ErrorKind::WouldBlock));
    }
}
