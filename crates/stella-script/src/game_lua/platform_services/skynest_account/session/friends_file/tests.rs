use super::*;
use std::sync::atomic::{AtomicU64, Ordering};

struct Fixture {
    root: PathBuf,
}
impl Fixture {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let root = std::env::temp_dir().join(format!(
            "stella-friends-file-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&root).unwrap();
        Self { root }
    }
    fn file(&self, account: &str) -> FriendsFile {
        FriendsFile::for_account(&self.root.join("fusion.registry"), account).unwrap()
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

// Independent OpenSSL AES-256-CBC/zero-IV/no-padding fixtures, derived from
// native 1006FBC98. The valid case has 16-byte text and native fill 0..14/16.
const VALID: [u8; 32] = [
    0xba, 0x16, 0x1c, 0x88, 0x9e, 0xe4, 0x59, 0x2d, 0xce, 0xd2, 0x97, 0x32, 0xa2, 0xbd, 0x04, 0x27,
    0x11, 0xf5, 0xa4, 0x4a, 0x57, 0x32, 0x1a, 0xcf, 0x04, 0x47, 0xb7, 0xa5, 0xd1, 0xc9, 0x21, 0x83,
];
const BAD_PADDING: [u8; 16] = [
    0x6a, 0xf3, 0x12, 0xb0, 0x64, 0xba, 0xca, 0x2b, 0x4c, 0x27, 0x9b, 0xf7, 0x6f, 0x41, 0x06, 0x65,
];
const ZERO_PADDING: [u8; 16] = [
    0xe1, 0x41, 0x67, 0xe9, 0x61, 0xf7, 0xd2, 0x95, 0xa8, 0x79, 0xd3, 0x47, 0x2c, 0x42, 0xdb, 0xce,
];

#[test]
fn native_friends_file_reads_independent_ciphertext_and_native_zero_padding() {
    let f = Fixture::new();
    let file = f.file("own");
    fs::write(file.path(), VALID).unwrap();
    assert_eq!(file.read().unwrap(), "0123456789abcdef");
    fs::write(file.path(), ZERO_PADDING).unwrap();
    assert_eq!(file.read().unwrap().as_bytes(), b"0123456789abcd\0\0");
}

#[test]
fn native_friends_file_missing_empty_and_corrupt_aes_are_empty_without_rewrite() {
    let f = Fixture::new();
    let file = f.file("own");
    assert!(file.read().unwrap().is_empty());
    assert!(!file.path().exists());
    for bytes in [&[][..], &[42][..], &BAD_PADDING[..]] {
        fs::write(file.path(), bytes).unwrap();
        assert!(file.read().unwrap().is_empty());
        assert_eq!(fs::read(file.path()).unwrap(), bytes);
    }
}

#[test]
fn native_friends_file_keeps_json_validation_separate_from_decryption() {
    let f = Fixture::new();
    let file = f.file("own");
    file.write("{").unwrap();
    assert_eq!(file.read().unwrap(), "{");
    assert!(serde_json::from_str::<serde_json::Value>(&file.read().unwrap()).is_err());
    fs::write(file.path(), encode_with_key(&[0xff], &FRIENDS_KEY).unwrap()).unwrap();
    assert!(file.read().unwrap_err().contains("UTF-8"));
}

#[test]
fn native_friends_file_account_scope_and_temporary_commit_leave_registry_untouched() {
    let f = Fixture::new();
    let registry = f.root.join("fusion.registry");
    fs::write(&registry, b"unrelated registry sentinel").unwrap();
    let own = f.file("own");
    let other = f.file("other");
    own.write(r#"{"friends":[]}"#).unwrap();
    other.write("other").unwrap();
    assert_eq!(own.path(), f.root.join("skynest_friends_store_own"));
    assert_eq!(own.read().unwrap(), r#"{"friends":[]}"#);
    assert_eq!(other.read().unwrap(), "other");
    assert_eq!(fs::read(&registry).unwrap(), b"unrelated registry sentinel");
    assert!(!f.root.join("skynest_friends_store_own.tmp").exists());
    let ciphertext = fs::read(own.path()).unwrap();
    assert!(ciphertext.len().is_multiple_of(16));
    assert!(!ciphertext.starts_with(b"{"));
    // Reopening ignores leftover temporary bytes, just as the native reader.
    fs::write(f.root.join("skynest_friends_store_own.tmp"), b"incomplete").unwrap();
    assert_eq!(f.file("own").read().unwrap(), r#"{"friends":[]}"#);
    own.write("").unwrap();
    assert_eq!(fs::read(own.path()).unwrap(), b"");
}

#[test]
fn native_friends_file_reports_io_and_failed_commit_without_destroying_old_file() {
    let f = Fixture::new();
    let file = f.file("own");
    file.write("old").unwrap();
    let previous = fs::read(file.path()).unwrap();
    fs::create_dir(f.root.join("skynest_friends_store_own.tmp")).unwrap();
    assert!(file.write("replacement").unwrap_err().contains("open"));
    assert_eq!(fs::read(file.path()).unwrap(), previous);
    let blocked = f.file("blocked");
    fs::create_dir(blocked.path()).unwrap();
    assert!(blocked.read().is_err());
    assert!(
        blocked
            .write("replacement")
            .unwrap_err()
            .contains("replace")
    );
    assert!(blocked.path().is_dir());
}

#[test]
fn native_friends_file_host_scope_and_size_guards_are_explicit_errors() {
    let f = Fixture::new();
    for account in ["../x", "x/y", "x\\y", "x\0y"] {
        assert!(FriendsFile::for_account(&f.root.join("fusion.registry"), account).is_err());
    }
    // Empty IDs remain valid native IDs, including in the filename.
    assert_eq!(f.file("").path(), f.root.join("skynest_friends_store_"));
    let file = f.file("huge");
    fs::File::create(file.path())
        .unwrap()
        .set_len(MAX_BYTES + 1)
        .unwrap();
    assert!(file.read().unwrap_err().contains("host byte limit"));
    assert!(
        file.write(&"x".repeat(MAX_BYTES as usize))
            .unwrap_err()
            .contains("host byte limit")
    );
}
