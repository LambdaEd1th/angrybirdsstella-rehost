use std::collections::HashMap;

use flate2::{Compression, GzBuilder};

use super::*;
use crate::bundle_format::{FORMAT_VERSION, ResourceFile, validate_relative_path};

struct Sandbox(PathBuf);

impl Sandbox {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let time = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "stella-bundle-test-{}-{time}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&root).unwrap();
        Self(root)
    }
}

impl Drop for Sandbox {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}

fn fixture(files: &[(&str, &[u8])]) -> (Vec<u8>, Vec<u8>) {
    let mut encoder = GzBuilder::new()
        .mtime(0)
        .write(Vec::new(), Compression::default());
    let mut manifest = Manifest {
        version: FORMAT_VERSION,
        files: Vec::new(),
    };
    for (path, bytes) in files {
        encoder.write_all(bytes).unwrap();
        manifest.files.push(ResourceFile {
            path: (*path).into(),
            size: bytes.len() as u64,
            sha256: Sha256::digest(bytes).into(),
        });
    }
    let mut compressed = BUNDLE_MAGIC.to_vec();
    compressed.extend(encoder.finish().unwrap());
    (serde_json::to_vec(&manifest).unwrap(), compressed)
}

#[test]
fn first_launch_and_cache_reuse_preserve_independent_saves() {
    let sandbox = Sandbox::new();
    let saves = sandbox.0.join("runtime/appdata");
    fs::create_dir_all(&saves).unwrap();
    let settings = saves.join("settings.lua");
    fs::write(&settings, b"existing-player-save").unwrap();
    let (manifest, compressed) = fixture(&[
        ("scripts/game.lua", b"game"),
        ("textures/test.png", b"pixels"),
    ]);
    let data = install_resources(&sandbox.0, &manifest, &compressed).unwrap();
    assert_eq!(fs::read(data.join("scripts/game.lua")).unwrap(), b"game");
    assert_eq!(fs::read(data.join("textures/test.png")).unwrap(), b"pixels");
    assert_eq!(data.parent().unwrap().join("appdata"), saves);
    assert_eq!(
        install_resources(&sandbox.0, &manifest, &compressed).unwrap(),
        data
    );
    assert_eq!(fs::read(settings).unwrap(), b"existing-player-save");
    assert_eq!(fs::read_dir(data.parent().unwrap()).unwrap().count(), 2);
}

#[test]
fn resource_updates_use_distinct_directories_and_the_same_save_root() {
    let sandbox = Sandbox::new();
    let (a, compressed_a) = fixture(&[("scripts/game.lua", b"first")]);
    let (b, compressed_b) = fixture(&[("scripts/game.lua", b"second")]);
    let old = install_resources(&sandbox.0, &a, &compressed_a).unwrap();
    let new = install_resources(&sandbox.0, &b, &compressed_b).unwrap();
    assert_ne!(old, new);
    assert_eq!(old.parent(), new.parent());
    assert_eq!(fs::read(old.join("scripts/game.lua")).unwrap(), b"first");
    assert_eq!(fs::read(new.join("scripts/game.lua")).unwrap(), b"second");
}

#[test]
fn corrupt_or_truncated_payload_never_installs_a_partial_tree() {
    let sandbox = Sandbox::new();
    let (manifest, mut compressed) = fixture(&[("scripts/game.lua", b"complete game")]);
    compressed.truncate(compressed.len() - 5);
    assert!(install_resources(&sandbox.0, &manifest, &compressed).is_err());
    assert_eq!(fs::read_dir(sandbox.0.join("runtime")).unwrap().count(), 0);
    let (_, valid) = fixture(&[("scripts/game.lua", b"complete game")]);
    assert!(install_resources(&sandbox.0, &manifest, &valid).is_ok());
}

#[test]
fn payload_hash_mismatch_and_trailing_bytes_are_rejected() {
    let sandbox = Sandbox::new();
    let (manifest, compressed) = fixture(&[("scripts/game.lua", b"correct")]);
    let (_, wrong) = fixture(&[("scripts/game.lua", b"changed")]);
    assert!(install_resources(&sandbox.0, &manifest, &wrong).is_err());
    let mut trailing = compressed.clone();
    trailing.push(0);
    assert!(install_resources(&sandbox.0, &manifest, &trailing).is_err());
    assert_eq!(fs::read_dir(sandbox.0.join("runtime")).unwrap().count(), 0);
}

#[test]
fn modified_cache_content_is_reported_without_changing_saves() {
    let sandbox = Sandbox::new();
    let (manifest, compressed) = fixture(&[("scripts/game.lua", b"game")]);
    let data = install_resources(&sandbox.0, &manifest, &compressed).unwrap();
    let saves = data.parent().unwrap().join("appdata");
    fs::create_dir(&saves).unwrap();
    fs::write(saves.join("settings.lua"), b"save").unwrap();
    fs::write(data.join("scripts/game.lua"), b"fake").unwrap();
    let error = install_resources(&sandbox.0, &manifest, &compressed).unwrap_err();
    assert!(error.to_string().contains("SHA-256 mismatch"));
    assert_eq!(fs::read(saves.join("settings.lua")).unwrap(), b"save");
}

#[test]
fn missing_cache_file_or_modified_manifest_is_reported() {
    let sandbox = Sandbox::new();
    let (manifest, compressed) = fixture(&[("scripts/game.lua", b"game")]);
    let data = install_resources(&sandbox.0, &manifest, &compressed).unwrap();
    fs::remove_file(data.join("scripts/game.lua")).unwrap();
    assert!(install_resources(&sandbox.0, &manifest, &compressed).is_err());
    fs::write(data.join("scripts/game.lua"), b"game").unwrap();
    fs::write(data.join(INSTALLED_MANIFEST), b"{}").unwrap();
    assert!(install_resources(&sandbox.0, &manifest, &compressed).is_err());
}

#[cfg(unix)]
#[test]
fn cache_symlinks_are_rejected_without_touching_their_targets() {
    let sandbox = Sandbox::new();
    let (manifest, compressed) = fixture(&[("scripts/game.lua", b"game")]);
    let data = install_resources(&sandbox.0, &manifest, &compressed).unwrap();
    let other = sandbox.0.join("other");
    fs::write(&other, b"game").unwrap();
    fs::remove_file(data.join("scripts/game.lua")).unwrap();
    std::os::unix::fs::symlink(&other, data.join("scripts/game.lua")).unwrap();
    assert!(install_resources(&sandbox.0, &manifest, &compressed).is_err());
    assert_eq!(fs::read(other).unwrap(), b"game");
}

#[test]
fn concurrent_first_launch_uses_only_one_complete_installation() {
    let sandbox = Sandbox::new();
    let (manifest, compressed) = fixture(&[("scripts/game.lua", &[42; 100_000])]);
    let paths = std::thread::scope(|scope| {
        let threads: Vec<_> = (0..4)
            .map(|_| scope.spawn(|| install_resources(&sandbox.0, &manifest, &compressed).unwrap()))
            .collect();
        threads
            .into_iter()
            .map(|thread| thread.join().unwrap())
            .collect::<Vec<_>>()
    });
    assert!(paths.iter().all(|path| path == &paths[0]));
    assert_eq!(fs::read_dir(sandbox.0.join("runtime")).unwrap().count(), 1);
}

#[test]
fn invalid_paths_versions_and_duplicate_entries_fail_before_writing() {
    let sandbox = Sandbox::new();
    for path in [
        "",
        "/root",
        "../outside",
        "a/../outside",
        "a\\outside",
        "C:/outside",
        "a//b",
        "a/./b",
        "a/NUL.dat",
        "a/COM1",
        "a/file.",
        "a/file ",
    ] {
        assert!(validate_relative_path(path).is_err(), "accepted {path}");
        let (manifest, compressed) = fixture(&[(path, b"invalid")]);
        assert!(install_resources(&sandbox.0, &manifest, &compressed).is_err());
    }
    let (bytes, compressed) = fixture(&[("a", b"one"), ("a", b"two")]);
    assert!(install_resources(&sandbox.0, &bytes, &compressed).is_err());
    let mut manifest: Manifest = serde_json::from_slice(&bytes).unwrap();
    manifest.version += 1;
    assert!(
        install_resources(
            &sandbox.0,
            &serde_json::to_vec(&manifest).unwrap(),
            &compressed
        )
        .is_err()
    );
    assert!(!sandbox.0.join("runtime").exists());
}

#[test]
fn directory_selection_is_independent_of_the_working_directory() {
    let root = std::env::temp_dir();
    let mut variables = HashMap::from([
        ("HOME", root.clone().into_os_string()),
        ("LOCALAPPDATA", root.clone().into_os_string()),
    ]);
    assert_eq!(
        user_data_root("macos", |key| variables.get(key).cloned()).unwrap(),
        root.join("Library/Application Support/Angry Birds Stella Rehost")
    );
    assert_eq!(
        user_data_root("windows", |key| variables.get(key).cloned()).unwrap(),
        root.join("Angry Birds Stella Rehost")
    );
    assert_eq!(
        user_data_root("linux", |key| variables.get(key).cloned()).unwrap(),
        root.join(".local/share/angry-birds-stella-rehost")
    );
    variables.insert("XDG_DATA_HOME", root.join("xdg").into_os_string());
    assert_eq!(
        user_data_root("linux", |key| variables.get(key).cloned()).unwrap(),
        root.join("xdg/angry-birds-stella-rehost")
    );
    variables.insert(
        "STELLA_USER_DATA_DIR",
        root.join("private").into_os_string(),
    );
    assert_eq!(
        user_data_root("macos", |key| variables.get(key).cloned()).unwrap(),
        root.join("private")
    );
    variables.insert("STELLA_USER_DATA_DIR", "relative".into());
    assert!(user_data_root("linux", |key| variables.get(key).cloned()).is_err());
}
