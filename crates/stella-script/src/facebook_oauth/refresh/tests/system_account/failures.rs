use super::*;

#[test]
fn facebook_system_account_rejected_failed_password_and_permission_paths_keep_native_errors() {
    for case in 0..11 {
        let graph = listener();
        let account = listener();
        let (session, adapter, tasks, _cache_file) = session(
            &graph,
            &account,
            if case == 8 { 3 } else { 1 },
            0.0,
            case != 5,
        );
        let error = match case {
            6 => json!({"error":{"code":190,"error_subcode":460}}),
            7 => json!({"error":{"code":190,"error_subcode":458}}),
            9 => json!({"error":{"code":200}}),
            10 => json!({"error":{"code":190,"error_subcode":65001}}),
            _ => json!({"error":{"code":190,"error_subcode":463}}),
        };
        let graph_server = thread::spawn(move || {
            let mut stream = accept(&graph);
            test_wire::read_request(&mut stream);
            reply(&mut stream, 400, error);
            graph
        });
        let mut steps = Vec::new();
        if !matches!(case, 6 | 8) {
            let (status, result) = match case {
                0 => (200, json!({"result":1})),
                1 => (200, json!({"result":2})),
                2 => (503, json!({"error":"renew unavailable"})),
                _ => (200, json!({"result":0})),
            };
            steps.push(("GET /renew ", status, result));
        }
        if matches!(case, 3 | 4) {
            steps.push((
                "GET /access?app_id=12345&audience=0 ",
                if case == 3 { 403 } else { 200 },
                json!({"token":null}),
            ));
        }
        let account_server = thread::spawn(move || {
            for (path, status, response) in steps {
                let mut stream = accept(&account);
                let (request, _) = test_wire::read_request(&mut stream);
                assert!(request.starts_with(path));
                reply(&mut stream, status, response);
            }
            account
        });
        let request = session.clone().prepare_user_profile();
        let worker = thread::spawn(move || request.execute());
        pump_until(&tasks, || worker.is_finished());
        assert!(
            matches!(worker.join().unwrap(), Err(SocialPlatformError::Http(400))),
            "case {case}"
        );
        drain(&tasks);
        let expected = if case == 9 {
            FacebookSessionState::Open
        } else {
            FacebookSessionState::Closed
        };
        assert_eq!(session.session_state(), expected, "case {case}");
        assert_eq!(
            session.token_cache.admitted(&[]).unwrap().is_some(),
            case == 9
        );
        let diagnostic = match case {
            2 => Some(SocialPlatformError::Http(503)),
            3 => Some(SocialPlatformError::Http(403)),
            4 => Some(SocialPlatformError::InvalidResponse),
            _ => None,
        };
        assert_eq!(session.take_refresh_error(), diagnostic, "case {case}");
        let events: &[&str] = match case {
            0..=2 | 5 => &["can_without_ui", "renew"],
            3..=4 => &["can_without_ui", "renew", "access"],
            6 => &["force_blocking"],
            8 => &[],
            _ => &["renew"],
        };
        adapter.assert_events(events);
        for socket in [graph_server.join().unwrap(), account_server.join().unwrap()] {
            assert!(
                matches!(socket.accept(), Err(e) if e.kind() == std::io::ErrorKind::WouldBlock),
                "unexpected replay/access in case {case}"
            );
        }
    }
}
