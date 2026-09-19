//! End-to-end session publication -> application scheduler -> IapManager.

use super::*;
use crate::{StellaLua, game_environment};

fn host_fixture() -> StellaLua {
    // The identity remains bound only to MemoryRefreshStore. No saved player
    // account/provider registry is read or modified by these loopback calls.
    let host = StellaLua::new("/tmp").unwrap();
    host.iap.use_session_provider();
    host.execute_source(
        r#"
        session_payment_registrations = 0
        session_payment_successes = 0
        registerPaymentCallbacks = function()
            session_payment_registrations = session_payment_registrations + 1
            _G.IAP.onPaymentInitialized = function()
                session_payment_successes = session_payment_successes + 1
            end
        end
    "#,
    )
    .unwrap();
    host
}

fn registrations(host: &StellaLua) -> u32 {
    game_environment(host.lua())
        .unwrap()
        .get("session_payment_registrations")
        .unwrap()
}

#[test]
fn session_success_publication_drives_iap_but_cached_fast_path_does_not_emit() {
    let host = host_fixture();
    let session = &host.skynest_account.session;
    let (config, server) = server(vec![(200, session_json("a", "r", json!([2])))]);
    session.acquire_session(&config).unwrap();
    assert_eq!(registrations(&host), 0, "publisher must not call Lua");
    server.join().unwrap();
    host.update(0.0).unwrap();
    assert_eq!(registrations(&host), 1);
    host.update(0.0).unwrap(); // async missing-provider failure
    session.acquire_session(&config).unwrap(); // closed loopback: no HTTP on cache hit
    host.update(0.0).unwrap();
    assert_eq!(
        registrations(&host),
        1,
        "cache hit must not restart failed IAP"
    );
    assert_eq!(
        game_environment(host.lua())
            .unwrap()
            .get::<u32>("session_payment_successes")
            .unwrap(),
        0
    );
}

#[test]
fn failed_or_malformed_session_does_not_publish_payment_initialization() {
    for (status, body) in [
        (500, "denied".to_owned()),
        (200, "{}".to_owned()),
        (201, session_json("a", "r", json!([]))),
    ] {
        let host = host_fixture();
        let (config, server) = server(vec![(status, body)]);
        assert!(
            host.skynest_account
                .session
                .acquire_session(&config)
                .is_err()
        );
        server.join().unwrap();
        host.update(1000.0).unwrap();
        assert_eq!(registrations(&host), 0);
        assert!(host.skynest_account.pop_session_success().is_none());
    }
}

#[test]
fn own_profile_continuation_publishes_after_profile_not_after_flat_tokens() {
    let host = host_fixture();
    let session = &host.skynest_account.session;
    let mut owner = session
        .own_profile_owner_for_request(session.request_owner(ProviderLevel::Level2))
        .unwrap();
    host.update(0.0).unwrap();
    assert_eq!(registrations(&host), 0);
    let before = session.login_profile_identity(owner).unwrap();
    assert!(
        session
            .publish_login_profile(
                &mut owner,
                &flat("a", "r", "1"),
                &profile(),
                &super::super::super::identifiers::Identifiers::synthetic(),
                before
            )
            .unwrap()
    );
    assert_eq!(session.level2_tokens().access_token, "a");
    assert_eq!(registrations(&host), 0);
    host.update(0.0).unwrap();
    assert_eq!(registrations(&host), 1);
}

#[test]
fn protected_401_renewal_emits_session_event_without_a_lua_login_callback() {
    let host = host_fixture();
    let session = &host.skynest_account.session;
    let (config, server) = server(vec![
        (401, "expired".to_owned()),
        (200, session_json("new-a", "new-r", json!([2]))),
        (200, "{}".to_owned()),
    ]);
    session.install_flat(&flat("old-a", "old-r", "1"));
    session
        .install_profile_if_epoch(session.epoch(), &profile())
        .unwrap();
    let (epoch, generation) = session.storage_lifetime();
    let response = session
        .execute_storage(
            &config,
            epoch,
            generation,
            &PreparedRequest {
                url: &config.endpoint.request_url("fixture-storage/state"),
                body: None,
                timeout: Duration::from_secs(3),
                still_current: None,
            },
        )
        .unwrap();
    assert_eq!(response.status(), 200);
    server.join().unwrap();
    assert_eq!(registrations(&host), 0);
    host.update(0.0).unwrap();
    assert_eq!(registrations(&host), 1);
}

#[test]
fn retired_session_event_cannot_consume_replacement_event_or_initialize_new_account() {
    let host = host_fixture();
    let session = &host.skynest_account.session;
    let (config, server) = server(vec![
        (200, session_json("old-a", "old-r", json!([]))),
        (200, session_json("new-a", "new-r", json!([]))),
    ]);
    session.acquire_session(&config).unwrap();
    session.logout().unwrap();
    session.acquire_session(&config).unwrap();
    server.join().unwrap();
    host.update(0.0).unwrap();
    assert_eq!(registrations(&host), 1);
    host.update(0.0).unwrap();
    host.update(10.0).unwrap();
    assert_eq!(registrations(&host), 2);
}

#[test]
fn logout_retires_initialization_result_and_delayed_retry() {
    for dispatch_failure_first in [false, true] {
        let host = host_fixture();
        let session = &host.skynest_account.session;
        let (config, server) = server(vec![(200, session_json("a", "r", json!([])))]);
        session.acquire_session(&config).unwrap();
        server.join().unwrap();
        host.update(0.0).unwrap();
        assert_eq!(registrations(&host), 1);
        if dispatch_failure_first {
            host.update(0.0).unwrap();
        }
        session.logout().unwrap();
        host.update(0.0).unwrap();
        host.update(1000.0).unwrap();
        host.update(0.0).unwrap();
        assert_eq!(registrations(&host), 1);
        assert!(host.skynest_account.pop_session_success().is_none());
    }
}

#[test]
fn failed_session_persistence_does_not_emit_a_success_event_before_recovery() {
    for fail_refresh in [false, true] {
        let host = host_fixture();
        let session = &host.skynest_account.session;
        let store = Arc::new(FailingLogoutStore::default());
        session.bind_store(store.clone()).unwrap();
        store.fail_refresh.store(fail_refresh, Ordering::Relaxed);
        store.fail_profile.store(!fail_refresh, Ordering::Relaxed);
        let (config, server) = server(vec![
            (
                200,
                session_json("uncommitted-a", "uncommitted-r", json!([])),
            ),
            (200, session_json("recovered-a", "recovered-r", json!([]))),
        ]);
        assert!(session.acquire_session(&config).is_err());
        host.update(0.0).unwrap();
        assert_eq!(registrations(&host), 0);
        store.fail_refresh.store(false, Ordering::Relaxed);
        store.fail_profile.store(false, Ordering::Relaxed);
        session.acquire_session(&config).unwrap();
        server.join().unwrap();
        host.update(0.0).unwrap();
        assert_eq!(registrations(&host), 1);
    }
}
