//! Real request boundaries with synthetic loopback responses and memory stores.

use super::super::super::signing::ClientSigning;
use super::*;

fn signed_fields(values: &BTreeMap<String, String>) -> (&str, &str) {
    assert_eq!(values["clientId"], "synthetic-client");
    for key in ["clientSignature", "clientSalt"] {
        let value = &values[key];
        assert_eq!(value.len(), 40);
        assert!(
            value
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'A'..=b'F').contains(&b))
        );
    }
    (&values["clientSignature"], &values["clientSalt"])
}

#[test]
fn generated_client_signing_session_rejection_rebuilds_pair_and_cache_hit_does_not_send() {
    let key = "synthetic-signing-key-never-sent";
    let (mut config, server) = server(vec![
        (401, "{}".into()),
        (200, session_json("new-a", "new-r", json!([1]))),
    ]);
    config.signing = ClientSigning::generated(key);
    let store = Arc::new(MemoryRefreshStore::default());
    store.store("stored-refresh").unwrap();
    let session = IdentitySession::with_refresh_store(store);
    session.acquire_session(&config).unwrap();
    let requests = server.join().unwrap();
    let mut fields = Vec::new();
    for request in &requests {
        assert!(
            request
                .start
                .starts_with("POST /proxy/session/1/apps/synthetic-client/sessions ")
        );
        assert_eq!(request.headers["content-type"], "application/json");
        assert!(!request.body.contains(key));
        assert!(!request.headers.contains_key("rovio-sgs"));
        let root: Value = serde_json::from_str(&request.body).unwrap();
        fields.push(
            serde_json::from_value::<BTreeMap<String, String>>(root["access"].clone()).unwrap(),
        );
    }
    let first = signed_fields(&fields[0]);
    let second = signed_fields(&fields[1]);
    assert_ne!(first.0, second.0);
    assert_ne!(first.1, second.1);
    let first: Value = serde_json::from_str(&requests[0].body).unwrap();
    let second: Value = serde_json::from_str(&requests[1].body).unwrap();
    assert_eq!(first["refresh"]["token"], "stored-refresh");
    assert!(second["refresh"].is_null());
    // Closed listener: another acquisition must use the active session.
    session.acquire_session(&config).unwrap();
}

#[test]
fn generated_client_signing_level1_401_rebuilds_pair_but_replays_frozen_operation() {
    let key = "synthetic-signing-key-never-sent";
    let (mut config, server) = server(vec![
        (200, flat_json("parent-a", "first-segment")),
        (401, "{}".into()),
        (200, flat_json("parent-b", "second-segment")),
        (200, "{}".into()),
    ]);
    config.signing = ClientSigning::generated(key);
    let session = IdentitySession::default();
    session
        .execute_get(&config, ProviderLevel::Level1, "synthetic-protected")
        .unwrap();
    let requests = server.join().unwrap();
    let mut fields = Vec::new();
    for index in [0, 2] {
        let request = &requests[index];
        assert!(
            request
                .start
                .starts_with("POST /proxy/identity/2.0/access ")
        );
        assert_eq!(
            request.headers["content-type"],
            "application/x-www-form-urlencoded"
        );
        assert!(!request.body.contains(key));
        // These three fixture values contain only URL-unreserved bytes. Inspect
        // the wire fields directly, without using the production form encoder.
        fields.push(
            request
                .body
                .split('&')
                .filter_map(|part| {
                    let (key, value) = part.split_once('=')?;
                    matches!(key, "clientId" | "clientSignature" | "clientSalt")
                        .then(|| (key.to_owned(), value.to_owned()))
                })
                .collect::<BTreeMap<_, _>>(),
        );
    }
    let first = signed_fields(&fields[0]);
    let second = signed_fields(&fields[1]);
    assert_ne!(first.0, second.0);
    assert_ne!(first.1, second.1);
    assert_eq!(requests[1].start, requests[3].start);
    assert_eq!(requests[1].body, requests[3].body);
    assert_eq!(requests[1].headers["x-access-token"], "parent-a");
    assert_eq!(requests[3].headers["x-access-token"], "parent-b");
    assert_eq!(requests[1].headers["rovio-sgs"], "first-segment");
    assert_eq!(requests[3].headers["rovio-sgs"], "second-segment");
}
