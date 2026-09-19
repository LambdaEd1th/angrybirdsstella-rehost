//! Guest-to-member continuation transitions, isolated synthetic identity stores.
use super::super::super::identifiers::Identifiers;
use super::*;

fn decoded(id: &str, details: Value) -> ProfileResponse {
    let mut raw = details.as_object().unwrap().clone();
    raw.insert("publicAccountId".into(), json!(id));
    super::super::protocol::parse_profile_value(&Value::Object(raw))
}

fn admitted(session: &IdentitySession) -> (OwnProfileOwner, LoginProfileIdentity) {
    let owner = session
        .own_profile_owner_for_request(session.request_owner(ProviderLevel::Level2))
        .unwrap();
    let before = session.login_profile_identity(owner).unwrap();
    (owner, before)
}

fn seeded(old: Option<&ProfileResponse>) -> IdentitySession {
    let session = IdentitySession::default();
    session.bind_success_events(crate::ApplicationEventScheduler::default());
    session.install_flat(&flat("old-a", "old-r", "old-s"));
    if let Some(old) = old {
        session
            .install_profile_if_epoch(session.epoch(), old)
            .unwrap();
    }
    session
}

#[test]
fn account_regeneration_matches_native_class_and_full_id_predicate() {
    let guest = decoded("same", json!({}));
    let member = decoded(
        "same",
        json!({"personal":{"email":"synthetic@example.invalid"}}),
    );
    let cases = [
        (Some(guest.clone()), member.clone(), true),
        (
            Some(guest.clone()),
            decoded("same", json!({"personal":{"email":" "}})),
            true,
        ),
        (
            Some(guest.clone()),
            decoded(
                "same",
                json!({"externalNetworks":[{"provider":"facebook","id":"synthetic"}]}),
            ),
            true,
        ),
        (
            Some(guest.clone()),
            decoded(
                "same",
                json!({"externalNetworks":[{"provider":"unknown","id":"synthetic"}]}),
            ),
            true,
        ),
        (
            Some(guest.clone()),
            decoded(
                "same",
                json!({"socialNetworks":[{"provider":"facebook","id":"synthetic"}]}),
            ),
            false,
        ),
        (
            Some(guest.clone()),
            decoded(
                "same",
                json!({"externalNetworks":[null,{"provider":"facebook","id":"synthetic"}]}),
            ),
            false,
        ),
        (
            Some(guest.clone()),
            decoded("same", json!({"personal":{"nickName":"changed"}})),
            false,
        ),
        (
            Some(guest.clone()),
            decoded(
                "same-longer",
                json!({"personal":{"email":"synthetic@example.invalid"}}),
            ),
            false,
        ),
        (
            Some(decoded("same-longer", json!({}))),
            member.clone(),
            false,
        ),
        (
            Some(guest.clone()),
            decoded(
                "",
                json!({"personal":{"email":"synthetic@example.invalid"}}),
            ),
            false,
        ),
        (Some(decoded("", json!({}))), member.clone(), false),
        (None, member.clone(), false),
        (Some(member.clone()), member, false),
        (
            Some(decoded(
                "same",
                json!({"personal":{"email":"old@example.invalid"}}),
            )),
            guest,
            false,
        ),
    ];
    for (index, (old, next, rotates)) in cases.into_iter().enumerate() {
        let session = seeded(old.as_ref());
        let identifiers = Identifiers::synthetic();
        let previous = identifiers.installation_id().unwrap();
        let (mut owner, before) = admitted(&session);
        assert_eq!(
            session.publish_login_profile(
                &mut owner,
                &flat("new-a", "new-r", "new-s"),
                &next,
                &identifiers,
                before
            ),
            Ok(true)
        );
        let current = identifiers.installation_id().unwrap();
        assert_eq!(current != previous, rotates, "case {index}");
        if rotates {
            assert_eq!(current.len(), 36);
            assert_eq!(current.as_bytes()[14], b'4');
        }
        assert!(session.own_profile_owner_is_current(owner));
        assert!(session.pop_success_owner().is_some());
        assert!(session.pop_success_owner().is_none());
        assert_eq!(session.level2_tokens().access_token, "new-a");
        assert_eq!(identifiers.persistent_guid, "fixture-device-sha1");
    }
}

#[test]
fn account_regeneration_uses_pre_request_snapshot_not_later_same_account_edits() {
    let guest = decoded("same", json!({}));
    let member = decoded(
        "same",
        json!({"personal":{"email":"synthetic@example.invalid"}}),
    );
    for (old, intermediate, rotates) in [(&guest, &member, true), (&member, &guest, false)] {
        let session = seeded(Some(old));
        let ids = Identifiers::synthetic();
        let previous = ids.installation_id().unwrap();
        let (mut owner, before) = admitted(&session);
        // Same-id profile edits do not replace the admitted token owner.
        session
            .install_profile_if_epoch(session.epoch(), intermediate)
            .unwrap();
        assert!(session.own_profile_owner_is_current(owner));
        assert_eq!(
            session.publish_login_profile(
                &mut owner,
                &flat("new", "new-r", "s"),
                &member,
                &ids,
                before
            ),
            Ok(true)
        );
        assert_eq!(ids.installation_id().unwrap() != previous, rotates);
    }
}

#[test]
fn account_regeneration_rejects_stale_and_reassigned_snapshots_before_publication() {
    let guest = decoded("same", json!({}));
    let member = decoded(
        "same",
        json!({"personal":{"email":"synthetic@example.invalid"}}),
    );
    for (logout, use_current_owner) in [(true, false), (false, false), (false, true)] {
        let session = seeded(Some(&guest));
        let ids = Identifiers::synthetic();
        let previous = ids.installation_id().unwrap();
        let (mut owner, before) = admitted(&session);
        if logout {
            session.logout().unwrap();
        } else {
            session.install_flat(&flat("replacement", "replacement-r", "s"));
        }
        if use_current_owner {
            owner = admitted(&session).0;
        }
        let tokens = session.level2_tokens().access_token;
        assert_eq!(
            session.publish_login_profile(
                &mut owner,
                &flat("late", "late-r", "s"),
                &member,
                &ids,
                before
            ),
            Ok(false)
        );
        assert_eq!(ids.installation_id().unwrap(), previous);
        assert_eq!(session.level2_tokens().access_token, tokens);
        assert!(session.pop_success_owner().is_none());
    }
}

#[test]
fn account_regeneration_waits_for_profile_and_refresh_persistence() {
    let guest = decoded("same", json!({}));
    let member = decoded(
        "same",
        json!({"personal":{"email":"synthetic@example.invalid"}}),
    );
    for fail_profile in [true, false] {
        let store = Arc::new(FailingLogoutStore::default());
        let session = IdentitySession::with_refresh_store(store.clone());
        session.bind_success_events(crate::ApplicationEventScheduler::default());
        session.install_flat(&flat("old-a", "old-r", "s"));
        session
            .install_profile_if_epoch(session.epoch(), &guest)
            .unwrap();
        store.fail_profile.store(fail_profile, Ordering::Relaxed);
        store.fail_refresh.store(!fail_profile, Ordering::Relaxed);
        let ids = Identifiers::synthetic();
        let previous = ids.installation_id().unwrap();
        let (mut owner, before) = admitted(&session);
        assert_eq!(
            session.publish_login_profile(
                &mut owner,
                &flat("new-a", "new-r", "s"),
                &member,
                &ids,
                before
            ),
            Err(StoreError::Io)
        );
        assert_eq!(ids.installation_id().unwrap(), previous);
        assert!(session.own_profile_owner_is_current(owner));
        assert!(session.pop_success_owner().is_none());
    }
}

#[test]
fn account_regeneration_write_failure_keeps_prior_publication_without_success() {
    use std::fs;
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let root = std::env::temp_dir().join(format!(
        "stella-regeneration-failure-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir(&root).unwrap();
    let ids = Identifiers::for_app_data("synthetic-guid", &root);
    let path = root.join("stella-installation.registry");
    let invalid = super::super::registry_codec::encode_registry(b"{\"id\":false}").unwrap();
    fs::write(&path, &invalid).unwrap();
    let store = Arc::new(MemoryRefreshStore::default());
    let session = IdentitySession::with_refresh_store(store.clone());
    session.bind_success_events(crate::ApplicationEventScheduler::default());
    session.install_flat(&flat("old-a", "old-r", "s"));
    session
        .install_profile_if_epoch(session.epoch(), &decoded("same", json!({})))
        .unwrap();
    let (mut owner, before) = admitted(&session);
    let original_owner = owner;
    let member = decoded(
        "same",
        json!({"personal":{"email":"synthetic@example.invalid"}}),
    );
    assert_eq!(
        session.publish_login_profile(
            &mut owner,
            &flat("new-a", "new-r", "s"),
            &member,
            &ids,
            before
        ),
        Err(StoreError::InvalidDocument)
    );
    assert_ne!(owner, original_owner);
    assert!(session.own_profile_owner_is_current(owner));
    assert_eq!(session.level2_tokens().access_token, "new-a");
    assert_eq!(store.load().unwrap(), "new-r");
    assert_eq!(store.load_profile().unwrap(), Some(member.raw));
    assert!(session.pop_success_owner().is_none());
    assert_eq!(fs::read(&path).unwrap(), invalid);
    fs::remove_dir_all(root).unwrap();
}
