//! Native capture-result events, distinct from the virtual host's pending code.

use super::*;

fn scanner_runtime() -> StellaLua {
    let runtime = StellaLua::new("/tmp").unwrap();
    runtime.set_qr_scanner_available(true).unwrap();
    runtime
        .execute_source(
            r#"
                qr_received = {}
                function notifyEventManager() end
                function update() end
                function qr_receive(code)
                    table.insert(qr_received, code)
                end
            "#,
        )
        .unwrap();
    runtime
}

fn received(runtime: &StellaLua) -> Vec<String> {
    game_environment(runtime.lua())
        .unwrap()
        .get::<mlua::Table>("qr_received")
        .unwrap()
        .sequence_values::<String>()
        .collect::<mlua::Result<_>>()
        .unwrap()
}

#[test]
fn start_posts_a_pending_capture_without_a_callback_and_never_calls_lua_inline() {
    let runtime = scanner_runtime();
    assert!(!runtime.submit_qr_code("before-start").unwrap());
    runtime.execute_source("QrScanner.start()").unwrap();
    assert!(!runtime.submit_qr_code("pending-after-start").unwrap());
    // No callback is needed to accept a frame. Installing one before delivery
    // receives that existing result, but the setter itself cannot call Lua.
    runtime
        .execute_source("QrScanner.setQrRecognizedCallback(qr_receive)")
        .unwrap();
    assert!(received(&runtime).is_empty());
    dispatch_registered_application_events(runtime.lua()).unwrap();
    assert_eq!(received(&runtime), ["before-start"]);
    dispatch_registered_application_events(runtime.lua()).unwrap();
    assert_eq!(received(&runtime), ["before-start"]);
}

#[test]
fn unsupported_start_does_not_arm_a_future_camera_without_another_start() {
    let runtime = scanner_runtime();
    runtime.set_qr_scanner_available(false).unwrap();
    assert!(!runtime.submit_qr_code("wait-for-camera").unwrap());
    runtime
        .execute_source("QrScanner.start(); QrScanner.setQrRecognizedCallback(qr_receive)")
        .unwrap();
    runtime.set_qr_scanner_available(true).unwrap();
    runtime.update(0.0).unwrap();
    assert!(received(&runtime).is_empty());
    runtime.execute_source("QrScanner.start()").unwrap();
    assert!(received(&runtime).is_empty());
    dispatch_registered_application_events(runtime.lua()).unwrap();
    assert_eq!(received(&runtime), ["wait-for-camera"]);
}

#[test]
fn posted_result_survives_stop_and_uses_the_callback_installed_at_delivery() {
    let runtime = scanner_runtime();
    runtime
        .execute_source(
            r#"
                QrScanner.setQrRecognizedCallback(function()
                    error("replaced callback must not run")
                end)
                QrScanner.start()
            "#,
        )
        .unwrap();
    assert!(runtime.submit_qr_code("captured").unwrap());
    assert!(received(&runtime).is_empty());
    runtime.set_qr_scanner_available(false).unwrap();
    runtime
        .execute_source("QrScanner.stop(); QrScanner.setQrRecognizedCallback(qr_receive)")
        .unwrap();
    dispatch_registered_application_events(runtime.lua()).unwrap();
    assert_eq!(received(&runtime), ["captured"]);

    runtime.set_qr_scanner_available(true).unwrap();
    assert!(!runtime.submit_qr_code("not-captured").unwrap());
    dispatch_registered_application_events(runtime.lua()).unwrap();
    assert_eq!(received(&runtime), ["captured"]);
    runtime.execute_source("QrScanner.start()").unwrap();
    assert_eq!(received(&runtime), ["captured"]);
    dispatch_registered_application_events(runtime.lua()).unwrap();
    assert_eq!(received(&runtime), ["captured", "not-captured"]);
}

#[test]
fn clearing_a_callback_discards_the_posted_result_without_replaying_it() {
    for clear in ["", "nil", "false", "42", "'not-a-function'", "{}"] {
        let runtime = scanner_runtime();
        runtime
            .execute_source("QrScanner.start(); QrScanner.setQrRecognizedCallback(qr_receive)")
            .unwrap();
        assert!(runtime.submit_qr_code("discarded").unwrap());
        runtime
            .execute_source(&format!("QrScanner.setQrRecognizedCallback({clear})"))
            .unwrap();
        dispatch_registered_application_events(runtime.lua()).unwrap();
        assert!(received(&runtime).is_empty(), "clear tag {clear}");
        runtime
            .execute_source("QrScanner.setQrRecognizedCallback(qr_receive)")
            .unwrap();
        dispatch_registered_application_events(runtime.lua()).unwrap();
        assert!(received(&runtime).is_empty(), "replay after clear {clear}");
        assert!(runtime.submit_qr_code("fresh").unwrap());
        dispatch_registered_application_events(runtime.lua()).unwrap();
        assert_eq!(received(&runtime), ["fresh"]);
    }
}

#[test]
fn no_callback_at_delivery_discards_capture_instead_of_waiting_for_registration() {
    let runtime = scanner_runtime();
    runtime.execute_source("QrScanner.start()").unwrap();
    assert!(runtime.submit_qr_code("without-callback").unwrap());
    dispatch_registered_application_events(runtime.lua()).unwrap();
    runtime
        .execute_source("QrScanner.setQrRecognizedCallback(qr_receive)")
        .unwrap();
    dispatch_registered_application_events(runtime.lua()).unwrap();
    assert!(received(&runtime).is_empty());
    assert!(runtime.submit_qr_code("new-frame").unwrap());
    dispatch_registered_application_events(runtime.lua()).unwrap();
    assert_eq!(received(&runtime), ["new-frame"]);
}

#[test]
fn empty_decode_releases_inflight_without_calling_the_recognized_callback() {
    let runtime = scanner_runtime();
    runtime
        .execute_source("QrScanner.start(); QrScanner.setQrRecognizedCallback(qr_receive)")
        .unwrap();
    assert!(runtime.submit_qr_code("").unwrap());
    dispatch_registered_application_events(runtime.lua()).unwrap();
    assert!(received(&runtime).is_empty());
    assert!(runtime.submit_qr_code("after-empty").unwrap());
    dispatch_registered_application_events(runtime.lua()).unwrap();
    assert_eq!(received(&runtime), ["after-empty"]);
}

#[test]
fn nul_is_truncated_only_when_the_successful_result_becomes_a_lua_argument() {
    let runtime = scanner_runtime();
    runtime
        .execute_source("QrScanner.start(); QrScanner.setQrRecognizedCallback(qr_receive)")
        .unwrap();
    for code in ["visible\0suffix", "\0nonempty-decoded-string"] {
        assert!(runtime.submit_qr_code(code).unwrap());
        dispatch_registered_application_events(runtime.lua()).unwrap();
    }
    // Decoder success is std::string nonempty, not strlen(c_str()) != 0.
    assert_eq!(received(&runtime), ["visible", ""]);
}

#[test]
fn busy_virtual_scanner_keeps_only_latest_pending_code_until_the_next_host_poll() {
    let runtime = scanner_runtime();
    runtime
        .execute_source("QrScanner.start(); QrScanner.setQrRecognizedCallback(qr_receive)")
        .unwrap();
    assert!(runtime.submit_qr_code("inflight").unwrap());
    assert!(!runtime.submit_qr_code("superseded-pending").unwrap());
    assert!(!runtime.submit_qr_code("latest-pending").unwrap());
    // Repeated start cannot duplicate the outstanding result or bypass its
    // native one-frame in-flight gate.
    runtime.execute_source("QrScanner.start()").unwrap();
    dispatch_registered_application_events(runtime.lua()).unwrap();
    assert_eq!(received(&runtime), ["inflight"]);
    // An already-active start is a strict native no-op, even now that the
    // previous decode has finished and the virtual source still has a code.
    runtime.execute_source("QrScanner.start()").unwrap();
    dispatch_registered_application_events(runtime.lua()).unwrap();
    assert_eq!(received(&runtime), ["inflight"]);
    runtime.update(0.0).unwrap();
    assert_eq!(received(&runtime), ["inflight", "latest-pending"]);
    runtime.update(0.0).unwrap();
    assert_eq!(received(&runtime), ["inflight", "latest-pending"]);
}

#[test]
fn callback_error_does_not_retry_a_result_or_leave_capture_permanently_busy() {
    let runtime = scanner_runtime();
    runtime
        .execute_source(
            r#"
                QrScanner.start()
                QrScanner.setQrRecognizedCallback(function(code)
                    qr_receive(code)
                    error("qr-callback-failed")
                end)
            "#,
        )
        .unwrap();
    assert!(runtime.submit_qr_code("failed").unwrap());
    let error = dispatch_registered_application_events(runtime.lua()).unwrap_err();
    assert!(error.to_string().contains("qr-callback-failed"));
    assert_eq!(received(&runtime), ["failed"]);
    dispatch_registered_application_events(runtime.lua()).unwrap();
    assert_eq!(received(&runtime), ["failed"]);
    runtime
        .execute_source("QrScanner.setQrRecognizedCallback(qr_receive)")
        .unwrap();
    assert!(runtime.submit_qr_code("after-error").unwrap());
    dispatch_registered_application_events(runtime.lua()).unwrap();
    assert_eq!(received(&runtime), ["failed", "after-error"]);
}

#[test]
fn qr_results_share_cross_service_fifo_and_the_reentrant_scheduler_cursor() {
    let runtime = scanner_runtime();
    runtime
        .execute_source(
            r#"
                _G.SkynestStorage.native_setKey("qr-before", "value", function()
                    qr_receive("before")
                end)
                QrScanner.start()
                QrScanner.setQrRecognizedCallback(function(code)
                    qr_receive(code .. "-begin")
                    _G.SkynestStorage.native_setKey("qr-nested", "value", function()
                        qr_receive("nested")
                    end)
                    AnimationWrapperNative.update(0)
                    qr_receive(code .. "-end")
                end)
            "#,
        )
        .unwrap();
    assert!(runtime.submit_qr_code("qr").unwrap());
    runtime
        .execute_source(
            r#"
                _G.SkynestStorage.native_setKey("qr-tail", "value", function()
                    qr_receive("tail")
                end)
            "#,
        )
        .unwrap();
    assert!(received(&runtime).is_empty());
    dispatch_registered_application_events(runtime.lua()).unwrap();
    assert_eq!(
        received(&runtime),
        ["before", "qr-begin", "tail", "nested", "qr-end"]
    );
}

#[test]
fn host_error_drops_an_unvisited_qr_result_without_poisoning_the_next_capture() {
    let runtime = scanner_runtime();
    runtime
        .execute_source(
            r#"
                _G.SkynestStorage.native_setKey("qr-prior-error", "value", function()
                    error("before-qr-dispatch")
                end)
                QrScanner.start()
                QrScanner.setQrRecognizedCallback(qr_receive)
            "#,
        )
        .unwrap();
    assert!(runtime.submit_qr_code("abandoned-active-result").unwrap());
    let error = runtime.update(0.0).unwrap_err();
    assert!(error.to_string().contains("before-qr-dispatch"));
    assert!(received(&runtime).is_empty());
    assert!(runtime.submit_qr_code("fresh-result").unwrap());
    runtime.update(0.0).unwrap();
    assert_eq!(received(&runtime), ["fresh-result"]);
}
