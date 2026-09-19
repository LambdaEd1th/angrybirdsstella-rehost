//! Runtime ownership tests use memory-only providers and synthetic loopback HTTP.

use super::super::super::{dispatch_local_completion, dispatch_online_completion};
use super::*;

fn configure_memory(runtime: &SkynestAccountRuntime, config: &IdentityConfig) {
    *runtime.compatible_url.lock().unwrap() = Some(config.endpoint.clone());
    *runtime.client_id.lock().unwrap() = config.client_id.clone();
    *runtime.client_signing.lock().unwrap() = config.signing.clone();
    // This fixture deliberately retains its injected MemoryRefreshStore. The
    // synthetic path is only an equality marker, never opened/read/written.
    *runtime.bound_registry.borrow_mut() = Some(registry_path(&runtime.registry_root, config));
    runtime.session.install_level1_flat(&access("level1"));
}

fn receive(listener: &TcpListener) -> (TcpStream, String) {
    let deadline = Instant::now() + Duration::from_secs(5);
    let mut stream = loop {
        match listener.accept() {
            Ok((stream, _)) => break stream,
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                assert!(Instant::now() < deadline, "synthetic request timed out");
                thread::sleep(Duration::from_millis(1));
            }
            Err(error) => panic!("loopback accept failed: {error}"),
        }
    };
    stream.set_nonblocking(false).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(5)))
        .unwrap();
    let mut bytes = Vec::new();
    loop {
        if let Some(header_end) = bytes.windows(4).position(|w| w == b"\r\n\r\n") {
            let headers = String::from_utf8_lossy(&bytes[..header_end]);
            let length = headers
                .lines()
                .find_map(|line| {
                    let (key, value) = line.split_once(':')?;
                    key.eq_ignore_ascii_case("content-length")
                        .then(|| value.trim().parse::<usize>().unwrap())
                })
                .unwrap_or(0);
            if bytes.len() >= header_end + 4 + length {
                return (stream, String::from_utf8(bytes).unwrap());
            }
        }
        let mut chunk = [0; 1024];
        let count = stream.read(&mut chunk).unwrap();
        assert_ne!(count, 0, "incomplete synthetic HTTP request");
        bytes.extend_from_slice(&chunk[..count]);
        assert!(bytes.len() <= 65536, "bounded synthetic request");
    }
}

fn respond(mut stream: TcpStream, status: u16, value: serde_json::Value) {
    let body = value.to_string();
    write!(
        stream,
        "HTTP/1.1 {status} Fixture\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    )
    .unwrap();
}

fn token_json(id: &str) -> serde_json::Value {
    let access = access(id);
    serde_json::json!({
        "accessToken": access.access_token, "refreshToken": access.refresh_token,
        "expiresIn": 3600, "segment": "synthetic-segments"
    })
}

fn assert_unchanged_account(runtime: &SkynestAccountRuntime, id: &str) {
    assert_eq!(runtime.session.profile().unwrap().public_account_id, id);
    assert_eq!(
        runtime.session.level2_tokens().access_token,
        access(id).access_token
    );
}

fn assert_no_callbacks(lua: &Lua) {
    assert_eq!(lua.globals().get::<u32>("login_successes").unwrap(), 0);
    assert_eq!(lua.globals().get::<u32>("login_failures").unwrap(), 0);
}

fn assert_no_request(listener: &TcpListener) {
    assert!(
        matches!(listener.accept(), Err(error) if error.kind() == std::io::ErrorKind::WouldBlock),
        "stale operation must not send another HTTP request"
    );
}

fn sign_in(runtime: &SkynestAccountRuntime) -> u64 {
    runtime.begin_interactive(false).unwrap();
    let id = runtime.ui_snapshot().unwrap().id;
    assert!(
        runtime
            .submit_login(id, "person@example.invalid", "synthetic-password")
            .unwrap()
    );
    id
}

fn ready_registration(runtime: &SkynestAccountRuntime, guest: bool) -> u64 {
    if guest {
        let mut profile = runtime.session.profile().unwrap();
        profile.personal.email.clear();
        profile.raw = serde_json::json!({"publicAccountId":"account-a"});
        assert!(
            runtime
                .session
                .install_profile_if_epoch(runtime.session.epoch(), &profile)
                .unwrap()
        );
    }
    runtime.begin_interactive(true).unwrap();
    let id = runtime.ui_snapshot().unwrap().id;
    assert!(
        runtime
            .submit_birthday(
                id,
                RegistrationBirthday {
                    day: 1,
                    month: 1,
                    year: 2000
                }
            )
            .unwrap()
    );
    assert!(
        runtime
            .submit_registration(
                id,
                "person@example.invalid",
                "synthetic-password",
                AccountGender::Female
            )
            .unwrap()
    );
    id
}

#[test]
fn identity_runtime_signin_stale_response_and_both_scheduler_stages_do_not_replace_new_identity() {
    for status in [200, 404] {
        for stage in 0..if status == 200 { 3 } else { 2 } {
            let (lua, runtime, store) = fixture();
            let (config, listener) = bind();
            configure_memory(&runtime, &config);
            let id = sign_in(&runtime);
            let (stream, request) = receive(&listener);
            assert!(request.starts_with("POST /identity/3.0/abid/login "));
            assert!(
                request
                    .to_ascii_lowercase()
                    .contains("x-access-token: account-a-synthetic-access\r\n")
            );
            if stage == 0 {
                replace_identity(&runtime.session, "account-c");
            }
            respond(stream, status, token_json("account-b"));
            await_profile_result(&runtime);
            if stage == 1 {
                replace_identity(&runtime.session, "account-c");
            }
            dispatch_online_completion(&lua, &runtime).unwrap();
            if stage == 2 {
                assert!(runtime.ui_snapshot().is_none());
                assert_eq!(runtime.completions.borrow().len(), 1);
                replace_identity(&runtime.session, "account-c");
            }
            dispatch_local_completion(&lua, &runtime).unwrap();
            assert_unchanged_account(&runtime, "account-c");
            assert_eq!(store.load().unwrap(), "account-c-synthetic-refresh");
            assert_eq!(
                store.load_profile().unwrap(),
                Some(raw_profile("account-c"))
            );
            assert_no_callbacks(&lua);
            if let Some(ui) = runtime.ui_snapshot() {
                assert_eq!(ui.id, id);
                assert!(!ui.busy, "cancelled submission must leave Progress");
                assert!(ui.field_error.is_none());
            }
            assert!(!runtime.state.lock().unwrap().login_in_progress);
            assert_no_request(&listener);
        }
    }
}

#[test]
fn identity_runtime_stale_signin_cannot_clear_a_new_submission_in_the_same_dialog() {
    let (lua, runtime, _) = fixture();
    let (config, listener) = bind();
    configure_memory(&runtime, &config);
    let id = sign_in(&runtime);
    let (old_stream, _) = receive(&listener);
    replace_identity(&runtime.session, "account-c");
    assert!(
        runtime
            .ui_action(id, AccountUiAction::ForgotPassword)
            .unwrap()
    );
    assert!(runtime.ui_action(id, AccountUiAction::Continue).unwrap());
    assert!(
        runtime
            .submit_login(id, "new@example.invalid", "new-synthetic-password")
            .unwrap()
    );
    let new_job = runtime.active_login_job.get();
    let (new_stream, request) = receive(&listener);
    assert!(
        request
            .to_ascii_lowercase()
            .contains("x-access-token: account-c-synthetic-access\r\n")
    );
    respond(old_stream, 404, serde_json::json!({}));
    await_profile_result(&runtime);
    dispatch_online_completion(&lua, &runtime).unwrap();
    assert!(runtime.ui_snapshot().unwrap().busy);
    assert_eq!(runtime.active_login_job.get(), new_job);
    assert!(runtime.state.lock().unwrap().login_in_progress);
    respond(new_stream, 404, serde_json::json!({}));
    await_profile_result(&runtime);
    dispatch_online_completion(&lua, &runtime).unwrap();
    let ui = runtime.ui_snapshot().unwrap();
    assert_eq!(ui.id, id);
    assert!(!ui.busy);
    assert_eq!(
        ui.field_error,
        Some(AccountFieldError {
            field: 18,
            message: 3
        })
    );
    assert_unchanged_account(&runtime, "account-c");
    assert_no_callbacks(&lua);
    assert_no_request(&listener);
}

#[test]
fn identity_runtime_401_identity_change_cancels_signin_and_guest_upgrade_without_stuck_progress() {
    for guest in [false, true] {
        let (lua, runtime, _) = fixture();
        let (config, listener) = bind();
        configure_memory(&runtime, &config);
        let id = if guest {
            ready_registration(&runtime, true)
        } else {
            sign_in(&runtime)
        };
        let (stream, request) = receive(&listener);
        assert!(request.starts_with(if guest {
            "POST /identity/3.0/guest/upgrade "
        } else {
            "POST /identity/3.0/abid/login "
        }));
        respond(stream, 401, serde_json::json!({}));
        let (renewal, request) = receive(&listener);
        assert!(request.starts_with("POST /session/1/apps/synthetic-client/sessions "));
        respond(
            renewal,
            200,
            serde_json::json!({
                "userAuth": token_json("account-b"), "profile": raw_profile("account-b"),
                "segments": [3], "config": {}
            }),
        );
        await_profile_result(&runtime);
        dispatch_online_completion(&lua, &runtime).unwrap();
        dispatch_local_completion(&lua, &runtime).unwrap();
        let ui = runtime.ui_snapshot().unwrap();
        assert_eq!(ui.id, id);
        assert_eq!(
            ui.view,
            if guest {
                AccountView::Register2
            } else {
                AccountView::SignIn
            }
        );
        assert!(!ui.busy);
        assert!(!runtime.state.lock().unwrap().login_in_progress);
        assert_unchanged_account(&runtime, "account-b");
        assert_no_callbacks(&lua);
        assert_no_request(&listener);
    }
}

#[test]
fn identity_runtime_registration_retains_install_authority_through_success_confirmation() {
    for stage in 0..4 {
        let (lua, runtime, _) = fixture();
        let (config, listener) = bind();
        configure_memory(&runtime, &config);
        let id = ready_registration(&runtime, false);
        let (stream, request) = receive(&listener);
        assert!(request.starts_with("POST /identity/2.0/abid/register "));
        assert!(
            request
                .to_ascii_lowercase()
                .contains("x-access-token: level1-synthetic-access\r\n")
        );
        if stage == 0 {
            replace_identity(&runtime.session, "account-c");
        }
        respond(stream, 200, token_json("account-b"));
        await_profile_result(&runtime);
        if stage == 1 {
            replace_identity(&runtime.session, "account-c");
        }
        dispatch_online_completion(&lua, &runtime).unwrap();
        if stage == 2 {
            assert_eq!(runtime.completions.borrow().len(), 1);
            replace_identity(&runtime.session, "account-c");
        }
        dispatch_local_completion(&lua, &runtime).unwrap();
        if stage == 3 {
            assert_eq!(
                runtime.ui_snapshot().unwrap().view,
                AccountView::ThanksForRegistering
            );
            replace_identity(&runtime.session, "account-c");
            assert!(runtime.ui_action(id, AccountUiAction::Continue).unwrap());
            dispatch_local_completion(&lua, &runtime).unwrap();
        }
        assert_unchanged_account(&runtime, "account-c");
        assert!(runtime.ui_snapshot().is_none_or(|ui| !ui.busy));
        assert!(!runtime.state.lock().unwrap().login_in_progress);
        assert_no_callbacks(&lua);
        assert_no_request(&listener);
    }
}

#[test]
fn identity_runtime_level1_email_keeps_native_same_owner_results_after_identity_replacement() {
    for stage in 0..3 {
        let (lua, runtime, _) = fixture();
        let (config, listener) = bind();
        configure_memory(&runtime, &config);
        runtime.begin_interactive(false).unwrap();
        let id = runtime.ui_snapshot().unwrap().id;
        assert!(
            runtime
                .validate_field(
                    id,
                    AccountValidationField::Email,
                    7,
                    "person@example.invalid"
                )
                .unwrap()
        );
        let (stream, request) = receive(&listener);
        assert!(request.starts_with("POST /identity/2.0/abid/validate/email "));
        assert!(
            request
                .to_ascii_lowercase()
                .contains("x-access-token: level1-synthetic-access\r\n")
        );
        if stage == 0 {
            replace_identity(&runtime.session, "account-c");
        }
        respond(stream, 200, serde_json::json!({"code":10}));
        await_profile_result(&runtime);
        if stage == 1 {
            replace_identity(&runtime.session, "account-c");
        }
        dispatch_online_completion(&lua, &runtime).unwrap();
        if stage == 2 {
            replace_identity(&runtime.session, "account-c");
        }
        dispatch_local_completion(&lua, &runtime).unwrap();
        let feedback = runtime.take_validation_results();
        assert_eq!(feedback.len(), 1);
        assert_eq!(feedback[0].id, id);
        assert_eq!(feedback[0].generation, 7);
        assert!(feedback[0].valid);
        assert!(feedback[0].error.is_none());
        assert_unchanged_account(&runtime, "account-c");
        assert_no_request(&listener);
    }
}
