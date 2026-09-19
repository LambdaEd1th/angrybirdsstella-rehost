//! Public host configuration and the original Lua bootstrap, not a mock facade.

use super::*;
use std::time::Instant;

#[test]
fn shipped_boot_keeps_online_identity_distinct_from_explicit_local_payment_provider() {
    for local_services in [false, true] {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let address = listener.local_addr().unwrap();
        let (release, ready) = mpsc::channel();
        let server = thread::spawn(move || {
            // Boot can decode the complete shipped asset catalog in a debug
            // build. Start the bounded accept clock only after boot; dropping
            // the sender on any earlier assertion failure also stops us.
            ready.recv().unwrap();
            let (mut stream, request) = identity_routes::accept_request(&listener);
            assert_eq!(
                request.lines().next(),
                Some("POST /session/1/apps/Purple/sessions HTTP/1.1")
            );
            // No account response is permitted to race the boot assertions.
            let body = r#"{"userAuth":{"accessToken":"fixture-a","refreshToken":"fixture-r","expiresIn":3600},"segments":[],"config":{},"profile":{"publicAccountId":"fixture-payment-account","personal":{"nickName":"Fixture"}}}"#;
            write!(
                stream,
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            )
            .unwrap();
        });
        let sandbox = ShippedDataSandbox::new("iap-session-boot");
        let host = StellaLua::new(&sandbox.data_root).unwrap();
        if local_services {
            host.enable_local_services().unwrap();
        }
        host.set_identity_url(&format!("http://{address}/identity/2.0"))
            .unwrap();
        host.boot("scripts/game.lua").unwrap();
        host.execute_source(
            r#"
            session_registration_count = 0
            local register = registerPaymentCallbacks
            registerPaymentCallbacks = function()
                session_registration_count = session_registration_count + 1
                register()
            end
        "#,
        )
        .unwrap();
        for _ in 0..3 {
            host.update(0.0).unwrap();
        }
        let native: mlua::Table = host.lua().globals().get("IAP").unwrap();
        let initialized: mlua::Function = native.get("native_isPaymentInitialized").unwrap();
        assert_eq!(initialized.call::<bool>(()).unwrap(), local_services);
        let environment = game_environment(host.lua()).unwrap();
        assert_eq!(
            environment
                .get::<u32>("session_registration_count")
                .unwrap(),
            0
        );
        release.send(()).unwrap();
        let account: mlua::Table = host.lua().globals().get("SkynestAccount").unwrap();
        let logged_in: mlua::Function = account.get("native_isLoggedIn").unwrap();
        let deadline = Instant::now() + Duration::from_secs(5);
        while !logged_in.call::<bool>(()).unwrap() {
            assert!(
                Instant::now() < deadline,
                "shipped auto-login did not complete"
            );
            host.update(1.0 / 60.0).unwrap();
            thread::sleep(Duration::from_millis(1));
        }
        server.join().unwrap();
        for _ in 0..3 {
            host.update(0.0).unwrap();
        }
        assert_eq!(initialized.call::<bool>(()).unwrap(), local_services);
        assert_eq!(
            environment
                .get::<u32>("session_registration_count")
                .unwrap(),
            u32::from(!local_services)
        );
        let catalog: mlua::Table = native
            .get::<mlua::Function>("native_getAvailableItems")
            .unwrap()
            .call(())
            .unwrap();
        assert_eq!(catalog.raw_len(), if local_services { 6 } else { 0 });
        if !local_services {
            // A live opt-in replaces the missing account provider. Its old
            // retry cannot be responsible for starting the new local store.
            host.enable_local_services().unwrap();
            for _ in 0..3 {
                host.update(0.0).unwrap();
            }
            assert!(initialized.call::<bool>(()).unwrap());
            assert_eq!(
                environment
                    .get::<u32>("session_registration_count")
                    .unwrap(),
                2
            );
            let catalog: mlua::Table = native
                .get::<mlua::Function>("native_getAvailableItems")
                .unwrap()
                .call(())
                .unwrap();
            assert_eq!(catalog.raw_len(), 6);
        }
        assert!(host.fallback_calls.lock().unwrap().is_empty());
        assert!(host.compatibility_bindings.lock().unwrap().is_empty());
    }
}
