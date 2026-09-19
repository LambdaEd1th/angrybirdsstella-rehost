use super::*;

#[test]
fn client_signing_reconfiguration_retires_owners_but_same_key_is_a_noop() {
    use super::super::{OfflineState, ProviderLevel, SkynestAccountRuntime};
    use crate::ApplicationEventScheduler;
    use std::sync::Mutex;

    let state = OfflineState::new("synthetic-signing/not-read-or-written.json".into());
    let runtime = SkynestAccountRuntime::new(
        Arc::new(Mutex::new(state)),
        ApplicationEventScheduler::default(),
        crate::game_lua::platform_services::skynest_account::identifiers::Identifiers::synthetic()
            .into(),
    );
    runtime
        .set_compatible_url("http://127.0.0.1:9/identity/3.0")
        .unwrap();
    runtime
        .set_compatible_client(Some("client"), Some("old signature"), Some("old salt"))
        .unwrap();
    let literal_owner = runtime.session.request_owner(ProviderLevel::Level2);
    runtime
        .set_compatible_signing_key(b" synthetic key ")
        .unwrap();
    assert!(!runtime.session.request_owner_is_current(literal_owner));
    let generated_owner = runtime.session.request_owner(ProviderLevel::Level2);
    let config = runtime.online_config().unwrap();
    assert!(
        matches!(config.signing, ClientSigning::Generated { ref key } if key.as_ref() == b" synthetic key ")
    );
    runtime
        .set_compatible_signing_key(b" synthetic key ")
        .unwrap();
    assert!(runtime.session.request_owner_is_current(generated_owner));
    runtime
        .set_compatible_signing_key(b"synthetic key")
        .unwrap();
    assert!(
        !runtime.session.request_owner_is_current(generated_owner),
        "key whitespace is significant"
    );
    let replacement_owner = runtime.session.request_owner(ProviderLevel::Level2);
    runtime
        .set_compatible_client(None, Some(" literal "), Some(" salt "))
        .unwrap();
    assert!(!runtime.session.request_owner_is_current(replacement_owner));
    let literal = runtime
        .online_config()
        .unwrap()
        .signing
        .credentials_with_uuid("client", || panic!("literal mode after switch"))
        .unwrap();
    assert_eq!(literal.signature, "literal");
    assert_eq!(literal.salt, "salt");
    assert!(runtime.session.level2_tokens().access_token.is_empty());
}

#[derive(serde::Deserialize)]
struct Vector {
    key: String,
    client: String,
    uuid: String,
    salt: String,
    derived: String,
    signature: String,
}

#[test]
fn client_signing_matches_independent_sha1_hmac_vectors() {
    // Independently generated using Python hashlib.sha1 and hmac.new, not
    // this implementation. Includes empty/long/UTF-8/NUL/whitespace inputs.
    let vectors: Vec<Vector> = serde_json::from_str(include_str!("vectors.json")).unwrap();
    assert_eq!(vectors.len(), 5);
    for vector in vectors {
        let salt = sha1_digest(vector.uuid.as_bytes());
        assert_eq!(upper_hex(&salt), vector.salt);
        assert_eq!(
            upper_hex(&derive_key(vector.key.as_bytes(), &salt)),
            vector.derived
        );
        let signed = ClientSigning::generated(&vector.key)
            .credentials_with_uuid(&vector.client, || Ok(vector.uuid))
            .unwrap();
        assert_eq!(signed.salt, vector.salt);
        assert_eq!(signed.signature, vector.signature);
    }
}

#[test]
fn client_signing_preserves_non_utf8_key_bytes() {
    // Independent hashlib/hmac vector: key is raw bytes, not a hex string or
    // text that can be trimmed/decoded lossily by the host configuration path.
    let signed = ClientSigning::generated([0, 255, 128, 1, b'a', b'\n'])
        .credentials_with_uuid("synthetic-client", || {
            Ok("00000000-0000-4000-8000-000000000000".into())
        })
        .unwrap();
    assert_eq!(signed.salt, "37965E58C97EA40F43A656BE5934FD50AE50987E");
    assert_eq!(signed.signature, "EC43CD998863411CBFF9B9D3FBD20D4A67ED9A20");
}

#[test]
fn client_signing_literal_mode_does_not_generate_or_interpret_salt() {
    let config =
        ClientSigning::literal("literal signature".into(), "not hex / explicit salt".into());
    let signed = config
        .credentials_with_uuid("client", || {
            panic!("literal mode must not call entropy source")
        })
        .unwrap();
    assert_eq!(signed.signature, "literal signature");
    assert_eq!(signed.salt, "not hex / explicit salt");
    let empty = ClientSigning::default()
        .credentials_with_uuid("client", || Err(SignatureError))
        .unwrap();
    assert!(empty.signature.is_empty());
    assert!(empty.salt.is_empty());
}

#[test]
fn client_signing_entropy_failure_is_sanitized_and_never_falls_back_to_literal_success() {
    let result = ClientSigning::generated("synthetic-key-not-in-error")
        .credentials_with_uuid("synthetic-client-not-in-error", || Err(SignatureError));
    let error = match result {
        Err(error) => error,
        Ok(_) => panic!("missing randomness must fail"),
    };
    let session_error = super::super::session::SessionError::from(error);
    assert_eq!(session_error.status, -1);
    assert!(!session_error.is_stale());
    assert_eq!(
        session_error.to_string(),
        "identity client salt generation failed"
    );
    assert_eq!(format!("{session_error:?}"), session_error.to_string());
}

#[test]
fn client_signing_generates_a_fresh_pair_for_every_metadata_build() {
    let config = ClientSigning::generated("synthetic-key");
    let mut salts = std::collections::BTreeSet::new();
    let mut signatures = std::collections::BTreeSet::new();
    for _ in 0..8 {
        let signed = config.credentials("synthetic-client").unwrap();
        for value in [&signed.signature, &signed.salt] {
            assert_eq!(value.len(), 40);
            assert!(
                value
                    .bytes()
                    .all(|b| b.is_ascii_digit() || (b'A'..=b'F').contains(&b))
            );
        }
        assert!(salts.insert(signed.salt));
        assert!(signatures.insert(signed.signature));
    }
}

#[test]
fn hmac_sha1_matches_independent_vectors_including_long_key() {
    for (key, message, expected) in [
        (
            b"key".to_vec(),
            "The quick brown fox jumps over the lazy dog",
            "DE7C9B85B8B78AA6BC8A7A36F70A90701C9DB4D9",
        ),
        (
            vec![0x0b; 20],
            "Hi There",
            "B617318655057264E28BC0B6FB378C8EF146BE00",
        ),
        (
            vec![0xaa; 80],
            "Test Using Larger Than Block-Size Key - Hash Key First",
            "AA4AE5E15272D00E95705637CE8A3B55ED402112",
        ),
        (Vec::new(), "", "FBDB1D1B18AA6C08324B7D64B71FB76370690E1D"),
    ] {
        assert_eq!(upper_hex(&hmac_sha1(&key, message.as_bytes())), expected);
    }
}
