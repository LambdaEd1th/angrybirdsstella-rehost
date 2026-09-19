use super::*;
use std::sync::atomic::{AtomicBool, Ordering};

#[test]
fn facebook_system_account_captures_adapter_and_sink_and_discards_retired_callbacks() {
    for retire in [false, true] {
        let graph = listener();
        let account = listener();
        let (session, adapter, tasks, _cache_file) = session(&graph, &account, 1, 0.0, true);
        let alive = Arc::new(AtomicBool::new(true));
        let lifetime = alive.clone();
        let old_sink = tasks.clone();
        session.set_application_dispatcher(Arc::new(move |task| {
            if lifetime.load(Ordering::SeqCst) {
                old_sink.lock().unwrap().push_back(task);
            }
        }));
        let graph_server = thread::spawn(move || {
            let mut stream = accept(&graph);
            test_wire::read_request(&mut stream);
            reply(
                &mut stream,
                400,
                json!({"error":{"code":190,"error_subcode":463}}),
            );
        });
        let (seen_tx, seen_rx) = mpsc::channel();
        let (release_tx, release_rx) = mpsc::channel();
        let account_server = thread::spawn(move || {
            let mut stream = accept(&account);
            test_wire::read_request(&mut stream);
            seen_tx.send(()).unwrap();
            release_rx.recv_timeout(Duration::from_secs(5)).unwrap();
            reply(&mut stream, 200, json!({"result":0}));
            if !retire {
                let mut stream = accept(&account);
                test_wire::read_request(&mut stream);
                reply(
                    &mut stream,
                    200,
                    json!({"token":"synthetic-captured-adapter"}),
                );
            }
            account
        });
        let request = session.clone().prepare_user_profile();
        let worker = thread::spawn(move || request.execute());
        pump_until(&tasks, || seen_rx.try_recv().is_ok());
        let new_tasks = Tasks::default();
        let new_sink = new_tasks.clone();
        session.set_application_dispatcher(Arc::new(move |task| {
            new_sink.lock().unwrap().push_back(task)
        }));
        let replacement = Arc::new(HttpAccount {
            root: "http://127.0.0.1:9".into(),
            without_ui: true,
            events: Mutex::new(Vec::new()),
        });
        session.set_system_account_adapter(replacement.clone());
        alive.store(!retire, Ordering::SeqCst);
        release_tx.send(()).unwrap();
        pump_until(&tasks, || worker.is_finished());
        assert!(
            new_tasks.lock().unwrap().is_empty(),
            "old callback rerouted to replacement sink"
        );
        let expected = if retire {
            SocialPlatformError::Cancelled
        } else {
            SocialPlatformError::GraphRetryRequired
        };
        assert!(matches!(worker.join().unwrap(), Err(error) if error == expected));
        drain(&tasks);
        replacement.assert_events(&[]);
        if retire {
            adapter.assert_events(&["can_without_ui", "renew"]);
            assert_eq!(session.session_state(), FacebookSessionState::Open);
            assert_eq!(
                session.token_cache.admitted(&[]).unwrap().unwrap().token,
                "synthetic-system-old"
            );
        } else {
            adapter.assert_events(&["can_without_ui", "renew", "access"]);
            assert_eq!(
                session.session_state(),
                FacebookSessionState::OpenTokenExtended
            );
        }
        graph_server.join().unwrap();
        let socket = account_server.join().unwrap();
        assert!(matches!(socket.accept(), Err(e) if e.kind() == std::io::ErrorKind::WouldBlock));
        assert_eq!(
            Arc::weak_count(&adapter),
            0,
            "completed callback retained its adapter"
        );
    }
}

#[test]
fn facebook_system_account_logout_retires_wait_and_late_restored_token() {
    let graph = listener();
    let account = listener();
    let (session, adapter, tasks, _cache_file) = session(&graph, &account, 1, 0.0, true);
    let graph_server = thread::spawn(move || {
        let mut stream = accept(&graph);
        test_wire::read_request(&mut stream);
        reply(
            &mut stream,
            400,
            json!({"error":{"code":190,"error_subcode":463}}),
        );
    });
    let (seen_tx, seen_rx) = mpsc::channel();
    let (release_tx, release_rx) = mpsc::channel();
    let account_server = thread::spawn(move || {
        let mut stream = accept(&account);
        test_wire::read_request(&mut stream);
        reply(&mut stream, 200, json!({"result":0}));
        let mut stream = accept(&account);
        test_wire::read_request(&mut stream);
        seen_tx.send(()).unwrap();
        release_rx.recv_timeout(Duration::from_secs(5)).unwrap();
        reply(
            &mut stream,
            200,
            json!({"token":"must-not-return-after-logout"}),
        );
    });
    let request = session.clone().prepare_user_profile();
    let worker = thread::spawn(move || request.execute());
    pump_until(&tasks, || seen_rx.try_recv().is_ok());
    session.logout().unwrap();
    pump_until(&tasks, || worker.is_finished());
    assert!(matches!(
        worker.join().unwrap(),
        Err(SocialPlatformError::Cancelled)
    ));
    release_tx.send(()).unwrap();
    account_server.join().unwrap();
    graph_server.join().unwrap();
    // Wait for the actual adapter callback to return and release its context.
    let deadline = Instant::now() + Duration::from_secs(5);
    while Arc::weak_count(&adapter) != 0 {
        drain(&tasks);
        assert!(Instant::now() < deadline);
        thread::sleep(Duration::from_millis(1));
    }
    drain(&tasks);
    assert_eq!(session.session_state(), FacebookSessionState::Closed);
    assert!(session.token_cache.admitted(&[]).unwrap().is_none());
    adapter.assert_events(&["can_without_ui", "renew", "access"]);
}
