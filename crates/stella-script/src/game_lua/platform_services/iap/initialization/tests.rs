use super::*;

fn fixture() -> StellaLua {
    let host = StellaLua::new("/tmp").unwrap();
    host.lua()
        .globals()
        .set(
            "fixture_wallet_busy",
            host.lua()
                .create_function({
                    let iap = host.iap.clone();
                    move |_, ()| Ok(iap.lock_state().wallet_processing)
                })
                .unwrap(),
        )
        .unwrap();
    host.execute_source(
        r#"
        payment_registrations = 0
        payment_successes = 0
        g_iapBundleId = "fixture-bundle"
        registerPaymentCallbacks = function()
            payment_registrations = payment_registrations + 1
            assert(not _G.IAP.native_isPaymentInitialized())
            _G.IAP.onPaymentInitialized = function(bundle)
                assert(bundle == "fixture-bundle")
                assert(_G.fixture_wallet_busy())
                assert(not _G.IAP.native_isPaymentInitialized())
                payment_successes = payment_successes + 1
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
        .get("payment_registrations")
        .unwrap()
}

fn notify_session(host: &StellaLua) {
    host.iap
        .session_succeeded(host.lua(), host.skynest_account.identity_lifetime())
        .unwrap();
}

fn inject_success(host: &StellaLua) {
    let mut state = host.iap.lock_state();
    let (_, Completion::Initialization { succeeded }) = state.completions.back_mut().unwrap()
    else {
        panic!("expected retained initialization result");
    };
    *succeeded = true;
}

#[test]
fn session_provider_waits_for_session_then_fails_asynchronously_without_lua_error() {
    let host = fixture();
    host.iap.use_session_provider();
    assert!(!complete_initialization(host.lua(), &host.iap).unwrap());
    host.update(1.0).unwrap();
    assert_eq!(registrations(&host), 0);
    notify_session(&host);
    assert_eq!(registrations(&host), 1);
    assert_eq!(host.iap.lock_state().initialization, 1);
    notify_session(&host);
    assert_eq!(registrations(&host), 1);
    host.update(0.0).unwrap();
    let state = host.iap.lock_state();
    assert_eq!(state.initialization, 0);
    assert_eq!(state.retry_delay, 20.0);
    assert!(!state.wallet_processing);
    assert!(state.pending_vouchers.is_empty());
    assert_eq!(
        game_environment(host.lua())
            .unwrap()
            .get::<u32>("payment_successes")
            .unwrap(),
        0
    );
}

#[test]
fn initialization_retry_uses_10_second_float_backoff_doubled_and_capped_at_900() {
    let host = fixture();
    host.iap.use_session_provider();
    notify_session(&host);
    host.update(0.0).unwrap();
    for (index, delay) in [10.0, 20.0, 40.0, 80.0, 160.0, 320.0, 640.0, 900.0, 900.0]
        .into_iter()
        .enumerate()
    {
        host.update(delay - 0.25).unwrap();
        assert_eq!(registrations(&host), index as u32 + 1);
        host.execute_source("AnimationWrapperNative.update(1000)")
            .unwrap();
        assert_eq!(
            registrations(&host),
            index as u32 + 1,
            "animation drain must pass S0=0"
        );
        host.update(0.25).unwrap();
        assert_eq!(registrations(&host), index as u32 + 2);
        assert_eq!(host.iap.lock_state().initialization, 1);
        host.update(0.0).unwrap();
        assert_eq!(host.iap.lock_state().initialization, 0);
        assert_eq!(
            host.iap.lock_state().retry_delay,
            ((delay as f32) * 4.0).min(900.0)
        );
    }
}

#[test]
fn session_can_retry_before_timer_and_success_does_not_reset_or_cancel_backoff() {
    let host = fixture();
    host.iap.use_session_provider();
    notify_session(&host);
    host.update(0.0).unwrap(); // timer at 10, next delay 20
    notify_session(&host);
    inject_success(&host);
    host.update(0.0).unwrap();
    assert!(host.iap.is_initialized());
    assert_eq!(host.iap.lock_state().retry_delay, 20.0);
    notify_session(&host);
    host.update(10.0).unwrap(); // retained timer must gate on state 2
    assert_eq!(registrations(&host), 2);
    assert!(host.iap.is_initialized());
    assert_eq!(host.iap.lock_state().retry_delay, 20.0);
}

#[test]
fn older_retry_timer_can_start_after_a_newer_session_attempt_fails() {
    let host = fixture();
    host.iap.use_session_provider();
    notify_session(&host);
    host.update(0.0).unwrap(); // timer 10
    notify_session(&host);
    host.update(0.0).unwrap(); // timer 20, both remain
    host.update(10.0).unwrap();
    assert_eq!(registrations(&host), 3);
    host.update(0.0).unwrap(); // timer 40
    host.update(10.0).unwrap(); // older timer 20 expires
    assert_eq!(registrations(&host), 4);
}

#[test]
fn provider_reconfiguration_retires_old_results_without_consuming_replacements() {
    let host = fixture();
    assert!(complete_initialization(host.lua(), &host.iap).unwrap()); // old offline success
    host.iap.use_session_provider();
    notify_session(&host); // new missing-provider error
    host.update(0.0).unwrap();
    assert_eq!(host.iap.lock_state().initialization, 0);
    assert_eq!(host.iap.lock_state().retry_delay, 20.0);
    assert_eq!(
        game_environment(host.lua())
            .unwrap()
            .get::<u32>("payment_successes")
            .unwrap(),
        0
    );
    assert_eq!(registrations(&host), 2);
}

#[test]
fn local_provider_overrides_identity_mode_and_keeps_success_order() {
    for configure_identity_first in [false, true] {
        let host = fixture();
        if configure_identity_first {
            host.iap.use_session_provider();
        }
        host.iap.enable_local_provider();
        host.iap.use_session_provider();
        assert!(complete_initialization(host.lua(), &host.iap).unwrap());
        host.update(0.0).unwrap();
        assert!(host.iap.is_initialized());
        assert_eq!(host.iap.lock_state().provider, ProviderMode::LocalStore);
        assert_eq!(
            game_environment(host.lua())
                .unwrap()
                .get::<u32>("payment_successes")
                .unwrap(),
            1
        );
        host.update(0.0).unwrap();
        assert!(!host.iap.lock_state().wallet_processing);
    }
}

#[test]
fn repeated_same_provider_configuration_does_not_reinitialize_or_reset_delay() {
    let host = fixture();
    host.iap.use_session_provider();
    notify_session(&host);
    host.update(0.0).unwrap();
    let generation = host.iap.lock_state().generation;
    host.iap.use_session_provider();
    assert_eq!(host.iap.lock_state().generation, generation);
    assert_eq!(host.iap.lock_state().retry_delay, 20.0);
    host.update(10.0).unwrap();
    assert_eq!(registrations(&host), 2);
}

#[test]
fn payment_retry_uses_raw_frame_delta_not_game_time_multiplier() {
    let host = fixture();
    host.iap.use_session_provider();
    notify_session(&host);
    host.update(0.0).unwrap();
    host.execute_source("setDeltaTimeMultiplier(0)").unwrap();
    host.update(9.75).unwrap();
    assert_eq!(registrations(&host), 1);
    host.update(0.25).unwrap();
    assert_eq!(registrations(&host), 2);
}

#[test]
fn script_registration_error_is_not_the_typed_native_cloud_service_exception() {
    let host = fixture();
    host.iap.use_session_provider();
    host.execute_source(
        "registerPaymentCallbacks = function() error('fixture-registration-error') end",
    )
    .unwrap();
    let error = host
        .iap
        .session_succeeded(host.lua(), host.skynest_account.identity_lifetime())
        .unwrap_err();
    assert!(error.to_string().contains("fixture-registration-error"));
    assert_eq!(host.iap.lock_state().initialization, 1);
    assert_eq!(host.iap.lock_state().retry_delay, 10.0);
    // Only the already posted provider failure schedules the first retry.
    host.update(0.0).unwrap();
    assert_eq!(host.iap.lock_state().initialization, 0);
    assert_eq!(host.iap.lock_state().retry_delay, 20.0);
}
