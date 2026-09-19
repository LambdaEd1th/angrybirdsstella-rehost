//! Synthetic temporary files only: never opens a normal runtime registry.

use super::*;
use serde_json::json;
use std::{sync::Barrier, thread};

#[test]
fn registry_namespace_updates_preserve_installation_identity_and_unrelated_native_fields() {
    let fixture = Fixture::new();
    let original = json!({
        "id": {"accountUUID": "synthetic-account"},
        "cloud": {"unrecognized-provider": [1, 2]},
        "fusion": {"installationID": "synthetic-installation", "Apprater": {"unknown": {"future": true}}},
        "unrecognized-root": "retain",
    });
    fixture.write_document(&original);
    let namespace = RegistryNamespace::open(fixture.path.clone(), &["fusion", "Apprater"]).unwrap();
    namespace.set("tryCount", json!(6)).unwrap();
    namespace.set("userPromptedLater", json!(true)).unwrap();
    assert_eq!(
        namespace.get("unknown").unwrap(),
        Some(json!({"future": true}))
    );
    let mut expected = original;
    expected["fusion"]["Apprater"]["tryCount"] = json!(6);
    expected["fusion"]["Apprater"]["userPromptedLater"] = json!(true);
    assert_eq!(fixture.document(), expected);
    let identity = fixture.open();
    assert_eq!(identity.installation_id().unwrap(), "synthetic-account");
    identity
        .regenerate_account_id_with(|| Ok("new-synthetic-account".to_owned()))
        .unwrap();
    assert_eq!(namespace.get("tryCount").unwrap(), Some(json!(6)));
    expected["id"]["accountUUID"] = json!("new-synthetic-account");
    assert_eq!(fixture.document(), expected);
}

#[test]
fn registry_namespace_null_upgrade_and_wrong_parent_errors_retain_input() {
    let fixture = Fixture::new();
    let namespace = RegistryNamespace::open(fixture.path.clone(), &["fusion", "Apprater"]).unwrap();
    assert_eq!(namespace.get("tryCount").unwrap(), None);
    assert!(!fixture.path.exists());
    namespace.set("tryCount", json!(1)).unwrap();
    assert_eq!(
        fixture.document(),
        json!({"fusion": {"Apprater": {"tryCount": 1}}})
    );
    for leaf in [json!(true), json!(5), json!("bad"), json!([])] {
        let input = json!({"fusion": {"Apprater": leaf}});
        fixture.write_document(&input);
        assert_eq!(namespace.get("tryCount").unwrap(), None);
        assert_eq!(
            namespace.set("tryCount", json!(1)),
            Err(StoreError::InvalidDocument)
        );
        assert_eq!(fixture.document(), input);
    }
    let input = json!({"fusion": false});
    fixture.write_document(&input);
    assert_eq!(namespace.get("tryCount"), Err(StoreError::InvalidDocument));
    assert_eq!(fixture.document(), input);
}

struct Fixture {
    root: PathBuf,
    path: PathBuf,
}
impl Fixture {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(1);
        let root = std::env::temp_dir().join(format!(
            "stella-registry-store-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&root).unwrap();
        let path = root.join("fusion.registry");
        Self { root, path }
    }
    fn write_document(&self, value: &Value) {
        fs::write(
            &self.path,
            encode_registry(&serde_json::to_vec(value).unwrap()).unwrap(),
        )
        .unwrap();
    }
    fn document(&self) -> Value {
        serde_json::from_slice(&decode_registry(&fs::read(&self.path).unwrap()).unwrap()).unwrap()
    }
    fn open(&self) -> RegistryStore {
        RegistryStore::open(self.path.clone()).unwrap()
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

#[test]
fn registry_store_absent_file_is_lazy_and_empty_not_a_written_reset() {
    let fixture = Fixture::new();
    let store = fixture.open();
    assert!(!fixture.path.exists());
    assert_eq!(store.load().unwrap(), "");
    assert!(store.load_profile().unwrap().is_none());
    assert!(!fixture.path.exists());
    assert!(store.lock_path.is_file());
}

#[cfg(unix)]
#[test]
fn registry_file_lock_explicit_release_outlives_duplicated_descriptors() {
    let fixture = Fixture::new();
    let path = fixture.root.join("lock-lifetime");
    let open = || {
        OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(&path)
            .unwrap()
    };
    // Deterministically reproduce close-only lifetime: this is also the open
    // description sharing used by a child between fork and close-on-exec.
    let original = open();
    original.try_lock().unwrap();
    let inherited = original.try_clone().unwrap();
    drop(original);
    let competing = open();
    assert!(matches!(
        competing.try_lock(),
        Err(std::fs::TryLockError::WouldBlock)
    ));
    inherited.unlock().unwrap();
    competing.try_lock().unwrap();
    competing.unlock().unwrap();
    drop(inherited);

    for fail_operation in [false, true] {
        let original = open();
        let inherited = original.try_clone().unwrap();
        let result = with_file_lock(original, || {
            assert!(matches!(
                competing.try_lock(),
                Err(std::fs::TryLockError::WouldBlock)
            ));
            if fail_operation {
                Err(StoreError::InvalidDocument)
            } else {
                Ok(())
            }
        });
        assert_eq!(
            result,
            if fail_operation {
                Err(StoreError::InvalidDocument)
            } else {
                Ok(())
            }
        );
        // Must succeed before the duplicated descriptor is dropped.
        competing.try_lock().unwrap();
        competing.unlock().unwrap();
        drop(inherited);
    }
    let original = open();
    let inherited = original.try_clone().unwrap();
    let panic = std::panic::catch_unwind(|| {
        let _: Result<(), StoreError> =
            with_file_lock(original, || panic!("synthetic registry operation panic"));
    });
    assert!(panic.is_err());
    competing.try_lock().unwrap();
    competing.unlock().unwrap();
    drop(inherited);
}

#[test]
fn registry_installation_creates_engine_id_then_copies_account_id_once() {
    let fixture = Fixture::new();
    fixture.write_document(&json!({"cloud":{"other":"retained"},"id":{"other":7}}));
    let store = fixture.open();
    let first = store.installation_id().unwrap();
    assert_eq!(first.len(), 36);
    assert_eq!(first.as_bytes()[14], b'4');
    assert!(matches!(first.as_bytes()[19], b'8' | b'9' | b'A' | b'B'));
    assert_eq!(first, first.to_ascii_uppercase());
    assert_eq!(
        fixture.document(),
        json!({"cloud":{"other":"retained"},"id":{"other":7,"accountUUID":first},"fusion":{"installationID":first}})
    );
    assert_eq!(
        fixture
            .open()
            .installation_id_with(|| panic!("must reuse"))
            .unwrap(),
        first
    );
    store.store("").unwrap();
    store.store_profile(None).unwrap();
    assert_eq!(
        store
            .installation_id_with(|| panic!("logout cannot rotate"))
            .unwrap(),
        first
    );
}

#[test]
fn registry_installation_reuses_exact_strings_and_only_consults_fusion_when_needed() {
    let fixture = Fixture::new();
    for value in ["", " lower-case/non-uuid \0 "] {
        fixture.write_document(&json!({"id":{"accountUUID":value},"fusion":false}));
        let before = fs::read(&fixture.path).unwrap();
        assert_eq!(
            fixture
                .open()
                .installation_id_with(|| panic!("present string"))
                .unwrap(),
            value
        );
        assert_eq!(fs::read(&fixture.path).unwrap(), before);
        fixture.write_document(
            &json!({"id":{"accountUUID":false},"fusion":{"installationID":value,"extra":1}}),
        );
        assert_eq!(
            fixture
                .open()
                .installation_id_with(|| panic!("copy engine id"))
                .unwrap(),
            value
        );
        assert_eq!(fixture.document()["id"]["accountUUID"], value);
        assert_eq!(fixture.document()["fusion"]["extra"], 1);
    }
}

#[test]
fn registry_installation_nonstring_leaves_regenerate_but_invalid_containers_fail() {
    let fixture = Fixture::new();
    for leaf in [Value::Null, json!(false), json!(8), json!([]), json!({})] {
        fixture
            .write_document(&json!({"id":{"accountUUID":leaf},"fusion":{"installationID":leaf}}));
        assert_eq!(
            fixture
                .open()
                .installation_id_with(|| Ok("generated".to_owned()))
                .unwrap(),
            "generated"
        );
        assert_eq!(fixture.document()["fusion"]["installationID"], "generated");
    }
    for document in [json!(false), json!({"id":[]}), json!({"fusion":7})] {
        fixture.write_document(&document);
        let before = fs::read(&fixture.path).unwrap();
        assert_eq!(
            fixture
                .open()
                .installation_id_with(|| panic!("invalid container")),
            Err(StoreError::InvalidDocument)
        );
        assert_eq!(fs::read(&fixture.path).unwrap(), before);
    }
}

#[test]
fn registry_installation_generation_and_lock_errors_publish_nothing() {
    let fixture = Fixture::new();
    fixture.write_document(&json!({"retained":true}));
    let before = fs::read(&fixture.path).unwrap();
    let store = fixture.open();
    assert_eq!(
        store.installation_id_with(|| Err(StoreError::Io)),
        Err(StoreError::Io)
    );
    assert_eq!(fs::read(&fixture.path).unwrap(), before);
    let lock = OpenOptions::new()
        .read(true)
        .write(true)
        .open(&store.lock_path)
        .unwrap();
    lock.try_lock().unwrap();
    assert_eq!(
        store.installation_id_with(|| panic!("locked store")),
        Err(StoreError::Io)
    );
    assert_eq!(fs::read(&fixture.path).unwrap(), before);
}

#[test]
fn registry_installation_concurrent_first_use_returns_one_committed_identity() {
    let fixture = Fixture::new();
    let stores = [fixture.open(), fixture.open()];
    let gate = Arc::new(Barrier::new(2));
    let joins = stores.map(|store| {
        let gate = gate.clone();
        thread::spawn(move || {
            gate.wait();
            store.installation_id().unwrap()
        })
    });
    let [first, second] = joins.map(|join| join.join().unwrap());
    assert_eq!(first, second);
    assert_eq!(fixture.document()["id"]["accountUUID"], first);
    assert_eq!(fixture.document()["fusion"]["installationID"], first);
}

#[test]
fn registry_store_round_trip_uses_native_cloud_strings_and_preserves_unknown_keys() {
    let fixture = Fixture::new();
    let original = json!({"unknownRoot":{"retained":[1,true,null,"value"]},"cloud":{"unknownCloud":{"inner":7},"CloudUserProfile_other":"do not change","RovioIdentityRefreshToken":"initial"}});
    fixture.write_document(&original);
    let store = fixture.open();
    let profile = json!({"publicAccountId":"synthetic-id","personal":{"email":"synthetic@example.invalid"},"unknownProfile":[7,8]});
    store
        .store("synthetic-refresh-not-a-real-credential")
        .unwrap();
    store.store_profile(Some(&profile)).unwrap();
    let reopened = fixture.open();
    assert_eq!(
        reopened.load().unwrap(),
        "synthetic-refresh-not-a-real-credential"
    );
    assert_eq!(reopened.load_profile().unwrap(), Some(profile.clone()));
    let document = fixture.document();
    assert_eq!(document["unknownRoot"], original["unknownRoot"]);
    assert_eq!(
        document["cloud"]["unknownCloud"],
        original["cloud"]["unknownCloud"]
    );
    assert_eq!(document["cloud"]["CloudUserProfile_other"], "do not change");
    assert_eq!(
        serde_json::from_str::<Value>(document["cloud"][PROFILE_KEY].as_str().unwrap()).unwrap(),
        profile
    );
    let encrypted = fs::read(&fixture.path).unwrap();
    for plain in [
        b"synthetic-refresh-not-a-real-credential".as_slice(),
        b"synthetic@example.invalid",
    ] {
        assert!(!encrypted.windows(plain.len()).any(|bytes| bytes == plain));
    }
}

#[test]
fn registry_store_clear_writes_empty_strings_without_deleting_other_registry_data() {
    let fixture = Fixture::new();
    fixture.write_document(&json!({"cloud":{"other":42}}));
    let store = fixture.open();
    store.store("refresh").unwrap();
    store
        .store_profile(Some(&json!({"email":"fixture"})))
        .unwrap();
    store.store("").unwrap();
    store.store_profile(None).unwrap();
    let document = fixture.document();
    assert_eq!(document["cloud"][REFRESH_KEY], "");
    assert_eq!(document["cloud"][PROFILE_KEY], "");
    assert_eq!(document["cloud"]["other"], 42);
    assert!(store.load_profile().unwrap().is_none());
}

#[test]
fn registry_store_profile_is_json_text_including_nonobject_native_defaults() {
    let fixture = Fixture::new();
    let store = fixture.open();
    for profile in [
        Value::Null,
        json!([]),
        json!(false),
        json!("scalar"),
        json!({}),
    ] {
        store.store_profile(Some(&profile)).unwrap();
        assert_eq!(store.load_profile().unwrap(), Some(profile));
    }
}

#[test]
fn registry_store_transactions_reload_changes_made_after_open() {
    let fixture = Fixture::new();
    fixture.write_document(&json!({"root":1,"cloud":{"original":"yes"}}));
    let first = fixture.open();
    let second = fixture.open();
    first.store("first").unwrap();
    second
        .store_profile(Some(&json!({"id":"profile"})))
        .unwrap();
    let mut updated = fixture.document();
    updated["added-after-open"] = json!({"retained":true});
    updated["cloud"]["future-field"] = json!([1, 2, 3]);
    fixture.write_document(&updated);
    first.store("second").unwrap();
    let document = fixture.document();
    assert_eq!(document["added-after-open"], updated["added-after-open"]);
    assert_eq!(document["cloud"]["future-field"], json!([1, 2, 3]));
    assert_eq!(
        second.load_profile().unwrap(),
        Some(json!({"id":"profile"}))
    );
}

#[test]
fn registry_store_same_path_instances_serialize_and_do_not_lose_unknown_edits() {
    let fixture = Fixture::new();
    let first = Arc::new(fixture.open());
    let alias = RegistryStore::open(fixture.root.join(".").join("fusion.registry")).unwrap();
    assert!(Arc::ptr_eq(&first.process_lock, &alias.process_lock));
    let second = Arc::new(alias);
    let barrier = Arc::new(Barrier::new(3));
    let mut tasks = Vec::new();
    for (prefix, store) in [("first", first.clone()), ("second", second.clone())] {
        let barrier = barrier.clone();
        tasks.push(thread::spawn(move || {
            barrier.wait();
            for index in 0..12 {
                store
                    .edit(|cloud| {
                        cloud.insert(format!("{prefix}-{index}"), json!([index, prefix]));
                    })
                    .unwrap();
            }
        }));
    }
    barrier.wait();
    for task in tasks {
        task.join().unwrap();
    }
    let document = fixture.document();
    assert_eq!(document["cloud"].as_object().unwrap().len(), 24);
    for prefix in ["first", "second"] {
        for index in 0..12 {
            assert_eq!(
                document["cloud"][format!("{prefix}-{index}")],
                json!([index, prefix])
            );
        }
    }
}

#[test]
fn registry_store_damaged_ciphertext_is_reported_and_never_overwritten() {
    let fixture = Fixture::new();
    fixture.write_document(&json!({"cloud":{}}));
    let store = fixture.open();
    let damaged = b"invalid ciphertext length";
    fs::write(&fixture.path, damaged).unwrap();
    assert_eq!(store.load().err(), Some(StoreError::InvalidCiphertext));
    assert_eq!(
        store.store("must-not-save").err(),
        Some(StoreError::InvalidCiphertext)
    );
    assert_eq!(
        store.store_profile(None).err(),
        Some(StoreError::InvalidCiphertext)
    );
    assert_eq!(
        RegistryStore::open(fixture.path.clone()).err(),
        Some(StoreError::InvalidCiphertext)
    );
    assert_eq!(fs::read(&fixture.path).unwrap(), damaged);
}

#[test]
fn registry_store_valid_wrong_type_root_or_cloud_reads_empty_and_rejects_mutation() {
    let fixture = Fixture::new();
    for document in [
        json!([]),
        json!("root"),
        json!(false),
        json!(15),
        json!({"cloud":[]}),
        json!({"cloud":"scalar"}),
        json!({"cloud":false}),
    ] {
        fixture.write_document(&document);
        let bytes = fs::read(&fixture.path).unwrap();
        let store = fixture.open();
        assert_eq!(store.load().unwrap(), "");
        assert!(store.load_profile().unwrap().is_none());
        assert_eq!(
            store.store("cannot turn non-null value into object").err(),
            Some(StoreError::InvalidDocument)
        );
        assert_eq!(
            store.store_profile(None).err(),
            Some(StoreError::InvalidDocument)
        );
        assert_eq!(fs::read(&fixture.path).unwrap(), bytes);
    }
}

#[test]
fn registry_store_empty_file_null_root_and_null_cloud_upgrade_only_on_write() {
    let fixture = Fixture::new();
    for initial in [
        Vec::new(),
        encode_registry(b"null").unwrap(),
        encode_registry(br#"{"unknown":1,"cloud":null}"#).unwrap(),
    ] {
        fs::write(&fixture.path, &initial).unwrap();
        let store = fixture.open();
        assert_eq!(store.load().unwrap(), "");
        assert!(store.load_profile().unwrap().is_none());
        assert_eq!(fs::read(&fixture.path).unwrap(), initial);
        store.store("new-refresh").unwrap();
        assert_eq!(store.load().unwrap(), "new-refresh");
        assert!(fixture.document()["cloud"].is_object());
    }
}

#[test]
fn registry_store_wrong_type_leaves_read_empty_and_explicit_store_replaces_them() {
    let fixture = Fixture::new();
    for wrong in [Value::Null, json!(false), json!(7), json!([]), json!({})] {
        fixture
            .write_document(&json!({"cloud":{REFRESH_KEY:wrong,PROFILE_KEY:wrong,"unknown":true}}));
        let store = fixture.open();
        assert_eq!(store.load().unwrap(), "");
        assert!(store.load_profile().unwrap().is_none());
        store.store("replacement").unwrap();
        store
            .store_profile(Some(&json!({"known":"profile"})))
            .unwrap();
        assert_eq!(store.load().unwrap(), "replacement");
        assert_eq!(
            store.load_profile().unwrap(),
            Some(json!({"known":"profile"}))
        );
        assert_eq!(fixture.document()["cloud"]["unknown"], true);
    }
}

#[test]
fn registry_store_bad_profile_text_is_local_to_profile_read_not_refresh_operations() {
    let fixture = Fixture::new();
    fixture.write_document(&json!({"cloud":{REFRESH_KEY:"valid-refresh",PROFILE_KEY:"not JSON"}}));
    let store = fixture.open();
    assert_eq!(store.load().unwrap(), "valid-refresh");
    assert_eq!(
        store.load_profile().err(),
        Some(StoreError::InvalidDocument)
    );
    store.store("next-refresh").unwrap();
    assert_eq!(store.load().unwrap(), "next-refresh");
    assert_eq!(fixture.document()["cloud"][PROFILE_KEY], "not JSON");
    store.store_profile(None).unwrap();
    assert!(store.load_profile().unwrap().is_none());
}

#[test]
fn registry_store_bad_outer_json_is_reported_and_never_reinitialized() {
    let fixture = Fixture::new();
    let store = fixture.open();
    let bytes = encode_registry(b"{broken").unwrap();
    fs::write(&fixture.path, &bytes).unwrap();
    assert_eq!(
        RegistryStore::open(fixture.path.clone()).err(),
        Some(StoreError::InvalidDocument)
    );
    assert_eq!(
        store.store("must-not-save").err(),
        Some(StoreError::InvalidDocument)
    );
    assert_eq!(fs::read(&fixture.path).unwrap(), bytes);
}

#[test]
fn registry_store_plaintext_is_not_a_fallback_for_ciphertext() {
    let fixture = Fixture::new();
    let bytes = br#"{"cloud":{"RovioIdentityRefreshToken":"not encrypted"}}"#;
    fs::write(&fixture.path, bytes).unwrap();
    assert!(RegistryStore::open(fixture.path.clone()).is_err());
    assert_eq!(fs::read(&fixture.path).unwrap(), bytes);
}

#[test]
fn registry_store_commit_failure_keeps_old_ciphertext_and_cleans_only_own_temporary() {
    let fixture = Fixture::new();
    fixture.write_document(&json!({"unknown":"must survive"}));
    let original = fs::read(&fixture.path).unwrap();
    let new = encode_registry(br#"{"cloud":{}}"#).unwrap();
    let result = atomic_replace_with(&fixture.path, &new, |temporary, destination| {
        assert_eq!(fs::read(temporary).unwrap(), new);
        assert_eq!(fs::read(destination).unwrap(), original);
        Err(std::io::Error::other("synthetic commit failure"))
    });
    assert_eq!(result, Err(StoreError::Io));
    assert_eq!(fs::read(&fixture.path).unwrap(), original);
    assert_eq!(fs::read_dir(&fixture.root).unwrap().count(), 1);
}

#[test]
fn registry_store_competing_lock_handle_returns_error_without_mutating_registry() {
    let fixture = Fixture::new();
    fixture.write_document(&json!({"cloud":{"RovioIdentityRefreshToken":"original"}}));
    let store = fixture.open();
    let original = fs::read(&fixture.path).unwrap();
    let lock = OpenOptions::new()
        .read(true)
        .write(true)
        .open(&store.lock_path)
        .unwrap();
    lock.lock().unwrap();
    assert_eq!(store.store("blocked").err(), Some(StoreError::Io));
    assert_eq!(fs::read(&fixture.path).unwrap(), original);
    lock.unlock().unwrap();
    store.store("after-unlock").unwrap();
    assert_eq!(store.load().unwrap(), "after-unlock");
}

#[test]
fn registry_store_errors_contain_no_path_plaintext_or_credentials() {
    for error in [
        StoreError::Io,
        StoreError::InvalidCiphertext,
        StoreError::InvalidDocument,
    ] {
        let display = error.to_string();
        assert!(!display.contains("refresh"));
        assert!(!display.contains("fusion.registry") && !display.contains("\\"));
        assert!(!format!("{error:?}").contains("synthetic"));
    }
}

#[cfg(unix)]
#[test]
fn registry_store_new_registry_and_lock_are_private_and_symlinks_are_rejected() {
    use std::os::unix::fs::{PermissionsExt, symlink};
    let fixture = Fixture::new();
    let store = fixture.open();
    store.store("synthetic-refresh").unwrap();
    assert_eq!(
        fs::metadata(&fixture.path).unwrap().permissions().mode() & 0o777,
        0o600
    );
    assert_eq!(
        fs::metadata(&store.lock_path).unwrap().permissions().mode() & 0o777,
        0o600
    );
    let target = fixture.root.join("untouched.registry");
    fs::rename(&fixture.path, &target).unwrap();
    let bytes = fs::read(&target).unwrap();
    symlink(&target, &fixture.path).unwrap();
    assert_eq!(store.store("do not follow").err(), Some(StoreError::Io));
    assert_eq!(
        RegistryStore::open(fixture.path.clone()).err(),
        Some(StoreError::Io)
    );
    assert_eq!(fs::read(&target).unwrap(), bytes);
}

#[test]
fn registry_account_regeneration_replaces_only_account_uuid_and_survives_reopen() {
    let fixture = Fixture::new();
    let original = json!({"id":{"accountUUID":"old-account","unknown":[1,2]},"fusion":{"installationID":"engine-original","unknown":false},"cloud":{"credentials":"synthetic-only"},"other":7});
    fixture.write_document(&original);
    let first = fixture.open().regenerate_account_id().unwrap();
    assert_eq!(first.len(), 36);
    assert_eq!(first.as_bytes()[14], b'4');
    assert!(matches!(first.as_bytes()[19], b'8' | b'9' | b'A' | b'B'));
    assert_eq!(first, first.to_ascii_uppercase());
    let mut expected = original;
    expected["id"]["accountUUID"] = json!(first);
    assert_eq!(fixture.document(), expected);
    assert_eq!(fixture.open().installation_id().unwrap(), first);
    let second = fixture.open().regenerate_account_id().unwrap();
    assert_ne!(first, second);
    expected["id"]["accountUUID"] = json!(second);
    assert_eq!(fixture.document(), expected);
}

#[test]
fn registry_account_regeneration_obeys_native_mutable_index_without_touching_fusion() {
    let fixture = Fixture::new();
    for input in [
        Value::Null,
        json!({}),
        json!({"id":null}),
        json!({"id":{"accountUUID":false},"fusion":false}),
    ] {
        fixture.write_document(&input);
        assert_eq!(
            fixture
                .open()
                .regenerate_account_id_with(|| Ok("new-id".into()))
                .unwrap(),
            "new-id"
        );
        let mut expected = if input.is_null() { json!({}) } else { input };
        expected["id"]["accountUUID"] = json!("new-id");
        assert_eq!(fixture.document(), expected);
    }
    for input in [json!(false), json!({"id":[]})] {
        fixture.write_document(&input);
        let before = fs::read(&fixture.path).unwrap();
        assert_eq!(
            fixture
                .open()
                .regenerate_account_id_with(|| Ok("new-id".into())),
            Err(StoreError::InvalidDocument)
        );
        assert_eq!(fs::read(&fixture.path).unwrap(), before);
    }
}

#[test]
fn registry_account_regeneration_generation_and_lock_failures_preserve_bytes() {
    let fixture = Fixture::new();
    fixture
        .write_document(&json!({"id":{"accountUUID":"old"},"fusion":{"installationID":"engine"}}));
    let store = fixture.open();
    let before = fs::read(&fixture.path).unwrap();
    assert_eq!(
        store.regenerate_account_id_with(|| Err(StoreError::Io)),
        Err(StoreError::Io)
    );
    assert_eq!(fs::read(&fixture.path).unwrap(), before);
    let lock = OpenOptions::new()
        .read(true)
        .write(true)
        .open(&store.lock_path)
        .unwrap();
    lock.try_lock().unwrap();
    assert_eq!(
        store.regenerate_account_id_with(|| panic!("generator must run inside the lock")),
        Err(StoreError::Io)
    );
    assert_eq!(fs::read(&fixture.path).unwrap(), before);
}
